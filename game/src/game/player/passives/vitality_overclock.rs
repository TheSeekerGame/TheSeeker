use super::{DamageModifiers, PassiveContext, PassiveEffect};
use crate::game::player::PlayerStatMod;

/// Vitality Overclock: damage scales with current HP% (HP drain handled elsewhere).
pub struct VitalityOverclockEffect;

impl PassiveEffect for VitalityOverclockEffect {
    fn modify_stats(
        &self,
        _stats: &mut PlayerStatMod,
        _context: &PassiveContext,
    ) {
        // Damage multiplier is applied in modify_damage to avoid double application
    }

    fn modify_damage(
        &self,
        modifiers: &mut DamageModifiers,
        context: &PassiveContext,
    ) {
        // Apply only in the first damage modification pass (not per-target) to avoid double application
        if context.target_position.is_none() {
            let ratio =
                context.health.current as f32 / context.health.max as f32;
            let multiplier = if ratio >= 0.9 {
                3.0
            } else if ratio >= 0.7 {
                2.0
            } else if ratio >= 0.5 {
                1.5
            } else if ratio >= 0.25 {
                1.25
            } else {
                1.0
            };
            modifiers.damage_multiplier *= multiplier;
        }
    }

    fn priority(&self) -> i32 {
        5
    }

    fn name(&self) -> &'static str {
        "VitalityOverclock"
    }
}
