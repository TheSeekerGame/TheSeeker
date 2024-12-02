pub mod darkness;
mod dmg_numbers;
pub mod dof;
mod fog;
pub mod hp_bar;
pub(crate) mod particles_util;

use bevy_hanabi::HanabiPlugin;

use crate::graphics::dmg_numbers::DmgNumbersPlugin;
use crate::graphics::dof::DepthOfFieldPlugin;
use crate::graphics::hp_bar::HpBarsPlugin;
use crate::prelude::*;

pub struct GraphicsFxPlugin;

impl Plugin for GraphicsFxPlugin {
    fn build(&self, app: &mut App) {
        // app.add_plugins(FogPlugin);
        // app.add_plugins(DarknessPlugin);
        app.add_plugins(DmgNumbersPlugin);
        app.add_plugins(HpBarsPlugin);
        app.add_plugins(HanabiPlugin);
        app.add_plugins(DepthOfFieldPlugin);
    }
}
