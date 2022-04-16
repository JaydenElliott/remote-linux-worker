//! Exposes all the required types and type impls for the
//! rlw server to run.

use crate::errors::RLWServerError;
use crate::job_processor::*;
use crate::processing::execute_command;

use std::{os::unix::prelude::ExitStatusExt, process::ExitStatus};

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

const COMMAND_DIR: &str = "./tests/test_env";

/// A user job containing information about the
/// underlying process.
pub struct Job {
    /// Job status
    pub status: Option<status_response::ProcessStatus>,

    /// Stderr and stdout output from the job.
    pub output: Vec<u8>,

    /// Job process ID
    pub pid: Option<u32>,
}

impl Job {
    pub fn new() -> Self {
        Self {
            status: None,
            output: Vec::new(),
            pid: None,
        }
    }
    pub fn new_command(
        &mut self,
        command: String,
        args: Vec<String>,
        command_type: CommandType,
    ) -> Result<(), RLWServerError> {
        let (tx_output, rx_output): (Sender<u8>, Receiver<u8>) = mpsc::channel();
        let (tx_pid, rx_pid): (Sender<u32>, Receiver<u32>) = mpsc::channel();

        let thread = thread::spawn(move || -> Result<ExitStatus, RLWServerError> {
            execute_command(command, args, Some(&tx_pid), &tx_output)
        });

        if let CommandType::Start = command_type {
            self.pid = Some(rx_pid.recv().expect("bad"));
            self.status = Some(status_response::ProcessStatus::Running(true))
        }

        for rec in rx_output {
            // println!("Rec {:?}", std::str::from_utf8(&rec));
            self.output.push(rec)
            // Send grpc msg here?
        }

        // Process finished
        let status = thread.join().expect("bad").expect("bad");

        // Finished with signal
        if let Some(s) = status.signal() {
            self.status = Some(status_response::ProcessStatus::Signal(s));
            return Ok(());
        }

        // Process finished with exit code
        if let Some(c) = status.code() {
            self.status = Some(status_response::ProcessStatus::Signal(c));
            return Ok(());
        }

        Err(RLWServerError(
            "Job processing thread closed before finishing the job".to_string(),
        ))
    }
}

pub enum CommandType {
    Start,
    Stop,
    Stream,
    Status,
}
