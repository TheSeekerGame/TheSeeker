use bevy::prelude::*;

use super::types::{CooldownMode, CooldownSpec, DashMetadata, SkillId};
use crate::game::player::states::skill::dashing::DASH_VELOCITY;
use theseeker_engine::physics::LinearVelocity;

use crate::game::gentstate::Facing;
use crate::game::player::states::{transition_action, Dashing};

pub(crate) const DASH_METADATA: DashMetadata = DashMetadata {
    duration_ticks: 6,
    cooldown: CooldownSpec {
        min_ticks: 0,
        max_ticks: 96,
        mode: CooldownMode::RateBased,
    },
    animation_key: "anim.player.Dash",
};

pub(super) fn try_start_dash_slot(
    ctx: &mut super::registry::SkillActivationContext,
    world: &mut super::activation::ActivationWorld,
) -> bool {
    let commands = &mut world.instant.commands;
    let cooldowns = &mut world.instant.cooldowns;
    if !(ctx.action_state.just_pressed(&ctx.slot)
        || ctx.buffer.check_buffered(ctx.slot, ctx.current_tick).is_some())
    {
        return false;
    }

    if !cooldowns.is_ready(ctx.entity, SkillId::Dash, ctx.current_tick) {
        return false;
    }

    // Build dashing state
    let mut dashing = Dashing::new();
    // Provide behavioral lifetime from local tuning (not cooldown spec)
    dashing = dashing.with_max_ticks(DASH_METADATA.duration_ticks);
    // Capture speed modifier at dash initiation
    let speed_mod = ctx.stats.map(|s| s.speed).unwrap_or(1.0);
    dashing = dashing.with_speed_mod(speed_mod);

    let used_dir = movement_skill_direction(ctx.move_dir, ctx.last_move_dir, ctx.facing);
    dashing.dir = used_dir;

    ctx.buffer.clear_action(ctx.slot);
    // Seed velocity immediately so collision stage sees non-zero movement this tick
    {
        let dir = used_dir;
        commands.entity(ctx.entity).insert(LinearVelocity(Vec2::new(
            dir * DASH_VELOCITY * speed_mod,
            0.0,
        )));
    }
    // Insert dash as an action state; while active it overrides locomotion
    transition_action(commands, ctx.entity, dashing);

    // Stamp cooldown now that dash triggered successfully.
    let cdr_snapshot = ctx.stats.map(|s| s.cdr).unwrap_or(1.0);
    cooldowns.start(
        ctx.entity,
        SkillId::Dash,
        DASH_METADATA.cooldown,
        cdr_snapshot,
        ctx.current_tick,
    );
    true
}

pub(super) fn movement_skill_direction(
    move_dir: f32,
    last_move_dir: f32,
    facing: &Facing,
) -> f32 {
    [move_dir, last_move_dir]
        .into_iter()
        .find(|direction| direction.abs() > f32::EPSILON)
        .map(f32::signum)
        .unwrap_or_else(|| facing.direction())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_input_controls_dash_direction() {
        assert_eq!(
            movement_skill_direction(-1.0, 1.0, &Facing::Right),
            -1.0
        );
    }

    #[test]
    fn neutral_input_uses_last_movement_direction() {
        assert_eq!(
            movement_skill_direction(0.0, -1.0, &Facing::Right),
            -1.0
        );
    }

    #[test]
    fn facing_is_used_before_any_movement_has_occurred() {
        assert_eq!(
            movement_skill_direction(0.0, 0.0, &Facing::Left),
            -1.0
        );
    }
}
