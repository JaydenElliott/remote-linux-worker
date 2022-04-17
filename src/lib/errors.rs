use core::fmt;
use std::io;
use std::sync::mpsc::{self, RecvError, SendError};

/// TODO: write up for this error
/// This is general purpose
/// in production would have different errors for different types etc..
#[derive(Debug)]
pub struct RLWServerError(pub String);

impl std::error::Error for RLWServerError {}

impl fmt::Display for RLWServerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<mpsc::SendError<u8>> for RLWServerError {
    fn from(err: SendError<u8>) -> RLWServerError {
        RLWServerError(err.to_string())
    }
}

impl From<mpsc::SendError<u32>> for RLWServerError {
    fn from(err: SendError<u32>) -> RLWServerError {
        RLWServerError(err.to_string())
    }
}

impl From<io::Error> for RLWServerError {
    fn from(err: io::Error) -> RLWServerError {
        RLWServerError(err.to_string())
    }
}

impl From<RecvError> for RLWServerError {
    fn from(err: RecvError) -> RLWServerError {
        RLWServerError(err.to_string())
    }
}
