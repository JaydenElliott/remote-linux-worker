# RLW - Remote Linux Worker
A lightweight remote linux process execution server-side library.

RLW exposes two sub-libraries:
1. `rlwp` - Remote Linux Worker Processor: an interface that allows users to start, stop, get the status of and stream linux process jobs.
2. `server` - An all-included gRPC server to expose the above functionality to a client. The server provides the following:
   - An API to handle jobs. The api protobuf specification can be found in `proto/service.proto`.
   - A secure mTLS configuration.
   - Client authorization.




## Examples

An example client/server implementation can be found in `remote-linux-worker/examples`. Ensure all the below commands are run from this directory.

### Server

To start the server run:
```
cargo run --bin rlw-server
```

Logging is used so ensure `RUST_LOG=info` to see the full log output.

<br>

### Client

The following are a set of examples commands used to interact with the server. To see the full CLI specification run.
 ```
cargo run --bin rlw-client --help

cargo run rlw-client {subcommand} --help
 ``` 

#### Start Job

Run a script: 
```
cargo run --bin rlw-client start /bin/bash ./stream_job.sh
```
Note: all jobs will run from the `examples/tests/test_env/` directory. This will later be configurable.

Run a command:
```
cargo run --bin rlw-client start echo hello world
```

#### Stop Job
Start job will return a job UUID. It is the client's responsible to store this for future commands.

```
cargo run --bin rlw-client stop ${UUID} 
```

A job can be forcefully closed with:
```
cargo run --bin rlw-client stop -f ${UUID} 
```


#### Stream Job
To obtain the history and live output of a job run:

```
cargo run --bin rlw-client stream ${UUID} 
```

If the output is valid utf-8, it can be returned as a string with:

```
cargo run --bin rlw-client stream -s ${UUID} 
```

Any non-utf-8 bytes will display an error and the stream will continue.


#### Job Status 
To get the job status (`Running`, `Exited with Code` or `Exited with Signal`) run:

```
cargo run --bin rlw-client status ${UUID} 
```

<br>

## Development

To run the library unit tests ensure you are in the projects top-most directory `..../remote-linux-worker` and run:
```
cargo test --lib
```


