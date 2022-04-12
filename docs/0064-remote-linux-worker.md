---
author: Jayden Elliott (jayden.elliott@outlook.com)
state: draft
---


# RFD 64 - Remote Linux Worker Library, API and CLI

## What

A lightweight remote linux process execution server-side library and client CLI.

## Why

Customers have expressed interest in a solution to expose their servers to their clients in a controlled and secure way.

I propose a library and client CLI solution that will achieve this goal through a gRPC API with the ability to configure connection security and access control.


## Details

### Library

#### High level details


The library is responsible for providing a gRPC server that processes requests to `start`, `stop`, `query` and `stream` the output of linux process jobs. Refer to [API](#api) for an overview of the exposed API features.

#### Interface

This library exports a single function with the signature:

```rust
pub async fn start_server(config: ServerSettings) -> Result<(), ServerError>>
```

When called, a gRPC server will be initialized with the parsed configuration settings and begin handling incoming requests. The user of this library will be required to implement an async runtime for their `main()` function, for this `tokio` is recommended:
```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  ...
  start_server(config).await?;
}
```

The library also provides a server configuration object to be parsed to `start_server()`:
```rust
pub struct ServerSettings {
    // TODO: Add extra configuration options:
    // - Request rate limits for DDOS protection
    // - Option to pipe logs to file (https://docs.rs/log4rs/1.0.0/log4rs/)
    // - Option to manually configure TLS:
    //    - to use TLS v1.2 for an old client implementation
    //    - to allow different private key encryption formats

    // A String containing the IPv6 address + port the user wishes to run the server on
    pub socket_address: String,
}
```


#### External Dependencies
The following basic dependencies will be required:
| Crate  | Ver.   | Description                                |
| ------ | ------ | ------------------------------------------ |
| tonic  | 0.6.2  | gRPC framework for server and client setup |
| rustls | 0.19.1 | TLS configuration                          |
| prost  | 0.9    | Protocol Buffer rust implementation        |

Note: `tonic` and `rustls` will not use their latest version, see [Trade-Offs Tonic](#external-dependency---tonic).

### API

The gRPC API exposes a single service `JobProcessorService`: 
```proto
// JobProcessorService describes an API interface to perform start, stop, stream 
// and status query requests for Linux process jobs.
service JobProcessorService {
  // Start starts a new linux process job
  rpc Start(StartRequest) returns(StartResponse);

  // Stop stops a currently running linux process job 
  rpc Stop(StopRequest) returns(google.protobuf.Empty);

  // Stream streams all previous and upcoming stderr and stdout data for a specified job
  rpc Stream(StreamRequest) returns(stream StreamResponse);

  // Status returns the status of a previous linux process job
  rpc Status(StatusRequest) returns(StatusResponse);

  // ListJobs returns a list of all current and previous process job uuids
  rpc ListJobs(google.protobuf.Empty) returns(ListJobsResponse);
}

```

#### Start
Start will begin a new job using the command and arguments specified.
```
// StartRequest describes StartRequest
message StartRequest {
  // command is the path to the script or the native linux command to execute
  string command = 1;

  // arguments specifies the arguments to run with the script/command being executed
  repeated string arguments = 2;
}
```

An example client message in Rust would look something like:
```rust
let request = Request::new(StartRequest{
  command: "cargo",
  arguments: vec!["--version"]
})
```

The response is a uuid generated for that job.

```
// StartResponse describes StartResponse
message StartResponse {
  // uuid is the unique identifier of the job started 
  string uuid = 1;
}
```

#### Stop

Stopping a job requires the uuid gathered in the StartResponse. The user has the option to gracefully kill the process, allowing the program to process to handle the request.

 It should be noted that when using SIGTERM (graceful = true), the process has the option to ignore the signal, thus if your process is persistently not stopping it is recommended to set graceful to false. 

```
// StopRequest describes StopRequest
message StopRequest {
  // uuid is the unique identifier of the job process to stop
  string uuid = 1;

  // graceful defines if a job should be killed using using SIGTERM (true) or SIGKILL (false). 
  // SIGTERM will be used by default
  bool graceful = 2;
}
```

StopRequest returns the `google.protobuf.Empty` type. The client can asynchronously check if the process is running or the exit code using the [StatusRequest](#status).


#### Status
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

- The server should have the option to manually configure the TLS settings.
  - A client may want to use TLS 1.2 due to compatibility with their custom client implementation.
  - A client may want to use different private key encryption standards.
- The server should have added security mechanisms such as DDOS protection through connection and rate limits on requests.
- The server should have the option to pipe logs to file and configure logs.



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


