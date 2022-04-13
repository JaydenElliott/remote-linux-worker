---
author: Jayden Elliott (jayden.elliott@outlook.com)
state: draft
---


# RFD 64 - Remote Linux Worker Library, API and CLI

## What

A lightweight remote linux process execution server-side library and client CLI.

## Why

Customers have expressed interest in a product that allows their clients to run commands on a server in a controlled and secure way.

Outlined below is a proposed library and client CLI solution that will achieve this goal through a gRPC API with strong connection security and the ability to manage access control.


## Details

### Library

#### High level details


The library is responsible for providing a gRPC server that processes requests to `start`, `stop`, `query` and `stream` the output of Linux process jobs. Refer to [API](#api) for an overview of the exposed API features.

#### Interface

The library exports a struct `Server` that exposes two functions:

A constructor with the signature:
```rust
pub fn new(config: ServerSettings) -> Self
```

and a function to run the server:

```rust
pub async fn run(&self) -> Result<(), ServerError>>
```
When called, a gRPC server will be initialized with the parsed configuration settings and begin handling incoming requests. The user of this library will be required to implement an async runtime for their `main()` function; for this `tokio` is recommended. Example usage:

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  let settings = ServerSettings::new("[::1]:50051");
  let server = Server::new(settings);
  server.run().await?;
}
```

The library also exports the struct `ServerSettings`:
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

With the following constructor:

```rust
pub fn new(socket_address: String) -> Self 
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

The gRPC API will expose a single service `JobProcessorService`: 
```proto
// JobProcessorService describes an API interface to perform start, stop, stream 
// and status query requests for Linux process jobs.
service JobProcessorService {
  // Start starts a new process job
  rpc Start(StartRequest) returns(StartResponse);

  // Stop stops a running process job 
  rpc Stop(StopRequest) returns(google.protobuf.Empty);

  // Stream streams all previous and upcoming stderr and stdout data for a specified job
  rpc Stream(StreamRequest) returns(stream StreamResponse);

  // Status returns the status of a previous process job
  rpc Status(StatusRequest) returns(StatusResponse);

  // ListJobs returns a list of all current and previous process job uuids
  rpc ListJobs(google.protobuf.Empty) returns(ListJobsResponse);
}

```

#### Start
Start will begin a new job using the command and arguments specified:
```proto
// StartRequest describes StartRequest
message StartRequest {
  // command is the path to the script or the native linux command to execute
  string command = 1;

  // arguments specifies the arguments to run with the script/command being executed
  repeated string arguments = 2;
}
```

An example client message in Rust:
```rust
let request = Request::new(StartRequest{
  command: "cargo",
  arguments: vec!["--version"]
})
```

The response is a UUID generated for that job:

```proto
// StartResponse describes StartResponse
message StartResponse {
  // uuid is the unique identifier of the job started 
  string uuid = 1;
}
```

#### Stop

Stopping a job requires the UUID gathered in the StartResponse. The user has the option to kill the process gracefully. This will give the process time to internally handle the kill request.

```proto
// StopRequest describes StopRequest
message StopRequest {
  // uuid is the unique identifier of the job process to stop
  string uuid = 1;

  // graceful defines if a job should be killed using using SIGTERM (true) or SIGKILL (false) 
  // SIGTERM will be used by default
  bool graceful = 2;
}
```

It should be noted that when using SIGTERM (graceful = true), the process has the option to ignore the signal, thus if your process is persistently not stopping it is recommended to set graceful to false. 

StopRequest returns the `google.protobuf.Empty` type. The client can then asynchronously check the exit status of the process using the [StatusRequest](#status). The reason for this design is that in the event a process takes a while or refuses to shutdown, the client should not be blocked and should be able to make other requests. 

#### Stream

Making a StreamRequest for a job will initially return the stdout and stderr output that was saved from the job's inception. It will then continue to stream all the job's new stdout/stderr messages until the job is finished or shutdown.

```proto

// StartResponse describes StartResponse
message StartResponse {
  // uuid is the unique identifier of the job started 
  string uuid = 1;
}

// StreamResponse describes StreamResponse
message StreamResponse {
  // stdout_output defines the stdout content being streamed to the client
  bytes stdout_output = 1;
    
  // stderr_output defines the stderr content being streamed to the client
  bytes stderr_output = 2;
}
```

#### Status

Querying the status of a job requires the UUID gathered in the StartResponse:

```proto
// StatusRequest describes StatusRequest
message StatusRequest {
  // uuid is the unique identifier of the job process to query
  string uuid = 1;
}
```

The response contains useful process metadata:

```proto
// StatusResponse describes StatusResponse
message StatusResponse {
  // running defines if a job is currently running
  bool running = 1;

  // exit_code represents the exit_code of the job if the underlying process has finished
  uint32 exit_code = 2; 

  // stderr_output is the standard error output of the job
  bytes stderr_output = 3;

  // stdout_output is the standard output of the job
  bytes stdout_output = 4;
}

```

A client-side example of processing stderr or stdout may look like:

```rust

let request = Request::new(StatusRequest {uuid: job_uuid});
let response = client.status_request(request).await?

let stdout_reader = BufReader::new(response.stdout_output.as_slice());
    let lines = stdout_reader.lines();
    for output in lines {
        println!("stdout item: {:?}", output);
    }

```

### ListJobs

A `ListRequest` will return the client's job history and jobs currently running. 

No input is required to make this request.

```proto
// ListJobsResponse describes ListJobsResponse
message ListJobsResponse {
  // current defines a list of uuids representing the current 
  // job processes running
  repeated string current = 1;

  // previous defines a list of uuids representing the previous
  // job processes that have finished or shutdown
  repeated string previous = 2;
}

```


### CLI

The client command line interface will allow users to access the API.

Each command consists of two components:
1. Subcommand (Start, Stop, etc.).
2. Arguments corresponding to that subcommand.

To view all subcommands, run:

```
$ rlw-client --help

USAGE:
    rlw-client [OPTIONS] <SUBCOMMAND>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -s, --address <address>     [default: http://[::1]:50051]

SUBCOMMANDS:
    help      Prints this message or the help of the given subcommand(s)
    list      Return a list of all current and previous process job uuids
    start     Start a new process job
    status    Return the status of a previous process job
    stop      Stop a running process
    stream    Stream all previous and upcoming stderr and stdout data for a specified job

```

To view the arguments required for that subcommand, run:
```
$ rlw-client {subcommand} --help
```

#### Example usage

Start
```
$ rlw-client start -c ls -- -a -l
```

Stop
```
$ rlw-client stop -f -u 0bcb5f36-b2d3-493e-9d76-f650ba225c5d
```

Stream
```
$ rlw-client stream -u 0bcb5f36-b2d3-493e-9d76-f650ba225c5d
```

Status
```
$ rlw-client status -u 0bcb5f36-b2d3-493e-9d76-f650ba225c5d
```

List
```
$ rlw-client list
```

### Security

#### Transport Encryption

mTLS
- The gRPC API uses mTLS to authenticate the connection between the server and client.
  - A certificate chain of size two will be used.
  - Each client and the sever will be provided a certificate signed by a custom self-signed 'root' certificate.
  - During the handshake, the client will authenticate the server's certificate against the root.
  - The server will then authenticate the client's certificate.

TLS Configuration
- The server and client will use TLS 1.3 to establish a secure connection.  
- The server will require the use of 2048-bit PKCS#8 RSA keys.
- The server will enable the following ciphers suites:
  - TLS_AES_128_GCM_SHA256,
  - TLS_AES_256_GCM_SHA384,
  - TLS_CHACHA20_POLY1305_SHA256
- From the list of available TLS 1.3 cipher suites these are the most secure as they implement Perfect Forward Secrecy. 
- Also, it is a requirement in the IETF TLS 1.3 Standard that to be to be TLS-compliant you must implement the above cipher suites (see [TLS 1.3 Standard Sec 9.1](https://datatracker.ietf.org/doc/html/rfc8446#section-9.1)). 

See [Security Trade-offs](#security-1) for tradeoffs regarding these points.

#### Authorization Scheme

When the server receives a request, it will do the following:
1. Verify the client's certificate.
2. Parse the client's name from the certificate.
3. Check the name against a list of authorized users stored in server memory. 
4. If they exist then continue. If they do not exist, return an unauthorized error.

Clients will also only be authorized to stop, query, list and stream jobs they themselves have started.

### Trade-offs and Future Considerations

#### Library

- All the user data (job information, who is a valid user), should be stored in a database.
  - This will allow for persistent sessions.
  - This will increase the security of sensitive information.
  
- The server should provide an option to execute the client's commands in a virtual runtime environment. This would be useful for the following reasons:
    - A library user may not want the client executing commands within their server.
    - This decreases the damage done by the execution of malicious binaries / commands.
  - Giving the client access to a specific subset of the filesystem using access-control-lists could provide similar functionality.


#### Security

ECDSA keys would be preferred over RSA Keys because:
  - ECDSA keys are more secure and performant, however `rustls` does not currently support parsing these keys.
  - A benefit of RSA keys are that they are more compatible with most systems due to it being the de facto standard.

Other changes to the private-key implementation that would increase security are:
- Storage of keys in a cloud based service such as 'GCP's Secret Manager' would decrease the risk of leaked private keys.
- Renewing keys and certificates periodically (e.g. yearly).

Certificates
- Self-signing certificates are not secure. The following would help increase security:
  - Obtaining certificates from a reliable CA.
  - Implementing public key pinning:
    - Reduces attack surface significantly but requires a significant amount of time and expertise to configure correctly.
    - If not done correctly this could lead to considerable server down time.
- The application's security would benefit from building a stronger chain of trust using a larger number of intermediary CAs.

TLS Configuration
- TLS 1.3 is used over TLS.2 due to decreased latency and increased security.
- A trade off is that custom client implementations may not support TLS 1.3.
  - Allowing configurable TLS versions would solve this, however the decreased security does need to be taken into consideration.  

Authorization
Using an embedded name to verify a client is sensitive to breaches. The following would help increase authorization security: 
- Using a username/password login, OAUTH, JWTs or 2FA.
- Pairing the above with a database would also allow client sessions to be persistent across multiple sessions.

DDOS Protection
- The server should have added security mechanisms such as DDOS protection through connection and rate limits on requests.


#### External Dependency - Tonic

Pros:
- Highly performant, flexible and well maintained library.
- Time saved with the client/server setup and integration with the generated gRPC code.

Cons: 
- `tonic`  v0.7.0 removed the ability to configure the TLS version and cipher suites using `rustls`. Because `tonic` will not allow you to do this manually, the only options are non-elegant, error-prone workarounds. Because of this, an older version of `tonic` will be used.
  - This is not a viable option for production.
  - For a larger project with more time, setting up the server from scratch would be a better option in order to solve this issue and provide more configurability.
