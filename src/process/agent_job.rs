use crate::api::Client;
use crate::config::execution_details::ExecutionDetails;
use crate::process::agent_exec;
use crate::THREADS_CONTROL;
use log::{error, info};
use std::io::Error;
use std::sync::atomic::Ordering;
use std::thread;
use std::thread::{sleep, JoinHandle};
use std::time::Duration;

pub fn listen(
    uri: String,
    token: String,
    unsecured_certificate: bool,
    with_proxy: bool,
    execution_details: ExecutionDetails,
    tenant_id: String,
) -> Result<JoinHandle<()>, Error> {
    info!("Starting listening jobs thread");
    let api = Client::new(uri, token, unsecured_certificate, with_proxy);
    let handle = thread::spawn(move || {
        // While no stop signal received
        while THREADS_CONTROL.load(Ordering::Relaxed) {
            let jobs = api.list_jobs(
                execution_details.is_service,
                execution_details.is_elevated,
                execution_details.executed_by_user.clone(),
                tenant_id.clone(),
            );
            match jobs {
                Ok(jobs) => {
                    jobs.iter().for_each(|j| {
                        info!("Start handling inject: {:?}", j.asset_agent_inject);
                        // 01. Remove the execution job
                        info!("Cleaning job: {:?}", j.asset_agent_id);
                        let clean_result =
                            api.clean_job(j.asset_agent_id.as_str(), tenant_id.clone());
                        // 02. Execute the command
                        if clean_result.is_ok() {
                            let _ = agent_exec::command_execution(
                                j.asset_agent_id.as_str(),
                                j.asset_agent_command.as_str(),
                            );
                        }
                        info!("Done handling inject: {:?}", j.asset_agent_inject);
                    });
                }
                Err(err) => {
                    error!("Fail getting jobs {}", err)
                }
            }
            // Wait for the next ping (30 secs)
            sleep(Duration::from_secs(30));
        }
    });
    Ok(handle)
}
