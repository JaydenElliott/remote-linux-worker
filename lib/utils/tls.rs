//! Utilities to setup mTLS

use crate::utils::errors::RLWServerError;
use rustls::{
    AllowAnyAuthenticatedClient, Certificate, PrivateKey, ProtocolVersion, RootCertStore,
    ServerConfig,
};
use rustls_pemfile::{certs, Item};
use std::{
    fs, io,
    net::{SocketAddr, SocketAddrV6},
    str::FromStr,
    sync::Arc,
};
use tonic::transport::ServerTlsConfig;
use x509_parser::{extensions::ParsedExtension, prelude::X509Certificate, traits::FromDer};

// To avoid privileged ports
const PORT_MIN: u16 = 1024;

/// Configure the custom mTLS settings for the gRPC server
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
    config.alpn_protocols = vec![b"h2".to_vec()];

    let tls_config = ServerTlsConfig::new()
        .rustls_server_config(config)
        .to_owned();

    Ok(tls_config)
}

/// Loads x509 certificates from file
fn load_certs(path: &str) -> Result<Vec<Certificate>, io::Error> {
    let f = fs::File::open(path)?;
    let mut reader = io::BufReader::new(f);
    let certs: Vec<Certificate> = certs(&mut reader)?.into_iter().map(Certificate).collect();
    Ok(certs)
}

/// Loads a private key from file
fn load_private_key(path: &str) -> Result<PrivateKey, io::Error> {
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

/// Parses an x509 certificate, returning any RFC822 name found in the SAN extension.
///
/// Will only search the last certificate in the chain for user identification.
/// # Arguments
/// - `certificates` - client certificate chain obtained from tonic::request::peer_certs().
///             
pub fn authorize_user(
    certificates: Option<Arc<Vec<tonic::transport::Certificate>>>,
) -> Result<Option<String>, String> {
    // No certificates are used in unit tests
    if cfg!(test) {
        return Ok(Some("testuser@foo.com".to_string()));
    }

    // Extract final certificate in chain
    let certificate = certificates
        .ok_or_else(|| "No certificates found".to_string())?
        .last()
        .ok_or_else(|| "No certificates found".to_string())?
        .clone()
        .into_inner();

    // Parse certificate
    let (_, cert) = X509Certificate::from_der(&certificate)
        .map_err(|e| format!("Invalid certificate: {:?}", e))?;

    // Get RFC822 name from SAN extension
    for ext in cert.extensions() {
        let parsed_ext = ext.parsed_extension();
        if let ParsedExtension::SubjectAlternativeName(san) = parsed_ext {
            for name in &san.general_names {
                if let x509_parser::extensions::GeneralName::RFC822Name(email) = name {
                    return Ok(Some(email.to_string()));
                }
            }
        }
    }
    // No RFC822 name found in cert
    Ok(None)
}

/// Ipv6 address parser and validator
///
/// Returns a generic address to integrate with the tonic server
pub fn ipv6_address_validator(address: &str) -> Result<SocketAddr, RLWServerError> {
    let addr = SocketAddrV6::from_str(address)
        .map_err(|e| RLWServerError(format!("Invalid Ipv6 address {:?}", e)))?;

    if addr.port() < PORT_MIN {
        return Err(RLWServerError(format!(
            "Invalid port number. Must be greater than {}",
            PORT_MIN
        )));
    }
    Ok(SocketAddr::from(addr))
}
