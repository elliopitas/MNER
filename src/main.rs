mod run;

use spdlog::prelude::*;
use std::fmt::format;
use clap::{Parser, Subcommand};
use anyhow::{Context, Result};
use futures::future::err;
use futures::FutureExt;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};

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
                std::env::set_var(key, value);
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
            let permutations = config_struct.get_arguments_permutations();
            debug!("Permutations: {:?}", permutations);
            let nodes = run::nodes::Nodes::new(&config_struct.hosts).await.expect("Failed to connect to nodes");
            let path = format!("/tmp/MNER/{}", &config_struct.name);
            let path_str = path.as_str();
            for node in nodes.nodes.iter() {
                debug!("Syncing {} to {}", &config_struct.workdir,  node.hostname);
                match node.rsync(&config_struct.workdir, format!("{path_str}/workdir").as_str()).await {
                    Ok(_) => {
                        debug!("Synced {} to {}/workdir", &config_struct.workdir,  node.hostname);

                        match node.rm(path_str).await {
                            Ok(_) => debug!("Removed {} from {}", path_str, node.hostname),
                            Err(err) => debug!("Failed to remove {} from {}\n{}", path_str, node.hostname, err)
                        }
                    },
                    Err(err) => error!("Failed to rsync data to host {}\n{}", node.hostname, err),
                }
            }


        }
        Commands::Collect => {
            println!("TODO: implement Collect");
        }
    }

    Ok(())
}
