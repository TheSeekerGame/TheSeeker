pub mod darkness;
mod dmg_numbers;
mod fog;

use crate::graphics::darkness::DarknessPlugin;
use crate::graphics::dmg_numbers::DmgNumbersPlugin;
use crate::graphics::fog::FogPlugin;
use crate::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::sprite::Material2d;

pub struct GraphicsFxPlugin;

impl Plugin for GraphicsFxPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FogPlugin);
        app.add_plugins(DarknessPlugin);
        app.add_plugins(DmgNumbersPlugin);
    }
}
