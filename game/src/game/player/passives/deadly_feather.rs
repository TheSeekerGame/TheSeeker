use super::{PassiveAction, PassiveContext, PassiveEffect, PassiveEvent};
use crate::game::player::PlayerStatMod;

/// Deadly Feather: airborne → +30% CDR and crit rhythm; grounded → 50% defense.
pub struct DeadlyFeatherEffect;

impl PassiveEffect for DeadlyFeatherEffect {
    fn modify_stats(
        &self,
        stats: &mut PlayerStatMod,
        context: &PassiveContext,
    ) {
        if context.in_air {
            stats.cdr *= 1.3;
        } else {
            stats.defense *= 0.5;
        }
    }

    fn on_event(
        &self,
        event: &PassiveEvent,
        context: &PassiveContext,
    ) -> Vec<PassiveAction> {
        match *event {
            PassiveEvent::HitCountAdvanced { hit_count, .. } => {
                // Only apply crit rhythm when airborne
                if context.in_air
                    && (hit_count % 5 == 0
                        || hit_count % 7 == 0
                        || hit_count % 11 == 0
                        || hit_count % 13 == 0)
                {
                    return vec![PassiveAction::ScheduleNextCrit];
                }
                vec![]
            },
            _ => vec![],
        }
    }

    fn priority(&self) -> i32 {
        5
    }

    fn name(&self) -> &'static str {
        "DeadlyFeather"
    }
}
