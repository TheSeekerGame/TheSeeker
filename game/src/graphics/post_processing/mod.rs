pub mod darkness;
pub mod floaters;
pub mod vignette;

use bevy::prelude::*;
use darkness::DarknessPlugin;
use floaters::FloaterPlugin;
use vignette::VignettePlugin;

pub struct PostProcessingPlugin;

impl Plugin for PostProcessingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            DarknessPlugin,
            VignettePlugin,
            // FloaterPlugin,
        ));
    }
}
