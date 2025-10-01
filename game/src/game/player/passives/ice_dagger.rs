use super::{DamageModifiers, PassiveContext, PassiveEffect};
use crate::game::player::PlayerStatMod;

/// Ice Dagger: enables backstab (2x damage when hitting from behind).
pub struct IceDaggerEffect;

impl PassiveEffect for IceDaggerEffect {
    fn modify_stats(
        &self,
        _stats: &mut PlayerStatMod,
        _context: &PassiveContext,
    ) {
    }

    fn modify_damage(
        &self,
        modifiers: &mut DamageModifiers,
        _context: &PassiveContext,
    ) {
        modifiers.can_backstab = true;
    }

    fn on_event(
        &self,
        event: &super::PassiveEvent,
        _context: &PassiveContext,
    ) -> Vec<super::PassiveAction> {
        match event {
            // When a backstab kills an enemy, refresh all skill cooldowns and fully refill energy
            super::PassiveEvent::BackstabKill { .. } => vec![
                super::PassiveAction::ResetCooldowns,
                super::PassiveAction::RefillEnergyFull,
            ],
            _ => vec![],
        }
    }

    fn priority(&self) -> i32 {
        10
    }

    fn name(&self) -> &'static str {
        "IceDagger"
    }
}
