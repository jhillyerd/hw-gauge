use lib;
use once_cell::sync::Lazy;
use std::ffi::OsString;
use std::sync::Mutex;
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

#[derive(PartialEq)]
enum RunMode {
    Run,
    Stop,
}

struct ServiceContext {
    run_mode: RunMode,
}

static CONTEXT: Lazy<Mutex<ServiceContext>> = Lazy::new(|| {
    Mutex::new(ServiceContext {
        run_mode: RunMode::Run,
    })
});

define_windows_service!(ffi_service_main, service_main);

fn main() -> Result<(), windows_service::Error> {
    // Register generated `ffi_service_main` with the system and start the service, blocking
    // this thread until the service is stopped.
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)?;
    Ok(())
}

fn service_main(_args: Vec<OsString>) {
    if let Err(e) = service_wrapper() {
        panic!("{}", e);
    }
}

fn service_wrapper() -> Result<(), windows_service::Error> {
    // Setup status tracking mutex and service event callback.
    let event_handler = |control_event| -> ServiceControlHandlerResult {
        match control_event {
            ServiceControl::Stop => {
                let mut context = CONTEXT
                    .lock()
                    .expect("Failed to lock context while handling stop event");
                context.run_mode = RunMode::Stop;
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };
    let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;

    // Notify windows we have started.
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
                eprintln!(
                    "Error: {:?}\nRetrying in {:?}...",
                    e,
                    lib::DETECT_RETRY_DELAY
                );
            }
        }
        std::thread::sleep(lib::DETECT_RETRY_DELAY);
    }

    // Notify windows we have stopped.
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
