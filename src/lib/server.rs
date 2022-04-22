//! Exposes all the required types and type impls for the
//! rlw server to run.

use crate::errors::RLWServerError;
use crate::job_processor::{
    job_processor_service_server::{JobProcessorService, JobProcessorServiceServer},
    *,
};
use crate::server_types::User;
use crate::utils;

use std::error::Error;
use std::sync::Mutex;
use std::{collections::HashMap, sync::Arc};
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport;
use tonic::{Request, Response, Status};

/// # RLW Server
/// A Remote Linux Worker server will listen for
/// requests to `start`, `stop`, `status` and `stream` process jobs.
///
/// This server is a wrapper around the `tonic::transport::Server`.
///
/// # Example Usage
///
/// ```no-run
/// use rlw::types::{Server, ServerSettings};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///  let settings = ServerSettings::new("[::1]:50051");
///  let server = Server::new(settings);
///  server.run().await?;
///  Ok(())
/// }
/// ```
pub struct Server {
    config: ServerSettings,
}

impl Server {
    // Create a new server instance
    pub fn new(config: ServerSettings) -> Self {
        Self { config }
    }

    /// Start the gRPC server using the configuration provided in `new(config)`
    /// and handle all incoming requests.
    ///
    /// TODO: If I get the time, update the Box<> to be a custom
    ///       server error type that converts all the other errors it it's type
    pub async fn run(&self) -> Result<(), Box<dyn Error>> {
        // Validate and parse the IPv6 address
        let addr = utils::ipv6_address_validator(&self.config.socket_address)?;

        // // Configure and initialize the server
        let processor = JobProcessor::new();
        let svc = JobProcessorServiceServer::new(processor);
        let tls_config = utils::configure_server_tls()?;

        log::info!(
            "Linux worker gRPC server listening on: {}",
            self.config.socket_address
        );
        transport::Server::builder()
            .tls_config(tls_config)?
            .add_service(svc)
            .serve(addr)
            .await?;

        Ok(())
    }
}

/// Server Configuration
pub struct ServerSettings {
    // TODO: Add extra configuration options:
    // - Rate limits on requests, for DDOS protection
    // - Option to pipe logs to file (https://docs.rs/log4rs/1.0.0/log4rs/)
    // - Option to manually configure TLS:
    //    - to use TLS v1.2 for an old client implementation
    //    - to allow different private key encryption formats
    // - Option to set the host and user CA
    /// A String containing the IPv6 address + port the user wishes to run the server on
    pub socket_address: String,
}

impl ServerSettings {
    pub fn new(socket_address: String) -> Self {
        Self { socket_address }
    }
}

/// Handles the incoming gRPC job process requests and
/// stores user data related to these requests
pub struct JobProcessor {
    // Maps usernames to users structs.
    // Stores all users and their jobs.
    user_table: Arc<Mutex<HashMap<String, Arc<User>>>>,
}

impl JobProcessor {
    /// Creates a new job processor. This should only be done once
    /// per server instance.
    pub fn new() -> Self {
        Self {
            user_table: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get a mutable reference to the user's User struct.
    /// If no user exists with the specified username, a new
    /// user will be created.
    pub fn get_user(&self, username: &str) -> Arc<User> {
        let table_arc = Arc::clone(&self.user_table);
        let mut table = table_arc.lock().unwrap();
        if !table.contains_key(username) {
            table.insert(String::from(username), Arc::new(User::new()));
        }
        Arc::clone(table.get(username).unwrap())
    }
}

#[tonic::async_trait]
/// gRPC Service Implementation
impl JobProcessorService for JobProcessor {
    /// Start a new process job
    async fn start(
        &self,
        request: Request<StartRequest>,
    ) -> Result<Response<StartResponse>, Status> {
        log::info!("Start Request");
        let req = request.into_inner();

        // Get user
        let username = "john"; // temp
        let user = self.get_user(username);
        let uuid = user.start_new_job(req.command, req.arguments);
        Ok(Response::new(StartResponse { uuid }))
    }

    /// Stop a running process job
    async fn stop(&self, request: Request<StopRequest>) -> Result<Response<()>, Status> {
        // Get user
        let username = "john"; // temp
        let user = self.get_user(username);
        let req = request.into_inner();
        let job = user.get_job(&req.uuid);
        job.stop_command(req.forced).await.unwrap();
        Ok(Response::new(()))
    }

    type StreamStream = ReceiverStream<Result<StreamResponse, Status>>;
    /// Stream all previous and upcoming stderr and stdout data for a specified job
    async fn stream(
        &self,
        request: tonic::Request<StreamRequest>,
    ) -> Result<tonic::Response<Self::StreamStream>, tonic::Status> {
        todo!();
    }
    /// Return the status of a previous process job
    async fn status(
        &self,
        request: Request<StatusRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const TESTING_SCRIPTS_DIR: &str = "../scripts/";

    /// Tests the starting a new job
    #[tokio::test(flavor = "multi_thread")]
    async fn test_start_request() -> Result<(), RLWServerError> {
        // Setup
        let job_processor = JobProcessor::new();
        let command = "/bin/bash".to_string();
        let arguments = vec![TESTING_SCRIPTS_DIR.to_string() + "start_request.sh"];
        let mock_request = Request::new(StartRequest { command, arguments });

        let response = job_processor.start(mock_request).await.unwrap();

        Ok(())
    }

    /// Tests the starting a new job
    #[tokio::test(flavor = "multi_thread")]
    async fn test_stop_request() -> Result<(), RLWServerError> {
        // Setup
        let job_processor = Arc::new(JobProcessor::new());

        let command = "/bin/bash".to_string();
        let arguments = vec![TESTING_SCRIPTS_DIR.to_string() + "start_request.sh"];
        let mock_start_request = Request::new(StartRequest { command, arguments });
        let j1 = Arc::clone(&job_processor);
        let uuid = j1
            .start(mock_start_request)
            .await
            .unwrap()
            .into_inner()
            .uuid;

        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        let j2 = Arc::clone(&job_processor);
        let mock_stop_request = Request::new(StopRequest {
            uuid,
            forced: false,
        });

        j2.stop(mock_stop_request).await.unwrap();
        Ok(())
    }
}
