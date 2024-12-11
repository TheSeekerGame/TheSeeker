pub mod ability_cooldown;
mod dmg_numbers;
pub mod dof;
pub mod enemy_hp_bar;
mod fog;
pub(crate) mod particles_util;
pub mod player_hp_bar;
pub mod post_processing;

use bevy_hanabi::HanabiPlugin;
// use post_processing::DarknessPlugin;
use post_processing::PostProcessingPlugin;

use crate::graphics::ability_cooldown::AbilityCooldownPlugin;
use crate::graphics::dmg_numbers::DmgNumbersPlugin;
use crate::graphics::dof::DepthOfFieldPlugin;
use crate::graphics::enemy_hp_bar::EnemyHpBarPlugin;
use crate::graphics::player_hp_bar::PlayerHpBarPlugin;
use crate::prelude::*;

pub struct GraphicsFxPlugin;

impl Plugin for GraphicsFxPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(PostProcessingPlugin);
        app.add_plugins(DmgNumbersPlugin);
        app.add_plugins(PlayerHpBarPlugin);
        app.add_plugins(EnemyHpBarPlugin);
        app.add_plugins(AbilityCooldownPlugin);
        app.add_plugins(HanabiPlugin);
        app.add_plugins(DepthOfFieldPlugin);
    }
}
