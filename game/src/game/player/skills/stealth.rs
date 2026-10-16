
use super::types::{CooldownMode, CooldownSpec,  StealthMetadata};

pub(crate) const STEALTH_METADATA: StealthMetadata = StealthMetadata {
    cooldown: CooldownSpec {
        min_ticks: 19,
        max_ticks: (5.0 * 96.0) as u32,
        mode: CooldownMode::RateBased,
    },
};
