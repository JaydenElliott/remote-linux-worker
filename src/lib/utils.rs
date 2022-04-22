use std::{
    net::{SocketAddr, SocketAddrV6},
    str::FromStr,
};

use rustls::{
    AllowAnyAuthenticatedClient, Certificate, PrivateKey, ProtocolVersion, RootCertStore,
    ServerConfig, TLSError,
};
use rustls_pemfile::{certs, Item};
use std::{fs, io};
use tonic::transport::ServerTlsConfig;

// Certificates and keys
const KEY: &str = "tls/server.key";
const CERT: &str = "tls/server.pem";
const CLIENT_CERT: &str = "tls/client_ca.pem";

// To avoid privileged ports
const PORT_MIN: u16 = 1024;
const PORT_MAX: u16 = 65535;

/// Ipv6 address parser and validator
///
/// Validates if the address is valid IPv6 and the port
/// is within range.
///
/// Returns a generic socket address in order to integrate with the tonic server
pub fn ipv6_address_validator(address: &str) -> Result<SocketAddr, Box<dyn std::error::Error>> {
    let addr =
        SocketAddrV6::from_str(address).map_err(|e| format!("Invalid Ipv6 address {:?}", e))?;

    if addr.port() < PORT_MIN || addr.port() > PORT_MAX {
        return Err(format!(
            "Invalid port number. Must be greater than {} and less than {}",
            PORT_MIN, PORT_MAX
        )
        .into());
    }
    Ok(SocketAddr::from(addr))
}

/// Configures the custom TLS settings for the gRPC server
pub fn configure_server_tls() -> Result<ServerTlsConfig, Box<dyn std::error::Error>> {
    let suites: Vec<&'static rustls::SupportedCipherSuite> = vec![
        &rustls::ciphersuite::TLS13_AES_256_GCM_SHA384,
        &rustls::ciphersuite::TLS13_AES_128_GCM_SHA256,
        &rustls::ciphersuite::TLS13_CHACHA20_POLY1305_SHA256,
    ];
    let protocol_version = vec![ProtocolVersion::TLSv1_3];

    // Setup certificates and private keys
    let mut client_auth_roots = RootCertStore::empty();
    let root_cert = load_certs(CLIENT_CERT)?;
    for cert in root_cert {
        client_auth_roots.add(&cert)?;
    }

    let cert_chain = load_certs(CERT)?;
    let priv_key = load_private_key(KEY)?;

    // Configure TLS
    let mut config = ServerConfig::new(AllowAnyAuthenticatedClient::new(client_auth_roots));
    config.set_single_cert(cert_chain, priv_key)?;
    config.ciphersuites = suites;
    config.versions = protocol_version;

    // TODO: remove if not required
    config.alpn_protocols = vec![b"h2".to_vec()];

    let tls_config = ServerTlsConfig::new()
        .rustls_server_config(config)
        .to_owned();

    Ok(tls_config)
}

/// Loads all x509 certificates from a file at {path}
fn load_certs(path: &str) -> Result<Vec<Certificate>, io::Error> {
    log::info!("Parsing certificates at path: {}", path);

    let f = fs::File::open(path)?;
    let mut reader = io::BufReader::new(f);
    let certs: Vec<Certificate> = certs(&mut reader)?
        .iter()
        .map(|v| Certificate(v.clone()))
        .collect();
    Ok(certs)
}

/// Loads a RSA, PKCS8 or EC private key from a file at {path}
fn load_private_key(path: &str) -> Result<PrivateKey, io::Error> {
    log::info!("Reading private key at path: {}", path);

    // Parse key from file
    let keyfile = fs::File::open(path)?;
    let mut reader = io::BufReader::new(keyfile);
    let key_string = rustls_pemfile::read_one(&mut reader)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("No PEM section found in file {}", path),
        )
    })?;

    // Verify it is a supported key type
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
