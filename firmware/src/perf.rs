use defmt::{error, info};
use heapless::Deque;
use shared::message::PerfData;

// Frames per second for interpolated display updates.
const FRAMES_PER_SECOND: u32 = 15;

// CPU bar fall-off rate in percentage points per second.
const FALL_PCT_PER_SECOND: f32 = 70.0;

/// Delay between animation frames in millseconds.
pub const FRAME_MS: u32 = 1000 / FRAMES_PER_SECOND;

const FALL_FRAC_PER_FRAME: f32 = FALL_PCT_PER_SECOND / 100.0 / FRAMES_PER_SECOND as f32;

// Frames of perf data queued for display.
pub type FramesDeque = Deque<PerfData, 64>;

/// Calculates what to display based on the previously stored state and new target state,
/// if present.
///
/// The returned PerfData should be stored as a basis for rendering future frames,
/// as it represents the final frame expected to be displayed on screen.
pub fn update_state(
    previous: Option<PerfData>,
    target: PerfData,
    frames: &mut FramesDeque,
) -> Option<PerfData> {
    match previous {
        // Displays new perf packet unaltered, as there is no history.
        None => {
            frames.push_back(target).ok();
            Some(target)
        }

        // Fresh data, sets up animated display from prev to target values.
        Some(prev) => {
            // Remove any unrendered frames in case of time skew with host.
            if !frames.is_empty() {
                // Under light load, this should be be 0 or 1.
                info!(
                    "Frame queue had {} unused entries at state update",
                    frames.len()
                );
                frames.clear();
            }

            // Generate upcoming frames. Does not schedule frame at 1s, as that
            // is when the next PerfData packet should arrive from the host.
            let mut prev = prev;
            for _ in 0..FRAMES_PER_SECOND {
                // Calculate perf data for this frame, store in prev for basis of next frame.
                prev = PerfData {
                    all_cores_load: update_cpu_load(prev.all_cores_load, target.all_cores_load),
                    all_cores_avg: target.all_cores_avg,
                    peak_core_load: update_cpu_load(prev.peak_core_load, target.peak_core_load),
                    memory_load: target.memory_load,
                    daytime: target.daytime,
                };

                if let Err(_) = frames.push_back(prev) {
                    error!("Frame queue is full");
                    break;
                }
            }

            // Return the last calculated frame to caller, instead of target, to continue
            // smooth animation to the from the last frame towards the next target.
            Some(prev)
        }
    }
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
