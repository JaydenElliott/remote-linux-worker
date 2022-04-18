//! Exposes the job type and it's implementation to the server

use crate::errors::RLWServerError;
use crate::job_processor::*;
use crate::processing::execute_command;

use tokio::sync::{mpsc as tokio_mpsc, Mutex};
use tokio::task::JoinHandle;
use tonic::Status;

use std::sync::{
    mpsc::{self, Receiver, Sender},
    Arc,
};
use std::{mem, os::unix::prelude::ExitStatusExt, process::ExitStatus, thread};

/**
 *
 *
 *
 *
 *
 * TODO FIGURE OUT WAY TO UPDATE PID IMMEDIATELY - just unlock mutex from grpc level ez
 */

// TODO update this
const STREAM_BUFFER_SIZE: usize = 100;

/// A user job containing information about the
/// underlying process.
pub struct Job {
    /// Job status
    pub status: Mutex<status_response::ProcessStatus>,

    /// Stderr and stdout output from the job.
    pub output: Mutex<Vec<u8>>,

    /// Job process ID
    pub pid: Mutex<Option<u32>>,
}

impl Job {
    /// Construct a new Job
    pub fn new() -> Self {
        Self {
            status: Mutex::new(status_response::ProcessStatus::Running(false)),
            output: Mutex::new(Vec::new()),
            pid: Mutex::new(None),
        }
    }

    /// Start a new process, and populates the job pid, output and status from the process.
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

        // Set Process ID
        let mut pid = self.pid.lock().await;
        *pid = Some(rx_pid.recv()?);
        mem::drop(pid); // release guard

        // Set status to Running
        let mut status = self.status.lock().await;
        *status = status_response::ProcessStatus::Running(true);
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
            *status = status_response::ProcessStatus::Signal(s);
            return Ok(());
        }

        // Finished with exit code
        if let Some(c) = status.code() {
            let mut status = self.status.lock().await;
            *status = status_response::ProcessStatus::ExitCode(c);
            return Ok(());
        }

        // Thread closed job not yet finished
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
    /// * `pid`    - the process id of the process to send the kill signal to.
    /// * `forced` - if true, the signal sent will be SIGKILL, and SIGTERM otherwise
    pub async fn stop_command(&self, forced: bool) -> Result<(), RLWServerError> {
        let (tx_output, rx_output): (Sender<u8>, Receiver<u8>) = mpsc::channel();

        // Get the PID of the job to kill
        let lock_handle = self.pid.lock().await;
        let pid = lock_handle
            .ok_or(RLWServerError(
                "There is no running process associated with this job".to_string(),
            ))?
            .to_string();
        mem::drop(lock_handle);

        // Send kill signal
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

    /// Stream the history and all upcoming output from `self.start_command()`.
    ///
    /// # Arguments
    /// * self: Arc<Job> - Arc of self. Required in order to use self.output in async tokio thread.
    ///
    /// # Returns
    ///
    /// The function returns a result with a tuple containing the following types:
    /// * StreamResponse Receiver - type to be used by tonic to stream output to the client.
    /// * JoinHandle              - used to propagate errors up to the main thread in order to be handled.
    pub async fn stream_job(
        self: Arc<Job>,
    ) -> Result<
        (
            tokio_mpsc::Receiver<Result<StreamResponse, Status>>,
            JoinHandle<Result<(), RLWServerError>>,
        ),
        RLWServerError,
    > {
        // TODO: update the buffer size - idk what it should be yet
        let (tx, rx) = tokio_mpsc::channel(STREAM_BUFFER_SIZE);
        let stream_handle = tokio::spawn(async move {
            let mut read_idx;

            // Send the client the job history
            {
                let output_guard = self.output.lock().await;
                let res = tx
                    .send(Ok(StreamResponse {
                        output: output_guard.clone(),
                    }))
                    .await;

                if let Err(e) = res {
                    return Err(RLWServerError(format!("{:?}", e)));
                }
                read_idx = output_guard.len();
            }

            // While the job is running, send new output entries
            while matches!(
                *self.status.lock().await,
                status_response::ProcessStatus::Running(true)
            ) {
                let output = self.output.lock().await;
                if read_idx < output.len() {
                    tx.send(Ok(StreamResponse {
                        output: vec![output[read_idx]],
                    }))
                    .await?;
                    read_idx += 1;
                }
            }

            // In the event that the process is no longer running but there is still
            // remaining output entries to stream - finish streaming.
            let output = self.output.lock().await;
            while read_idx < output.len() {
                let resp = StreamResponse {
                    output: vec![output[read_idx]],
                };
                tx.send(Ok(resp)).await?;
                read_idx += 1;
            }
            Ok(())
        });
        Ok((rx, stream_handle))
    }

    /// Returns the job status object
    pub async fn status(&self) -> Result<StatusResponse, RLWServerError> {
        let status = self.status.lock().await.clone();
        Ok(StatusResponse {
            process_status: Some(status),
        })
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

        // Figure out a way to remove this
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
        let arc2 = Arc::clone(&job_arc);
        let task2 = tokio::spawn(async move {
            let (mut rx, stream_handle) = arc2.stream_job().await.expect("Bad");
            while let Some(i) = rx.recv().await {
                println!("i = {:?}", i.expect("bad"));
            }

            stream_handle.await.expect("bad").expect("bad");
        });

        let _ = task1.await?;
        let _ = task2.await?;

        Ok(())
    }
}
