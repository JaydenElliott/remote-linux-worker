---
author: Jayden Elliott (jayden.elliott@outlook.com)
state: draft
---


# RFD 64 - Remote Linux Worker Library, API and CLI

## What

A lightweight remote linux process execution server-side library and client CLI.

## Why

Customers have expressed interest in a solution to expose their servers to their clients in a controlled and secure way.

This library/client product will do this through a lightweight gRPC API with the ability to configure connection security and access control.


## Details

### Library

#### High level details

todo!

#### Interface

todo!

#### External Dependencies
The following dependencies will be required:
| Crate  | Ver.   | Description                                |
| ------ | ------ | ------------------------------------------ |
| tonic  | 0.6.2  | gRPC framework for server and client setup |
| rustls | 0.19.1 | TLS configuration                          |
| prost  | 0.9    | Protocol Buffer Rust implementation        |
| log    | 0.4.16 | Server logging                             |

Note: `tonic` and `rustls` will not use their latest version, see [Trade-Offs Tonic](#external-dependency---tonic).

#### Example Usage

todo!

### API

todo!

### CLI

todo!

### Security

#### Transport Encryption
- The server and client will use TLS 1.3 to establish a secure connection.
- The server will enable the following ciphers suites. These ciphers are defined as 'secure' by the IETF and are the only ciphers suites allowed in TLS 1.3 that enable Perfect Forward Secrecy.
  - TLS_AES_128_GCM_SHA256,
  - TLS_AES_256_GCM_SHA384,
  - TLS_CHACHA20_POLY1305_SHA256

#### mTLS Implementation

todo!

#### Auth Scheme

todo!


### Trade-offs and Future Considerations

#### Library

todo!

#### Security

ECDSA keys would be preferred over RSA Keys.
  - ECDSA keys are more secure and performant, however `rustls` does not currently support parsing these keys.
  - A benefit of RSA keys are that they are more compatible with most systems due to it being the de facto standard.

Other changes to the private-key implementation that would increase security are:
- Storage of keys in a cloud based service such as 'GCP's Secret Manager' would decrease the risk of leaked private keys.
- Renewing keys and certificates periodically (e.g. yearly).

Certificates
- Self-signing certificates are not secure. The following would help increase security:
  - Obtaining certificates from a reliable CA would be required at production.
  - Public key pinning.
    - Reduces attack surface significantly but requires a significant amount of time and expertise to configure correctly.


Authorization
todo!

#### External Dependency - Tonic

Pros:
- Highly performant, flexible and well maintained library.
- Seamless integration with the TLS library `rustls`.  
- Time saved with client/server setup and integration with the generated gRPC code.

Cons: 
- `tonic` version 0.7.0 removed the ability to configure the TLS version and cipher suites using `rustls`. Because `tonic` will not allow you to do this manually, the only options are very convoluted workarounds, which are messy and not intended to be used. Because of this, an older version of `tonic` will be required.
  - This is not a viable option for production.
  - For a larger project with more time, setting up the server from scratch may a be a better option to solve this issue and also give more configurability and a smaller dependency list. 
    - Reaching the


