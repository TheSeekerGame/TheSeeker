pub mod capsule_dim;
pub mod darkness;
pub mod foreground_fog;
pub mod vignette;

use bevy::prelude::*;
use capsule_dim::CapsuleDimPlugin;
use darkness::DarknessPlugin;
use foreground_fog::ForegroundFogPlugin;
use vignette::VignettePlugin;

pub struct PostProcessingPlugin;

impl Plugin for PostProcessingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            DarknessPlugin,
            CapsuleDimPlugin,
            ForegroundFogPlugin,
            VignettePlugin,
            // FloaterPlugin, // Disabled - relied on 3D pipeline
        ));
    }
}
