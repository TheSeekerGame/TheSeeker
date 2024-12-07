use theseeker_engine::audio::PrecisionMixerControl;

use crate::prelude::*;

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, manage_audio_delay);
    }
}

#[derive(Default)]
struct DelayManagerState {
    hyst_counter: u8,
    last_gt: u64,
    last_at: u64,
    target: u64,
}

fn manage_audio_delay(
    gt: Res<GameTime>,
    q_mixer: Query<&PrecisionMixerControl>,
    mut state: Local<DelayManagerState>,
) {
    let Ok(ctl) = q_mixer.get_single() else {
        return;
    };
    let sample = ctl.controller.sample_count();
    let samples_per_tick = ctl.controller.sample_rate() as f64 / gt.hz;
    let atick = (sample as f64 / samples_per_tick) as u64;

    let gt_step = gt.tick() - state.last_gt;
    let at_step = atick.max(state.last_at) - state.last_at;
    if gt_step < state.target && at_step < state.target {
        state.hyst_counter += 1;
        if state.hyst_counter == 16 {
            state.target -= 1;
            state.hyst_counter = 0;
        }
    } else {
        state.hyst_counter = 0;
    }
    if gt_step > state.target || at_step > state.target {
        state.target += 1;
    }

    state.last_gt = gt.tick();
    state.last_at = atick;

    let range_max = gt.tick();
    let range_min = gt.tick().max(state.target * 2) - state.target * 2;

    if (atick > range_max || atick < range_min) && !ctl.controller.has_playing()
    {
        eprintln!("AUDIO RESET");
        let new_atick = gt.tick().max(state.target) - state.target;
        ctl.controller
            .reset_sample_counter(new_atick as i64 * samples_per_tick as i64);
        state.last_at = new_atick;
    }
}
