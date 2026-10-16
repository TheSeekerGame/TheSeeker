mod particle_presentation;
mod fullscreen;
pub(crate) mod quad_mesh;
pub mod background_blur;
pub mod corpse_loot_particles;
mod dmg_numbers;
pub use dmg_numbers::NoDamageNumbers;
pub mod enemy_hp;
pub(crate) mod particles_util;
pub mod pixel_perfect;
pub mod pixel_view;
pub mod player_hp;
pub mod post_processing;
pub mod progression_particles;
pub mod projectile_particles;
pub mod ripple;
pub mod snow;

use bevy_hanabi::HanabiPlugin;
use post_processing::PostProcessingPlugin;
use theseeker_engine::effects::GhostingDefaults;

use crate::graphics::background_blur::BackgroundBlurPlugin;
use crate::graphics::corpse_loot_particles::CorpseLootParticlesPlugin;
use crate::graphics::dmg_numbers::DmgNumbersPlugin;
use crate::graphics::enemy_hp::EnemyHpBarPlugin;
use crate::graphics::pixel_perfect::{
    pixel_perfect_layers, PixelPerfectPlugin,
};
use crate::graphics::player_hp::LowHealthVignettePlugin;
use crate::graphics::progression_particles::ProgressionParticlesPlugin;
use crate::graphics::projectile_particles::AttackParticlesPlugin;
use crate::graphics::ripple::RippleFxPlugin;
use crate::graphics::snow::SnowPlugin;
use crate::prelude::*;

pub struct GraphicsFxPlugin;

impl Plugin for GraphicsFxPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(PostProcessingPlugin);
        app.add_plugins(DmgNumbersPlugin);
        app.add_plugins(LowHealthVignettePlugin);
        app.add_plugins(EnemyHpBarPlugin);
        app.add_plugins(BackgroundBlurPlugin);
        app.add_plugins(HanabiPlugin);
        app.add_plugins(particle_presentation::ParticlePresentationPlugin);
        app.add_plugins(AttackParticlesPlugin);
        app.add_plugins(CorpseLootParticlesPlugin);
        app.add_plugins(ProgressionParticlesPlugin);
        app.add_plugins(SnowPlugin);
        app.add_plugins(pixel_view::PixelViewPlugin);
        app.add_plugins(PixelPerfectPlugin);
        app.add_plugins(RippleFxPlugin);
        assemble_render_passes(app);

        // Configure all ghosts to render on the pixel-perfect layer by default
        app.insert_resource(GhostingDefaults {
            render_layers: Some(pixel_perfect_layers()),
        });
    }
}

fn assemble_render_passes(app: &mut App) {
    use bevy::core_pipeline::{schedule::Core2d, Core2dSystems};
    use bevy::render::RenderApp;
    let Some(render) = app.get_sub_app_mut(RenderApp) else {
        warn!("Render app unavailable; custom rendering passes disabled");
        return;
    };
    render.add_systems(Core2d, (
        pixel_perfect::pixel_composite_pass,
        post_processing::darkness::darkness_pass,
        post_processing::capsule_dim::capsule_dim_pass,
        ripple::ripple_pass,
        post_processing::vignette::vignette_pass,
    ).chain().in_set(Core2dSystems::PostProcess));
}

#[cfg(test)]
pub(crate) mod tests;
