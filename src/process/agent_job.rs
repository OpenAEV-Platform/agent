use log::{error, info};
use std::io::Error;
use std::sync::atomic::{Ordering};
use std::thread;
use std::thread::{JoinHandle, sleep};
use std::time::Duration;
use crate::api::Client;
use crate::config::execution_details::ExecutionDetails;
use crate::process::agent_exec;
use crate::THREADS_CONTROL;

pub fn listen(uri: String, token: String, unsecured_certificate: bool, with_proxy: bool, is_service: bool) -> Result<JoinHandle<()>, Error> {
    info!("Starting listening jobs thread");
    let api = Client::new(uri, token, unsecured_certificate, with_proxy);
    let execution_details = ExecutionDetails::new(is_service).unwrap();
    let handle = thread::spawn(move || {
        // While no stop signal received
        while THREADS_CONTROL.load(Ordering::Relaxed) {
            let jobs = api.list_jobs(execution_details.is_service.clone(), execution_details.is_elevated.clone(), execution_details.executed_by_user.clone());
            if jobs.is_ok() {
                jobs.unwrap().iter().for_each(|j| {
                    info!("Start handling inject: {:?}", j.asset_agent_inject);
                    // 01. Remove the execution job
                    info!("Cleaning job: {:?}", j.asset_agent_id);
                    let clean_result = api.clean_job(j.asset_agent_id.as_str());
                    // 02. Execute the command
                    if clean_result.is_ok() {
                        let _ = agent_exec::command_execution(j.asset_agent_id.as_str(), j.asset_agent_command.as_str());
                    }
                    info!("Done handling inject: {:?}", j.asset_agent_inject);
                });
            } else {
                error!("Fail getting jobs {}", jobs.unwrap_err())
            }
            // Wait for the next ping (30 secs)
            sleep(Duration::from_secs(30));
        }
    });
    Ok(handle)
}