use super::{DamageModifiers, PassiveContext, PassiveEffect};
use crate::game::player::PlayerStatMod;

/// Sharpshooter: damage scales with distance between player's last grounded position
/// and the target being damaged.
pub struct SharpshooterEffect;

impl PassiveEffect for SharpshooterEffect {
    fn modify_stats(
        &self,
        _stats: &mut PlayerStatMod,
        _context: &PassiveContext,
    ) {
        // No longer modifying stats globally, only per-target damage
    }

    fn modify_damage(
        &self,
        modifiers: &mut DamageModifiers,
        context: &PassiveContext,
    ) {
        // Calculate multiplier based on distance between last grounded position and target
        if let (Some(target_pos), Some(last_grounded_pos)) = (
            context.target_position,
            context.last_grounded_position,
        ) {
            let distance = target_pos.distance(last_grounded_pos);

            let mult = if distance >= 250.0 {
                4.0
            } else if distance >= 200.0 {
                3.5
            } else if distance >= 150.0 {
                3.0
            } else if distance >= 120.0 {
                2.5
            } else if distance >= 90.0 {
                2.0
            } else if distance >= 60.0 {
                1.5
            } else if distance >= 30.0 {
                1.25
            } else {
                1.0
            };

            modifiers.damage_multiplier *= mult;
        }
    }

    fn priority(&self) -> i32 {
        5
    }

    fn name(&self) -> &'static str {
        "Sharpshooter"
    }
}
