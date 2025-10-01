use super::{PassiveAction, PassiveContext, PassiveEffect, PassiveEvent};
use crate::game::player::skills::types::SkillId;
use crate::game::player::PlayerStatMod;

/// Pulse Drive: Dash Strike critical hits trigger all equipped instant skills.
pub struct PulseDriveEffect;

impl PassiveEffect for PulseDriveEffect {
    fn modify_stats(
        &self,
        _stats: &mut PlayerStatMod,
        _context: &PassiveContext,
    ) {
        // No persistent stat adjustments; effect is event-driven.
    }

    fn on_event(
        &self,
        event: &PassiveEvent,
        _context: &PassiveContext,
    ) -> Vec<PassiveAction> {
        match event {
            PassiveEvent::CriticalHit {
                damage_source,
                source_skill: Some(SkillId::DashStrike),
                ..
            } => vec![PassiveAction::TriggerInstantSkills {
                source_damage: Some(*damage_source),
            }],
            _ => vec![],
        }
    }

    fn priority(&self) -> i32 {
        5
    }

    fn name(&self) -> &'static str {
        "PulseDrive"
    }
}
