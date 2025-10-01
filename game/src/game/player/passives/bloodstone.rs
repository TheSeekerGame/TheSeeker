/// Bloodstone: heal 2 HP on enemy kill or XP orb pickup.
use super::{PassiveAction, PassiveContext, PassiveEffect, PassiveEvent};
use crate::game::player::PlayerStatMod;

pub struct BloodstoneEffect;

impl PassiveEffect for BloodstoneEffect {
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
            PassiveEvent::EnemyKilled { .. } => {
                vec![PassiveAction::Heal(2)]
            },
            PassiveEvent::XpOrbPickup => {
                vec![PassiveAction::Heal(2)]
            },
            _ => vec![],
        }
    }

    fn name(&self) -> &'static str {
        "Bloodstone"
    }
}
