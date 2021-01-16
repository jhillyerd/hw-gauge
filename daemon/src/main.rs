use std::thread;
use std::time::Duration;
use systemstat::{data::CPULoad, Platform, System};

fn main() {
    let sys = System::new();
    let cpu_load = sys.cpu_load().unwrap();
    let load_agg = sys.cpu_load_aggregate().unwrap();

    thread::sleep(Duration::from_secs(1));

    let load_agg = load_agg.done().unwrap();
    println!("aggregate: {:.2}", total_load(&load_agg) * 100.0);

    let cpu_load = cpu_load.done().unwrap();
    let min_idle = cpu_load
        .iter()
        .min_by(|a, b| a.idle.partial_cmp(&b.idle).unwrap());
    if let Some(min_idle) = min_idle {
        println!("peak core: {:.2}", total_load(min_idle) * 100.0);
    }
}

fn total_load(load: &CPULoad) -> f32 {
    1.0f32 - load.idle
}
