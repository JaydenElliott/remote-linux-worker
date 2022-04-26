//! This library exposes two modules - a linux job processor and a gRPC server to process linux jobs
//!
//! # Linux Job Processor
//! Provides a job object that exposes functions to Start, Stop, Stream and query the Status of a linux process.
//!
//! # gRPC Server
//! A secure gRPC server exposing an API to Start, Stop, Stream and query the Status of linux process jobs.
//! The server utilizes a strong set of TLS authentication and authorization is managed through certificate usernames.

pub mod rlwp;
pub mod server;
mod utils;
