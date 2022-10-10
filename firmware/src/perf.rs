use cortex_m::asm;
use defmt::{debug, error};
use fugit::ExtU32;
use shared::message::PerfData;

// Frames per second for interpolated display updates.
const FRAMES_PER_SECOND: u32 = 15;

// CPU bar fall-off rate in percentage points per second.
const FALL_PCT_PER_SECOND: f32 = 25.0;

const FRAME_MS: u32 = 1000 / FRAMES_PER_SECOND;
const FALL_FRAC_PER_FRAME: f32 = FALL_PCT_PER_SECOND / 100.0 / FRAMES_PER_SECOND as f32;

/// Represents the previous and current PerfData states.
pub struct State {
    pub previous: Option<PerfData>,
    pub current: Option<PerfData>,
}

/// Calculates what to display based on the previously stored state (input.previous),
/// and newly received state (input.current), if present.
///
/// result.previous should be stored for the next call, and result.current should be
/// rendered to the display.
///
/// This function also schedules a followup call to app::show_perf to render a follow-up
/// value before the next set of perf data is received from the host.
pub fn update_state(input: State) -> State {
    let mut update_type_descr = "None";

    let result: State = match (input.previous, input.current) {
        // Displays previous perf packet unaltered, as there is no new perf data.
        (Some(prev), None) => {
            update_type_descr = "Prev-Unalt";

            State {
                previous: Some(prev),
                current: Some(prev),
            }
        }

        // Displays new perf packet unaltered, as there is no history.
        (None, Some(new)) => {
            update_type_descr = "New-Unalt";

            State {
                previous: Some(new),
                current: Some(new),
            }
        }

        // Fresh data, sets up animated display from prev to target values.
        (Some(prev), Some(target)) => {
            update_type_descr = "Blended";

            // Schedule immediate & upcoming frames. Does not schedule frame at 1s, as that
            // is when the next PerfData packet should arrive from the host.
            let mut prev = prev;
            for frame in 0..FRAMES_PER_SECOND {
                // Calculate perf data for this frame, store in prev for basis of next frame.
                prev = PerfData {
                    all_cores_load: update_cpu_load(prev.all_cores_load, target.all_cores_load),
                    all_cores_avg: target.all_cores_avg,
                    peak_core_load: update_cpu_load(prev.peak_core_load, target.peak_core_load),
                    memory_load: target.memory_load,
                    daytime: target.daytime,
                };

                let delay = frame * FRAME_MS;

                if let Err(_) = crate::app::show_perf::spawn_after(delay.millis(), prev) {
                    error!("Failed to request show_perf::spawn_after");
                    asm::bkpt();
                }
            }

            // Return the last calculated frame to caller, instead of target, to continue
            // smooth animation to the from the last frame towards the next target.
            State {
                previous: Some(prev),
                current: None,
            }
        }

        // No data, this is expected during startup.
        _ => State {
            previous: None,
            current: None,
        },
    };

    debug!("Will display [{}]: {:?}", update_type_descr, result.current);

    result
}

// Approximates a VU-meter, jumps up quickly, falls slowly.
fn update_cpu_load(prev_load: f32, target_load: f32) -> f32 {
    if target_load > prev_load {
        // Jump to higher loads immediately.
        target_load
    } else {
        // Ease in to lower loads.
        // debug!("target: {}, prev: {}, fallcfg: {}", target_load, prev_load, FALL_FRAC_PER_FRAME);
        f32::max(target_load, prev_load - FALL_FRAC_PER_FRAME)
    }
}
