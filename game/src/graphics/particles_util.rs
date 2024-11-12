use crate::prelude::Handle;
use bevy_hanabi::EffectAsset;

pub trait BuildParticles {
    fn with_lingering_particles(&mut self, handle: Handle<EffectAsset>) -> &mut Self;
}
