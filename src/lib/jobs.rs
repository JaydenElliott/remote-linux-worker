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

// TODO: update this
const STREAM_BUFFER_SIZE: usize = 4096;

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
    use super::*;
    use std::sync::{atomic::AtomicUsize, Arc};

    /// Tests the creation of a new job
    #[tokio::test(flavor = "multi_thread")]
    async fn test_create() -> Result<(), RLWServerError> {
        // Setup
        let job = Job::new();
        let command = "echo".to_string();
        let test_string = "hello_test_create".to_string();

        // Start
        job.start_command(command, vec![test_string.clone()])
            .await?;

        let output = job.output.lock().await;
        let expected = test_string + "\n";

        assert_eq!(*output, expected.as_bytes());
        Ok(())
    }

    /// Tests if the stop_command() function works
    ///
    /// Files used: tests/scripts/stop_job.sh
    ///
    /// The test process is the following:
    /// 1. Start a job with the test script.
    /// 2. Stop the job before it has time to finish.
    /// 3. Check if the resulting output length is smaller than the expected
    ///    output length had the job not failed
    ///
    /// TODO: With more time a better test simulation would be:
    /// 1. Start a job with the test script and calculate the size of the output.
    /// 2. Run the job again, but after 1/2 a second, stop the job.
    /// 3. Assert that the output of the second job is smaller than the first.
    /// This would remove the need to find the output of the first job to hard
    /// code the length, like it was done below.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_stop() -> Result<(), RLWServerError> {
        // Setup
        let job = Job::new();
        let job_arc = Arc::new(job);

        let command = "/bin/bash".to_string();
        let args = vec!["../scripts/stop_job.sh".to_string()];

        // To test the stopped job against
        let output_len = 18;

        let start_job = Arc::clone(&job_arc);
        let start_handle: JoinHandle<Result<(), RLWServerError>> = tokio::spawn(async move {
            start_job
                .start_command(command, args)
                .await
                .map_err(|_| RLWServerError("Failed to process command".to_string()))?;
            Ok(())
        });

        // Arbitrary time between client starting and stopping job
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        // Stop job
        let stop_job = Arc::clone(&job_arc);
        let stop_handle: JoinHandle<Result<(), RLWServerError>> = tokio::spawn(async move {
            stop_job.stop_command(false).await?;
            assert!(stop_job.output.lock().await.len() < output_len);
            Ok(())
        });

        start_handle
            .await
            .map_err(|e| RLWServerError(format!("{:?}", e)))??;
        stop_handle
            .await
            .map_err(|e| RLWServerError(format!("{:?}", e)))??;

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_stop2() -> Result<(), RLWServerError> {
        // Setup First Job
        let j1 = Job::new();
        let command_j1 = "/bin/bash".to_string();
        let args_j1 = vec!["../scripts/stop_job.sh".to_string()];
        let output_len_j1 = Arc::new(AtomicUsize::new(0));

        // Setup Second Job
        let j2 = Job::new();
        let j2_ptr = Arc::new(j2);
        let command_j2 = "/bin/bash".to_string();
        let args_j2 = vec!["../scripts/stop_job.sh".to_string()];
        let output_len_j1_clone = output_len_j1.clone();

        // Start and finish first job
        let start_j1_handle: JoinHandle<Result<(), RLWServerError>> = tokio::spawn(async move {
            j1.start_command(command_j1, args_j1)
                .await
                .map_err(|_| RLWServerError("Failed to process command".to_string()))?;
            let len = j1.output.lock().await.len();
            output_len_j1.store(len, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        });

        start_j1_handle
            .await
            .map_err(|e| RLWServerError(format!("{:?}", e)))??;

        // Temporarily start second job
        let start_j2 = Arc::clone(&j2_ptr);
        let start_j2_handle: JoinHandle<Result<(), RLWServerError>> = tokio::spawn(async move {
            start_j2
                .start_command(command_j2, args_j2)
                .await
                .map_err(|_| RLWServerError("Failed to process command".to_string()))?;
            Ok(())
        });

        // Arbitrary time between client starting and stopping job
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        // Stop second job
        let stop_j2 = Arc::clone(&j2_ptr);
        let stop_handle: JoinHandle<Result<(), RLWServerError>> = tokio::spawn(async move {
            stop_j2.stop_command(false).await?;

            // Get the length of the first job's output vector
            let len_j1 = output_len_j1_clone.load(std::sync::atomic::Ordering::Relaxed);

            // Get the length of the second job's output vector
            let len_j2 = stop_j2.output.lock().await.len();

            assert!(len_j2 < len_j1);
            Ok(())
        });

        start_j2_handle
            .await
            .map_err(|e| RLWServerError(format!("{:?}", e)))??;
        stop_handle
            .await
            .map_err(|e| RLWServerError(format!("{:?}", e)))??;

        Ok(())
    }

    // async fn test_stop3() -> Result<(), RLWServerError> {
    //     // Setup
    //     let j1 = Job::new();
    //     let job_arc = Arc::new(job);

    //     let command_j1 = "/bin/bash".to_string();
    //     let args_j1 = vec!["../scripts/stop_job.sh".to_string()];
    //     let output_len_j1 = Arc::new(AtomicUsize::new(0));
    //     let output_len_j1_clone = output_len_j1.clone();

    //     let command_j2 = "/bin/bash".to_string();
    //     let args_j2 = vec!["../scripts/stop_job.sh".to_string()];

    //     let start_j1 = Arc::clone(&job_arc);
    //     let start_j1_handle: JoinHandle<Result<(), RLWServerError>> = tokio::spawn(async move {
    //         start_j1
    //             .start_command(command_j1, args_j1)
    //             .await
    //             .map_err(|_| RLWServerError("Failed to process command".to_string()))?;
    //         let len = start_j1.output.lock().await.len();
    //         output_len_j1.store(len, std::sync::atomic::Ordering::Relaxed);
    //         Ok(())
    //     });

    //     start_j1_handle
    //         .await
    //         .map_err(|e| RLWServerError(format!("{:?}", e)))??;

    //     let start_j2 = Arc::clone(&job_arc);
    //     let start_j2_handle: JoinHandle<Result<(), RLWServerError>> = tokio::spawn(async move {
    //         start_j2
    //             .start_command(command_j2, args_j2)
    //             .await
    //             .map_err(|_| RLWServerError("Failed to process command".to_string()))?;
    //         Ok(())
    //     });

    //     // Arbitrary time between client starting and stopping job
    //     tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

    //     // Stop job
    //     let stop_job = Arc::clone(&job_arc);
    //     let stop_handle: JoinHandle<Result<(), RLWServerError>> = tokio::spawn(async move {
    //         stop_job.stop_command(false).await?;

    //         let len_j1 = output_len_j1_clone.load(std::sync::atomic::Ordering::Relaxed);
    //         let len_j2 = stop_job.output.lock().await.len();

    //         println!("j1 {} and j2 = {}", len_j1, len_j2);
    //         Ok(())
    //     });

    //     start_j2_handle
    //         .await
    //         .map_err(|e| RLWServerError(format!("{:?}", e)))??;
    //     stop_handle
    //         .await
    //         .map_err(|e| RLWServerError(format!("{:?}", e)))??;

    //     Ok(())
    // }
}
