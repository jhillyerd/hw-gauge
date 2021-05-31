use lib;
use log::{debug, error, info};
use std::ffi::OsString;
use std::fs::File;
use std::time::Duration;
use windows_service::{
    define_windows_service,
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
};

// Windows service parameters.
const SERVICE_NAME: &str = "hw-cpu";
const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;
const LOG_FILE: &str = "hw-cpu-service.log";

define_windows_service!(ffi_service_main, service_main);

fn main() -> Result<(), windows_service::Error> {
    // Register generated `ffi_service_main` with the system and start the service, blocking
    // this thread until the service is stopped.
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)?;
    Ok(())
}

fn service_main(_args: Vec<OsString>) {
    init_logging();

    if let Err(e) = service_wrapper() {
        error!("{}", e);
        panic!("{}", e);
    }
}

fn service_wrapper() -> Result<(), windows_service::Error> {
    // Setup status tracking mutex and service event callback.
    let event_handler = |control_event| -> ServiceControlHandlerResult {
        debug!(
            "Received Windows service control event: {:?}",
            control_event
        );
        match control_event {
            ServiceControl::Stop => {
                lib::stop();
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };
    let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;

    debug!("Notifying Windows that the service has started");
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
        match lib::detectsend_loop() {
            Ok(()) => break,
            Err(e) => {
                error!("{:?}", e);
                info!("Retrying in {:?}", lib::DETECT_RETRY_DELAY);
            }
        }
        std::thread::sleep(lib::DETECT_RETRY_DELAY);
    }

    debug!("Notifying Windows that the service has stopped");
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

/// Opens the log file in TEMP/TMP and registers a global logger.
fn init_logging() {
    let mut path = std::env::temp_dir();
    path.push(LOG_FILE);
    let file = File::create(path).expect("Failed to create log file");
    let _ = simplelog::WriteLogger::init(
        simplelog::LevelFilter::Info,
        simplelog::Config::default(),
        file,
    );
}
