//! Command line arguments for the client

use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub struct Args {
    /// The server IPv6 IP + port to connect to.
    #[structopt(short = "a", long, default_value = "http://[::1]:50051")]
    pub address: String,

    /// The API method to call (Start, Stop, Status, Stream)
    #[structopt(subcommand)]
    pub api_command: WorkerAPI,
}

#[derive(StructOpt, Debug)]
pub enum WorkerAPI {
    /// Start a new process job
    Start {
        /// Command + arguments you wish to execute
        /// Example : `cargo run`, `ls -la `, `/bin/bash script.sh`
        #[structopt(subcommand)]
        ext_command: ExtCommand,
    },

    /// Stop a running process
    Stop {
        /// Process uuid
        uuid: String,

        /// Forcefully shutdown the process using SIGKILL
        #[structopt(short = "f", long)]
        forced: bool,
    },

    /// Stream all previous and upcoming stderr and stdout data for a specified job
    Stream {
        /// Process uuid
        uuid: String,
    },

    /// Return the status of a previous process job
    Status {
        /// Process uuid
        uuid: String,
    },
}

#[derive(StructOpt, Debug)]
pub enum ExtCommand {
    #[structopt(external_subcommand)]
    Args(Vec<String>),
}
