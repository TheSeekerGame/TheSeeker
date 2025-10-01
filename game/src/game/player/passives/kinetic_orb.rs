use super::{PassiveContext, PassiveEffect};
use crate::game::player::PlayerStatMod;

/// Kinetic Orb: behavior is implemented in spawns/kinetic_orb runtime.
/// This PassiveEffect is a no-op for stat/damage mods, but provides a name for debug consistency.
pub struct KineticOrbEffect;

impl PassiveEffect for KineticOrbEffect {
    fn modify_stats(
        &self,
        _stats: &mut PlayerStatMod,
        _context: &PassiveContext,
    ) {
    }
    fn name(&self) -> &'static str {
        "Kinetic Orb"
    }
}
