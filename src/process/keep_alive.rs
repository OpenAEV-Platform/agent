use crate::api::Client;
use crate::config::execution_details::ExecutionDetails;
use crate::THREADS_CONTROL;
use log::{error, info};
use std::io::Error;
use std::sync::atomic::Ordering;
use std::thread;
use std::thread::{sleep, JoinHandle};
use std::time::Duration;

pub fn ping(
    uri: String,
    token: String,
    unsecured_certificate: bool,
    with_proxy: bool,
    installation_mode: String,
    service_name: String,
    execution_details: ExecutionDetails,
    tenant_id: String,
) -> Result<JoinHandle<()>, Error> {
    info!("Starting ping thread");
    let api = Client::new(uri, token, unsecured_certificate, with_proxy);
    let handle = thread::spawn(move || {
        // While no stop signal received
        while THREADS_CONTROL.load(Ordering::Relaxed) {
            // Register, ping the agent
            let register = api.register_agent(
                execution_details.is_service,
                execution_details.is_elevated,
                execution_details.executed_by_user.clone(),
                installation_mode.clone(),
                service_name.clone(),
                tenant_id.clone(),
            );
            if register.is_err() {
                error!("Fail registering the agent {}", register.unwrap_err())
            }
            // Wait for the next ping (2 minutes)
            sleep(Duration::from_secs(120));
        }
    });
    Ok(handle)
}
