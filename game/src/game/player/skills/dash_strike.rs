
use super::types::{CooldownMode, CooldownSpec, SkillId};

use crate::game::player::states::{transition_action, DashStrike};

const DASH_STRIKE_COOLDOWN: CooldownSpec = CooldownSpec {
    min_ticks: 0,
    max_ticks: 192,
    mode: CooldownMode::RateBased,
};

pub(super) fn try_start_dash_strike_slot(
    ctx: &mut super::registry::SkillActivationContext,
    world: &mut super::activation::ActivationWorld,
) -> bool {
    let commands = &mut world.instant.commands;
    let cooldowns = &mut world.instant.cooldowns;
    // Only react on just-pressed or buffered presses for this slot
    if !(ctx.action_state.just_pressed(&ctx.slot)
        || ctx.buffer.check_buffered(ctx.slot, ctx.current_tick).is_some())
    {
        return false;
    }

    if ctx.ground.is_grounded {
        return false;
    }

    if !cooldowns.is_ready(ctx.entity, SkillId::DashStrike, ctx.current_tick) {
        return false;
    }

    let used_dir = super::dash::movement_skill_direction(
        ctx.move_dir,
        ctx.last_move_dir,
        ctx.facing,
    );

    ctx.buffer.clear_action(ctx.slot);

    let mut ds = DashStrike::new();
    ds.dir = used_dir;
    transition_action(commands, ctx.entity, ds);

    let cdr_snapshot = ctx.stats.map(|s| s.cdr).unwrap_or(1.0);
    cooldowns.start(
        ctx.entity,
        SkillId::DashStrike,
        DASH_STRIKE_COOLDOWN,
        cdr_snapshot,
        ctx.current_tick,
    );
    true
}
