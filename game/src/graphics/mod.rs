pub mod darkness;
mod dmg_numbers;
mod fog;
pub mod hp_bar;
pub(crate) mod particles_util;

use crate::game::enemy::Enemy;
use crate::graphics::darkness::DarknessPlugin;
use crate::graphics::dmg_numbers::DmgNumbersPlugin;
use crate::graphics::fog::FogPlugin;
use crate::graphics::hp_bar::HpBarsPlugin;
use crate::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::sprite::Material2d;
use bevy_hanabi::{HanabiPlugin, ParticleEffect};
use bevy_inspector_egui::quick::FilterQueryInspectorPlugin;

pub struct GraphicsFxPlugin;

impl Plugin for GraphicsFxPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FogPlugin);
        app.add_plugins(DarknessPlugin);
        app.add_plugins(DmgNumbersPlugin);
        app.add_plugins(HpBarsPlugin);
        app.add_plugins(HanabiPlugin);
    }
}
