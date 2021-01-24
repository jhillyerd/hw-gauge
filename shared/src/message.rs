use defmt::Format;
use serde::{Deserialize, Serialize};

#[derive(Debug, Format, Serialize, Deserialize)]
pub enum FromHost {
    ClearScreen,
    ShowPerf(PerfData),
}

#[derive(Clone, Copy, Debug, Format, Serialize, Deserialize)]
pub struct PerfData {
    // Aggregate load of all CPU cores, 0-1.0;
    pub all_cores_load: f32,
    // Load on peak core, 0-1.0;
    pub peak_core_load: f32,
}
