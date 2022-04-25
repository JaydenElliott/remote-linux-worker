# RLW - Remote Linux Worker
A remote linux process execution library and gRPC server.

RLW exposes two sub-libraries:
1. `rlwp` - Remote Linux Worker Processor: an interface that allows users to start, stop, stream, and get the status of linux process jobs.
2. `server` - An all-included gRPC server to expose the above functionality to a client. The server provides the following:
   - An API to handle jobs. The api protobuf specification can be found in `proto/service.proto`.
   - A secure mTLS configuration.
   - Client authorization.


## Development

To run the library unit tests ensure that you are in the `remote-linux-worker/` directory and run:
```
cargo test --lib
```

## Examples

An example client / server implementation can be found in `remote-linux-worker/examples`. Ensure all of the below commands are run from the `examples/` directory.

### TL;DR
Start the server with `cargo run --bin rlw-server`.
In a separate window / tab use the below command to start a job (run `stream_job.sh`) and pipe the UUID to the stream command to get the output:
```
cargo run --bin rlw-client start /bin/bash ./stream_job.sh | xargs -I {} cargo run --bin rlw-client stream "{}"
```

### Server

To start the server, run:
```
cargo run --bin rlw-server
```

Logging is used, so ensure `$RUST_LOG` is set to `info` to see the full log output.


### Client

The following are a set of example commands used to interact with the server. To see the full CLI specification, first build the examples, then run.
 ```
cargo run --bin rlw-client --help

cargo run rlw-client {subcommand} --help
 ``` 

The commands below are prefaced with `cargo run --bin`. If you would prefer to run the binary directly (much faster), copy `examples/target/debug/rlw-client` to the `examples/` directory and use that instead.

#### Start Job

Run a script: 
```
cargo run --bin rlw-client start /bin/bash ./stream_job.sh
```
All jobs will run from the `examples/tests/test_env/` directory. This will later be configurable.

Run a command:
```
cargo run --bin rlw-client start echo hello world
```

Start job will return a job UUID. It is the client's responsibility to store this for future commands.

#### Stop Job

```
cargo run --bin rlw-client stop {UUID} 
```

A job can be forcefully shutdown with:
```
cargo run --bin rlw-client stop -f {UUID} 
```


#### Stream Job
To obtain the history and live output of a job run:

```
cargo run --bin rlw-client stream {UUID} 
```

#### Job Status 
To get the job status (`Running`, `Exited with Code` or `Exited with Signal`), run:

```
cargo run --bin rlw-client status {UUID} 
```