pub mod ability_cooldown;
mod dmg_numbers;
pub use dmg_numbers::NoDamageNumbers;
pub mod enemy_hp;
pub(crate) mod particles_util;
pub mod player_hp;
pub mod post_processing;
pub mod projectile_particles;

use bevy_hanabi::HanabiPlugin;
use post_processing::PostProcessingPlugin;

use crate::graphics::ability_cooldown::AbilityCooldownPlugin;
use crate::graphics::dmg_numbers::DmgNumbersPlugin;
use crate::graphics::enemy_hp::EnemyHpBarPlugin;
use crate::graphics::player_hp::PlayerHpBarPlugin;
use crate::graphics::projectile_particles::AttackParticlesPlugin;
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
        app.add_plugins(AttackParticlesPlugin);
        // xp_smear removed per revert to stable particles
    }
}
