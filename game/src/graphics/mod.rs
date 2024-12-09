mod dmg_numbers;
pub mod dof;
mod fog;
pub mod hp_bar;
pub(crate) mod particles_util;
pub mod post_processing;

use bevy_hanabi::HanabiPlugin;
// use post_processing::DarknessPlugin;
use post_processing::PostProcessingPlugin;

use crate::graphics::dmg_numbers::DmgNumbersPlugin;
use crate::graphics::dof::DepthOfFieldPlugin;
use crate::graphics::hp_bar::HpBarsPlugin;
use crate::prelude::*;

pub struct GraphicsFxPlugin;

impl Plugin for GraphicsFxPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(PostProcessingPlugin);
        app.add_plugins(DmgNumbersPlugin);
        app.add_plugins(HpBarsPlugin);
        app.add_plugins(HanabiPlugin);
        app.add_plugins(DepthOfFieldPlugin);
    }
}
