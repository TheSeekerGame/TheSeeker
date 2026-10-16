

use super::types::{CooldownMode, CooldownSpec,  SpinnerMetadata};

pub(crate) const SPINNER_METADATA: SpinnerMetadata = SpinnerMetadata {
    cooldown: CooldownSpec {
        min_ticks: 0,
        max_ticks: 2880,
        mode: CooldownMode::RateBased,
    },
};
