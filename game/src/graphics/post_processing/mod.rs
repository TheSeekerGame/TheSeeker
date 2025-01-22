pub mod darkness;
pub mod floaters;
pub mod vignette;

use bevy::asset::load_internal_asset;
use bevy::prelude::*;
use darkness::DarknessPlugin;
use floaters::FloaterPlugin;
use vignette::VignettePlugin;

pub const PERLIN_3D_SHADER_HANDLE: Handle<Shader> =
    Handle::weak_from_u128(307185293026626584719943815439941085085);

pub struct PostProcessingPlugin;

impl Plugin for PostProcessingPlugin {
    fn build(&self, app: &mut App) {
        // Load general shaders
        load_internal_asset!(
            app,
            PERLIN_3D_SHADER_HANDLE,
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/shaders/perlin_noise_3d.wgsl"
            ),
            Shader::from_wgsl
        );

        app.add_plugins((
            DarknessPlugin,
            VignettePlugin,
            FloaterPlugin,
        ));
    }
}
