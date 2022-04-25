//! # Remote Linux Worker Processor
//! Provides a job object that exposes functions to Start, Stop, Stream, and query the Status of a linux process.

mod job;
mod processing;
pub use self::job::Job;
