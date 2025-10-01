use super::{DamageModifiers, PassiveContext, PassiveEffect};
use crate::game::player::PlayerStatMod;

/// Heavy Boots: stationary → 2x damage, 2x defense; moving → 0.5x.
pub struct HeavyBootsEffect;

impl PassiveEffect for HeavyBootsEffect {
    fn modify_stats(
        &self,
        stats: &mut PlayerStatMod,
        context: &PassiveContext,
    ) {
        // Positive buff only when stationary (near-zero velocity or no movement input) and not jumping
        let is_stationary = context.velocity.length() < 0.01
            || context.movement_input.abs() < 0.01;
        let not_jumping = !context.jump_pressed;

        if is_stationary && not_jumping {
            stats.damage *= 2.0;
            stats.defense *= 2.0;
        } else {
            stats.damage *= 0.5;
            stats.defense *= 0.5;
        }
    }

    fn modify_damage(
        &self,
        _modifiers: &mut DamageModifiers,
        _context: &PassiveContext,
    ) {
        // Effect is player-state-based, not target-based; damage is already applied via modify_stats
    }

    fn priority(&self) -> i32 {
        5
    }

    fn name(&self) -> &'static str {
        "HeavyBoots"
    }
}
