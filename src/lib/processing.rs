//! Module for processing client jobs
use crate::errors::RLWServerError;
use std::{
    io::{BufReader, Read},
    process::{ExitStatus, Stdio},
};

use std::process::Command;
use std::sync::mpsc::Sender;
use std::thread;

const COMMAND_DIR: &str = "./tests/test_env";

/// Executes a command using the arguments provided, streaming the output result.
///
/// # Arguments
///
/// * `command`   - Command to execute. Examples: "cargo", "ls", , "/bin/bash"
/// * `args`      - Arguments to accompany the command. Examples: "--version", "-a", "./file.sh"
/// * `tx_pid`    - The channel producer used to send the process PID of the job started.
///                 Is `Some` if the command is `start` and None if `stop`.
///                 once and before any output.
/// * `tx_output` - The channel producer used to stream the command results
pub fn execute_command(
    command: String,
    args: Vec<String>,
    tx_pid: Option<&Sender<u32>>,
    tx_output: &Sender<u8>,
) -> Result<ExitStatus, RLWServerError> {
    // Start process
    let mut output = Command::new(command)
        .args(args)
        .current_dir(COMMAND_DIR)
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    // Send PID
    if let Some(t) = tx_pid {
        t.send(output.id())?;
    }

    // Setup stream readers
    let stdout_reader = BufReader::new(output.stdout.take().ok_or(RLWServerError(
        "Unable to read from stdout stream".to_string(),
    ))?);

    let stderr_reader = BufReader::new(output.stderr.take().ok_or(RLWServerError(
        "Unable to read from stderr stream".to_string(),
    ))?);

    // Concurrently read from stderr and send output down channel
    let tx_output_err = tx_output.clone();
    let thread = thread::spawn(move || -> Result<(), RLWServerError> {
        for byte in stderr_reader.bytes() {
            tx_output_err.send(byte?)?;
        }
        Ok(())
    });

    // Read from stdout and send output down channel
    for byte in stdout_reader.bytes() {
        tx_output.send(byte?)?;
    }

    if let Err(e) = thread.join() {
        return Err(RLWServerError(format!("{:?}", e)));
    }

    // Return exit code or terminating signal
    let status = output.wait()?;
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_script() {}
}
