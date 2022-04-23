//! # Remote Linux Worker Processor
//! A library providing a job object which exposes functions to Start, Stop, Stream and query the Status of a linux process.

mod job;
mod processing;
pub use self::job::Job;
