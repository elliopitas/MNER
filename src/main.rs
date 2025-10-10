mod run;

use std::collections::HashMap;
use std::fmt::Arguments;

use std::fs;
use clap::{Parser, Subcommand};
use futures::future::join_all;
use itertools::Itertools;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use toml;
use serde::Deserialize;

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
    },
    Collect,
}

fn main() {
    let args = Args::parse();
    match args.command {
        Commands::Run { config, output } => {
            println!("Running with config: {} and output:{}", config, output);
            let config_struct = run::config_file::parse_config(&config);
            println!("Loaded config: {:?}", config_struct);
            let permutations = config_struct.get_arguments_permutations();
            println!("Permutations: {:?}", permutations);

        }
        Commands::Collect => {
            println!("TODO: implement Collect");
        }
    }
}

