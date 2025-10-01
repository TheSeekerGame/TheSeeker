use bevy::prelude::*;

use theseeker_engine::physics::PhysicsWorld;

use super::cooldowns::Cooldowns;
use super::types::{CooldownMode, CooldownSpec, SkillId, SpinnerMetadata};
use crate::game::gentstate::Facing;
use crate::game::player::input_buffer::InputBuffer;
use crate::game::player::PlayerAction;

pub(crate) const SPINNER_METADATA: SpinnerMetadata = SpinnerMetadata {
    cooldown: CooldownSpec {
        min_ticks: 0,
        max_ticks: 5760,
        mode: CooldownMode::RateBased,
    },
};

/// Try to deploy the Spinner skill: instantaneous spawn, can be used in air/ground, parallel with other skills.
pub fn try_start_spinner_slot(
    entity: Entity,
    commands: &mut Commands,
    action_state: &leafwing_input_manager::action_state::ActionState<
        PlayerAction,
    >,
    buffer: &mut InputBuffer,
    player_transform: &Transform,
    facing: &Facing,
    now_tick: u64,
    cooldowns: &mut ResMut<Cooldowns>,
    stats: Option<&crate::game::player::PlayerStatMod>,
    _spatial_query: &PhysicsWorld,
    slot_action: PlayerAction,
) -> bool {
    // Input gate: just pressed or buffered
    if !(action_state.just_pressed(&slot_action)
        || buffer.check_buffered(slot_action, now_tick).is_some())
    {
        return false;
    }

    // Cooldown gate
    if !cooldowns.is_ready(entity, SkillId::Spinner, now_tick) {
        return false;
    }

    // Spawn spinner slightly in front of player in facing direction
    let dir = facing.direction();
    let spawn_offset = Vec2::new(10.0 * dir, 0.0);
    let spawn_pos = player_transform.translation.truncate() + spawn_offset;
    crate::game::player::spawns::spinner::spawn_spinner(
        commands, entity, spawn_pos, dir,
    );

    let cdr_snapshot = stats.map(|s| s.cdr).unwrap_or(1.0);
    cooldowns.start(
        entity,
        SkillId::Spinner,
        SPINNER_METADATA.cooldown,
        cdr_snapshot,
        now_tick,
    );

    // Clear consumed input
    buffer.clear_action(slot_action);
    true
}
