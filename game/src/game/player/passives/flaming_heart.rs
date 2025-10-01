use super::{PassiveAction, PassiveContext, PassiveEffect, PassiveEvent};
use crate::game::player::PlayerStatMod;

/// Flaming Heart: when HP < 25%, schedule a critical on every 2nd or 3rd hit.
pub struct FlamingHeartEffect;

impl PassiveEffect for FlamingHeartEffect {
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
        match *event {
            PassiveEvent::HitCountAdvanced { hit_count, .. } => {
                let low_hp = context.health.current < context.health.max / 4;
                if low_hp && (hit_count % 2 == 0 || hit_count % 3 == 0) {
                    return vec![PassiveAction::ScheduleNextCrit];
                }
                vec![]
            },
            _ => vec![],
        }
    }

    fn priority(&self) -> i32 {
        10
    }

    fn name(&self) -> &'static str {
        "FlamingHeart"
    }
}
