//! Exposes all the required types and type impls for the
//! rlw server to run.

use crate::server::api::JobProcessor;
use crate::utils::errors::RLWServerError;
use crate::utils::{
    job_processor_api::{job_processor_service_server::JobProcessorServiceServer, *},
    tls::{configure_server_tls, ipv6_address_validator},
};

use tonic::transport;

/// # RLW Server
/// A Remote Linux Worker server will listen for
/// requests to `start`, `stop`, `status` and `stream` process jobs.
///
/// This server is a wrapper around the `tonic::transport::Server`.
///
/// # Example Usage
///
/// ```no_run
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
    pub async fn run(&self) -> Result<(), RLWServerError> {
        // Validate and parse the IPv6 address
        let addr = ipv6_address_validator(&self.config.socket_address)?;

        // // Configure and initialize the server
        let processor = JobProcessor::new();
        let svc = JobProcessorServiceServer::new(processor);
        let tls_config = configure_server_tls()?;

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

// #[cfg(test)]
// mod tests {

//     use rustls::ServerConfig;

//     use crate::job_processor::status_response::ProcessStatus;

//     use super::*;
//     const TESTING_SCRIPTS_DIR: &str = "../scripts/";

//     /// TODO: Wont stop
//     #[tokio::test(flavor = "multi_thread")]
//     async fn test_run_server() -> Result<(), RLWServerError> {
//         let settings = ServerSettings::new(String::from("[::1]:50051"));
//         let server = Server::new(settings);
//         server.run().await.unwrap();
//         Ok(())
//     }

//     /// Tests the starting a new job
//     #[tokio::test(flavor = "multi_thread")]
//     async fn test_start_request() -> Result<(), RLWServerError> {
//         // Setup
//         let job_processor = JobProcessor::new();
//         let command = "/bin/bash".to_string();
//         let arguments = vec![TESTING_SCRIPTS_DIR.to_string() + "start_request.sh"];
//         let mock_request = Request::new(StartRequest { command, arguments });

//         job_processor.start(mock_request).await.unwrap();

//         Ok(())
//     }

//     /// Tests the starting and stopping of a single job
//     #[tokio::test(flavor = "multi_thread")]
//     async fn test_stop_request() -> Result<(), RLWServerError> {
//         // Setup
//         let job_processor = Arc::new(JobProcessor::new());

//         let command = "/bin/bash".to_string();
//         let arguments = vec![TESTING_SCRIPTS_DIR.to_string() + "start_request.sh"];
//         let mock_start_request = Request::new(StartRequest { command, arguments });
//         let uuid = job_processor
//             .start(mock_start_request)
//             .await
//             .unwrap()
//             .into_inner()
//             .uuid;

//         tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

//         let mock_stop_request = Request::new(StopRequest {
//             uuid,
//             forced: false,
//         });

//         job_processor.stop(mock_stop_request).await.unwrap();

//         // DO A TEST SIMILAR TO BEFORE
//         Ok(())
//     }

//     /// Tests the status request of a new job
//     #[tokio::test(flavor = "multi_thread")]
//     async fn test_status_request() -> Result<(), RLWServerError> {
//         // Setup
//         let job_processor = Arc::new(JobProcessor::new());

//         let command = "/bin/bash".to_string();
//         let arguments = vec![TESTING_SCRIPTS_DIR.to_string() + "start_request.sh"];
//         let mock_start_request = Request::new(StartRequest { command, arguments });
//         let uuid = job_processor
//             .start(mock_start_request)
//             .await
//             .unwrap()
//             .into_inner()
//             .uuid;

//         tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

//         let mock_status_request = Request::new(StatusRequest { uuid: uuid.clone() });
//         let status = job_processor
//             .status(mock_status_request)
//             .await
//             .unwrap()
//             .into_inner();

//         assert_eq!(status.process_status, Some(ProcessStatus::Running(true)));

//         let mock_stop_request = Request::new(StopRequest {
//             uuid: uuid.clone(),
//             forced: false,
//         });
//         job_processor.stop(mock_stop_request).await.unwrap();

//         tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
//         let mock_status_request2 = Request::new(StatusRequest { uuid });
//         let status = job_processor
//             .status(mock_status_request2)
//             .await
//             .unwrap()
//             .into_inner();

//         assert_eq!(status.process_status, Some(ProcessStatus::Signal(15)));
//         Ok(())
//     }

//     /// Tests the status request of a new job
//     #[tokio::test(flavor = "multi_thread")]
//     async fn test_stream_request() -> Result<(), RLWServerError> {
//         // Will need to test this on the client end
//         // Setup
//         let job_processor = Arc::new(JobProcessor::new());

//         let command = "/bin/bash".to_string();
//         let arguments = vec![TESTING_SCRIPTS_DIR.to_string() + "stream_job.sh"];
//         let mock_start_request = Request::new(StartRequest { command, arguments });
//         let uuid = job_processor
//             .start(mock_start_request)
//             .await
//             .unwrap()
//             .into_inner()
//             .uuid;

//         tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

//         let mock_stream_request = Request::new(StreamRequest { uuid });
//         let mut stream = job_processor
//             .stream(mock_stream_request)
//             .await
//             .unwrap()
//             .into_inner()
//             .into_inner();
//         while let Some(msg) = stream.recv().await {
//             println!("Stream = {:?}", msg);
//         }
//         Ok(())
//     }
// }
