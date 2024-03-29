//! API implementation to be used by the server

use crate::rlwp::Job;
use crate::server::user::User;
use crate::utils::errors::{RLWServerError, GENERAL_SERVER_ERR, NO_UUID_JOB};
use crate::utils::job_processor_api::{job_processor_service_server::JobProcessorService, *};
use crate::utils::tls;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc as tokio_mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

// TODO: Update this to be a more appropriate size:
//
// Run multiple client streaming tests to determine the maximum number of items
// that appeared in the buffer at once. Set the STREAM_BUFFER_SIZE to be
// 1.5 times larger than this.
const STREAM_BUFFER_SIZE: usize = 4096;

/// Handles the incoming gRPC job process requests and
/// stores user data related to these requests
pub struct JobProcessor {
    // Stores all users and their jobs.
    // Maps usernames to users structs.
    user_table: Arc<Mutex<HashMap<String, Arc<User>>>>,
}

#[tonic::async_trait]
/// gRPC Service Implementation
impl JobProcessorService for JobProcessor {
    /// Start a new process job
    async fn start(
        &self,
        request: Request<StartRequest>,
    ) -> Result<Response<StartResponse>, Status> {
        log::info!("Start Request");

        // Get User
        let user_id = tls::get_authenticated_user_id(&request)?;
        let user = self.get_user(&user_id).map_err(|e| {
            log::error!("Start request error: {:?}", e);
            Status::unknown(GENERAL_SERVER_ERR)
        })?;

        // Start Job
        let req = request.into_inner();
        let uuid = user
            .start_new_job(req.command, req.arguments)
            .map_err(|e| {
                log::error!("Start request error: {:?}", e);
                Status::unknown(GENERAL_SERVER_ERR)
            })?;
        Ok(Response::new(StartResponse { uuid }))
    }

    /// Stop a running process job
    async fn stop(&self, request: Request<StopRequest>) -> Result<Response<()>, Status> {
        log::info!("Stop Request");
        let user_id = tls::get_authenticated_user_id(&request)?;

        // Get Job
        let req = request.into_inner();
        let job = self.get_users_job(&user_id, &req.uuid).map_err(|e| {
            log::error!("Stop Request Error {:?}", e);
            Status::not_found(NO_UUID_JOB)
        })?;

        // Stop Job
        job.stop_command(req.forced).await.map_err(|e| {
            log::error!("Stop Request Error: {:?}", e);
            Status::unknown(GENERAL_SERVER_ERR)
        })?;
        Ok(Response::new(()))
    }

    type StreamStream = ReceiverStream<Result<StreamResponse, Status>>;
    /// Stream all previous and upcoming stderr and stdout data for a specified job
    async fn stream(
        &self,
        request: tonic::Request<StreamRequest>,
    ) -> Result<tonic::Response<Self::StreamStream>, tonic::Status> {
        log::info!("Stream Request");
        let user_id = tls::get_authenticated_user_id(&request)?;

        // Get Job
        let req = request.into_inner();
        let job = self.get_users_job(&user_id, &req.uuid).map_err(|e| {
            log::error!("Stream Request Error {:?}", e);
            Status::not_found(NO_UUID_JOB)
        })?;

        // Initialize a channel, send the client the receiver and forward
        // job output through the sender.
        let (tx, rx) = tokio_mpsc::channel(STREAM_BUFFER_SIZE);
        tokio::task::spawn(async {
            if let Err(e) = job.stream_job(tx).await {
                log::error!("Stream Request Error {:?}", e);
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    /// Return the status of a previous process job
    async fn status(
        &self,
        request: Request<StatusRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        log::info!("Status Request");
        let user_id = tls::get_authenticated_user_id(&request)?;

        // Get job
        let req = request.into_inner();
        let job = self.get_users_job(&user_id, &req.uuid).map_err(|e| {
            log::error!("Stream Request Error {:?}", e);
            Status::not_found(NO_UUID_JOB)
        })?;

        // Get status
        let status = job.status.lock().await.clone();
        Ok(Response::new(StatusResponse {
            process_status: Some(status),
        }))
    }
}

impl JobProcessor {
    /// Creates a new job processor. This should only be done once per server instance.
    pub fn new() -> Self {
        Self {
            user_table: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Useful function that returns a job from a user id and job uuid.
    ///
    /// # Arguments
    /// * `user_id` - id/email parsed through client certificate
    /// * `uuid`     - job uuid obtained from start request.
    #[allow(clippy::needless_question_mark)]
    fn get_users_job(&self, user_id: &str, uuid: &str) -> Result<Arc<Job>, RLWServerError> {
        let user = self.get_user(user_id)?;
        Ok(user.get_job(uuid)?)
    }

    /// Returns user object associated with user_id
    ///
    /// # Arguments
    /// * `user_id` - id/email parsed through client certificate
    fn get_user(&self, user_id: &str) -> Result<Arc<User>, RLWServerError> {
        let mut table = self
            .user_table
            .lock()
            .map_err(|e| RLWServerError(format!("Lock poison error: {:?}", e)))?;

        // Insert new user
        if !table.contains_key(user_id) {
            table.insert(String::from(user_id), Arc::new(User::new()));
        }

        // Get and return user
        let user = table
            .get(user_id)
            .ok_or_else(|| RLWServerError("Unable to find user in table".to_string()))?;
        Ok(Arc::clone(user))
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    const TESTING_SCRIPTS_DIR: &str = "../scripts/";

    /// Tests the starting a new job
    #[tokio::test(flavor = "multi_thread")]
    async fn test_start_request() -> Result<(), RLWServerError> {
        // Setup
        let job_processor = JobProcessor::new();
        let command = "/bin/bash".to_string();
        let arguments = vec![TESTING_SCRIPTS_DIR.to_string() + "start_request.sh"];

        // Start request
        let mock_request = Request::new(StartRequest { command, arguments });
        job_processor
            .start(mock_request)
            .await
            .map_err(|e| RLWServerError(format!("Start request error: {:?}", e)))?;

        Ok(())
    }

    /// Tests the starting and stopping of a single job
    #[tokio::test(flavor = "multi_thread")]
    async fn test_stop_request() -> Result<(), RLWServerError> {
        // Setup
        let job_processor = Arc::new(JobProcessor::new());
        let command = "/bin/bash".to_string();
        let arguments = vec![TESTING_SCRIPTS_DIR.to_string() + "start_request.sh"];

        // Start request
        let mock_start_request = Request::new(StartRequest { command, arguments });
        let uuid = job_processor
            .start(mock_start_request)
            .await
            .map_err(|e| RLWServerError(format!("Start request error: {:?}", e)))?
            .into_inner()
            .uuid;

        // Arbitrary time between start and stop request
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        // Stop Request
        let mock_stop_request = Request::new(StopRequest {
            uuid,
            forced: false,
        });
        job_processor
            .stop(mock_stop_request)
            .await
            .map_err(|e| RLWServerError(format!("Stop request error: {:?}", e)))?;
        Ok(())
    }

    /// Tests the status request of a new job
    ///
    /// Process:
    /// 1. Start a job.
    /// 2. Query the status to ensure it is running.
    /// 3. Stop the job.
    /// 4. Query the status to ensure it stopped with Signal(15).  
    #[tokio::test(flavor = "multi_thread")]
    async fn test_status_request() -> Result<(), RLWServerError> {
        // Setup
        let job_processor = Arc::new(JobProcessor::new());
        let command = "/bin/bash".to_string();
        let arguments = vec![TESTING_SCRIPTS_DIR.to_string() + "start_request.sh"];

        // Start request
        let mock_start_request = Request::new(StartRequest { command, arguments });
        let uuid = job_processor
            .start(mock_start_request)
            .await
            .map_err(|e| RLWServerError(format!("Start request error: {:?}", e)))?
            .into_inner()
            .uuid;
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        // First status request
        let mock_status_request = Request::new(StatusRequest { uuid: uuid.clone() });
        let status = job_processor
            .status(mock_status_request)
            .await
            .map_err(|e| RLWServerError(format!("Status request error: {:?}", e)))?
            .into_inner();

        assert_eq!(
            status.process_status,
            Some(status_response::ProcessStatus::Running(true))
        );

        // Stop request
        let mock_stop_request = Request::new(StopRequest {
            uuid: uuid.clone(),
            forced: false,
        });
        job_processor
            .stop(mock_stop_request)
            .await
            .map_err(|e| RLWServerError(format!("Stop request error: {:?}", e)))?;

        // Give the process time to shutdown
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

        // Second status request
        let mock_status_request2 = Request::new(StatusRequest { uuid });
        let status = job_processor
            .status(mock_status_request2)
            .await
            .map_err(|e| RLWServerError(format!("Status request error: {:?}", e)))?
            .into_inner();

        assert_eq!(
            status.process_status,
            Some(status_response::ProcessStatus::Signal(15))
        );
        Ok(())
    }
}
