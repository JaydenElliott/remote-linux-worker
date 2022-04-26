//! Utility functions for the client

use tonic::transport::{Certificate, ClientTlsConfig, Identity};

// TODO: Make these configurable (file, cmd line or env variable)
const CLIENT_KEY: &str = "examples/tls/client.key";
const CLIENT_CERT: &str = "examples/tls/client.pem";
const SERVER_ROOT_CERT: &str = "examples/tls/rootCA.pem";

// Domain to verify the server's TLS cert against
const DOMAIN: &str = "localhost";

/// Generates the client TLS configuration object.
pub async fn configure_mtls() -> Result<ClientTlsConfig, Box<dyn std::error::Error>> {
    let server_ca_pem = tokio::fs::read(SERVER_ROOT_CERT).await?;
    let server_root_ca_cert = Certificate::from_pem(server_ca_pem);
    let client_cert = tokio::fs::read(CLIENT_CERT).await?;
    let client_key = tokio::fs::read(CLIENT_KEY).await?;
    let client_identity = Identity::from_pem(client_cert, client_key);

    let tls_config = ClientTlsConfig::new()
        .domain_name(DOMAIN)
        .ca_certificate(server_root_ca_cert)
        .identity(client_identity);

    Ok(tls_config)
}
