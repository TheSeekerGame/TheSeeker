use bevy::prelude::*;

use crate::game::player::sensors::GroundSensor;
use crate::game::player::skills::types::{
    CooldownMode, CooldownSpec, ExplosiveMineMetadata, 
};
use theseeker_engine::physics::GROUNDED_THRESHOLD;

pub(crate) const EXPLOSIVE_MINE_METADATA: ExplosiveMineMetadata =
    ExplosiveMineMetadata {
        cooldown: CooldownSpec {
            min_ticks: 0,
            max_ticks: 100,
            mode: CooldownMode::RateBased,
        },
    };

pub const AIR_MINE_MAX_GROUND_DISTANCE: f32 = 36.0;

/// Determine a valid mine spawn position based on the player's transform and
/// ground proximity. Returns `None` when we are too far from the ground.
pub fn resolve_spawn_position(
    transform: &Transform,
    ground: &GroundSensor,
) -> Option<Vec2> {
    let mut spawn_pos = transform.translation.truncate();
    if !ground.is_grounded {
        if ground.distance > AIR_MINE_MAX_GROUND_DISTANCE {
            return None;
        }

        let delta_y = (GROUNDED_THRESHOLD - ground.distance).min(0.0);
        spawn_pos.y += delta_y;
    }
    Some(spawn_pos)
}
