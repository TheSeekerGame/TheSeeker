use bevy::prelude::*;

use super::types::{CooldownMode, CooldownSpec, QuarkPylonMetadata};
use crate::game::gentstate::Facing;

pub(crate) const QUARK_PYLON_METADATA: QuarkPylonMetadata =
    QuarkPylonMetadata {
        cooldown: CooldownSpec {
            min_ticks: 0,
            max_ticks: 960,
            mode: CooldownMode::RateBased,
        },
        lifetime_ticks: 1024,
        pulse_interval_ticks: 16,
    };

/// Pick a placement point for the Quark Pylon near the player; can be in air.
pub(crate) fn find_quark_pylon_placement(
    player_transform: &Transform,
    facing: &Facing,
) -> Option<Vec2> {
    let player_pos = player_transform.translation.truncate();
    let dir = facing.direction();
    let offset = Vec2::new(dir * 8.0, 0.0);
    Some(player_pos + offset)
}

pub(crate) fn active_pylon_count(
    owner: Entity,
    pylon_query: &Query<&crate::game::player::spawns::quark_pylon::QuarkPylon>,
) -> usize {
    pylon_query
        .iter()
        .filter(|p| p.owner == owner && p.counts_for_cooldown)
        .count()
}
