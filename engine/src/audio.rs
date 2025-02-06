use bevy::audio::AddAudioSource;

use crate::prelude::*;

mod mixer;

pub use mixer::PreciseAudioId;

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<PrecisionMixerInstance>();
        app.add_systems(Startup, setup_precisionmixer);
    }
}

fn setup_precisionmixer(
    mut commands: Commands,
    mut ass: ResMut<Assets<PrecisionMixerInstance>>,
) {
    let controller = mixer::PrecisionMixerController::new(2, 48_000, 96.0);
    let handle = ass.add(PrecisionMixerInstance {
        controller: controller.clone(),
    });
    commands.spawn((
        PrecisionMixerControl {
            controller: controller.clone(),
        },
        AudioSourceBundle {
            source: handle,
            ..Default::default()
        },
    ));
}

#[derive(Component)]
pub struct PrecisionMixerControl {
    pub controller: Arc<mixer::PrecisionMixerController>,
}

#[derive(Asset, TypePath)]
struct PrecisionMixerInstance {
    controller: Arc<mixer::PrecisionMixerController>,
}

impl Decodable for PrecisionMixerInstance {
    type Decoder = mixer::PrecisionMixer;
    type DecoderItem = mixer::MySample;

    fn decoder(&self) -> Self::Decoder {
        mixer::PrecisionMixer::new(self.controller.clone())
    }
}

#[derive(Component)]
pub struct LabeledBackgroundSound {
    pub label: String,
}
