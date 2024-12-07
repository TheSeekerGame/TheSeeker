pub mod darkness;
pub mod vignette;

use bevy::prelude::*;
use darkness::DarknessPlugin;
use vignette::VignettePlugin;

pub struct PostProcessingPlugin;

impl Plugin for PostProcessingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((DarknessPlugin, VignettePlugin));
    }
}
