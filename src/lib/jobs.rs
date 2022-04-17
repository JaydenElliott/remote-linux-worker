//! Exposes all the required types and type impls for the
//! rlw server to run.

use tokio::sync::Mutex;
use tonic::codegen::http::status;

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
    pub status: Mutex<Option<status_response::ProcessStatus>>,

    /// Stderr and stdout output from the job.
    pub output: Mutex<Vec<u8>>,

    /// Job process ID
    pub pid: Mutex<Option<u32>>,
}

impl Job {
    pub fn new() -> Self {
        Self {
            status: Mutex::new(None),
            output: Mutex::new(Vec::new()),
            pid: Mutex::new(None),
        }
    }
    pub async fn new_command(
        // &mut self,
        &self,
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
            // send down channel
            let mut pid = self.pid.lock().await;
            *pid = Some(rx_pid.recv()?);

            let mut status = self.status.lock().await;
            *status = Some(status_response::ProcessStatus::Running(true))
        }

        for rec in rx_output {
            // println!("Rec {:?}", std::str::from_utf8(&[rec]));
            self.output.lock().await.push(rec);
            // self.output.push(rec)
            // Send grpc msg here?
        }

        // Process finished
        let status = thread
            .join()
            .map_err(|e| RLWServerError(format!("Error joining on processing thread {:?}", e)))??;

        println!("\nhere\n");
        // Finished with signal
        if let Some(s) = status.signal() {
            // self.status = Some(status_response::ProcessStatus::Signal(s));

            let mut status = self.status.lock().await;
            *status = Some(status_response::ProcessStatus::Signal(s));
            return Ok(());
        }

        // Process finished with exit code
        if let Some(c) = status.code() {
            // self.status = Some(status_response::ProcessStatus::Signal(c));

            println!("\nhere2\n");
            let mut status = self.status.lock().await;
            *status = Some(status_response::ProcessStatus::ExitCode(c));
            return Ok(());
        }

        Err(RLWServerError(
            "Job processing thread closed before finishing the job".to_string(),
        ))
        // Ok(())
    }

    pub async fn read_test(&self) {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        // while matches!(
        //     self.status.lock().await.as_ref().expect("bad"),
        //     status_response::ProcessStatus::Running(true)
        // ) {
        // tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        //     println!("Rec {:?}", std::str::from_utf8(&*self.output.lock().await));
        // }

        loop {
            let status = self.status.lock().await;
            if matches!(*status, Some(status_response::ProcessStatus::Running(true))) {
                println!(
                    "\nRec {:?}\n",
                    std::str::from_utf8(&*self.output.lock().await)
                );
            } else {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
    }
}

pub enum CommandType {
    Start,
    Stop,
    Stream,
    Status,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn run_script() -> Result<(), Box<dyn std::error::Error>> {
        let job = Job::new();
        let job_arc = Arc::new(job);

        let arc1 = Arc::clone(&job_arc);
        let task1 = tokio::spawn(async move {
            arc1.new_command(
                "/bin/bash".to_string(),
                vec!["./test1.sh".to_string()],
                CommandType::Start,
            )
            .await
            .expect("bad in here");
        });

        let arc2 = Arc::clone(&job_arc);
        let task2 = tokio::spawn(async move {
            arc2.read_test().await;
        });

        let _ = task1.await?;
        let _ = task2.await?;

        Ok(())
        // job.new_command(
        //     "/bin/bash".to_string(),
        //     vec!["./test1.sh".to_string()],
        //     CommandType::Start,
        // )
        // .expect("bad123");
    }
}
