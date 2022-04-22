mod job_processor {
    tonic::include_proto!("job_processor");
}
mod errors;
pub mod jobs;
mod processing;
pub mod server;
mod server_types;
mod utils;
