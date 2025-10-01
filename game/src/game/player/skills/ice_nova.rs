use bevy::prelude::*;

use crate::game::player::input_buffer::InputBuffer;
use crate::game::player::skills::cooldowns::Cooldowns;
use crate::game::player::skills::types::{CooldownMode, CooldownSpec, IceNovaMetadata, SkillId};
use crate::game::player::spawns::ice_nova::spawn_ice_nova;
use crate::game::player::{PlayerAction, PlayerStatMod};

pub(crate) const ICE_NOVA_METADATA: IceNovaMetadata = IceNovaMetadata {
    cooldown: CooldownSpec {
        min_ticks: 0,
        max_ticks: 900,
        mode: CooldownMode::RateBased,
    },
};

/// Try to activate the Ice Nova skill from a toolbar slot.
/// Instantly spawns the nova visual and schedules the burst logic.
pub fn try_start_ice_nova_slot(
    entity: Entity,
    commands: &mut Commands,
    action_state: &leafwing_input_manager::action_state::ActionState<
        PlayerAction,
    >,
    buffer: &mut InputBuffer,
    transform: &Transform,
    now_tick: u64,
    cooldowns: &mut ResMut<Cooldowns>,
    stats: Option<&PlayerStatMod>,
    slot_action: PlayerAction,
) -> bool {
    // Allow either a fresh press or buffered input (captured during other states)
    if !(action_state.just_pressed(&slot_action)
        || buffer.check_buffered(slot_action, now_tick).is_some())
    {
        return false;
    }

    // Cooldown gate
    if !cooldowns.is_ready(entity, SkillId::IceNova, now_tick) {
        return false;
    }

    // Spawn the nova at the player's current location
    spawn_ice_nova(
        commands,
        entity,
        transform.translation.truncate(),
    );

    let cdr_snapshot = stats.map(|s| s.cdr).unwrap_or(1.0);
    cooldowns.start(
        entity,
        SkillId::IceNova,
        ICE_NOVA_METADATA.cooldown,
        cdr_snapshot,
        now_tick,
    );

    // Clear buffered input if it was consumed
    buffer.clear_action(slot_action);
    true
}
