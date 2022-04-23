//! Common error handling, utilities and TLS configuration
pub mod errors;
pub mod tls;
pub mod job_processor_api {
    tonic::include_proto!("job_processor_api");
}
