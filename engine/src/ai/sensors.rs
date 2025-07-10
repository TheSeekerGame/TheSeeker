//! Sensor systems that update AI perception components.
//! 
//! Sensors run before the brain system and translate world state into
//! simple components that FSM conditions can evaluate. This separation
//! keeps FSM logic pure and testable.

use bevy::prelude::*;
use crate::animation::AnimLoop;
use crate::gent::Gent;
use crate::physics::{CollisionGroups, PhysicsWorld, ENEMY, GROUND};

// Import the components from the parent module
use super::{FsmInstance, TargetSensor, GroundSensor, RangeSensor, HealthSensor};

/// Component that holds cached archetype stats to avoid per-frame asset lookups
#[derive(Component)]
pub struct CachedArchetypeStats {
    pub vision_range: f32,
    pub melee_range: f32,
    pub fall_accel: f32,
    pub needs_line_of_sight: bool,
}

/// Marker component for entities that can be targeted by AI
#[derive(Component)]
pub struct AiTarget;

/// Component that prevents AI from targeting an entity (e.g., stealthed player)
#[derive(Component)]
pub struct AiTargetInvisible;

/// Update target sensors - finds closest visible AiTarget.
/// When target lost (stealth/LOS), clears entity and sets dist2=MAX.
/// This makes distance_gt conditions return true, triggering Patrol.
pub fn sensor_target(
    mut enemy_query: Query<(
        &mut TargetSensor,
        &Transform,
        &CachedArchetypeStats,
    ), With<FsmInstance>>, 
    target_query: Query<(Entity, &Transform), (With<AiTarget>, Without<AiTargetInvisible>)>,
    spatial_query: PhysicsWorld,
) {
    // For single-target scenarios, get the first valid target
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
                let dest   = target_tf.translation.xy();
                let dir    = dest - origin;
                let dist   = dir.length();
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
    mut query: Query<(&TargetSensor, &mut RangeSensor, &CachedArchetypeStats)>,
) {
    for (target_sensor, mut range_sensor, cached_stats) in query.iter_mut() {
        let vision_range = cached_stats.vision_range;
        let melee_range = cached_stats.melee_range;

        if target_sensor.entity.is_some() {
            let vision_sq = vision_range * vision_range;
            let melee_sq  = melee_range  * melee_range;
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
    gfx_query: Query<&crate::script::ScriptPlayer<crate::assets::animation::SpriteAnimation>>,
    compiled_assets: Res<Assets<super::CompiledFsm>>,
) {
    for (gent, mut fsm) in enemy_query.iter_mut() {
        if let Some(compiled) = compiled_assets.get(&fsm.brain) {
            if let Ok(player) = gfx_query.get(gent.e_gfx) {
                let mut bits: u32 = 0;
                for (idx, name) in compiled.inner.slot_names.iter().enumerate() {
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
pub fn sensor_health<T: Component>(
    mut query: Query<(&T, &mut HealthSensor)>,
) where
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