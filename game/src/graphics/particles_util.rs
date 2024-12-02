use bevy_hanabi::EffectAsset;

use crate::prelude::Handle;

pub trait BuildParticles {
    fn with_lingering_particles(
        &mut self,
        handle: Handle<EffectAsset>,
    ) -> &mut Self;
}
