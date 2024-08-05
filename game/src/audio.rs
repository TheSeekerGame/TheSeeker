use theseeker_engine::audio::PrecisionMixerControl;

use crate::prelude::*;

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), reset_audio);
    }
}

fn reset_audio(
    t: Res<Time>,
    q_mixer: Query<&PrecisionMixerControl>,
) {
    const AUDIO_DELAY_MS: i32 = 250;
    let ctl = q_mixer.single();
    ctl.controller.trigger_reset(-(t.elapsed().as_millis() as i32 - AUDIO_DELAY_MS));
}
