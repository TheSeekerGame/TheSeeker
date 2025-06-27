pub mod darkness;
pub mod vignette;

use bevy::asset::load_internal_asset;
use bevy::prelude::*;
// use floaters::FloaterPlugin; // Deleted file
use darkness::DarknessPlugin;
use vignette::VignettePlugin;

pub struct PostProcessingPlugin;

impl Plugin for PostProcessingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            VignettePlugin,
            DarknessPlugin,
            // FloaterPlugin, // Disabled - relied on 3D pipeline
        ));
    }
}
