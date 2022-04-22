//! Exposes all the required types and type impls for the
//! rlw server to run.

use crate::job_processor::{
    job_processor_service_server::{JobProcessorService, JobProcessorServiceServer},
    *,
};
use crate::jobs::Job;
use crate::utils;

use std::sync::Mutex;
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};
use std::{error::Error, sync::atomic::AtomicBool};
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport;
use tonic::{Request, Response, Status};
use uuid::Uuid;

/// # RLW Server
/// A Remote Linux Worker server will listen for
/// requests to `start`, `stop`, `status` and `stream` process jobs.
///
/// This server is a wrapper around the `tonic::transport::Server`.
///
/// # Example Usage
///
/// ```no-run
/// use rlw::types::{Server, ServerSettings};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///  let settings = ServerSettings::new("[::1]:50051");
///  let server = Server::new(settings);
///  server.run().await?;
///  Ok(())
/// }
/// ```
pub struct Server {
    config: ServerSettings,
}

impl Server {
    // Create a new server instance
    pub fn new(config: ServerSettings) -> Self {
        Self { config }
    }

    /// Start the gRPC server using the configuration provided in `new(config)`
    /// and handle all incoming requests.
    ///
    /// TODO: If I get the time, update the Box<> to be a custom
    ///       server error type that converts all the other errors it it's type
    pub async fn run(&self) -> Result<(), Box<dyn Error>> {
        // Validate and parse the IPv6 address
        let addr = utils::ipv6_address_validator(&self.config.socket_address)?;

        // // Configure and initialize the server
        let processor = JobProcessor::new();
        let svc = JobProcessorServiceServer::new(processor);
        let tls_config = utils::configure_server_tls()?;

        log::info!(
            "Linux worker gRPC server listening on: {}",
            self.config.socket_address
        );
        transport::Server::builder()
            .tls_config(tls_config)?
            .add_service(svc)
            .serve(addr)
            .await?;

        Ok(())
    }
}

/// Server Configuration
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

impl ServerSettings {
    pub fn new(socket_address: String) -> Self {
        Self { socket_address }
    }
}

/// Handles the incoming gRPC job process requests and
/// stores user data related to these requests
pub struct JobProcessor {
    // Maps usernames to users structs.
    // Stores all users and their jobs.
    user_table: Arc<Mutex<HashMap<String, Arc<User>>>>,
}

impl JobProcessor {
    /// Creates a new job processor. This should only be done once
    /// per server instance.
    pub fn new() -> Self {
        Self {
            user_table: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get a mutable reference to the user's User struct.
    /// If no user exists with the specified username, a new
    /// user will be created.
    pub fn get_user(&self, username: &str) -> Arc<User> {
        let table_arc = Arc::clone(&self.user_table);
        let mut table = table_arc.lock().unwrap();
        if !table.contains_key(username) {
            table.insert(String::from(username), Arc::new(User::new()));
        }
        Arc::clone(table.get(username).unwrap())
    }
}

#[tonic::async_trait]
impl JobProcessorService for JobProcessor {
    /// Start a new process job
    async fn start(
        &self,
        request: Request<StartRequest>,
    ) -> Result<Response<StartResponse>, Status> {
        log::info!("Start Request");

        // Get user
        let username = "john"; // temp
        let user = self.get_user(username);

        todo!()
    }

    /// Stop a running process job
    async fn stop(&self, request: Request<StopRequest>) -> Result<Response<()>, Status> {
        todo!()
    }

    type StreamStream = ReceiverStream<Result<StreamResponse, Status>>;
    /// Stream all previous and upcoming stderr and stdout data for a specified job
    async fn stream(
        &self,
        request: tonic::Request<StreamRequest>,
    ) -> Result<tonic::Response<Self::StreamStream>, tonic::Status> {
        todo!();
    }
    /// Return the status of a previous process job
    async fn status(
        &self,
        request: Request<StatusRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        todo!()
    }
}

/// Stores a single user's job information.
pub struct User {
    /// Maps job uuid to Job
    jobs: Arc<Mutex<HashMap<String, Arc<Job>>>>,

    /// A queue storing new job requests.
    /// Each job request contains a new Command and
    /// a list of arguments to accompany the command.
    job_queue: Arc<Mutex<VecDeque<(String, Vec<String>)>>>,
}

impl User {
    /// Returns a new User
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(Mutex::new(HashMap::new())),
            job_queue: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// TODO: check no race condition on starting a job
    /// it returning "one",no thread started, and teh worker
    /// thread finishing
    pub fn new_job(self: Arc<Self>, command: String, args: Vec<String>) -> String {
        let uuid = Uuid::new_v4().to_string();

        // Add new job to queue
        let first = self.add_to_queue(command, args);
        if !first {
            return uuid;
        }
        // Spawn queue processor thread
        let handler = self.clone();
        let uuid_t = uuid.clone();
        tokio::spawn(async move {
            handler.new_queue_processor(&uuid_t).await;
        });
        // start new thread
        // thread will take a new start request from back of queue
        // crate a new job from it, append it to list of jobs and work on it
        uuid
    }

    /// Process that will continue to work through jobs
    /// on the queue, until it is empty
    async fn new_queue_processor(&self, uuid: &str) {
        while let Some((command, args)) = self.get_new_from_queue() {
            let new_job = self.get_new_job(uuid);
            new_job.start_command(command, args).await;
        }
    }

    /// Add a new command set to the queue
    ///
    /// * Returns
    /// * bool - true if the queue was empty before adding the new command set - this implying
    ///          that a new thread needs starting to process the queue.
    ///          
    fn add_to_queue(&self, command: String, args: Vec<String>) -> bool {
        let queue_arc = Arc::clone(&self.job_queue);
        let mut queue = queue_arc.lock().unwrap();
        queue.push_back((command, args));
        queue.len() == 1
    }

    /// Get a command set from the queue
    fn get_new_from_queue(&self) -> Option<(String, Vec<String>)> {
        let queue_arc = Arc::clone(&self.job_queue);
        let mut queue = queue_arc.lock().unwrap();
        queue.pop_front()
    }

    /// Create a new job and return a pointer to it
    fn get_new_job(&self, uuid: &str) -> Arc<Job> {
        let jobs_arc = Arc::clone(&self.jobs);
        let mut jobs = jobs_arc.lock().unwrap();
        jobs.insert(uuid.to_string(), Arc::new(Job::new()));
        let job = Arc::clone(jobs.get(uuid).unwrap());
        job
    }

    /// Get a pointer to a job from its uuid
    fn get_job(&self, uuid: &str) -> Arc<Job> {
        let jobs_arc = &*Arc::clone(&self.jobs);
        let job = Arc::clone(jobs_arc.lock().unwrap().get(uuid).unwrap());
        job
    }

    /// Returns a job status response if the job exists,
    /// else returns None.
    pub async fn get_job_status(&self, uuid: &str) -> Option<StatusResponse> {
        let job = self.get_job(uuid);
        let status = job.status.lock().await.clone();
        Some(StatusResponse {
            process_status: Some(status),
        })
    }
}
