use super::{DamageModifiers, PassiveContext, PassiveEffect};
use crate::game::player::PlayerStatMod;

/// Pack Killer: Damage scales based on enemy clustering.
/// - +30% damage per enemy near the target (within 24 pixel radius)
/// - -50% damage to isolated enemies (no other enemies nearby)
///
/// This encourages targeting tightly clustered enemies and makes single-target
/// weapons like the bow more effective against dense groups.
pub struct PackKillerEffect;

impl PassiveEffect for PackKillerEffect {
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
        // Only apply when we have target position (per-target processing in apply_damage)
        if context.target_position.is_some() {
            let enemies_nearby = context.enemies_near_target;

            if enemies_nearby == 0 {
                // Isolated enemy: -50% damage
                modifiers.damage_multiplier *= 0.5;
            } else {
                // Clustered enemies: +30% per nearby enemy
                let bonus = 1.0 + (enemies_nearby as f32 * 0.30);
                modifiers.damage_multiplier *= bonus;
            }
        }
    }

    fn priority(&self) -> i32 {
        5
    }

    fn name(&self) -> &'static str {
        "PackKiller"
    }
}
