//! Primary module for running the server and exposing the gRPC interface

use crate::server::api::JobProcessor;
use crate::utils::{
    errors::RLWServerError,
    job_processor_api::job_processor_service_server::JobProcessorServiceServer,
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
/// ```rust, ignore
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///  let settings = ServerSettings::new(ADDR, KEY, CERT, CLIENT_CERT);
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
    pub async fn run(&self) -> Result<(), RLWServerError> {
        // Validate and parse the IPv6 address
        let addr = ipv6_address_validator(&self.config.address)?;

        // // Configure and initialize the server
        let processor = JobProcessor::new();
        let svc = JobProcessorServiceServer::new(processor);
        let tls_config =
            configure_server_tls(&self.config.key, &self.config.cert, &self.config.client_ca)?;

        log::info!(
            "Linux worker gRPC server listening on: {}",
            self.config.address
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
    /// IPv6 address + port the user wishes to run the server on
    pub address: String,

    // Server public key
    pub key: String,

    // Server certificate
    pub cert: String,

    // Client ca certificate to verify the client against
    pub client_ca: String,
    /*
    TODO: Add extra configuration options:
    - Rate limits on requests, for DDOS protection
    - Option to pipe logs to file (https://docs.rs/log4rs/1.0.0/log4rs/)
    - Option to manually configure TLS:
       - to use TLS v1.2 for an old client implementation
       - to allow different private key encryption formats
    */
}

impl ServerSettings {
    pub fn new(address: String, key: String, cert: String, client_ca: String) -> Self {
        Self {
            address,
            key,
            cert,
            client_ca,
        }
    }
}
