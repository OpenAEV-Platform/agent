use super::Client;
use crate::common::error_model::Error;
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize)]
pub struct JobResponse {
    pub asset_agent_id: String,
    pub asset_agent_inject: Option<String>,
    #[allow(dead_code)]
    pub asset_agent_agent: String,
    pub asset_agent_command: String,
}

impl Client {
    pub fn list_jobs(
        &self,
        is_service: bool,
        is_elevated: bool,
        executed_by_user: String,
    ) -> Result<Vec<JobResponse>, Error> {
        // Post the input to the OpenAEV API
        let post_data = json!({
          "asset_external_reference": mid::get("openaev").unwrap(),
          "agent_is_service": is_service,
          "agent_is_elevated": is_elevated,
          "agent_executed_by_user": executed_by_user
        });
        match self.post("/api/endpoints/jobs").json(&post_data).send() {
            Ok(response) => {
                if response.status().is_success() {
                    response
                        .json::<Vec<JobResponse>>()
                        .map_err(|e| Error::Internal(e.to_string()))
                } else {
                    let msg = response
                        .text()
                        .unwrap_or_else(|_| "Unknown error".to_string());
                    Err(Error::Api(msg))
                }
            }
            Err(err) => Err(Error::Internal(err.to_string())),
        }
    }
    pub fn clean_job(&self, job_id: &str) -> Result<(), Error> {
        // Post the input to the OpenAEV API
        match self.delete(&format!("/api/endpoints/jobs/{job_id}")).send() {
            Ok(response) => {
                if response.status().is_success() {
                    Ok(())
                } else {
                    let msg = response
                        .text()
                        .unwrap_or_else(|_| "Unknown error".to_string());
                    Err(Error::Api(msg))
                }
            }
            Err(err) => Err(Error::Internal(err.to_string())),
        }
    }
}
