use super::{DamageModifiers, PassiveContext, PassiveEffect};
use crate::game::player::PlayerStatMod;

/// Prime Sygil: Deal double damage to enemies above 80% health.
pub struct PrimeSygilEffect;

impl PassiveEffect for PrimeSygilEffect {
    fn modify_stats(
        &self,
        _stats: &mut PlayerStatMod,
        _context: &PassiveContext,
    ) {
    }

    fn modify_damage(
        &self,
        modifiers: &mut DamageModifiers,
        context: &PassiveContext,
    ) {
        if let Some(pct) = context.target_health_pct {
            if pct > 0.80 {
                modifiers.damage_multiplier *= 2.0;
            }
        }
    }

    fn priority(&self) -> i32 {
        5
    }

    fn name(&self) -> &'static str {
        "Prime Sygil"
    }
}
