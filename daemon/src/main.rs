use postcard;
use serialport::{SerialPort, SerialPortInfo, SerialPortType};
use shared::message;
use std::io;
use std::thread;
use std::time::Duration;
use systemstat::{data::CPULoad, Platform, System};

const USB_VENDOR_ID: u16 = 0x1209; // pid.codes VID.
const USB_PRODUCT_ID: u16 = 0x0001; // In house private testing only.

const SEND_PERIOD: Duration = Duration::from_secs(1);
const CPU_POLL_PERIOD: Duration = Duration::from_secs(1);
const RETRY_DELAY: Duration = Duration::from_secs(10);

#[derive(Debug)]
enum Error {
    PortNotFound,
    IO(io::Error),
    Serial(serialport::Error),
}

fn main() {
    loop {
        println!(
            "Error: {:?}\nRetrying in {:?}...",
            detectsend_loop(),
            RETRY_DELAY
        );
        std::thread::sleep(RETRY_DELAY);
    }
}

fn detectsend_loop() -> Result<(), Error> {
    let pinfo = detect_port()?;
    let mut port = open_port(&pinfo)?;
    println!("Sending to detected device on port: {}", pinfo.port_name);

    loop {
        match write_perf_data(&mut port) {
            Ok(_) => {}
            Err(err) => {
                return Err(Error::IO(err));
            }
        }

        // TODO factor in start time for correct period.
        std::thread::sleep(SEND_PERIOD);
    }
}

/// Looks for our monitor hardware on available serial ports.
fn detect_port() -> Result<SerialPortInfo, Error> {
    // Detect serial port for monitor hardware.
    let ports = serialport::available_ports().map_err(Error::Serial)?;

    let port = ports.into_iter().find(|p| match &p.port_type {
        SerialPortType::UsbPort(info) => info.vid == USB_VENDOR_ID && info.pid == USB_PRODUCT_ID,
        _ => false,
    });

    port.ok_or(Error::PortNotFound)
}

/// Opens serial port, and sets DTR.
fn open_port(port_info: &SerialPortInfo) -> Result<Box<dyn SerialPort>, Error> {
    let mut port = serialport::new(port_info.port_name.clone(), 115200)
        .open()
        .map_err(Error::Serial)?;
    port.write_data_terminal_ready(true)
        .map_err(Error::Serial)?;

    Ok(port)
}

/// CPU load.
fn write_perf_data(w: &mut Box<dyn SerialPort>) -> io::Result<usize> {
    fn total_load(load: &CPULoad) -> f32 {
        1.0f32 - load.idle
    }

    // Capture CPU metrics.
    let sys = System::new();
    let cpu_load = sys.cpu_load().unwrap();
    let load_agg = sys.cpu_load_aggregate().unwrap();
    thread::sleep(CPU_POLL_PERIOD);
    let load_agg = load_agg.done().unwrap();

    // Select least idle core.
    let cpu_load = cpu_load.done().unwrap();
    let min_idle = cpu_load
        .iter()
        .min_by(|a, b| a.idle.partial_cmp(&b.idle).unwrap());

    let perf = message::PerfData {
        all_cores_load: total_load(&load_agg),
        peak_core_load: total_load(min_idle.unwrap_or(&load_agg)),
    };

    // Serialize into FromHost message.
    let msg = message::FromHost::ShowPerf(perf);
    let msg_bytes = postcard::to_allocvec_cobs(&msg).expect("COB serialization failed");

    w.write(&msg_bytes)
}
