//! Error handling wrappers

use crate::utils::job_processor_api::StreamResponse;
use core::fmt;
use std::io;
use std::sync::mpsc::{self, RecvError, SendError};

/*
TODO in the future:
1: Create  proper error handling struct with an enum of different error types.
   Will be able to replace the const &str below with the fmt::Display trait.

2: Implement proper From() functions for the error conversions below.

*/

// General server error to send to client
pub const GENERAL_SERVER_ERR: &str = "Server Error";

// No associated job
pub const NO_UUID_JOB: &str = "No job found with the provided uuid";

/// A general purpose error wrapper for the rlw server
#[derive(Debug)]
pub struct RLWServerError(pub String);

// General error implementation
impl std::error::Error for RLWServerError {}

// New error from a string
impl fmt::Display for RLWServerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

// Process output channel error
impl From<mpsc::SendError<Vec<u8>>> for RLWServerError {
    fn from(err: SendError<Vec<u8>>) -> RLWServerError {
        RLWServerError(err.to_string())
    }
}

// Process output channel error
impl From<RecvError> for RLWServerError {
    fn from(err: RecvError) -> RLWServerError {
        RLWServerError(err.to_string())
    }
}

// PID channel errors
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
impl<T> From<tokio::sync::mpsc::error::SendError<Result<StreamResponse, T>>> for RLWServerError {
    fn from(err: tokio::sync::mpsc::error::SendError<Result<StreamResponse, T>>) -> RLWServerError {
        RLWServerError(err.to_string())
    }
}

// General tonic error
impl From<tonic::transport::Error> for RLWServerError {
    fn from(err: tonic::transport::Error) -> RLWServerError {
        RLWServerError(err.to_string())
    }
}
