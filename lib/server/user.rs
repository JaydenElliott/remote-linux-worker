//! Handles all the user level functionality for the server.
//! Includes job handling, user queue handling and the storage of job information.

use crate::rlwp::Job;
use crate::utils::errors::RLWServerError;

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

    /// A queue storing new job requests. Each job request contains a new command,
    /// a list of arguments to accompany the command and a job uuid.
    #[allow(clippy::type_complexity)]
    job_queue: Arc<Mutex<VecDeque<(String, Vec<String>, String)>>>,
}

impl User {
    /// Returns a new User
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(Mutex::new(HashMap::new())),
            job_queue: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Starts a new process job
    ///
    /// One of two scenarios can occur:
    /// 1. The queue is empty: an async task will be spawned. This task will process the job provided.
    ///    When finished it will attempt to process a new job if the queue in non-empty.
    ///
    /// 2. The queue is non-empty: the job will be added to the queue to be processed by the spawned task.
    pub fn start_new_job(
        self: Arc<Self>,
        command: String,
        args: Vec<String>,
    ) -> Result<String, RLWServerError> {
        let uuid = Uuid::new_v4().to_string();

        // Add new job to queue
        let first = self.add_to_queue(command, args, uuid.clone())?;
        if !first {
            return Ok(uuid);
        }

        // Spawn queue processor thread
        tokio::spawn(async move {
            // Process jobs while the queue is non-empty
            while let Some((command, args, uuid)) = self.get_command_from_queue() {
                // Create a new job
                match self.get_new_job(&uuid) {
                    Ok(job) => {
                        // Start job
                        if let Err(e) = job.start_command(command, args).await {
                            // Cant return error here as thread is never explicitly joined.
                            // Log error and move on to the next job in the queue.
                            log::error!("Start new job error: {:?}", e);
                        }
                    }
                    Err(e) => {
                        // Cant return error here as thread is never explicitly joined.
                        // Log error and move on to the next job in the queue.
                        log::error!("Get new job error: {:?}", e);
                    }
                }
            }
        });

        Ok(uuid)
    }

    /// Add a new command set to the queue
    ///
    /// * Returns
    ///  bool - true if the queue was empty before adding the new command set - this implying
    ///         that a new thread need to be spawned to process the job.
    ///          
    fn add_to_queue(
        &self,
        command: String,
        args: Vec<String>,
        uuid: String,
    ) -> Result<bool, RLWServerError> {
        let queue_arc = Arc::clone(&self.job_queue);
        let mut queue = queue_arc
            .lock()
            .map_err(|e| RLWServerError(format!("Job queue lock error: {:?}", e)))?;
        queue.push_back((command, args, uuid));
        Ok(queue.len() == 1)
    }

    /// Get a command set from the queue
    fn get_command_from_queue(&self) -> Option<(String, Vec<String>, String)> {
        let queue_arc = Arc::clone(&self.job_queue);
        let mut queue = queue_arc.lock().ok()?;
        queue.pop_front()
    }

    /// Create a new job and return a pointer to it
    fn get_new_job(&self, uuid: &str) -> Result<Arc<Job>, RLWServerError> {
        let jobs_arc = Arc::clone(&self.jobs);
        let mut jobs = jobs_arc
            .lock()
            .map_err(|e| RLWServerError(format!("Job HashMap lock error: {:?}", e)))?;
        jobs.insert(uuid.to_string(), Arc::new(Job::default()));
        let job = Arc::clone(jobs.get(uuid).ok_or_else(|| {
            RLWServerError(
                "Undefined behavior: inserted new job, then failed to retrieve it".to_string(),
            )
        })?);
        Ok(job)
    }

    /// Get a pointer to a job from its uuid
    pub fn get_job(&self, uuid: &str) -> Result<Arc<Job>, RLWServerError> {
        let jobs_arc = &*Arc::clone(&self.jobs);
        let job = Arc::clone(
            jobs_arc
                .lock()
                .map_err(|e| RLWServerError(format!("Lock poison error: {:?}", e)))?
                .get(uuid)
                .ok_or_else(|| {
                    RLWServerError("No job with the specified uuid exists".to_string())
                })?,
        );
        Ok(job)
    }
}
