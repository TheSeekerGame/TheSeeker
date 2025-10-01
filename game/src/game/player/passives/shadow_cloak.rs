use super::{PassiveContext, PassiveEffect};
use crate::game::player::PlayerStatMod;

/// Shadow Cloak: behavior is implemented by systems in stealth/damage pipelines.
/// This PassiveEffect intentionally does not modify stats or damage directly,
/// but exists to keep skill and animation-slot plumbing consistent.
pub struct ShadowCloakEffect;

impl PassiveEffect for ShadowCloakEffect {
    fn modify_stats(
        &self,
        _stats: &mut PlayerStatMod,
        _context: &PassiveContext,
    ) {
    }

    fn name(&self) -> &'static str {
        "Shadow Cloak"
    }
}
