use crate::game::player::PlayerStatMod;

use super::{PassiveContext, PassiveEffect};

/// Battery Pack: grants an extra active skill slot (5th slot) while equipped.
pub struct BatteryPackEffect;

impl PassiveEffect for BatteryPackEffect {
    fn modify_stats(
        &self,
        stats: &mut PlayerStatMod,
        _context: &PassiveContext,
    ) {
        // Currently implemented via runtime checks of equipped passives; no numeric stat change here
        let _ = stats; // Explicitly no-op
    }

    fn name(&self) -> &'static str {
        "Battery Pack"
    }
}
