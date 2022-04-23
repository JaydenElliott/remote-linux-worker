//! Exposes the job type and it's implementation to the server

use crate::rlwp::processing::execute_command;
use crate::utils::errors::RLWServerError;
use crate::utils::job_processor_api::{status_response::ProcessStatus, *};

use log;
use nix::{sys::signal, unistd::Pid};
use std::borrow::BorrowMut;
use std::sync::{
    mpsc::{self, Receiver, Sender},
    Arc,
};
use std::{os::unix::prelude::ExitStatusExt, process::ExitStatus};
use synchronoise::SignalEvent;
use tokio::sync::{mpsc as tokio_mpsc, Mutex};
use tonic::Status;

/// A user job containing information about the
/// underlying process.
pub struct Job {
    /// Job status
    pub status: Mutex<status_response::ProcessStatus>,

    /// Stderr and stdout output from the job
    pub output: Mutex<Vec<u8>>,

    /// Job process ID
    pub pid: Mutex<Option<u32>>,

    /// New job output signal to send to a streaming thread
    pub output_signal: Arc<SignalEvent>,
}

impl Job {
    /// Construct a new Job
    pub fn new() -> Self {
        Self {
            status: Mutex::new(status_response::ProcessStatus::Running(false)),
            output: Mutex::new(Vec::new()),
            pid: Mutex::new(None),
            // output_signal: Arc::new(SignalEvent::auto(false)),
            output_signal: Arc::new(SignalEvent::manual(false)),
        }
    }

    /// Starts a new process and populates the job pid, output and status from the process.
    ///
    /// # Arguments
    ///
    /// * `command`      - Command to execute. Examples: "cargo", "ls", "/bin/bash".
    /// * `args`         - Arguments to accompany the command. Examples: "--version", "-a", "./file.sh".
    pub async fn start_command(
        &self,
        command: String,
        args: Vec<String>,
    ) -> Result<(), RLWServerError> {
        let (tx_output, rx_output): (Sender<Vec<u8>>, Receiver<Vec<u8>>) = mpsc::channel();
        let (tx_pid, rx_pid): (Sender<u32>, Receiver<u32>) = mpsc::channel();

        let thread = std::thread::spawn(move || -> Result<ExitStatus, RLWServerError> {
            let res = execute_command(command, args, Some(tx_pid), tx_output);
            res
        });

        // Process job
        // let thread = tokio::task::spawn_blocking(move || -> Result<ExitStatus, RLWServerError> {
        //     execute_command(command, args, Some(&tx_pid), &tx_output)
        // });

        // Set Process ID
        {
            let mut pid = self.pid.lock().await;
            *pid = Some(rx_pid.recv()?);
        }

        // Set status to Running
        {
            let mut status = self.status.lock().await;
            *status = status_response::ProcessStatus::Running(true);
        }

        // Populate stdout/stderr output
        let new_output = Arc::clone(&self.output_signal);
        for rec in rx_output {
            self.output.lock().await.extend(rec);
            new_output.signal();
        }
        // let thread_status = thread
        //     .await
        //     .map_err(|e| RLWServerError(format!("Error joining on processing thread {:?}", e)))??;

        // TODO: FIX!
        let thread_status = thread.join().unwrap().unwrap();

        // Process finished with signal
        if let Some(s) = thread_status.signal() {
            let mut status = self.status.lock().await;
            *status = status_response::ProcessStatus::Signal(s);
            return Ok(());
        }

        // Process finished with exit code
        if let Some(c) = thread_status.code() {
            let mut status = self.status.lock().await;
            *status = status_response::ProcessStatus::ExitCode(c);
            return Ok(());
        }

        Err(RLWServerError(
            "Job processing thread closed before it had finished processing".to_string(),
        ))
    }

    /// Kill the requested process.
    ///
    /// # Arguments
    ///
    /// * `forced` - if true, the signal sent will be SIGKILL, and SIGTERM otherwise.
    pub async fn stop_command(&self, forced: bool) -> Result<(), RLWServerError> {
        let process_status = self.status.lock().await;
        if let ProcessStatus::Running(true) = *process_status {
            // Get the PID of the job to kill
            let pid = self.pid.lock().await.ok_or(RLWServerError(
                "There is no running process associated with this job".to_string(),
            ))?;

            if forced {
                signal::kill(Pid::from_raw(pid as i32), nix::sys::signal::SIGKILL).map_err(
                    |e| RLWServerError(format!("Error sending SIGKILL signal: {:?}", e)),
                )?;
            } else {
                signal::kill(Pid::from_raw(pid as i32), nix::sys::signal::SIGTERM).map_err(
                    |e| RLWServerError(format!("Error sending SIGTERM signal: {:?}", e)),
                )?;
            }
        }
        Ok(())
    }

    /// Stream the history and all upcoming output from `self.start_command()`.
    ///
    /// # Arguments
    /// * `self: Arc<Job>` - Arc of self. Required in order to use self.output in an async tokio thread.
    /// * `tx_stream` -    - Channel sender to send stream results to.
    pub async fn stream_job(
        self: Arc<Job>,
        tx_stream: tokio_mpsc::Sender<Result<StreamResponse, Status>>,
    ) -> Result<(), RLWServerError> {
        // Maintain an index into job.output representing
        // what has been already been streamed.
        let mut read_idx;

        // Send the client the job history
        {
            let output_guard = self.output.lock().await;
            tx_stream
                .send(Ok(StreamResponse {
                    output: output_guard.clone(),
                }))
                .await?;

            read_idx = output_guard.len();
        }

        // While the job is running wait until there is a new output signal.
        // When the signal is received send the new output to the client.
        let new_output_signal = self.output_signal.clone();
        while let status_response::ProcessStatus::Running(true) = *self.status.lock().await {
            new_output_signal.wait();
            // New output has been added. Send this to the client.
            let output = self.output.lock().await;
            if read_idx < output.len() {
                log::error!("sending output");
                tx_stream
                    .send(Ok(StreamResponse {
                        output: output[read_idx..output.len()].to_vec(),
                    }))
                    .await?;
                read_idx = output.len();
            }
            // new_output_signal.reset(); // TODO: HACK FIX THIS
        }

        log::error!("ending");
        // In the event that the process is no longer running but there is still
        // remaining output entries to stream - finish streaming.
        {
            let output = self.output.lock().await;
            if read_idx < output.len() {
                tx_stream
                    .send(Ok(StreamResponse {
                        output: output[read_idx..output.len()].to_vec(),
                    }))
                    .await?;
            }
        }
        Ok(())
    }

    /// Returns the job's status object
    pub async fn status(&self) -> Result<StatusResponse, RLWServerError> {
        let status = self.status.lock().await.clone();
        Ok(StatusResponse {
            process_status: Some(status),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{atomic::AtomicUsize, Arc};
    use tokio::task::JoinHandle;

    const TESTING_SCRIPTS_DIR: &str = "../scripts/";

    /// Tests the starting a new job
    #[tokio::test]
    async fn test_new_job() -> Result<(), RLWServerError> {
        // Setup
        let job = Job::new();
        let command = "echo".to_string();
        let test_string = "hello_test_new_job".to_string();

        // Start
        job.start_command(command, vec![test_string.clone()])
            .await?;

        let output = job.output.lock().await;
        let expected = test_string + "\n";
        assert_eq!(*output, expected.as_bytes());

        Ok(())
    }

    /// Tests starting and stopping a job
    ///
    /// Files used: tests/scripts/stop_job.sh
    ///
    /// The test process is as follows:
    /// 1. Start the first job with the test script and calculate the size of the resulting output.
    /// 2. Run the job again and after 0.3 seconds send a signal to stop the job.
    /// 3. Assert that the output of the second job is smaller than the first.
    /// 4. Assert that the final status of the second job is Signal(15).
    #[tokio::test(flavor = "multi_thread")]
    async fn test_stop() -> Result<(), RLWServerError> {
        // Setup First Job
        let first_job = Job::new();
        let command1 = "/bin/bash".to_string();
        let args1 = vec![TESTING_SCRIPTS_DIR.to_string() + "stop_job.sh"];

        let first_output_len = Arc::new(AtomicUsize::new(0));

        // Setup Second Job
        let second_job = Job::new();
        let job2_arc = Arc::new(second_job);
        let command2 = "/bin/bash".to_string();
        let args2 = vec![TESTING_SCRIPTS_DIR.to_string() + "stop_job.sh"];
        let first_output_len_clone = first_output_len.clone();

        // Start and finish first job
        let first_job_handle: JoinHandle<Result<(), RLWServerError>> = tokio::spawn(async move {
            first_job
                .start_command(command1, args1)
                .await
                .map_err(|_| RLWServerError("Failed to process first job".to_string()))?;
            let len = first_job.output.lock().await.len();
            first_output_len.store(len, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        });

        first_job_handle
            .await
            .map_err(|e| RLWServerError(format!("{:?}", e)))??;

        // Temporarily start second job
        let start_job2_arc = Arc::clone(&job2_arc);
        let start_job2_handle: JoinHandle<Result<(), RLWServerError>> = tokio::spawn(async move {
            start_job2_arc
                .start_command(command2, args2)
                .await
                .map_err(|_| RLWServerError("Failed to process second job".to_string()))?;
            Ok(())
        });

        // Arbitrary time between client starting and stopping job
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        // Stop second job
        let stop_job2_arc = Arc::clone(&job2_arc);
        let stop_job2_handle: JoinHandle<Result<(), RLWServerError>> = tokio::spawn(async move {
            stop_job2_arc.stop_command(false).await?;
            Ok(())
        });

        start_job2_handle.await.map_err(|e| {
            RLWServerError(format!("Error joining the start job 2 thread {:?}", e))
        })??;
        stop_job2_handle.await.map_err(|e| {
            RLWServerError(format!("Error joining the stop job 2 thread {:?}", e))
        })??;

        // Get job 1 and job 2's output vector lengths
        let final_arc = Arc::clone(&job2_arc);
        let len_j1 = first_output_len_clone.load(std::sync::atomic::Ordering::Relaxed);
        let len_j2 = final_arc.output.lock().await.len();

        assert!(len_j2 < len_j1);
        assert_eq!(
            *final_arc.status.lock().await,
            status_response::ProcessStatus::Signal(15)
        );
        Ok(())
    }
    #[tokio::test(flavor = "multi_thread")]
    async fn test_stream() -> Result<(), RLWServerError> {
        // Setup
        let job = Job::new();
        let job_arc = Arc::new(job);

        // Start job
        let start_ptr = Arc::clone(&job_arc);
        let start_handle: JoinHandle<Result<(), RLWServerError>> = tokio::spawn(async move {
            start_ptr
                .start_command(
                    "/bin/bash".to_string(),
                    vec!["../scripts/stream_job.sh".to_string()],
                )
                .await
                .map_err(|_| RLWServerError("Failed to map result to utf8 str".to_string()))?;
            Ok(())
        });

        // Arbitrary time between client starting and stopping job
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        let stream_ptr = Arc::clone(&job_arc);
        let (tx, mut rx) = tokio_mpsc::channel(2024);
        let stream_handle: JoinHandle<Result<(), RLWServerError>> = tokio::spawn(async move {
            stream_ptr.stream_job(tx).await?;
            Ok(())
        });

        while let Some(i) = rx.recv().await {
            println!(
                "i = {:?}",
                std::str::from_utf8(i.expect("bad").output.as_slice())
            );
        }
        let _ = start_handle.await.expect("bad");
        let _ = stream_handle.await.expect("bad");
        Ok(())
    }
}
