pub mod ability_cooldown;
mod dmg_numbers;
pub mod dof;
pub mod enemy_hp;
mod fog;
pub(crate) mod particles_util;
pub mod player_hp;
pub mod post_processing;

use bevy_hanabi::HanabiPlugin;
// use post_processing::DarknessPlugin;
use post_processing::PostProcessingPlugin;

use crate::graphics::ability_cooldown::AbilityCooldownPlugin;
use crate::graphics::dmg_numbers::DmgNumbersPlugin;
use crate::graphics::dof::DepthOfFieldPlugin;
use crate::graphics::enemy_hp::EnemyHpBarPlugin;
use crate::graphics::player_hp::PlayerHpBarPlugin;
use crate::prelude::*;

pub struct GraphicsFxPlugin;

impl Plugin for GraphicsFxPlugin {
    fn build(&self, app: &mut App) {
        // app.add_plugins(DepthOfFieldPlugin); // Commented out to disable Depth of Field
        app.add_plugins(PostProcessingPlugin); // Commented out to disable post-processing
        app.add_plugins(DmgNumbersPlugin);
        app.add_plugins(PlayerHpBarPlugin);
        app.add_plugins(EnemyHpBarPlugin);
        app.add_plugins(AbilityCooldownPlugin);
        app.add_plugins(HanabiPlugin);
    }
}
