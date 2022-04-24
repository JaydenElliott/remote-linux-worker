//! Example Client Implementation

mod job_processor_api {
    tonic::include_proto!("job_processor_api");
}
mod args;
mod utils;

use crate::job_processor_api::{
    job_processor_service_client::JobProcessorServiceClient, StartRequest,
};
use crate::utils::configure_mtls;

use args::ExtCommand;
use job_processor_api::{StatusRequest, StopRequest, StreamRequest};
use std::time::Duration;
use structopt::StructOpt;
use tonic::{transport::Channel, Request};

// TODO: move these into a configuration file
const REQUEST_TIMEOUT: u64 = 60;
const CONNECTION_TIMEOUT: u64 = 60;
const RATE_LIMIT_CONNECTIONS: u64 = 32;
const RATE_LIMIT_DURATION: u64 = 1;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup client configuration
    let opt = args::Args::from_args();
    let tls_config = configure_mtls().await?;
    let channel = Channel::from_shared(opt.address)?
        .timeout(Duration::from_secs(REQUEST_TIMEOUT))
        .connect_timeout(Duration::from_secs(CONNECTION_TIMEOUT))
        .rate_limit(
            RATE_LIMIT_CONNECTIONS,
            Duration::from_secs(RATE_LIMIT_DURATION),
        )
        .tls_config(tls_config)?
        .connect()
        .await?;
    let mut client = JobProcessorServiceClient::new(channel);

    // Process request
    match opt.api_command {
        args::WorkerAPI::Start { ext_command } => start_request(&mut client, ext_command).await?,
        args::WorkerAPI::Stop { uuid, forced } => stop_request(&mut client, uuid, forced).await?,
        args::WorkerAPI::Stream { uuid, as_string } => {
            stream_request(&mut client, uuid, as_string).await?
        }
        args::WorkerAPI::Status { uuid } => status_request(&mut client, uuid).await?,
    };
    Ok(())
}

/// Process a start request
async fn start_request(
    client: &mut JobProcessorServiceClient<Channel>,
    external_command: ExtCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    let (command, arguments) = match external_command {
        ExtCommand::Args(args) => (args[0].clone(), args[1..].to_vec()),
    };
    let request = Request::new(StartRequest { command, arguments });
    let response = client.start(request).await?.into_inner();
    println!("Job UUID: {}", response.uuid);
    Ok(())
}

/// Process a stop request
async fn stop_request(
    client: &mut JobProcessorServiceClient<Channel>,
    uuid: String,
    forced: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let request = Request::new(StopRequest { uuid, forced });
    client.stop(request).await?;
    Ok(())
}

/// Process a stream request
/// If `as_string` is true, utf8-valid bytes will be displayed as strings
async fn stream_request(
    client: &mut JobProcessorServiceClient<Channel>,
    uuid: String,
    as_string: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let request = Request::new(StreamRequest { uuid });
    let mut stream = client.stream(request).await?.into_inner();
    while let Some(response) = stream.message().await? {
        if as_string {
            match std::str::from_utf8(&response.output) {
                Ok(s) => println!("{}", s),
                Err(_) => println!("Error decoding output bytes"),
            }
        } else {
            println!("{:?}", &response.output);
        }
    }
    Ok(())
}

/// Process a stream request
async fn status_request(
    client: &mut JobProcessorServiceClient<Channel>,
    uuid: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let request = Request::new(StatusRequest { uuid });
    let response = client.status(request).await?.into_inner();
    println!("{:?}", response.process_status);
    Ok(())
}
