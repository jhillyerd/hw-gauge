use avg::Averager;
use postcard;
use serialport::{SerialPort, SerialPortInfo, SerialPortType};
use shared::message;
use std::io;
use std::thread;
use std::time::Duration;
use systemstat::{data::CPULoad, Platform, System};

mod avg;

/// Delay between attempts to detect device USB Serial port.
pub const DETECT_RETRY_DELAY: Duration = Duration::from_secs(10);

const USB_VENDOR_ID: u16 = 0x1209; // pid.codes VID.
const USB_PRODUCT_ID: u16 = 0x0001; // In house private testing only.

const SEND_PERIOD: Duration = Duration::from_secs(1);
const CPU_POLL_PERIOD: Duration = Duration::from_secs(1);
const AVG_CPU_SAMPLES: usize = 15; // Seconds of data for CPU average.

#[derive(Debug)]
pub enum Error {
    PortNotFound,
    IO(io::Error),
    Serial(serialport::Error),
}

pub fn detectsend_loop() -> Result<(), Error> {
    let pinfo = detect_port()?;
    let mut port = open_port(&pinfo)?;
    println!("Sending to detected device on port: {}", pinfo.port_name);

    let mut cpu_avg = Averager::new(AVG_CPU_SAMPLES);
    loop {
        match write_perf_data(&mut port, &mut cpu_avg, daytime()) {
            Ok(_) => {}
            Err(err) => {
                return Err(Error::IO(err));
            }
        }

        // TODO factor in start time for correct period.
        std::thread::sleep(SEND_PERIOD - CPU_POLL_PERIOD);
    }
}

/// Returns true if local time is between 6am and 6pm.
fn daytime() -> bool {
    let now = time::OffsetDateTime::try_now_local();
    if let Ok(now) = now {
        return 6 < now.hour() && now.hour() < 18;
    }

    false
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
fn write_perf_data(
    w: &mut Box<dyn SerialPort>,
    cpu_avg: &mut Averager,
    daytime: bool,
) -> io::Result<usize> {
    fn busy_fraction(load: &CPULoad) -> f32 {
        1.0f32 - load.idle
    }

    // Capture CPU metrics.
    let sys = System::new();
    let cpu_load = sys.cpu_load().unwrap();
    let load_agg = sys.cpu_load_aggregate().unwrap();
    thread::sleep(CPU_POLL_PERIOD);

    // Load across all cores.
    let load_agg = load_agg.done().unwrap();

    // Select least idle core.
    let cpu_load = cpu_load.done().unwrap();
    let min_idle = cpu_load
        .iter()
        .min_by(|a, b| a.idle.partial_cmp(&b.idle).unwrap())
        .unwrap_or(&load_agg);

    // Average all cores load over time.
    let all_cores_load = busy_fraction(&load_agg);
    cpu_avg.add_sample(all_cores_load as f64);

    // Memory usage.
    let mem = sys.memory().unwrap();
    let memory_load = 1.0 - ((mem.free.as_u64() as f32) / (mem.total.as_u64() as f32));

    let perf = message::PerfData {
        all_cores_load: busy_fraction(&load_agg),
        all_cores_avg: cpu_avg.average().unwrap_or_default() as f32,
        peak_core_load: busy_fraction(&min_idle),
        memory_load,
        daytime,
    };

    // Serialize into FromHost message.
    let msg = message::FromHost::ShowPerf(perf);
    let msg_bytes = postcard::to_allocvec_cobs(&msg).expect("COB serialization failed");

    w.write(&msg_bytes)
}
