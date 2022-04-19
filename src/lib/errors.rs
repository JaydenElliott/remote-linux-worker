use core::fmt;
use std::io;
use std::sync::mpsc::{self, RecvError, SendError};

use crate::job_processor;

/// A general purpose error wrapper for the rlw server. This was to avoid using
/// Box<dyn std::error::Error>> or converting all the errors within the codebase.
///
/// TODO: In the future proper `From` implementations will need to be written.
#[derive(Debug)]
pub struct RLWServerError(pub String);

// General error implementation
impl std::error::Error for RLWServerError {}

// Create new error from a string
impl fmt::Display for RLWServerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

// Process output channel error
impl From<mpsc::SendError<u8>> for RLWServerError {
    fn from(err: SendError<u8>) -> RLWServerError {
        RLWServerError(err.to_string())
    }
}

// Process output channel error
impl From<RecvError> for RLWServerError {
    fn from(err: RecvError) -> RLWServerError {
        RLWServerError(err.to_string())
    }
}

// PID channel sending errors
impl From<mpsc::SendError<u32>> for RLWServerError {
    fn from(err: SendError<u32>) -> RLWServerError {
        RLWServerError(err.to_string())
    }
}

// Command processing IO errors
impl From<io::Error> for RLWServerError {
    fn from(err: io::Error) -> RLWServerError {
        RLWServerError(err.to_string())
    }
}

// Streaming tokio channel error
impl<T> From<tokio::sync::mpsc::error::SendError<Result<job_processor::StreamResponse, T>>>
    for RLWServerError
{
    fn from(
        err: tokio::sync::mpsc::error::SendError<Result<job_processor::StreamResponse, T>>,
    ) -> RLWServerError {
        RLWServerError(err.to_string())
    }
}
