//! Utility functions for the client

use tonic::transport::{Certificate, ClientTlsConfig, Identity};

// TODO: Make these configurable through a config file
// or environment variables
const CLIENT_KEY: &str = "temp/tls/client1.key";
const CLIENT_CERT: &str = "temp/tls/client1.pem";
const SERVER_ROOT_CERT: &str = "temp/tls/ca.pem";

pub async fn configure_server_tls() -> Result<ClientTlsConfig, Box<dyn std::error::Error>> {
    let server_ca_pem = tokio::fs::read(SERVER_ROOT_CERT).await?;
    let server_root_ca_cert = Certificate::from_pem(server_ca_pem);
    let client_cert = tokio::fs::read(CLIENT_CERT).await?;
    let client_key = tokio::fs::read(CLIENT_KEY).await?;
    let client_identity = Identity::from_pem(client_cert, client_key);

    let tls_config = ClientTlsConfig::new()
        .domain_name("localhost")
        .ca_certificate(server_root_ca_cert)
        .identity(client_identity);

    Ok(tls_config)
}
