use bevy::prelude::*;

use super::cooldowns::Cooldowns;
use super::types::{AmplifiedBellMetadata, CooldownMode, CooldownSpec, SkillId};
use crate::game::gentstate::Facing;
use crate::game::player::input_buffer::InputBuffer;
use crate::game::player::PlayerAction;
use theseeker_engine::physics::{
    Collider, ColliderShapeAccess, CollisionGroups as InteractionGroups, Group,
    PhysicsWorld, ENEMY, GROUNDED_THRESHOLD,
};

const BELL_HALF_WIDTH: f32 = 6.0;
const BELL_HALF_HEIGHT: f32 = 8.0;

pub(crate) const AMPLIFIED_BELL_METADATA: AmplifiedBellMetadata = AmplifiedBellMetadata {
    cooldown: CooldownSpec {
        min_ticks: 0,
        max_ticks: 1152,
        mode: CooldownMode::RateBased,
    },
};

/// Determine a valid placement point for the amplified bell given the current player pose.
pub fn find_amplified_bell_placement(
    player_transform: &Transform,
    facing: &Facing,
    spatial_query: &PhysicsWorld,
    owner: Entity,
) -> Option<Vec2> {
    let player_pos = player_transform.translation.truncate();
    let down_dir = Vec2::NEG_Y;
    let dir = facing.direction();
    let candidate_offsets = [16.0, 12.0, 8.0, 4.0, 0.0];

    for offset in candidate_offsets.into_iter() {
        let x = player_pos.x + dir * offset;
        let start_y = player_pos.y + 4.0;
        if let Some((_hit, toi)) = spatial_query.ray_cast(
            Vec2::new(x, start_y),
            down_dir,
            64.0,
            true,
            InteractionGroups::new(
                Group::all(),
                theseeker_engine::physics::GROUND,
            ),
            None,
        ) {
            let ground_y = start_y - toi.time_of_impact;
            if player_pos.y - ground_y > 50.0 {
                continue;
            }
            let clearance = GROUNDED_THRESHOLD + 0.1;
            let bell_center_y = ground_y + BELL_HALF_HEIGHT + clearance;
            let candidate_center = Vec2::new(x, bell_center_y);

            let overlaps_ground = spatial_query.intersect(
                candidate_center,
                Collider::cuboid(BELL_HALF_WIDTH, BELL_HALF_HEIGHT).shape(),
                InteractionGroups::new(
                    Group::all(),
                    theseeker_engine::physics::GROUND,
                ),
                Some(owner),
            );
            if !overlaps_ground.is_empty() {
                continue;
            }

            let overlaps_enemy = spatial_query.intersect(
                candidate_center,
                Collider::cuboid(BELL_HALF_WIDTH, BELL_HALF_HEIGHT).shape(),
                InteractionGroups::new(Group::all(), ENEMY),
                Some(owner),
            );
            if !overlaps_enemy.is_empty() {
                continue;
            }

            return Some(candidate_center);
        }
    }

    None
}

/// Try to deploy the Amplified Bell: validate placement and spawn the bell entity.
pub fn try_start_amplified_bell_slot(
    entity: Entity,
    commands: &mut Commands,
    action_state: &leafwing_input_manager::action_state::ActionState<
        PlayerAction,
    >,
    buffer: &mut InputBuffer,
    player_transform: &Transform,
    facing: &Facing,
    now_tick: u64,
    cooldowns: &mut ResMut<Cooldowns>,
    stats: Option<&crate::game::player::PlayerStatMod>,
    spatial_query: &PhysicsWorld,
    slot_action: PlayerAction,
) -> bool {
    // Check input request (just pressed or buffered)
    if !(action_state.just_pressed(&slot_action)
        || buffer.check_buffered(slot_action, now_tick).is_some())
    {
        return false;
    }

    // Cooldown gate check only (stamp after successful placement)
    if !cooldowns.is_ready(entity, SkillId::AmplifiedBell, now_tick) {
        return false;
    }

    let Some(center) = find_amplified_bell_placement(
        player_transform,
        facing,
        spatial_query,
        entity,
    ) else {
        // No valid placement found; cancel skill (also clear buffered input)
        buffer.clear_action(slot_action);
        return false;
    };

    // Spawn the bell entity via the spawns module
    crate::game::player::spawns::amplified_bell::spawn_bell(
        commands,
        entity,
        center,
        facing.direction(),
    );

    // Start cooldown now that placement succeeded
    let cdr_snapshot = stats.map(|s| s.cdr).unwrap_or(1.0);
    cooldowns.start(
        entity,
        SkillId::AmplifiedBell,
        AMPLIFIED_BELL_METADATA.cooldown,
        cdr_snapshot,
        now_tick,
    );

    buffer.clear_action(slot_action);
    true
}
