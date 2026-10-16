use bevy::prelude::*;

pub mod chilled;
pub mod frozen;
pub mod stealthed;

/// Top-level plugin for shared gameplay effects.
pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(frozen::FrozenEffectPlugin);
    }
}
