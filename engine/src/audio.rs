use crate::prelude::*;
use crate::time::{GameTickSet, MasterClockSet};

mod custom;
mod device;
mod mixer;
pub mod transport;

pub use custom::{AudioClock, AudioEngine, AudioEngineConfig, AudioInstanceId,
    AudioPlaybackConfig, AudioSet, AudioStartTime};
pub use mixer::{AudioBus, MixerDiagnostics};

pub struct AudioPlugin;

#[cfg(test)]
pub(crate) fn test_engine(config: &AudioEngineConfig) -> AudioEngine { AudioEngine::build(config).0 }

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AudioEngineConfig>()
            .init_resource::<AudioClock>()
            .init_asset::<bevy::audio::AudioSource>()
            .init_asset_loader::<bevy::audio::AudioLoader>();
        let config = app.world().resource::<AudioEngineConfig>().clone();
        let (engine, stream) = custom::AudioEngine::build(&config);
        app.insert_resource(engine).insert_non_send(stream);
        app.configure_sets(Update, AudioSet::Flush.in_set(GameTickSet::Post));
        app.add_systems(Update, (
            (custom::update_audio_engine, custom::prepare_audio).chain().in_set(AudioSet::Update),
            custom::flush_audio.in_set(AudioSet::Flush).after(MasterClockSet::Update),
        ));
    }
}
