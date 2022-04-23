use std::{
    net::{SocketAddr, SocketAddrV6},
    str::FromStr,
};

use rustls::{
    AllowAnyAuthenticatedClient, Certificate, PrivateKey, ProtocolVersion, RootCertStore,
    ServerConfig,
};
use rustls_pemfile::{certs, Item};
use std::{fs, io};
use tonic::transport::ServerTlsConfig;

use crate::utils::errors::RLWServerError;

// TODO: make these configurable
// Certificates and keys

// To avoid privileged ports
const PORT_MIN: u16 = 1024;
const PORT_MAX: u16 = 65535;

/// Configures the custom TLS settings for the gRPC server
pub fn configure_server_tls(
    server_key: &str,
    server_cert: &str,
    client_cert: &str,
) -> Result<ServerTlsConfig, RLWServerError> {
    let suites: Vec<&'static rustls::SupportedCipherSuite> = vec![
        &rustls::ciphersuite::TLS13_AES_256_GCM_SHA384,
        &rustls::ciphersuite::TLS13_AES_128_GCM_SHA256,
        &rustls::ciphersuite::TLS13_CHACHA20_POLY1305_SHA256,
    ];
    let protocol_version = vec![ProtocolVersion::TLSv1_3];

    // Setup client authentication
    let mut client_auth_roots = RootCertStore::empty();
    let root_cert = load_certs(client_cert)?;
    for cert in root_cert {
        client_auth_roots
            .add(&cert)
            .map_err(|e| RLWServerError(format!("Certificate Error: {:?}", e)))?;
    }

    // Load certificates and keys
    let cert_chain = load_certs(server_cert)?;
    let priv_key = load_private_key(server_key)?;

    // Configure TLS
    let mut config = ServerConfig::new(AllowAnyAuthenticatedClient::new(client_auth_roots));
    config
        .set_single_cert(cert_chain, priv_key)
        .map_err(|e| RLWServerError(format!("Invalid certificate error: {:?}", e)))?;
    config.ciphersuites = suites;
    config.versions = protocol_version;

    // TODO: remove if not required
    config.alpn_protocols = vec![b"h2".to_vec()];

    let tls_config = ServerTlsConfig::new()
        .rustls_server_config(config)
        .to_owned();

    Ok(tls_config)
}

/// Loads x509 certificates from file
fn load_certs(path: &str) -> Result<Vec<Certificate>, io::Error> {
    log::info!("Parsing certificates at path: {}", path);
    let f = fs::File::open(path)?;
    let mut reader = io::BufReader::new(f);
    let certs: Vec<Certificate> = certs(&mut reader)?.into_iter().map(Certificate).collect();
    Ok(certs)
}

/// Loads a private key from file
fn load_private_key(path: &str) -> Result<PrivateKey, io::Error> {
    log::info!("Reading private key at path: {}", path);

    let keyfile = fs::File::open(path)?;
    let mut reader = io::BufReader::new(keyfile);
    let key_string = rustls_pemfile::read_one(&mut reader)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("No PEM found in file {}", path),
        )
    })?;

    // Verify the key type is supported by rustls
    match key_string {
        Item::RSAKey(k) => Ok(PrivateKey(k)),
        Item::PKCS8Key(k) => Ok(PrivateKey(k)),
        Item::ECKey(k) => Ok(PrivateKey(k)),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Key type not supported",
        )),
    }
}

/// Ipv6 address parser and validator
///
/// Validates if the address is valid IPv6 and the port
/// is within range.
///
/// Returns a generic address in order to integrate with the tonic server
pub fn ipv6_address_validator(address: &str) -> Result<SocketAddr, RLWServerError> {
    let addr = SocketAddrV6::from_str(address)
        .map_err(|e| RLWServerError(format!("Invalid Ipv6 address {:?}", e)))?;

    if addr.port() < PORT_MIN || addr.port() > PORT_MAX {
        return Err(RLWServerError(format!(
            "Invalid port number. Must be greater than {} and less than {}",
            PORT_MIN, PORT_MAX
        )));
    }
    Ok(SocketAddr::from(addr))
}
