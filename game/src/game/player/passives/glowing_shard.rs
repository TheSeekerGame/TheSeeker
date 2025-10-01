use super::{PassiveContext, PassiveEffect};
use crate::game::player::PlayerStatMod;

/// Glowing Shard: Defense scales with the number of nearby enemies (+1x per enemy)
pub struct GlowingShardEffect;

impl PassiveEffect for GlowingShardEffect {
    fn modify_stats(
        &self,
        stats: &mut PlayerStatMod,
        context: &PassiveContext,
    ) {
        // Defense scales linearly with the count of nearby enemies
        stats.defense *= 1.0 + context.enemies_nearby as f32;
    }

    fn priority(&self) -> i32 {
        5
    }

    fn name(&self) -> &'static str {
        "GlowingShard"
    }
}
