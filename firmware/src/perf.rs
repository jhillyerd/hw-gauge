use defmt::debug;
use fugit::ExtU32;
use shared::message::PerfData;

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

        // Displays average of new and previous perf packets.
        (Some(prev), Some(new)) => {
            update_type_descr = "Averaged";

            // Schedule display of unaltered packet.
            crate::app::show_perf::spawn_after(500.millis(), None).ok();

            State {
                previous: Some(new),
                current: Some(PerfData {
                    all_cores_load: update_cpu_load(prev.all_cores_load, new.all_cores_load),
                    all_cores_avg: new.all_cores_avg,
                    peak_core_load: update_cpu_load(prev.peak_core_load, new.peak_core_load),
                    memory_load: new.memory_load,
                    daytime: new.daytime,
                }),
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

// Averages previous and new CPU loads together.
fn update_cpu_load(prev_load: f32, new_load: f32) -> f32 {
    (prev_load + new_load) / 2.0
}
