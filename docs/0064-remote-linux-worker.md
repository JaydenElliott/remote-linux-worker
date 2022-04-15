---
author: Jayden Elliott (jayden.elliott@outlook.com)
state: draft
---


# RFD 64 - Remote Linux Worker Library, API and CLI

## What

A remote linux process execution server-side library and client CLI. The library/client will have a strong focus on transport layer security and the ability to manage access control.

## Why

Customers have expressed interest in a product that allows their clients to run commands on a server in a controlled and secure way.

## Details

### Library

#### High Level Details


The library is responsible for providing a gRPC server that processes requests to `start`, `stop`, query the `status`, and `stream` the output of Linux process jobs. Refer to [API](#api) for an overview of the exposed API features.

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
When called, a gRPC server will be initialized with the parsed configuration settings and begin handling incoming requests. The user of this library will be required to implement a `tokio` async runtime to run the server. Example usage:

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
    // - Rate limits on requests, for DDOS protection
    // - Option to pipe logs to file (https://docs.rs/log4rs/1.0.0/log4rs/)
    // - Option to manually configure TLS:
    //    - to use TLS v1.2 for an old client implementation
    //    - to allow different private key encryption formats
    // - Option to set the host and user CA

    /// A String containing the IPv6 address + port the user wishes to run the server on
    pub socket_address: String,
}
```

With the following constructor:

```rust
pub fn new(socket_address: String) -> Self 
```

#### General Data Structure and Streaming Logic Overview

The server will hold a hashmap that maps usernames to `User` structs. When a user makes a request to `stop`, `status` or `stream`, the server will access this structure to see if the user exists and if the requested job is available to them.

```rust
/// Stores usernames and associated User objects
struct UserTable {
    /// Maps a username to a user object
    users: HashMap<String, User>,
}
```

Each user struct will contain a hashmaps of jobs and a job queue:
- The hashmap of jobs (mapping uuid to job) is used to store all the information regarding the job (i.e. is it still running, the stderr and stdout, exit code etc.)
- The job queue contains the jobs that a client has requested be started.

The job processing logic will be: 
- As `start` requests for a particular client come in, the main thread will be adding these jobs to the client's queue.
- A second thread will be reading from this queue, processing the jobs and sending the output down a channel.
- A third thread will be on the receiving end of the channel, writing the stderr and stdout to the `output` field of `Jobs`.

```rust
/// Stores a single user's job information.
struct User {
    /// Maps a job uuid to a Job
    jobs: HashMap<String, Jobs>,

    /// A queue storing new job requests.
    job_queue: Mutex<VecDeque<StartRequest>>,
}
```

``` rust
/// A user job containing information about the
/// underlying process.
pub struct Job {
    /// Job status
    status: status_response::ProcessStatus,

    /// Stderr and stdout output from the job.
    output: Vec<u8>,
}
```

Where `status_reponse::ProcessStatus` is the prost generated status enum. 

<br>


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
// JobProcessorService describes an API interface to perform start, stop, stream, 
// and status requests for Linux process jobs.
service JobProcessorService {
  // Start starts a new process job
  rpc Start(StartRequest) returns(StartResponse);

  // Stop stops a running process job 
  rpc Stop(StopRequest) returns(google.protobuf.Empty);

  // Stream streams all previous and upcoming stderr and stdout data for a specified job
  rpc Stream(StreamRequest) returns(stream StreamResponse);

  // Status returns the status of a previous process job
  rpc Status(StatusRequest) returns(StatusResponse);
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

Stopping a job requires the UUID returned in the StartResponse. The user has the option to force kill the process with SIGKILL by setting `forced` to true. By not providing `forced` or setting it to false the process will be killed using SIGTERM.

```proto
// StopRequest describes StopRequest
message StopRequest {
  // uuid is the unique identifier of the job process to stop
  string uuid = 1;

  // forced defines if a job should be killed using using SIGKILL (true) or SIGTERM (false)
  // SIGTERM will be used by default
  bool forced = 2;
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
  // output defines the stdout and stderr content being streamed to the client
  bytes output = 1;
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

  oneof process_status {
    // running defines if a job is currently running
    bool running = 1;

    // exit_code represents the exit_code of the job if the underlying process has finished
    uint32 exit_code = 2; 

    // signal represents the signal code that terminated a process.
    // In the event of a `stop` request this will either be 9 (SIGKILL) or 
    // 15 (SIGTERM).
    uint32 signal = 3;
  }

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
    -s, --address <address>                                     [default: http://[::1]:50051]
    -c, --cert <cert>    
    -p, --private-key <private-key>    

SUBCOMMANDS:
    help      Prints this message or the help of the given subcommand(s)
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
$ rlw-client -c $CERT -p $KEY start ls -a -l
```

Stop
```
$ rlw-client -c $CERT -p $KEY stop -f 0bcb5f36-b2d3-493e-9d76-f650ba225c5d 
```

Stream
```
$ rlw-client -c $CERT -p $KEY stream 0bcb5f36-b2d3-493e-9d76-f650ba225c5d
```

Status
```
$ rlw-client -c $CERT -p $KEY status 0bcb5f36-b2d3-493e-9d76-f650ba225c5d
```

### Security

#### Transport Encryption

mTLS
- The gRPC API uses mTLS to authenticate the connection between the server and client.
  - A certificate chain of size two will be used (root and server/client certificate). No intermediary certificates will be used.
  - Each client and the server will be provided a certificate signed by a custom self-signed 'root' certificate.
  - During the handshake, the client will authenticate the server's certificate against the root.
  - The server will then authenticate the client's certificate.

TLS Configuration
- The server and client will use TLS 1.3 to establish a secure connection.  
- The server will enable the following ciphers suites:
  - TLS_AES_128_GCM_SHA256,
  - TLS_AES_256_GCM_SHA384,
  - TLS_CHACHA20_POLY1305_SHA256
- From the list of available TLS 1.3 cipher suites these are the most secure as they implement Perfect Forward Secrecy. 
- Also, it is a requirement in the IETF TLS 1.3 Standard that to be to be TLS-compliant you must implement the above cipher suites (see [TLS 1.3 Standard Sec 9.1](https://datatracker.ietf.org/doc/html/rfc8446#section-9.1)). 

See [Security Trade-offs](#security-1) for trade-offs regarding these points.

#### Authorization Scheme
Client's will only be able to access jobs that they themselves started. To enforce this, when the server receives a request to `stop`, `status` or `stream` a job, it will do the following:
1. Verify the client's certificate.
2. Parse the client's name from the certificate.
3. Check the parsed name corresponds to the same name that started the job originally.

### Trade-offs and Future Considerations

#### Library

- All the user data (job information, who is a valid user), should be stored in a database.
  - This will allow for persistent sessions.
  - This will increase the security of sensitive information.
  
- The server should provide an option to execute the client's commands in a virtual runtime environment. This would be useful for the following reasons:
    - A library user may not want the client executing commands within their server.
    - This decreases the potential damage done by the execution of malicious binaries / commands.
  - Giving the client access to a specific subset of the filesystem using access-control-lists could provide similar functionality.


#### Security

Changes to the private-key implementation that would increase security are:
- Storage of keys in a cloud based service such as 'GCP's Secret Manager' would decrease the risk of leaked private keys.
- Renewing keys and certificates periodically (e.g. yearly).

Certificates
- Self-signing certificates are not secure. The following would help increase security:
  - Implementing public key pinning:
    - Reduces attack surface significantly but requires a significant amount of time and expertise to configure correctly.
    - If not done correctly this could lead to considerable server down time.

TLS Configuration
- TLS 1.3 is used over TLS 1.2 due to decreased latency and increased security.
- A trade off is that custom client implementations may not support TLS 1.3.
  - Allowing configurable TLS versions would solve this, however the cost of decreased security does need to be taken into consideration.  
- The server should have the option to configure host and user CAs.

Authorization could be further secured by:
  - Using a username/password login, OAUTH, JWTs or 2FA.
  - Pairing the above with a database would allow client sessions to be persistent across multiple sessions.

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
