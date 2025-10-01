use super::{PassiveAction, PassiveContext, PassiveEffect, PassiveEvent};
use crate::game::player::PlayerStatMod;

/// Rabbit's Foot: +20% speed and +1 extra mid-air jump. Crit rhythm contributed via events.
pub struct RabbitsFootEffect;

impl PassiveEffect for RabbitsFootEffect {
    fn modify_stats(
        &self,
        stats: &mut PlayerStatMod,
        _context: &PassiveContext,
    ) {
        stats.speed *= 1.2;
        stats.extra_jumps = stats.extra_jumps.saturating_add(1);
    }

    fn on_event(
        &self,
        event: &PassiveEvent,
        _context: &PassiveContext,
    ) -> Vec<PassiveAction> {
        match *event {
            // Deterministic crit rhythm contribution
            PassiveEvent::HitCountAdvanced { hit_count, .. } => {
                if hit_count % 37 == 0
                    || hit_count % 41 == 0
                    || hit_count % 43 == 0
                    || hit_count % 47 == 0
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
        "RabbitsFoot"
    }
}
