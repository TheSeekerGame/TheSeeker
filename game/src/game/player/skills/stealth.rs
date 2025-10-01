use bevy::prelude::*;

use super::cooldowns::Cooldowns;
use super::types::{CooldownMode, CooldownSpec, SkillId, StealthMetadata};

use crate::game::player::PlayerAction;

pub(crate) const STEALTH_METADATA: StealthMetadata = StealthMetadata {
    cooldown: CooldownSpec {
        min_ticks: 19,
        max_ticks: (5.0 * 96.0) as u32,
        mode: CooldownMode::RateBased,
    },
};

/// Try to start Stealth: gate via cooldowns and insert the `StealthEffect`.
pub fn try_start_stealth_slot(
    entity: Entity,
    commands: &mut Commands,
    action_state: &leafwing_input_manager::action_state::ActionState<
        PlayerAction,
    >,
    now_tick: u64,
    cooldowns: &mut ResMut<Cooldowns>,
    _stats: Option<&crate::game::player::PlayerStatMod>,
    slot_action: PlayerAction,
) -> bool {
    if !action_state.just_pressed(&slot_action) {
        return false;
    }

    // Delayed cooldown model: gate by cooldown readiness, but start cooldown when stealth ends
    if !cooldowns.is_ready(entity, SkillId::Stealth, now_tick) {
        return false;
    }

    // Insert the effect; visuals/speed handled by the effect systems
    // Use a hard tick deadline to ensure expiry is robust.
    commands
        .entity(entity)
        .insert(crate::game::effects::stealthed::StealthEffect::new(now_tick));

    true
}
