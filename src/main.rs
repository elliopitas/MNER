mod run;

use spdlog::prelude::*;
use clap::{Parser, Subcommand};
use anyhow::{Context, Result};
use std::sync::Arc;
use futures::future::{join_all};
use tokio::process::{Command};
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use crossbeam_queue::ArrayQueue;
use log::info;
use crate::run::config_file::Permutation;

#[derive(Parser, Debug)]
#[command(author = "Georgios Constantinides", version = "0.0.1", about = "Run experiments with a permutation of different parameters on multiple ssh nodes", long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Run {
        #[arg(default_value = "experiment.toml")]
        config: String,
        #[arg(default_value = "results")]
        output: String,
        #[arg(short, long)]
        ssh_keys: Vec<String>,
    },
    Collect,
}
struct SshAgent{
    pid: u32,
}

impl Drop for SshAgent {
    fn drop(&mut self) {
        info!("Shutting down ssh-agent (PID: {})...", self.pid);
        let _ = std::process::Command::new("kill")
            .arg(self.pid.to_string())
            .status();
    }
}

async unsafe fn setup_ssh_agent(keys: &[String]) -> Result<SshAgent> {
    let agent_output = Command::new("ssh-agent")
        .arg("-s")
        .output()
        .await
        .context("Failed to start ssh-agent.")?;
    
    if !agent_output.status.success() {
        anyhow::bail!("ssh-agent exited with an error: {}", String::from_utf8_lossy(&agent_output.stderr));
    }

    let stdout = String::from_utf8(agent_output.stdout)?;
    let mut agent_pid = None;
    for line in stdout.lines() {
        if let Some(var_line) = line.split(';').next() {
            if let Some((key, value)) = var_line.split_once('=') {
                unsafe {
                    std::env::set_var(key, value);
                }
                if key == "SSH_AGENT_PID" {
                    agent_pid = value.parse::<u32>().ok();
                }
            }
        }
    }

    let pid = agent_pid.context("Could not parse SSH_AGENT_PID from ssh-agent output.")?;
    info!("ssh-agent started with PID: {}", pid);

    let mut cmd = Command::new("ssh-add");
    if keys.is_empty() {
        warn!("No SSH keys specified, attempting to add default identities...");
    } else {
        info!("Adding specified SSH keys to agent: {:?}", keys);
        cmd.args(keys);
    }
    let add_status = cmd.status().await.context("Failed to execute ssh-add.")?;
    if !add_status.success() {
        anyhow::bail!("ssh-add failed");
    }

    Ok(SshAgent { pid })
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    spdlog::default_logger().set_level_filter(spdlog::LevelFilter::All);
    match args.command {
        Commands::Run { config, output, ssh_keys } => unsafe {
            let _agent = setup_ssh_agent(&ssh_keys).await?;

            debug!("Running with config: {} and output:{}", config, output);
            let config_struct = run::config_file::Config::new(&config);
            debug!("Loaded config: {:?}", config_struct);
            let mut permutations = config_struct.get_arguments_permutations();
            debug!("Permutations: {:?}", permutations);
            let results_path_string = format!("./results/{}", &config_struct.name);
            let results_path = Path::new(results_path_string.as_str());
            match fs::create_dir_all(results_path){
                Ok(_) => {
                    for entry in fs::read_dir(results_path)? {
                        let valid_entry = entry.expect("Error reading result folder name");
                        let folder_name = valid_entry.file_name();
                        let folder_name_str = folder_name.to_str().expect("Failed to convert results folder name to string");
                        if permutations.contains_key(folder_name_str) && valid_entry.path().join("complete").exists() {
                            permutations.remove(folder_name_str).expect("Failed to remove found permutation");
                        }
                    }
                let queue = Arc::new(ArrayQueue::<Permutation>::new(permutations.len()));
                    for permutation in permutations {
                        queue.push(Permutation {
                            id: permutation.0,
                            parameters: permutation.1
                        }).expect("Task queue full. This should not have happened");
                    }

                    let nodes = run::nodes::Nodes::new(&config_struct.hosts).await.expect("Failed to connect to nodes");

                    let temp_path_string = format!("/tmp/MNER/{}", &config_struct.name);
                    let temp_path = Path::new(temp_path_string.as_str());
                    let temp_workdir =  temp_path.join("workdir");
                    let temp_workdir_str = temp_workdir.to_str().expect("failed to create path string for workdir");
                    let temp_results_path = temp_path.join("results");
                    let temp_results_path = &temp_results_path;
                    let temp_workdir_executable_path = temp_workdir.join(&config_struct.executable);
                    let temp_workdir_executable_str = temp_workdir_executable_path.to_str().expect("failed to create temp workdir executable path string");

                    let config_struct = &config_struct;
                    let queue = queue.clone();
                    let node_futures = nodes.nodes.iter().map(|node| {
                        let queue = queue.clone();
                        async move {
                        debug!("Syncing {} to {}", &config_struct.workdir,  node.hostname);
                        match node.rsync_to(&config_struct.workdir, temp_workdir_str, false).await {
                            Ok(_) => {
                                debug!("Synced {} to {}/workdir", &config_struct.workdir,  node.hostname);
                                let concurrency = if config_struct.threads_per_task == 0 {1} else  {node.threads/config_struct.threads_per_task};
                                let mut node_worker_futures = Vec::with_capacity(concurrency);
                                for _ in 0..concurrency{

                                    let queue = queue.clone();
                                    node_worker_futures.push(async move {
                                        loop{
                                            match queue.pop() {
                                                None => break,
                                                Some(permutation) => {
                                                    let tmp_permutation_result_path = temp_results_path.join(&permutation.id);
                                                    let tmp_permutation_result_path_str = tmp_permutation_result_path.to_str().expect("failed to create path string for job result");
                                                    match node.client.execute(format!("mkdir -p {tmp_permutation_result_path_str} && cd {tmp_permutation_result_path_str} && {temp_workdir_executable_str} {}", permutation.parameters).as_str()).await {
                                                        Ok(output) =>{
                                                            let permutation_result_path = results_path.join(&permutation.id);
                                                            let permutation_result_path_str = permutation_result_path.to_str().expect("failed to convert permutation_result_path to string");
                                                            let _ = fs::remove_dir_all(&permutation_result_path);
                                                            match fs::create_dir_all(&permutation_result_path){
                                                                Ok(_)=>{
                                                                    if output.exit_status == 0{
                                                                        match node.rsync_from(tmp_permutation_result_path_str, permutation_result_path_str, true).await{
                                                                            Ok(_) => {
                                                                                if let Err(err) = File::create(permutation_result_path.join("succeeded")) {
                                                                                    error!("failed to create \"succeeded\" file for job {}\n{}", permutation.id, err);
                                                                                }
                                                                            },
                                                                            Err(err) => error!("failed to rsync completed data of task from {} to {}\n{}",tmp_permutation_result_path_str, permutation_result_path_str, err)
                                                                        }
                                                                    }else {
                                                                        match File::create(permutation_result_path.join("failed")){
                                                                            Ok(_) => {}, //create empty file
                                                                            Err(err) => error!("failed to create \"failed\" file for job {}\n{}", permutation.id, err)
                                                                        }
                                                                    }

                                                                    match File::create(permutation_result_path.join("stdout")) {
                                                                        Ok(mut file) => {
                                                                            if let Err(err) = file.write(output.stdout.as_bytes()){
                                                                                error!("failed to write data to stdout file for {}\n{}", permutation.id, err);
                                                                            }
                                                                        },
                                                                        Err(err) => error!("failed to create stdout file for {}\n{}", permutation.id, err)
                                                                    }
                                                                    match File::create(permutation_result_path.join("stderr")) {
                                                                        Ok(mut file) => {
                                                                            if let Err(err) = file.write(output.stderr.as_bytes()){
                                                                                error!("failed to write data to stderr file for {}\n{}", permutation.id, err);
                                                                            }
                                                                        },
                                                                        Err(err) => error!("failed to create stderr file for {}\n{}", permutation.id, err)
                                                                    }
                                                                },
                                                                Err(err) => error!("failed to create result for job: {}\n{}", permutation.id, err)
                                                            }
                                                        },
                                                        Err(err) => error!("failed to execute task {} on {}\n{}", permutation.id, node.hostname, err)
                                                    }
                                                }
                                            }
                                        }
                                    });
                                }
                                join_all(node_worker_futures).await;
                            },
                            Err(err) => error!("Failed to rsync data to host {}. It will be skipped\n{}", node.hostname, err),
                        }
                    }
                    });
                    join_all(node_futures).await;

                    let cleanup_futures = nodes.nodes.iter().map(|node| async {
                        match node.rm(temp_path_string.as_str()).await {
                            Ok(_) => debug!("Removed {} from {}", temp_path_string, node.hostname),
                            Err(err) => debug!("Failed to remove {} from {}\n{}", temp_path_string, node.hostname, err)
                        }
                    });
                    join_all(cleanup_futures).await;
                },
                Err(err) => error!("Failed to create results directory\n{}", err),
            }

        }
        Commands::Collect => {
            println!("TODO: implement Collect");
        }
    }

    Ok(())
}
