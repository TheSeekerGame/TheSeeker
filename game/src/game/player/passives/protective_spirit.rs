use super::{PassiveAction, PassiveContext, PassiveEffect, PassiveEvent};
use crate::game::player::PlayerStatMod;

/// Protective Spirit: cap incoming damage at one third of max HP (via compensating heal).
pub struct ProtectiveSpiritEffect;

impl PassiveEffect for ProtectiveSpiritEffect {
    fn modify_stats(
        &self,
        _stats: &mut PlayerStatMod,
        _context: &PassiveContext,
    ) {
    }

    fn on_event(
        &self,
        event: &PassiveEvent,
        context: &PassiveContext,
    ) -> Vec<PassiveAction> {
        match event {
            PassiveEvent::DamageTaken(info) => {
                let max_allowed = (context.health.max as f32) / 3.0;
                if info.amount > max_allowed {
                    let capped = max_allowed.max(0.0);
                    let actual = info.amount;
                    let excess = (actual - capped).round() as u32;
                    if excess > 0 {
                        return vec![PassiveAction::Heal(excess)];
                    }
                }
                vec![]
            },
            _ => vec![],
        }
    }

    fn priority(&self) -> i32 {
        20
    }

    fn name(&self) -> &'static str {
        "ProtectiveSpirit"
    }
}
