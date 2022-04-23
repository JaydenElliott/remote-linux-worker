//! # gRPC Server
//! A secure gRPC server exposing an API to Start, Stop, Stream and query the Status of linux process jobs.
//! The server utilizes a strong set of TLS authentication and authorization is managed through certificate usernames.
mod api;
pub mod server;
mod user;
