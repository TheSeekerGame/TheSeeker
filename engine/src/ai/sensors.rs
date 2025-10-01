//! Sensor systems that update AI perception components.
//!
//! Sensors run before the brain system and translate world state into
//! simple components that FSM conditions can evaluate. This separation
//! keeps FSM logic pure and testable.

use crate::animation::AnimLoop;
use crate::gent::Gent;
use crate::physics::{CollisionGroups, PhysicsWorld, ENEMY, GROUND};
use bevy::prelude::*;

// Import the components from the parent module
use super::{
    FsmInstance, GroundSensor, HealthSensor, PerceivedGroundSensor,
    PerceivedRangeSensor, PerceivedTargetSensor, RangeSensor, SensorHistory,
    TargetSensor,
};

/// Component that holds cached archetype stats to avoid per-frame asset lookups
#[derive(Component)]
pub struct CachedArchetypeStats {
    pub vision_range: f32,
    pub melee_range: f32,
    pub needs_line_of_sight: bool,
}

/// Marker component for entities that can be targeted by AI
#[derive(Component)]
pub struct AiTarget;

/// Component that prevents AI from targeting an entity (e.g., stealthed player)
#[derive(Component)]
pub struct AiTargetInvisible;

/// Update target sensors – selects a visible `AiTarget` if present (single-player assumption).
/// When the target is hidden by stealth or line-of-sight (LOS) is blocked, clears `entity` and
/// sets `dist2 = f32::MAX`. This ensures `distance_gt` conditions evaluate to true so patrol
/// logic can take over deterministically.
pub fn sensor_target(
    mut enemy_query: Query<
        (
            &mut TargetSensor,
            &Transform,
            &CachedArchetypeStats,
        ),
        With<FsmInstance>,
    >,
    target_query: Query<
        (Entity, &Transform),
        (
            With<AiTarget>,
            Without<AiTargetInvisible>,
        ),
    >,
    spatial_query: PhysicsWorld,
) {
    // Assumes a single player; use the first visible target returned by the query
    let target_info = target_query.iter().next();

    for (mut target_sensor, enemy_tf, cached_stats) in enemy_query.iter_mut() {
        let vision_range = cached_stats.vision_range;
        let needs_los = cached_stats.needs_line_of_sight;

        if let Some((target_e, target_tf)) = target_info {
            // Compute squared distance in 2-D
            let delta = target_tf.translation - enemy_tf.translation;
            let dist2 = delta.x * delta.x + delta.y * delta.y;
            let vision_range_sq = vision_range * vision_range;

            // Line-of-sight check for ranged enemies
            let mut los_blocked = false;
            if needs_los && dist2 <= vision_range_sq {
                let origin = enemy_tf.translation.xy();
                let dest = target_tf.translation.xy();
                let dir = dest - origin;
                let dist = dir.length();
                if dist > 0.1 {
                    if let Some((_hit, toi)) = spatial_query.ray_cast(
                        origin,
                        dir / dist,
                        dist,
                        true,
                        CollisionGroups::new(ENEMY, GROUND),
                        None,
                    ) {
                        los_blocked = toi.time_of_impact < dist;
                    }
                }
            }

            if los_blocked {
                // Clear target when line-of-sight is blocked
                target_sensor.entity = None;
                target_sensor.dist2 = f32::MAX;
            } else {
                target_sensor.entity = Some(target_e);
                target_sensor.dist2 = dist2;
            }
        } else {
            // No visible target
            target_sensor.entity = None;
            target_sensor.dist2 = f32::MAX;
        }
    }
}

/// Update range sensors based on target distance
pub fn sensor_range(
    mut query: Query<(
        &TargetSensor,
        &mut RangeSensor,
        &CachedArchetypeStats,
    )>,
) {
    for (target_sensor, mut range_sensor, cached_stats) in query.iter_mut() {
        let vision_range = cached_stats.vision_range;
        let melee_range = cached_stats.melee_range;

        if target_sensor.entity.is_some() {
            let vision_sq = vision_range * vision_range;
            let melee_sq = melee_range * melee_range;
            range_sensor.in_melee = target_sensor.dist2 <= melee_sq;
            range_sensor.in_aggro = target_sensor.dist2 <= vision_sq;
        } else {
            range_sensor.in_melee = false;
            range_sensor.in_aggro = false;
        }
    }
}

/// Update ground sensors based on navigation state
pub fn sensor_ground<T: Component>(
    mut query: Query<(&mut GroundSensor, &T), With<FsmInstance>>,
) where
    T: GroundedCheck,
{
    for (mut ground_sensor, nav) in query.iter_mut() {
        ground_sensor.on = nav.is_grounded();
    }
}

/// Trait for components that can indicate grounded state
pub trait GroundedCheck {
    fn is_grounded(&self) -> bool;
}

/// Maps ScriptPlayer animation slots to bit flags for FSM slot conditions
pub fn sensor_slots(
    mut enemy_query: Query<(&Gent, &mut FsmInstance)>,
    gfx_query: Query<
        &crate::script::ScriptPlayer<crate::assets::animation::SpriteAnimation>,
    >,
    compiled_assets: Res<Assets<super::CompiledFsm>>,
) {
    for (gent, mut fsm) in enemy_query.iter_mut() {
        if let Some(compiled) = compiled_assets.get(&fsm.brain) {
            if let Ok(player) = gfx_query.get(gent.e_gfx) {
                let mut bits: u32 = 0;
                for (idx, name) in compiled.inner.slot_names.iter().enumerate()
                {
                    if player.has_slot(name) {
                        bits |= 1 << idx;
                    }
                }
                fsm.slot_bits = bits;
            }
        }
    }
}

/// Sensor for health zero condition
pub fn sensor_health<T: Component>(mut query: Query<(&T, &mut HealthSensor)>)
where
    T: HealthCheck,
{
    for (health, mut sensor) in query.iter_mut() {
        sensor.zero = health.is_zero();
    }
}

/// Trait for components that can provide health information
pub trait HealthCheck {
    fn is_zero(&self) -> bool;
}

/// Reset anim_tick when animation loops for frame-exact action timing
pub fn sensor_reset_timer_on_anim_loop(
    mut fsm_query: Query<(&Gent, &mut FsmInstance)>,
    anim_loop_query: Query<&AnimLoop>,
) {
    for (gent, mut fsm) in fsm_query.iter_mut() {
        if anim_loop_query.get(gent.e_gfx).is_ok() {
            fsm.anim_tick = 0;
        }
    }
}

/// Update sensor history ring buffers with current sensor values
pub fn update_sensor_history(
    mut query: Query<(
        &TargetSensor,
        &GroundSensor,
        &RangeSensor,
        &mut SensorHistory,
    )>,
) {
    for (target, ground, range, mut history) in query.iter_mut() {
        let write_idx = history.write_index as usize;

        // Push current sensor values to ring buffer
        history.target_history[write_idx] = (target.entity, target.dist2);
        history.ground_history[write_idx] = ground.on;
        history.range_history[write_idx] = (range.in_melee, range.in_aggro);

        // Advance write index
        history.write_index = (history.write_index + 1) % 32;
    }
}

/// Update perceived sensors from historical values based on reaction time
pub fn update_perceived_sensors(
    mut query: Query<(
        &SensorHistory,
        &mut PerceivedTargetSensor,
        &mut PerceivedGroundSensor,
        &mut PerceivedRangeSensor,
    )>,
) {
    for (history, mut p_target, mut p_ground, mut p_range) in query.iter_mut() {
        let read_idx = history.get_read_index();

        // Read from history at delayed position
        let (target_entity, target_dist2) = history.target_history[read_idx];
        p_target.entity = target_entity;
        p_target.dist2 = target_dist2;

        p_ground.on = history.ground_history[read_idx];

        let (in_melee, in_aggro) = history.range_history[read_idx];
        p_range.in_melee = in_melee;
        p_range.in_aggro = in_aggro;
    }
}

/// Copy actual sensors to perceived sensors for enemies without reaction time
pub fn copy_actual_to_perceived(
    mut query: Query<
        (
            &TargetSensor,
            &GroundSensor,
            &RangeSensor,
            &mut PerceivedTargetSensor,
            &mut PerceivedGroundSensor,
            &mut PerceivedRangeSensor,
        ),
        Without<SensorHistory>,
    >,
) {
    for (target, ground, range, mut p_target, mut p_ground, mut p_range) in
        query.iter_mut()
    {
        // Direct copy for enemies without reaction time
        p_target.entity = target.entity;
        p_target.dist2 = target.dist2;
        p_ground.on = ground.on;
        p_range.in_melee = range.in_melee;
        p_range.in_aggro = range.in_aggro;
    }
}

/// Initialize sensor history for enemies with reaction time > 0
pub fn initialize_sensor_history(
    mut commands: Commands,
    query: Query<
        (
            Entity,
            &super::EnemyArchHandle,
            &TargetSensor,
            &GroundSensor,
            &RangeSensor,
        ),
        Added<super::EnemyArchHandle>,
    >,
    arch_assets: Res<Assets<super::asset::EnemyArchetype>>,
) {
    for (entity, arch_handle, target, ground, range) in query.iter() {
        if let Some(archetype) = arch_assets.get(&arch_handle.0) {
            if let Some(stats) = &archetype.stats {
                if stats.reaction_time > 0 {
                    // Create and initialize sensor history
                    let mut history = SensorHistory::new(stats.reaction_time);

                    // Pre-fill history with current sensor values
                    for i in 0..32 {
                        history.target_history[i] =
                            (target.entity, target.dist2);
                        history.ground_history[i] = ground.on;
                        history.range_history[i] =
                            (range.in_melee, range.in_aggro);
                    }

                    commands.entity(entity).insert(history);
                }
            }
        }
    }
}
