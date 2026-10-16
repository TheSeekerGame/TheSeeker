
use super::types::{CooldownMode, CooldownSpec, SkillId};
use crate::game::player::states::{transition_action, Spitting};

/// Cooldown uses 72 ticks at 96 Hz.
const SPIT_COOLDOWN: CooldownSpec = CooldownSpec {
    min_ticks: 0,
    max_ticks: 72,
    mode: CooldownMode::RateBased,
};

pub(super) fn try_start_spit_slot(
    ctx: &mut super::registry::SkillActivationContext,
    world: &mut super::activation::ActivationWorld,
) -> bool {
    let commands = &mut world.instant.commands;
    let cooldowns = &mut world.instant.cooldowns;
    // Must be grounded to use Spit
    if !ctx.ground.is_grounded {
        return false;
    }

    // Check for input (fresh press or buffered)
    if !(ctx.action_state.just_pressed(&ctx.slot)
        || ctx.buffer.check_buffered(ctx.slot, ctx.current_tick).is_some())
    {
        return false;
    }

    // Cooldown gate
    if !cooldowns.is_ready(ctx.entity, SkillId::Spit, ctx.current_tick) {
        return false;
    }

    // Enter the Spitting state and block locomotion
    transition_action(commands, ctx.entity, Spitting::new(*ctx.facing));

    // Start cooldown
    let cdr_snapshot = ctx.stats.map(|s| s.cdr).unwrap_or(1.0);
    cooldowns.start(
        ctx.entity,
        SkillId::Spit,
        SPIT_COOLDOWN,
        cdr_snapshot,
        ctx.current_tick,
    );

    // Clear buffered input
    ctx.buffer.clear_action(ctx.slot);
    true
}
