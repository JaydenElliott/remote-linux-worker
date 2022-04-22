//! Handles all the type implementations for the server

use crate::jobs::Job;

use std::sync::Mutex;
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};
use uuid::Uuid;
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
        uuid
    }

    /// Process that will continue to work through jobs
    /// on the queue, until it is empty
    async fn new_queue_processor(&self, uuid: &str) {
        while let Some((command, args)) = self.get_new_from_queue() {
            let job = self.get_new_job(uuid);
            job.start_command(command, args).await.unwrap(); // TODO: Check this actually waits
        }
    }

    /// Add a new command set to the queue
    ///
    /// * Returns
    ///  bool - true if the queue was empty before adding the new command set - this implying
    ///         that a new thread needs starting to process the queue.
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

    // /// TODO: Might not need this
    // /// Returns a job status response if the job exists,
    // /// else returns None.
    // pub async fn get_job_status(&self, uuid: &str) -> Option<StatusResponse> {
    //     let job = self.get_job(uuid);
    //     let status = job.status.lock().await.clone();
    //     Some(StatusResponse {
    //         process_status: Some(status),
    //     })
    // }
}
