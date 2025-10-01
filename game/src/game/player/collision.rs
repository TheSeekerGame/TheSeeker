//! Player collision and ground maintenance systems.
//!
//! This module integrates the custom movement model (no physics simulation) with
//! Rapier shape casts to resolve player contacts against ground and enemies.
//!
//! Key points:
//! - Iterative geometric resolution to handle corners/stacked contacts without forces.
//! - Enemy hits are detected but can be selectively ignored for resolution during
//!   specific skills (e.g., flicker strike, burning dash, whirling).
//! - Position is updated once per tick from the accumulated resolution plus velocity.
//! - A separate ground-maintenance system keeps a precise gap above ground when grounded.
use bevy::prelude::*;
use glam::{Vec2, Vec2Swizzles, Vec3Swizzles};
use theseeker_engine::physics::{
    into_vec2, Collider, ColliderShapeAccess, CollisionGroups, LinearVelocity,
    PhysicsWorld, ShapeCastStatus, ShapeCaster, ENEMY, GROUND,
    GROUNDED_THRESHOLD, PLAYER,
};

use crate::game::enemy::Enemy;
use crate::game::player::{
    sensors::GroundSensor,
    states::{
        transition_action, BurningDashing, DashStrike, Dashing, Falling,
        Grounded, Ready, Whirling,
    },
    Player, PlayerStateSet, WallSlideTime,
};
use crate::game::effects::stealthed::StealthEffect;
use crate::prelude::*;

/// Player collision systems.
pub(crate) struct PlayerCollisionPlugin;

impl Plugin for PlayerCollisionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                player_collisions,
                player_grounded_position_maintenance.after(player_collisions),
            )
                .chain()
                .in_set(PlayerStateSet::Collisions)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

pub fn player_collisions(
    spatial_query: PhysicsWorld,
    mut q_gent: Query<
        (
            Entity,
            &mut Transform,
            &mut LinearVelocity,
            &Collider,
            Option<&mut WallSlideTime>,
            Option<&mut Dashing>,
            Has<DashStrike>,
            Has<Whirling>,
            Has<BurningDashing>,
            Has<crate::game::player::states::FlickerStriking>,
            Has<crate::game::player::states::locomotion::running::AutoDodgeRolling>,
            Option<&Falling>,
            Has<StealthEffect>,
        ),
        With<Player>,
    >,
    mut q_enemy: Query<(Entity, &mut Collider), (With<Enemy>, Without<Player>)>,
    is_bell: Query<(), With<crate::game::player::spawns::amplified_bell::Bell>>,
    mut commands: Commands,
    time: Res<GameTime>,
) {
    for (
        entity,
        mut pos,
        mut linear_velocity,
        collider,
        slide,
        mut dashing,
        is_dash_strike,
        is_whirling,
        is_burning_dashing,
        is_flicker_striking,
        is_auto_dodge_rolling,
        falling_state,
        is_stealthed,
    ) in q_gent.iter_mut()
    {
        let mut shape = collider.shared_shape().clone();
        let original_pos = pos.translation.xy();
        let mut accumulated_movement = Vec2::ZERO; // Accumulates displacement from collision resolution
        let z = pos.translation.z;
        let mut projected_velocity = linear_velocity.0;
        let is_wall_sliding =
            falling_state.as_ref().and_then(|f| f.wall_slide).is_some();

        // When whirling, burning dashing, dash striking, auto-dodge rolling, or stealthed, pass through enemies but still collide with ground
        // Flicker striking should still detect collisions for damage purposes but not resolve wall collisions
        let filter = if is_whirling
            || is_burning_dashing
            || is_dash_strike
            || is_auto_dodge_rolling
            || is_stealthed
        {
            GROUND
        } else {
            ENEMY | GROUND
        };
        let mut interaction = CollisionGroups::new(PLAYER, filter);

        let mut wall_slide = false;
        let dir = linear_velocity.0.x.signum();
        // Iterative collision resolution to robustly handle corners and stacked contacts.
        // Cap iterations to avoid pathological loops on complex geometry.
        let mut iteration_count = 0;

        while let Ok(shape_dir) = Dir2::new(projected_velocity) {
            iteration_count += 1;
            if iteration_count > 5 {
                break;
            }

            // Cast from current accumulated position
            let cast_origin = original_pos + accumulated_movement;
            // Small buffer for precise collision
            const CAST_BUFFER: f32 = 0.1;

            if let Some((e, first_hit)) = spatial_query.shape_cast(
                cast_origin,
                shape_dir,
                &*shape,
                projected_velocity.length() + CAST_BUFFER,
                interaction,
                Some(entity),
            ) {
                // Ignore collisions with spawned player bells (they are pass-through props)
                if is_bell.get(e).is_ok() {
                    // Continue the loop without resolving against this entity
                    // Constrain subsequent casts to ground only for the remainder of this tick
                    interaction = CollisionGroups::new(PLAYER, GROUND);
                    continue;
                }
                // Enemy collisions
                if let Ok((_enemy, mut _collider)) = q_enemy.get_mut(e) {
                    // After first enemy hit, constrain to ground only for the remainder of this tick
                    // (unless flicker striking, which should pass through everything)
                    if !is_flicker_striking {
                        interaction = CollisionGroups::new(PLAYER, GROUND);
                    }

                    match first_hit.status {
                        // Not yet penetrating
                        _ if first_hit.time_of_impact > 0.001 => {
                            // Ignore enemy collision while dashing (except dash-strike).
                            // Flicker striking handles its own collision detection.
                            if dashing.is_none()
                                && !is_dash_strike
                                && !is_flicker_striking
                            {
                                if let Some(details) = first_hit.details {
                                    let sliding_plane =
                                        into_vec2(details.normal1);
                                    // Threshold on normal.x to classify vertical wall vs floor/ceiling
                                    let threshold = 0.1;
                                    // Check if this is a vertical wall (normal is mostly horizontal)
                                    if sliding_plane.x.abs() > threshold {
                                        // This is a wall collision - stop horizontal movement into the wall
                                        projected_velocity.x = 0.0;
                                    }
                                }
                            }
                        },
                        // Already inside: mark as inside to skip next frame
                        _ => {
                            if !is_flicker_striking {
                                theseeker_engine::physics::inside::set(
                                    &mut commands,
                                    entity,
                                    e,
                                );
                            }
                        },
                    }
                // Ground collisions
                } else {
                    match first_hit.status {
                        ShapeCastStatus::Converged
                        | ShapeCastStatus::OutOfIterations => {
                            // No bounce or friction; geometric resolution only
                            let sliding_plane = into_vec2(
                                first_hit
                                    .details
                                    .map(|d| d.normal1)
                                    .unwrap_or_default(),
                            );

                            // Skip collision resolution during flicker striking
                            if is_flicker_striking {
                                // Flicker striking passes through walls, so we don't resolve collisions
                                // but we still detect them for potential damage purposes
                                continue;
                            }

                            // Project velocity along collision surface to prevent clipping
                            projected_velocity = linear_velocity.0
                                - sliding_plane
                                    * linear_velocity.0.dot(sliding_plane);

                            // No friction injected by this system
                            let friction_vec = Vec2::ZERO;

                            // Update wall slide tracking for WallSlideTime component
                            if is_wall_sliding && sliding_plane.x.abs() > 0.9 {
                                // Verify player is properly against the wall
                                if spatial_query
                                    .ray_cast(
                                        pos.translation.xy(),
                                        Vec2::new(dir, 0.0),
                                        shape
                                            .as_cuboid()
                                            .unwrap()
                                            .half_extents
                                            .x
                                            + 0.1,
                                        true,
                                        theseeker_engine::physics::groups::player_body(),
                                        Some(entity),
                                    )
                                    .is_some()
                                {
                                    wall_slide = true;
                                }
                            }

                            projected_velocity += friction_vec; // no bounce/friction from here

                            // Apply collision resolution with a small skin width to avoid jitter at boundaries
                            const SKIN_WIDTH: f32 = 0.02;
                            // Calculate movement from collision point, not from original position
                            let movement_distance = (first_hit.time_of_impact
                                - SKIN_WIDTH)
                                .max(0.0);
                            accumulated_movement +=
                                shape_dir.xy() * movement_distance;
                        },
                        ShapeCastStatus::PenetratingOrWithinTargetDist => {
                            // For ground penetration, we don't need special handling
                        },
                        ShapeCastStatus::Failed => {
                            // Collision failed - continue
                        },
                    }
                }

                // During wall sliding, ensure we never have upward velocity
                // to maintain deterministic downward movement
                if is_wall_sliding && projected_velocity.y > 0.0 {
                    projected_velocity.y = 0.0;
                }

                linear_velocity.0 = projected_velocity;
                // Do not update position inside the loop; apply after accumulation
            } else {
                break;
            }
        }

        // If horizontal velocity is zero, cancel horizontal dashes immediately (downward dashes persist)
        if projected_velocity.x.abs() < 0.00001 {
            if dashing.is_some() {
                // Exit dash now; also zero velocity to ensure clean handover to locomotion
                linear_velocity.0 = Vec2::ZERO;
                transition_action(&mut commands, entity, Ready);
            }
        }

        // Apply accumulated movement plus any remaining per-tick displacement
        let final_pos = original_pos + accumulated_movement + linear_velocity.0;
        pos.translation = final_pos.extend(z);

        if let Some(mut slide) = slide {
            if wall_slide {
                slide.0 = 0.0;
            } else {
                slide.0 += 1.0 / time.hz as f32;
            }
        }
    }
}

//

/// Maintains player position relative to ground when grounded
/// This prevents gradual sinking into the ground by ensuring the player
/// stays exactly GROUNDED_THRESHOLD distance above the ground
pub fn player_grounded_position_maintenance(
    spatial_query: PhysicsWorld,
    mut query: Query<
        (
            Entity,
            &mut Transform,
            &ShapeCaster,
            &GroundSensor,
            Has<Grounded>,
            Has<Dashing>,
            Option<&Falling>,
        ),
        With<Player>,
    >,
) {
    for (
        entity,
        mut transform,
        shape_caster,
        ground_sensor,
        has_grounded_marker,
        is_dashing,
        falling_state,
    ) in query.iter_mut()
    {
        // Only adjust position if:
        // 1. We have the Grounded marker
        // 2. The sensor says we're grounded
        // 3. We're not dashing (dashing has its own collision handling)
        // 4. We're not wall sliding (wall sliding handles its own movement)
        // 5. We're actually close to the ground (not falling from height)
        let is_wall_sliding =
            falling_state.as_ref().and_then(|f| f.wall_slide).is_some();
        if has_grounded_marker
            && ground_sensor.is_grounded
            && !is_dashing
            && !is_wall_sliding
        {
            // Perform our own raycast to get exact ground distance
            if let Some((_, toi)) = shape_caster.cast(
                &spatial_query.context(),
                &transform,
                Some(entity),
            ) {
                let distance = toi.time_of_impact;

                // CRITICAL: Only adjust if we're already very close to ground
                // This prevents teleporting player to ground when falling
                const MAX_ADJUSTMENT_DISTANCE: f32 = 5.0;
                if distance > MAX_ADJUSTMENT_DISTANCE {
                    // Player is too far from ground - this is likely a sensor error
                    // Don't adjust position for this entity; continue with others
                    continue;
                }

                // Only adjust if we're not at the exact correct distance
                // Allow a small tolerance to prevent constant micro-adjustments
                const POSITION_TOLERANCE: f32 = 0.05;
                let distance_error = (distance - GROUNDED_THRESHOLD).abs();

                if distance_error > POSITION_TOLERANCE {
                    // Adjust Y position to maintain exact GROUNDED_THRESHOLD distance
                    let adjustment = GROUNDED_THRESHOLD - distance;
                    transform.translation.y += adjustment;
                }
            }
        }
    }
}
