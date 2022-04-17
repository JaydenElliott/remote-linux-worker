//! Exposes all the required types and type impls for the
//! rlw server to run.

use tokio::sync::Mutex;
use tonic::codegen::http::status;

use crate::errors::RLWServerError;
use crate::job_processor::*;
use crate::processing::execute_command;

use std::{os::unix::prelude::ExitStatusExt, process::ExitStatus};

use std::sync::mpsc::{self, Receiver, Sender};
use std::{mem, thread};

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
    /// Construct a new Job
    pub fn new() -> Self {
        Self {
            status: Mutex::new(None),
            output: Mutex::new(Vec::new()),
            pid: Mutex::new(None),
        }
    }

    /// Start a new process, and populates the job pid, output
    /// and status from the process.
    ///
    /// # Arguments
    ///
    /// * `command`      - Command to execute. Examples: "cargo", "ls", , "/bin/bash"
    /// * `args`         - Arguments to accompany the command. Examples: "--version", "-a", "./file.sh"
    pub async fn start_command(
        &self,
        command: String,
        args: Vec<String>,
    ) -> Result<(), RLWServerError> {
        let (tx_output, rx_output): (Sender<u8>, Receiver<u8>) = mpsc::channel();
        let (tx_pid, rx_pid): (Sender<u32>, Receiver<u32>) = mpsc::channel();

        // Process job
        let thread = thread::spawn(move || -> Result<ExitStatus, RLWServerError> {
            execute_command(command, args, Some(&tx_pid), &tx_output)
        });

        let mut pid = self.pid.lock().await;
        *pid = Some(rx_pid.recv()?);
        mem::drop(pid); // release guard

        let mut status = self.status.lock().await;
        *status = Some(status_response::ProcessStatus::Running(true));
        mem::drop(status); // release guard

        // Populate stdout/stderr output
        for rec in rx_output {
            self.output.lock().await.push(rec);
        }

        // Process finished
        let status = thread
            .join()
            .map_err(|e| RLWServerError(format!("Error joining on processing thread {:?}", e)))??;

        // Finished with signal
        if let Some(s) = status.signal() {
            let mut status = self.status.lock().await;
            *status = Some(status_response::ProcessStatus::Signal(s));
            return Ok(());
        }

        // Finished with exit code
        if let Some(c) = status.code() {
            let mut status = self.status.lock().await;
            *status = Some(status_response::ProcessStatus::ExitCode(c));
            return Ok(());
        }

        // Thread closed but job had not finished
        Err(RLWServerError(
            "Job processing thread closed before it had finished processing".to_string(),
        ))
    }

    /// Kill the requested process.
    ///
    /// Sending a kill signal starts a new process; because of this the request is
    /// handled similarly to `job.start_command()`. The output of the kill process
    /// will be sent to the same output buffer as the process you are signaling to
    /// kill. The status of the kill signal process will not be stored.
    ///
    /// # Arguments
    ///
    /// * `pid` - the process id of the process to send the kill signal to.
    /// * `forced` - if true, the signal sent will be SIGKILL, and SIGTERM otherwise
    pub async fn stop_command(&self, forced: bool) -> Result<(), RLWServerError> {
        let (tx_output, rx_output): (Sender<u8>, Receiver<u8>) = mpsc::channel();

        let lock_handle = self.pid.lock().await;
        let pid = lock_handle
            .ok_or(RLWServerError(
                "There is no running process associated with this job".to_string(),
            ))?
            .to_string();
        mem::drop(lock_handle);

        // Process kill signal
        let thread = thread::spawn(move || -> Result<ExitStatus, RLWServerError> {
            if forced {
                return execute_command(
                    "kill".to_string(),
                    vec!["-9".to_string(), pid],
                    None,
                    &tx_output,
                );
            }
            return execute_command(
                "kill".to_string(),
                vec!["-15".to_string(), pid],
                None,
                &tx_output,
            );
        });

        // Populate stdout/stderr output
        for rec in rx_output {
            self.output.lock().await.push(rec);
        }

        // Process finished
        thread.join().map_err(|e| {
            RLWServerError(format!("Error joining on kill signal thread {:?}", e))
        })??;

        Ok(())
    }

    /// Stream the output of the job process.
    pub async fn stream_output(&self) {
        let mut read_idx: usize = 0;
        while matches!(
            *self.status.lock().await,
            Some(status_response::ProcessStatus::Running(true))
        ) {
            let o = self.output.lock().await;
            // Only read each letter once
            // If finished reading "so far"
            // just spin and wait
            if read_idx < o.len() {
                // stream o[read_idx]
                println!("Read idx = {:?}", o[read_idx]);
                read_idx += 1;
            } else {
                // should I sleep here
            }
        }

        // In the event that the process is no longer running,
        // but the output wasn't finished being streamed, stream
        // the rest.
        let o = self.output.lock().await;
        while read_idx < o.len() {
            println!("Read idx after = {:?}", o[read_idx]);
            read_idx += 1;
        }
    }
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
            arc1.start_command("/bin/bash".to_string(), vec!["./test2.sh".to_string()])
                .await
                .expect("bad in here");
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        let arc2 = Arc::clone(&job_arc);
        let task2 = tokio::spawn(async move {
            arc2.stream_output().await;
        });

        // tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        // let arc3 = Arc::clone(&job_arc);
        // let arc4 = Arc::clone(&job_arc);
        // let pid = arc3.pid.lock().await.expect("no pid");
        // let task3 = tokio::spawn(async move {
        //     arc4.stop_command(true).await.expect("bad in here");
        // });

        let _ = task1.await?;
        let _ = task2.await?;
        // let _ = task3.await?;

        Ok(())
    }
}
