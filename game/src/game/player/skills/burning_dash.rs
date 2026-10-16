
use super::types::{BurningDashMetadata, CooldownMode, CooldownSpec, SkillId};
use crate::game::player::{
    states::{transition_action, BurningDashing},
};

pub(crate) const BURNING_DASH_METADATA: BurningDashMetadata =
    BurningDashMetadata {
        min_health_to_start: 10,
        delayed_cooldown: CooldownSpec {
            min_ticks: 0,
            max_ticks: 48,
            mode: CooldownMode::RateBased,
        },
        animation_key: "anim.player.BurningDash",
    };

pub(super) fn try_start_burning_dash_slot(
    ctx: &mut super::registry::SkillActivationContext,
    world: &mut super::activation::ActivationWorld,
) -> bool {
    let commands = &mut world.instant.commands;
    let cooldowns = &mut world.instant.cooldowns;
    // Check if the slot action is pressed (this is what makes it channeled)
    if !ctx.action_state.pressed(&ctx.slot) {
        return false;
    }
    // Gate by delayed cooldown if present
    if !cooldowns.is_ready(ctx.entity, SkillId::BurningDash, ctx.current_tick) {
        return false;
    }

    // Check minimum health requirement
    if ctx.health.current < BURNING_DASH_METADATA.min_health_to_start {
        return false;
    }

    // Get speed modifier from stats
    let speed_mod = ctx.stats.map(|s| s.speed).unwrap_or(1.0);

    // Transition to BurningDashing state
    let mut burning_dashing = BurningDashing::new().with_speed_mod(speed_mod);
    burning_dashing.slot_action = Some(ctx.slot);

    transition_action(commands, ctx.entity, burning_dashing);

    true
}
