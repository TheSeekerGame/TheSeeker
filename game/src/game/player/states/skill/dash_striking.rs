use crate::camera::CameraShake;
use crate::game::combat::damage_pipeline::NoOnHitShake;
use bevy::prelude::*;
use bevy::transform::components::GlobalTransform;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::{Collider, LinearVelocity};
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::GameTickUpdate;

use crate::game::gentstate::Facing;
use crate::game::player::skills::types::{
    dash_strike_metadata,
    dash_strike_variant_metadata,
    SkillId,
};
use crate::game::player::{
    states::{transition_action, DashStrike, Ready},
    Player, PlayerStateSet,
};

// For sensors and enemy proximity/damage events
use crate::game::combat::{DamageSource, Stealthed};
use crate::game::enemy::Enemy;
use crate::game::player::sensors::{CeilingSensor, GroundSensor, WallSensor};
use crate::game::player::player_anim::set_direction_slots;

/// Dash Strike state: separate skill with its own movement/animation/damage
pub struct DashStrikingStatePlugin;

impl Plugin for DashStrikingStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                dash_strike_enter_system,
                dash_strike_update_system,
            )
                .chain()
                .run_if(any_with_component::<DashStrike>)
                .in_set(PlayerStateSet::Behavior),
        );
    }
}

// Movement tuning
const STRIKE_DIAG_SPEED: f32 = 4.167; // per-axis for 45° (≈564 px/s total at 96 Hz)
const OUTRO_LOCK_TICKS: u32 = 8; // freeze for 1 tick on collision

// Impact damage (applied once on wall/ground/ceiling collision)
const STRIKE_IMPACT_RADIUS: f32 = 24.0; // px
const STRIKE_IMPACT_DAMAGE: f32 = 120.0;

const STRIKE_IMPACT_SHAKE_STRENGTH: f32 = 5.0;
const STRIKE_IMPACT_SHAKE_DURATION_SECS: f32 = 0.30;
const STRIKE_IMPACT_SHAKE_FREQUENCY: f32 = 3.0;

fn dash_strike_enter_system(
    mut q: Query<
        (
            Entity,
            &mut DashStrike,
            &Facing,
            &Gent,
            &mut LinearVelocity,
            Option<&crate::game::player::states::LastMoveDir>,
        ),
        (With<Player>, Added<DashStrike>),
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>>,
) {
    for (_, mut ds, facing, gent, mut vel, last_move_dir) in
        q.iter_mut()
    {
        // Variant-specific duration pulled from skill metadata
        ds.max_ticks = dash_strike_variant_metadata(ds.variant).state_duration_ticks;

        // Seed initial velocity by variant
        let dir = if ds.dir.abs() > 0.0 {
            ds.dir.signum()
        } else if let Some(lmd) = last_move_dir {
            if lmd.0.abs() > 0.0 {
                lmd.0.signum()
            } else {
                facing.direction()
            }
        } else {
            facing.direction()
        };
        match ds.variant {
            crate::game::player::skills::types::Variant::Up => {
                vel.0 = Vec2::new(
                    dir * STRIKE_DIAG_SPEED,
                    STRIKE_DIAG_SPEED,
                );
            },
            crate::game::player::skills::types::Variant::Down => {
                vel.0 = Vec2::new(
                    dir * STRIKE_DIAG_SPEED,
                    -STRIKE_DIAG_SPEED,
                );
            },
            _ => {
                vel.0 = Vec2::new(
                    dir * STRIKE_DIAG_SPEED,
                    -STRIKE_DIAG_SPEED,
                );
            },
        }

        // Play DashStrike animation and set direction + variant loop slot
        if let Ok(mut script) = gfx_query.get_mut(gent.e_gfx) {
            set_direction_slots(&mut script, facing);
            script.play_key(dash_strike_metadata().animation_key);
            match ds.variant {
                crate::game::player::skills::types::Variant::Up => {
                    script.set_slot("DashStrikeVariantUp", true);
                    script.set_slot("DashStrikeVariantDown", false);
                },
                crate::game::player::skills::types::Variant::Down => {
                    script.set_slot("DashStrikeVariantUp", false);
                    script.set_slot("DashStrikeVariantDown", true);
                },
                _ => {
                    script.set_slot("DashStrikeVariantUp", false);
                    script.set_slot("DashStrikeVariantDown", false);
                },
            }
        }

        // Lifetime comes from state default; no override here
    }
}

fn dash_strike_update_system(
    mut commands: Commands,
    mut q: Query<
        (
            Entity,
            &mut DashStrike,
            &Facing,
            &mut LinearVelocity,
            &Transform,
            &Gent,
            Option<&crate::game::effects::stealthed::StealthEffect>,
            &GroundSensor,
            &WallSensor,
            &CeilingSensor,
            Option<&crate::game::player::states::LastMoveDir>,
        ),
        With<Player>,
    >,
    targets: Query<
        (Entity, &Transform),
        Or<(
            With<Enemy>,
            With<crate::game::player::spawns::amplified_bell::Bell>,
        )>,
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>>,
) {
    for (
        entity,
        mut ds,
        facing,
        mut vel,
        transform,
        gent,
        is_stealthed,
        ground,
        walls,
        ceiling,
        last_move_dir,
    ) in q.iter_mut()
    {
        // Maintain velocity unless collided
        let dir = if ds.dir.abs() > 0.0 {
            ds.dir.signum()
        } else if let Some(lmd) = last_move_dir {
            if lmd.0.abs() > 0.0 {
                lmd.0.signum()
            } else {
                facing.direction()
            }
        } else {
            facing.direction()
        };
        if !ds.collided {
            match ds.variant {
                crate::game::player::skills::types::Variant::Up => {
                    vel.0 = Vec2::new(
                        dir * STRIKE_DIAG_SPEED,
                        STRIKE_DIAG_SPEED,
                    );
                },
                crate::game::player::skills::types::Variant::Down => {
                    vel.0 = Vec2::new(
                        dir * STRIKE_DIAG_SPEED,
                        -STRIKE_DIAG_SPEED,
                    );
                },
                _ => {
                    vel.0 = Vec2::new(
                        dir * STRIKE_DIAG_SPEED,
                        -STRIKE_DIAG_SPEED,
                    );
                },
            }
        } else {
            vel.0 = Vec2::ZERO; // lock motion during outro
        }

        // Collision detection to start outro lock and switch frames via slots
        if !ds.collided {
            let hit_wall = (dir < 0.0 && walls.left_contact)
                || (dir > 0.0 && walls.right_contact);
            let hit_ground = ground.is_grounded;
            let hit_ceiling = ceiling.is_touching_ceiling;

            let should_collide = match ds.variant {
                crate::game::player::skills::types::Variant::Up => {
                    hit_wall || hit_ceiling
                },
                crate::game::player::skills::types::Variant::Down => {
                    hit_wall || hit_ground
                },
                _ => hit_wall || hit_ground,
            };

            if should_collide {
                ds.collided = true;
                ds.lock_ticks = 0;
                vel.0 = Vec2::ZERO;
                // Impact-only AoE damage: apply once on collision
                let player_pos = transform.translation;
                // Use a 2-tick lifetime so the combat pipeline always sees the
                // spawned damage source after deferred command application.
                let mut ds_comp =
                    DamageSource::new(2, entity, STRIKE_IMPACT_DAMAGE);
                ds_comp.max_targets = 64; // generous cap for AoE
                                          // Build target set from proximity; include enemies and bells
                for (t_entity, t_tf) in targets.iter() {
                    let dist = player_pos
                        .truncate()
                        .distance(t_tf.translation.truncate());
                    if dist <= STRIKE_IMPACT_RADIUS {
                        ds_comp.target_set.insert(t_entity);
                    }
                }
                if !ds_comp.target_set.is_empty() {
                    let mut e = commands.spawn((
                        ds_comp,
                        Collider::ball(STRIKE_IMPACT_RADIUS),
                        // Place DS at player position for backstab orientation and effects
                        Transform::from_translation(player_pos),
                        GlobalTransform::from_translation(player_pos),
                        theseeker_engine::physics::groups::player_attack(),
                        NoOnHitShake,
                        crate::game::combat::damage_source::DamageSourceSkill(
                            SkillId::DashStrike,
                        ),
                    ));
                    if is_stealthed.is_some() {
                        e.insert(Stealthed);
                    }
                }

                // Special stronger screen shake regardless of targets
                commands.insert_resource(CameraShake::new(
                    STRIKE_IMPACT_SHAKE_STRENGTH,
                    STRIKE_IMPACT_SHAKE_DURATION_SECS,
                    STRIKE_IMPACT_SHAKE_FREQUENCY,
                ));
                // Switch to appropriate impact outro frames
                if let Ok(mut script) = gfx_query.get_mut(gent.e_gfx) {
                    match ds.variant {
                        crate::game::player::skills::types::Variant::Up => {
                            script.set_slot("DashStrikeImpactUp", true)
                        },
                        crate::game::player::skills::types::Variant::Down => {
                            script.set_slot("DashStrikeImpactDown", true)
                        },
                        _ => {},
                    }
                }
            }
        }

        // Advance and handle outro locking
        ds.tick = ds.tick.saturating_add(1);
        if ds.tick >= ds.max_ticks && !ds.collided {
            // End naturally when duration elapses
            // Reset any variant/impact slots to avoid stale state
            if let Ok(mut script) = gfx_query.get_mut(gent.e_gfx) {
                script.set_slot("DashStrikeVariantUp", false);
                script.set_slot("DashStrikeVariantDown", false);
                script.set_slot("DashStrikeImpactUp", false);
                script.set_slot("DashStrikeImpactDown", false);
                // Ensure we are back to Idle immediately
                set_direction_slots(&mut script, facing);
                script.play_key("anim.player.Idle");
            }
            end_dash_strike(&mut commands, entity);
            continue;
        }

        if ds.collided {
            // No-op: slots already set at collision time above
            ds.lock_ticks = ds.lock_ticks.saturating_add(1);
            if ds.lock_ticks >= OUTRO_LOCK_TICKS {
                // Reset slots on exit
                if let Ok(mut script) = gfx_query.get_mut(gent.e_gfx) {
                    script.set_slot("DashStrikeVariantUp", false);
                    script.set_slot("DashStrikeVariantDown", false);
                    script.set_slot("DashStrikeImpactUp", false);
                    script.set_slot("DashStrikeImpactDown", false);
                    // Ensure we are back to Idle immediately
                    set_direction_slots(&mut script, facing);
                    script.play_key("anim.player.Idle");
                }
                end_dash_strike(&mut commands, entity);
            }
        }
    }
}

fn end_dash_strike(commands: &mut Commands, entity: Entity) {
    transition_action(commands, entity, Ready);
}

