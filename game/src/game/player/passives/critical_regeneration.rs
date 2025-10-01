use super::{PassiveAction, PassiveContext, PassiveEffect, PassiveEvent};
use crate::game::player::PlayerStatMod;

/// Critical Regeneration: heal 24 HP on critical hits.
pub struct CriticalRegenerationEffect;

impl PassiveEffect for CriticalRegenerationEffect {
    fn modify_stats(
        &self,
        _stats: &mut PlayerStatMod,
        _context: &PassiveContext,
    ) {
    }

    fn on_event(
        &self,
        event: &PassiveEvent,
        _context: &PassiveContext,
    ) -> Vec<PassiveAction> {
        match event {
            PassiveEvent::CriticalHit { .. } => vec![PassiveAction::Heal(24)],
            _ => vec![],
        }
    }

    fn priority(&self) -> i32 {
        10
    }

    fn name(&self) -> &'static str {
        "CriticalRegeneration"
    }
}
