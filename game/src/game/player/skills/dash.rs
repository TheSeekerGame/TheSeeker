use bevy::prelude::*;

use super::cooldowns::Cooldowns;
use super::types::{CooldownMode, CooldownSpec, DashMetadata, SkillId};
use crate::game::player::states::skill::dashing::DASH_VELOCITY;
use theseeker_engine::physics::LinearVelocity;

use crate::game::gentstate::Facing;
use crate::game::player::input_buffer::InputBuffer;
use crate::game::player::states::{transition_action, Dashing};
use crate::game::player::PlayerAction;

pub(crate) const DASH_METADATA: DashMetadata = DashMetadata {
    duration_ticks: 6,
    cooldown: CooldownSpec {
        min_ticks: 0,
        max_ticks: 96,
        mode: CooldownMode::RateBased,
    },
    animation_key: "anim.player.Dash",
    overrides_locomotion: true,
};

/// Try to start the Dash skill: choose variant, stamp cooldown, seed velocity, and enter `Dashing`.
pub fn try_start_dash_slot(
    entity: Entity,
    commands: &mut Commands,
    action_state: &leafwing_input_manager::action_state::ActionState<
        PlayerAction,
    >,
    buffer: &mut InputBuffer,
    facing: &Facing,
    now_tick: u64,
    cooldowns: &mut ResMut<Cooldowns>,
    stats: Option<&crate::game::player::PlayerStatMod>,
    autoaim_decisive: bool,
    slot_action: PlayerAction,
) -> bool {
    if !(action_state.just_pressed(&slot_action)
        || buffer.check_buffered(slot_action, now_tick).is_some())
    {
        return false;
    }

    let _ = autoaim_decisive; // Unused parameter; reserved for future behavior refinement

    if !cooldowns.is_ready(entity, SkillId::Dash, now_tick) {
        return false;
    }

    // Build dashing state
    let mut dashing = Dashing::new();
    // Provide behavioral lifetime from local tuning (not cooldown spec)
    dashing = dashing.with_max_ticks(DASH_METADATA.duration_ticks);
    // Capture speed modifier at dash initiation
    let speed_mod = stats.map(|s| s.speed).unwrap_or(1.0);
    dashing = dashing.with_speed_mod(speed_mod);

    // Choose dash horizontal direction: prefer active input over facing (fixes auto-aim flipping).
    let input_raw = action_state.clamped_value(&PlayerAction::Move);
    let input_dir = if input_raw > 0.0 {
        1.0
    } else if input_raw < 0.0 {
        -1.0
    } else {
        0.0
    };
    let used_dir = if input_dir != 0.0 {
        input_dir
    } else {
        facing.direction()
    };
    dashing.dir = used_dir;

    buffer.clear_action(slot_action);
    // Seed velocity immediately so collision stage sees non-zero movement this tick
    {
        let dir = used_dir;
        commands.entity(entity).insert(LinearVelocity(Vec2::new(
            dir * DASH_VELOCITY * speed_mod,
            0.0,
        )));
    }
    // Insert dash as an action state; while active it overrides locomotion
    transition_action(commands, entity, dashing);

    // Stamp cooldown now that dash triggered successfully.
    let cdr_snapshot = stats.map(|s| s.cdr).unwrap_or(1.0);
    cooldowns.start(
        entity,
        SkillId::Dash,
        DASH_METADATA.cooldown,
        cdr_snapshot,
        now_tick,
    );
    true
}
