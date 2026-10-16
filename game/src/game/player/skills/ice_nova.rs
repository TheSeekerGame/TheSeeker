
use crate::game::player::skills::types::{
    CooldownMode, CooldownSpec, IceNovaMetadata, 
};

pub(crate) const ICE_NOVA_METADATA: IceNovaMetadata = IceNovaMetadata {
    cooldown: CooldownSpec {
        min_ticks: 0,
        max_ticks: 900,
        mode: CooldownMode::RateBased,
    },
};
