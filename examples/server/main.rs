use rlw::server::{Server, ServerSettings};

const KEY: &str = "tls/server.key";
const CERT: &str = "tls/server.pem";
const CLIENT_CERT: &str = "tls/client_ca.pem";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = ServerSettings::new(
        "[::1]:50051".to_string(),
        KEY.to_string(),
        CERT.to_string(),
        CLIENT_CERT.to_string(),
    );
    let server = Server::new(settings);
    server.run().await?;
    Ok(())
}
