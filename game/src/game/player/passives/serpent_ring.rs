use super::{PassiveContext, PassiveEffect};
use crate::game::player::PlayerStatMod;

/// Serpent Ring: +20% speed, +33% CDR. Max HP halving is handled elsewhere.
pub struct SerpentRingEffect;

impl PassiveEffect for SerpentRingEffect {
    fn modify_stats(
        &self,
        stats: &mut PlayerStatMod,
        _context: &PassiveContext,
    ) {
        stats.speed *= 1.2;
        stats.cdr *= 1.33;
    }

    fn animation_slots(&self) -> Vec<(&'static str, bool)> {
        vec![("SerpentRing", true)]
    }

    fn priority(&self) -> i32 {
        10
    }

    fn name(&self) -> &'static str {
        "SerpentRing"
    }
}
