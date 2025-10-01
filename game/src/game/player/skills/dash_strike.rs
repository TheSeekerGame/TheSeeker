use bevy::prelude::*;

use super::cooldowns::Cooldowns;
use super::types::{
    DashStrikeMetadata,
    DashStrikeVariantMetadata,
    SkillId,
    Variant,
};

use crate::game::gentstate::Facing;
use crate::game::player::input_buffer::{InputBuffer, InputVariant};
use crate::game::player::states::{transition_action, DashStrike};
use crate::game::player::PlayerAction;

pub(crate) const DASH_STRIKE_METADATA: DashStrikeMetadata = DashStrikeMetadata {
    cooldown: super::types::CooldownSpec {
        min_ticks: 0,
        max_ticks: 192,
        mode: super::types::CooldownMode::RateBased,
    },
    animation_key: "anim.player.SwordDashStrike",
    variants: [
        DashStrikeVariantMetadata {
            state_duration_ticks: 16,
        },
        DashStrikeVariantMetadata {
            state_duration_ticks: 16,
        },
        DashStrikeVariantMetadata {
            state_duration_ticks: 16,
        },
    ],
};

/// Try to start the Dash Strike skill: choose variant, stamp cooldown, capture direction, and enter `DashStrike`.
pub fn try_start_dash_strike_slot(
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
    slot_action: PlayerAction,
    is_grounded: bool,
) -> bool {
    // Only react on just-pressed or buffered presses for this slot
    if !(action_state.just_pressed(&slot_action)
        || buffer.check_buffered(slot_action, now_tick).is_some())
    {
        return false;
    }

    // Select variant: Up when a modifier is present, otherwise default to Down
    let buffered = buffer.check_buffered(slot_action, now_tick);
    let variant = if let Some(buffered) = buffered {
        match buffered.variant_modifier {
            InputVariant::Up => Variant::Up,
            _ => Variant::Down,
        }
    } else if action_state.pressed(&PlayerAction::UpModifier)
        || action_state.pressed(&PlayerAction::Jump)
    {
        Variant::Up
    } else {
        Variant::Down
    };

    // Downward dash strike requires being airborne
    if is_grounded && matches!(variant, Variant::Down) {
        return false;
    }

    if !cooldowns.is_ready(entity, SkillId::DashStrike, now_tick) {
        return false;
    }

    // Choose horizontal direction from last movement input; fallback to facing
    // We avoid using facing directly when inputs are neutral for consistency with design
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

    buffer.clear_action(slot_action);

    // Build DashStrike state with captured dir and variant
    let mut ds = DashStrike::new(variant);
    ds.dir = used_dir;
    transition_action(commands, entity, ds);

    let cdr_snapshot = stats.map(|s| s.cdr).unwrap_or(1.0);
    cooldowns.start(
        entity,
        SkillId::DashStrike,
        DASH_STRIKE_METADATA.cooldown,
        cdr_snapshot,
        now_tick,
    );
    true
}
