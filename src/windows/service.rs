use std::error::Error;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[cfg(windows)]
pub mod service_stub {
    use super::Result;
    use std::ffi::OsString;
    use std::sync::mpsc::channel;
    use std::sync::mpsc::RecvTimeoutError;
    use std::time::Duration;
    use windows_service::define_windows_service;
    use windows_service::service::ServiceControl;
    use windows_service::service::ServiceControlAccept;
    use windows_service::service::ServiceExitCode;
    use windows_service::service::ServiceState;
    use windows_service::service::ServiceStatus;
    use windows_service::service::ServiceType;
    use windows_service::service_control_handler::ServiceControlHandlerResult;
    use windows_service::service_control_handler::{self};
    use windows_service::service_dispatcher;
    use windows_service_detector::is_running_as_windows_service;

    const SERVICE_NAME: &str = "OAEVAgentService";
    const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;

    pub fn is_windows_service() -> bool {
        is_running_as_windows_service().unwrap_or(false)
    }

    pub fn run() -> Result<()> {
        service_dispatcher::start(SERVICE_NAME, ffi_service_main).map_err(|e| e.into())
    }
    define_windows_service!(ffi_service_main, service_main);

    pub fn service_main(_args: Vec<OsString>) {
        if let Err(_e) = run_service() {}
    }

    pub fn run_service() -> Result<()> {
        let (shutdown_tx, shutdown_rx) = channel();

        let event_handler = move |control_event| -> ServiceControlHandlerResult {
            match control_event {
                ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
                ServiceControl::Stop => {
                    shutdown_tx.send(()).unwrap();
                    ServiceControlHandlerResult::NoError
                }
                _ => ServiceControlHandlerResult::NotImplemented,
            }
        };

        let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;
        status_handle.set_service_status(ServiceStatus {
            service_type: SERVICE_TYPE,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;

        loop {
            match shutdown_rx.recv_timeout(Duration::from_secs(1)) {
                Ok(_) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => (),
            };
        }

        status_handle.set_service_status(ServiceStatus {
            service_type: SERVICE_TYPE,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;

        Ok(())
    }
}

#[cfg(not(target_os = "windows"))]
pub mod service_stub {
    use super::Result;

    pub fn run() -> Result<()> {
        Ok(())
    }

    pub fn is_windows_service() -> bool {
        false
    }
}
