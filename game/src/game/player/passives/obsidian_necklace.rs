use super::{PassiveAction, PassiveContext, PassiveEffect, PassiveEvent};
use crate::game::player::PlayerStatMod;

/// Obsidian Necklace: on critical hit, reduce all cooldowns by 0.5s and add 0.5 energy to channeled skills.
pub struct ObsidianNecklaceEffect;

impl PassiveEffect for ObsidianNecklaceEffect {
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
        match *event {
            PassiveEvent::CriticalHit { .. } => vec![
                PassiveAction::ReduceCooldowns(0.5),
                // Grant energy to all channelled skills (Whirl, Flicker, etc.)
                PassiveAction::AddEnergy(0.5),
            ],
            // Deterministic crit rhythm contribution
            PassiveEvent::HitCountAdvanced { hit_count, .. } => {
                if hit_count % 23 == 0
                    || hit_count % 29 == 0
                    || hit_count % 31 == 0
                {
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
        "ObsidianNecklace"
    }
}
