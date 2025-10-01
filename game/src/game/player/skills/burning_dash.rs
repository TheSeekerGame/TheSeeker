use bevy::prelude::*;
use leafwing_input_manager::prelude::*;

use super::cooldowns::Cooldowns;
use super::types::{BurningDashMetadata, CooldownMode, CooldownSpec, SkillId};
use crate::game::combat::Health;
use crate::game::player::{
    states::{transition_action, BurningDashing},
    InputBuffer, PlayerAction,
};

pub(crate) const BURNING_DASH_METADATA: BurningDashMetadata = BurningDashMetadata {
    min_health_to_start: 10,
    delayed_cooldown: CooldownSpec {
        min_ticks: 0,
        max_ticks: 48,
        mode: CooldownMode::RateBased,
    },
    animation_key: "anim.player.BurningDash",
};

/// Try to start a burning dash from a specific slot action.
/// Returns true if burning dash was initiated, false otherwise.
pub fn try_start_burning_dash_slot(
    entity: Entity,
    commands: &mut Commands,
    action_state: &ActionState<PlayerAction>,
    _buffer: &mut InputBuffer,
    health: &Health,
    stats: Option<&crate::game::player::PlayerStatMod>,
    slot_action: PlayerAction,
    now_tick: u64,
    cooldowns: &mut ResMut<Cooldowns>,
) -> bool {
    // Check if the slot action is pressed (this is what makes it channeled)
    if !action_state.pressed(&slot_action) {
        return false;
    }
    // Gate by delayed cooldown if present
    if !cooldowns.is_ready(entity, SkillId::BurningDash, now_tick) {
        return false;
    }

    // Check minimum health requirement
    if health.current < BURNING_DASH_METADATA.min_health_to_start {
        return false;
    }

    // Get speed modifier from stats
    let speed_mod = stats.map(|s| s.speed).unwrap_or(1.0);

    // Transition to BurningDashing state
    let mut burning_dashing = BurningDashing::new().with_speed_mod(speed_mod);
    burning_dashing.slot_action = Some(slot_action);

    transition_action(commands, entity, burning_dashing);

    true
}
