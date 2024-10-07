use crate::camera::CameraRig;
use crate::game::attack::{Attack, Pushback};
use crate::game::enemy::Enemy;
use crate::game::gentstate::{Facing, TransitionQueue, Transitionable};
use crate::game::player::{
    Attacking, CanAttack, CanDash, CoyoteTime, Dashing, Falling, Grounded, HitFreezeTime, Idle,
    Jumping, Player, PlayerAction, PlayerConfig, PlayerGfx, PlayerStateSet, Running, WallSlideTime,
    WhirlAbility,
};
use crate::prelude::{
    any_with_component, App, BuildChildren, Commands, DetectChanges, Direction2d, Entity,
    IntoSystemConfigs, Plugin, Query, Res, ResMut, Transform, TransformBundle, With, Without,
};
use bevy::prelude::Has;

use bevy::sprite::Sprite;
use bevy::transform::TransformSystem::TransformPropagate;
use glam::{Vec2, Vec2Swizzles, Vec3Swizzles};
use leafwing_input_manager::action_state::ActionState;
use rapier2d::geometry::{Group, InteractionGroups};
use rapier2d::parry::query::TOIStatus;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::{
    into_vec2, update_sprite_colliders, AnimationCollider, Collider, LinearVelocity, PhysicsWorld,
    ShapeCaster, ENEMY_HURT, ENEMY_INSIDE, GROUND, PLAYER, PLAYER_ATTACK,
};
use theseeker_engine::prelude::{GameTickUpdate, GameTime};
use theseeker_engine::script::ScriptPlayer;

use super::{CanStealth, JumpCount, Knockback, PlayerPushback, Stealthing};

///Player behavior systems.
///Do stuff here in states and add transitions to other states by pushing
///to a TransitionQueue.
pub(crate) struct PlayerBehaviorPlugin;

impl Plugin for PlayerBehaviorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                (
                    player_idle.run_if(any_with_component::<Idle>),
                    add_attack,
                    player_stealth,
                    player_whirl.before(player_attack),
                    player_attack.run_if(any_with_component::<Attacking>),
                    player_move,
                    player_can_dash.run_if(any_with_component::<CanDash>),
                    player_can_stealth.run_if(any_with_component::<CanStealth>),
                    player_run.run_if(any_with_component::<Running>),
                    player_jump.run_if(any_with_component::<Jumping>),
                    player_dash.run_if(any_with_component::<Dashing>),
                    player_grounded.run_if(any_with_component::<Grounded>),
                    player_falling.run_if(any_with_component::<Falling>),
                    player_pushback.run_if(any_with_component::<PlayerPushback>),
                    player_sliding
                        .before(player_jump)
                        .run_if(any_with_component::<Falling>),
                )
                    .in_set(PlayerStateSet::Behavior)
                    .before(update_sprite_colliders),
                //consider a set for all movement/systems modify velocity, then collisions/move
                //moves based on velocity
                (
                    // hitfreeze,
                    set_movement_slots,
                    player_collisions,
                )
                    .chain()
                    .before(TransformPropagate)
                    .in_set(PlayerStateSet::Collisions),
            )
                .chain(),
        );
    }
}

pub fn player_stealth(
    mut query: Query<
        (
            &mut Stealthing,
            &mut TransitionQueue,
            &Gent,
        ),
        With<Player>,
    >,
    mut sprites: Query<&mut Sprite>,
    config: Res<PlayerConfig>,
    time: Res<GameTime>,
) {
    for (mut stealthing, mut transitions, gent) in query.iter_mut() {
        let mut sprite = sprites.get_mut(gent.e_gfx).unwrap();
        if stealthing.is_added() {
            // turn player stealth
            sprite.color = sprite.color.with_a(0.5);
        } else {
            stealthing.duration += 1.0 / time.hz as f32;
            if stealthing.duration > config.stealth_duration {
                sprite.color = sprite.color.with_a(1.);

                stealthing.duration = 0.0;
                transitions.push(Stealthing::new_transition(
                    CanStealth::new(&config),
                ));
            }
        }
    }
}
pub fn player_can_stealth(
    mut q_gent: Query<
        (
            &ActionState<PlayerAction>,
            &mut CanStealth,
            &mut TransitionQueue,
            &Gent,
        ),
        (With<Player>, With<Gent>),
    >,
    mut sprites: Query<&mut Sprite, With<PlayerGfx>>,
    time: Res<GameTime>,
    mut rig: ResMut<CameraRig>,
) {
    for (action_state, mut can_stealth, mut transition_queue, gent) in q_gent.iter_mut() {
        can_stealth.remaining_cooldown -= 1.0 / time.hz as f32;
        if can_stealth.is_added() {
            let mut sprite = sprites.get_mut(gent.e_gfx).unwrap();
            sprite.color = sprite.color.with_a(1.0);
        }
        if action_state.just_pressed(&PlayerAction::Stealth) {
            if can_stealth.remaining_cooldown <= 0.0 {
                transition_queue.push(CanStealth::new_transition(
                    Stealthing::default(),
                ));
            } else {
                rig.trauma += 0.23;
            }
        }
    }
}
pub fn player_whirl(
    mut q_gent: Query<
        (
            &ActionState<PlayerAction>,
            &mut WhirlAbility,
            &mut TransitionQueue,
            Option<&Grounded>,
            Option<&Attacking>,
        ),
        (
            With<Player>,
            With<Gent>,
            Without<Dashing>,
        ),
    >,
    time: Res<GameTime>,
    mut command: Commands,
    config: Res<PlayerConfig>,
) {
    let min_ticks = 48.0;
    for (action_state, mut whirl, mut transition_queue, grounded, attacking) in q_gent.iter_mut() {
        let mut stop_whirling = false;
        if action_state.pressed(&PlayerAction::Whirl) {
            // only start if we have enough whirl energy for full rotation
            if whirl.energy - (min_ticks * config.whirl_cost / time.hz as f32) > 0.0
                && grounded.is_some()
            {
                if attacking.is_none() {
                    transition_queue.push(CanAttack::new_transition(
                        Attacking::default(),
                    ));
                }
                whirl.active = true;
            } else {
                stop_whirling = true;
            }
        } else {
            stop_whirling = true;
        }

        if stop_whirling {
            if whirl.active {
                if whirl.active_ticks as f32 > min_ticks {
                    if whirl.active {
                        transition_queue.push(Attacking::new_transition(
                            CanAttack::default(),
                        ));
                    }
                    if let Some(whirl_attack) = whirl.attack_entity {
                        command.entity(whirl_attack).despawn();
                        whirl.attack_entity = None;
                    }
                    whirl.active = false;
                    whirl.active_ticks = 0;
                };
            }
        }

        if whirl.active {
            whirl.active_ticks += 1;
            whirl.energy -= config.whirl_cost / time.hz as f32;
        } else {
            whirl.energy = (whirl.energy + config.whirl_regen / time.hz as f32)
                .clamp(0.0, config.max_whirl_energy);
        }
    }
}

fn hitfreeze(
    mut player_q: Query<
        (
            Entity,
            &mut HitFreezeTime,
            &mut LinearVelocity,
        ),
        (With<Player>),
    >,
    attack_q: Query<(Entity, &Attack)>,
    config: Res<PlayerConfig>,
) {
    // Track if we need to initialize a hitfreeze affect
    for ((attack_entity, attack)) in attack_q.iter() {
        if !attack.damaged.is_empty() {
            // Make sure the entity doing the attack is actually the player
            if let Ok((entity, mut hitfreeze, _)) = player_q.get_mut(attack.attacker) {
                // If its the same exact attack entity as the last time the affect was activated.
                // (for example, if the attack wasn't despawned yet) we don't want to
                // trigger a timer reset again.
                if let Some(hitfreeze_last_entity) = hitfreeze.1 {
                    if hitfreeze_last_entity == attack_entity {
                        continue;
                    }
                }
                hitfreeze.0 = 0;
                hitfreeze.1 = Some(attack_entity);
            }
        }
    }

    for ((entity, mut hitfreeze, mut linear_vel)) in player_q.iter_mut() {
        if hitfreeze.0 < u32::MAX {
            hitfreeze.0 += 1;
        }
        // Where the actual affect is applied.
        // if its desired to check if its being applied in another system, can do a query and this
        // same check,
        if hitfreeze.0 < config.hitfreeze_ticks {
            linear_vel.0 = Vec2::ZERO;
        }
    }
}

fn player_idle(
    mut query: Query<
        (
            &ActionState<PlayerAction>,
            &mut TransitionQueue,
        ),
        (With<Grounded>, With<Idle>, With<Player>),
    >,
) {
    for (action_state, mut transitions) in query.iter_mut() {
        // println!("is idle");
        // check for direction input
        let mut direction: f32 = 0.0;
        // println!("idleing, {:?}", action_state.get_pressed());
        if action_state.pressed(&PlayerAction::Move) {
            direction = action_state.value(&PlayerAction::Move);
            // println!("moving??")
        }
        if direction != 0.0 {
            transitions.push(Idle::new_transition(Running));
        }
    }
}

fn player_move(
    config: Res<PlayerConfig>,
    mut q_gent: Query<
        (
            &mut LinearVelocity,
            &ActionState<PlayerAction>,
            &mut Facing,
            Option<&Grounded>,
            Option<&Stealthing>,
            Option<&Dashing>,
        ),
        (
            Without<PlayerPushback>,
            With<Player>,
        ),
    >,
) {
    for (mut velocity, action_state, mut facing, grounded, stealth, dashing) in q_gent.iter_mut() {
        let mut direction: f32 = 0.0;
        // Uses high starting acceleration, to emulate "shoving" off the ground/start
        // Acceleration is per game tick.
        let initial_accel = config.move_accel_init;
        let accel = config.move_accel;

        // What "%" does our character get slowed down per game tick.
        // Todo: Have this value be determined by tile type at some point?
        let ground_friction = 0.7;
        let stealth_boost = if stealth.is_some() { 1.15 } else { 1.0 };
        direction = action_state.value(&PlayerAction::Move);
        let new_vel = if action_state.just_pressed(&PlayerAction::Move)
            && action_state.value(&PlayerAction::Move) != 0.0
        {
            (velocity.x + accel * direction * ground_friction) * stealth_boost
        } else if action_state.pressed(&PlayerAction::Move)
            && action_state.value(&PlayerAction::Move) != 0.0
        {
            (velocity.x + initial_accel * direction * ground_friction) * stealth_boost
        } else {
            // de-acceleration profile
            if grounded.is_some() {
                velocity.x + ground_friction * -velocity.x
            } else {
                // airtime de-acceleration profile
                if action_state.just_released(&PlayerAction::Move) {
                    velocity.x + initial_accel * 0.5 * action_state.value(&PlayerAction::Move)
                } else {
                    let max_vel = velocity.x.abs();
                    (velocity.x + accel * -velocity.x.signum()).clamp(-max_vel, max_vel)
                }
            }
        };

        if dashing.is_none() {
            velocity.x = new_vel.clamp(
                -config.max_move_vel * stealth_boost,
                config.max_move_vel * stealth_boost,
            );
        }
        if direction > 0.0 {
            *facing = Facing::Right;
        } else if direction < 0.0 {
            *facing = Facing::Left;
        }
    }
}

fn set_movement_slots(
    mut q_gent: Query<(&LinearVelocity, &Gent), (With<Player>)>,
    mut q_gfx_player: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for (velocity, gent) in q_gent.iter_mut() {
        if let Ok(mut player) = q_gfx_player.get_mut(gent.e_gfx) {
            if velocity.length() > 0.001 {
                if velocity.x.abs() > velocity.y.abs() {
                    player.set_slot("MovingVertically", false);
                    player.set_slot("MovingHorizontally", true);
                } else {
                    player.set_slot("MovingVertically", true);
                    player.set_slot("MovingHorizontally", false);
                }
            } else {
                player.set_slot("MovingVertically", false);
                player.set_slot("MovingHorizontally", false);
            }

            if velocity.y > 0.001 {
                player.set_slot("MovingUp", true);
            } else {
                player.set_slot("MovingUp", false);
            }
            if velocity.y < -0.001 {
                player.set_slot("MovingDown", true);
            } else {
                player.set_slot("MovingDown", false);
            }
            if velocity.x.abs() > 50.0 {
                player.set_slot("MovingSideways", true);
            } else {
                player.set_slot("MovingSideways", false);
            }
        }
    }
}

fn player_run(
    mut q_gent: Query<
        (
            &ActionState<PlayerAction>,
            &mut TransitionQueue,
        ),
        (
            With<Player>,
            With<Grounded>,
            With<Running>,
        ),
    >,
) {
    for (action_state, mut transitions) in q_gent.iter_mut() {
        let mut direction: f32 = 0.0;
        if action_state.pressed(&PlayerAction::Move) {
            direction = action_state.value(&PlayerAction::Move);
        }
        //should it account for decel and only transition to idle when player stops completely?
        //shouldnt be able to transition to idle if we also jump
        if direction == 0.0 && action_state.released(&PlayerAction::Jump) {
            transitions.push(Running::new_transition(Idle));
        }
    }
}

fn player_jump(
    mut query: Query<
        (
            &ActionState<PlayerAction>,
            &mut LinearVelocity,
            &mut Jumping,
            &mut TransitionQueue,
        ),
        With<Player>,
    >,
    config: Res<PlayerConfig>,
) {
    for (action_state, mut velocity, mut jumping, mut transitions) in query.iter_mut() {
        //can enter state and first frame jump not pressed if you tap
        //i think this is related to the fixedtimestep input
        // print!("{:?}", action_state.get_pressed());

        let deaccel_rate = config.jump_fall_accel;

        if jumping.is_added() {
            velocity.y += config.jump_vel_init;
        } else {
            if (velocity.y - deaccel_rate < 0.0) || action_state.released(&PlayerAction::Jump) {
                transitions.push(Jumping::new_transition(Falling));
            }
            velocity.y -= deaccel_rate;
        }

        velocity.y = velocity.y.clamp(0., config.jump_vel_init);
    }
}

pub fn player_can_dash(
    mut q_gent: Query<
        (
            &ActionState<PlayerAction>,
            &Facing,
            &mut CanDash,
            &mut LinearVelocity,
            &mut TransitionQueue,
            Option<&mut HitFreezeTime>,
        ),
        (With<Player>, With<Gent>),
    >,
    time: Res<GameTime>,
    config: Res<PlayerConfig>,
    mut rig: ResMut<CameraRig>,
) {
    for (action_state, facing, mut can_dash, mut velocity, mut transition_queue, hitfreeze) in
        q_gent.iter_mut()
    {
        can_dash.remaining_cooldown -= 1.0 / time.hz as f32;
        if action_state.just_pressed(&PlayerAction::Dash) {
            if can_dash.remaining_cooldown <= 0.0 {
                transition_queue.push(CanDash::new_transition(
                    Dashing::default(),
                ));
                velocity.x = config.dash_velocity * facing.direction();
                velocity.y = 0.0;
                if let Some(mut hitfreeze) = hitfreeze {
                    *hitfreeze = HitFreezeTime(u32::MAX, None)
                }
            } else {
                rig.trauma += 0.23;
            }
        }
    }
}

pub fn player_dash(
    mut query: Query<
        (
            &Facing,
            &mut LinearVelocity,
            &mut Dashing,
            &mut TransitionQueue,
            Has<Grounded>,
            Option<&mut HitFreezeTime>,
        ),
        With<Player>,
    >,
    config: Res<PlayerConfig>,
    time: Res<GameTime>,
) {
    for (facing, mut velocity, mut dashing, mut transitions, is_grounded, hitfreeze) in
        query.iter_mut()
    {
        if dashing.is_added() {
            velocity.x = config.dash_velocity * facing.direction();
            velocity.y = 0.0;
            if let Some(mut hitfreeze) = hitfreeze {
                *hitfreeze = HitFreezeTime(u32::MAX, None)
            }
        } else {
            dashing.duration += 1.0 / time.hz as f32;
            if dashing.duration > config.dash_duration {
                dashing.duration = 0.0;
                transitions.push(Dashing::new_transition(CanDash::new(
                    &config,
                )));
                if is_grounded {
                    transitions.push(Running::new_transition(Idle));
                } else {
                    transitions.push(Running::new_transition(Falling));
                }
                transitions.push(Attacking::new_transition(
                    CanAttack::default(),
                ));
            }
        }
    }
}

pub fn player_collisions(
    spatial_query: Res<PhysicsWorld>,
    mut q_gent: Query<
        (
            Entity,
            &mut Transform,
            &mut LinearVelocity,
            &Collider,
            Option<&mut WallSlideTime>,
            Option<&mut Dashing>,
            Option<&WhirlAbility>,
        ),
        (With<Player>),
    >,
    mut q_enemy: Query<(Entity, &mut Collider), (With<Enemy>, Without<Player>)>,
    mut commands: Commands,
    time: Res<GameTime>,
    config: Res<PlayerConfig>,
) {
    for (entity, mut pos, mut linear_velocity, collider, slide, dashing, whirl) in q_gent.iter_mut()
    {
        let mut shape = collider.0.shared_shape().clone();
        let mut original_pos = pos.translation.xy();
        let mut possible_pos = pos.translation.xy();
        let z = pos.translation.z;
        let mut projected_velocity = linear_velocity.xy();
        // println!("initial projected_velocity");
        // dbg!(projected_velocity);
        let mut interaction = InteractionGroups {
            memberships: PLAYER,
            filter: Group::from_bits_truncate(0b10010),
        };

        let mut wall_slide = false;
        let dir = linear_velocity.x.signum();
        // We loop over the shape cast operation to check if the new trajectory might *also* collide.
        // This can happen in a corner for example, where the first collision is on one wall, and
        // so the velocity is only stopped in the x direction, but not the y, so without the extra
        // check with the new velocity and position, the y might clip the player through the roof
        // of the corner.
        //if we are not moving, we can not shapecast in direction of movement
        while let Ok(shape_dir) = Direction2d::new(projected_velocity) {
            if let Some((e, first_hit)) = spatial_query.shape_cast(
                possible_pos,
                shape_dir,
                &*shape,
                projected_velocity.length() / time.hz as f32 + 0.5,
                interaction,
                Some(entity),
            ) {
                //If we are colliding with an enemy
                if let Ok((enemy, mut collider)) = q_enemy.get_mut(e) {
                    //change collision groups to only include ground so on the next loop we can
                    //ignore enemies/check our ground collision
                    interaction = InteractionGroups {
                        memberships: PLAYER,
                        filter: GROUND,
                    };
                    match first_hit.status {
                        //if we are not yet inside the enemy, collide, but not if we are falling
                        //from above
                        TOIStatus::Converged | TOIStatus::OutOfIterations => {
                            // if we are also dashing, or whirling, ignore the collision entirely
                            if dashing.is_none()
                                && (whirl.is_none() || whirl.is_some_and(|w| !w.active))
                            {
                                let sliding_plane = into_vec2(first_hit.normal1);
                                //configurable theshold for collision normal/sliding plane in case of physics instability
                                let threshold = 0.000001;
                                if !(1. - threshold..=1. + threshold).contains(&sliding_plane.y) {
                                    projected_velocity.x = linear_velocity.x
                                        - sliding_plane.x * linear_velocity.xy().dot(sliding_plane);
                                }
                            }
                        },
                        //if we are already inside, modify the enemies collision group and add
                        //Inside so next frame we dont collide with them
                        TOIStatus::Penetrating => {
                            collider.0.set_collision_groups(InteractionGroups {
                                memberships: ENEMY_INSIDE,
                                filter: Group::all(),
                            });
                            commands.entity(enemy).insert(crate::game::enemy::Inside);
                        },
                        //maybe failed never happens?
                        TOIStatus::Failed => println!("failed"),
                    }
                //otherwise we are colliding with the ground
                } else {
                    match first_hit.status {
                        TOIStatus::Converged | TOIStatus::OutOfIterations => {
                            // Applies a very small amount of bounce, as well as sliding to the character
                            // the bounce helps prevent the player from getting stuck.
                            let sliding_plane = into_vec2(first_hit.normal1);

                            let bounce_coefficient = if dashing.is_some() { 0.0 } else { 0.05 };
                            let bounce_force = -sliding_plane
                                * linear_velocity.dot(sliding_plane)
                                * bounce_coefficient;

                            projected_velocity = linear_velocity.xy()
                                - sliding_plane * linear_velocity.xy().dot(sliding_plane);

                            // Applies downward friction only when player tries to push
                            // against the wall while falling. Ignores x component.
                            let friction_coefficient = config.sliding_friction;
                            let friction_force = if projected_velocity.y < -0.0 {
                                // make sure at least 1/2 of player is against the wall
                                // (because it looks wierd to have the character hanging by their head)
                                if let Some((e, first_hit)) = spatial_query.ray_cast(
                                    pos.translation.xy(),
                                    Vec2::new(dir, 0.0),
                                    shape.as_cuboid().unwrap().half_extents.x + 0.1,
                                    true,
                                    InteractionGroups {
                                        memberships: PLAYER,
                                        filter: GROUND,
                                    },
                                    Some(entity),
                                ) {
                                    wall_slide = true;
                                    -(projected_velocity.y * friction_coefficient)
                                } else {
                                    0.0
                                }
                            } else {
                                0.0
                            };
                            let friction_vec = Vec2::new(0.0, friction_force);

                            projected_velocity += friction_vec + bounce_force;

                            possible_pos =
                                (pos.translation.xy() + (shape_dir.xy() * (first_hit.toi - 0.01)));
                        },
                        TOIStatus::Penetrating => {
                            let depenetration = -linear_velocity.0;
                            projected_velocity += depenetration;
                            possible_pos = original_pos;
                        },
                        TOIStatus::Failed => println!("failed"),
                    }
                }
                linear_velocity.0 = projected_velocity;
                pos.translation = possible_pos.extend(z);
            } else {
                break;
            }
        }

        // if the final collision results in zero x velocity, cancel the active dash
        if projected_velocity.x.abs() < 0.00001 {
            if let Some(mut dashing) = dashing {
                dashing.duration = f32::MAX;
            }
        }

        pos.translation =
            (pos.translation.xy() + linear_velocity.xy() * (1.0 / time.hz as f32)).extend(z);

        if let Some(mut slide) = slide {
            if wall_slide {
                slide.0 = 0.0;
            } else {
                slide.0 += 1.0 / time.hz as f32;
            }
        }
    }
}

/// Tries to keep the characters shape caster this far above the ground
///
/// Needs to be non-zero to avoid getting stuck in the ground.
const GROUNDED_THRESHOLD: f32 = 1.0;

fn player_grounded(
    spatial_query: Res<PhysicsWorld>,
    mut query: Query<
        (
            Entity,
            &ShapeCaster,
            &ActionState<PlayerAction>,
            &LinearVelocity,
            &mut Transform,
            &mut TransitionQueue,
            Option<&mut CoyoteTime>,
            &mut JumpCount,
        ),
        (
            With<Player>,
            With<Grounded>,
            Without<Dashing>,
        ),
    >,
    time: Res<GameTime>,
    config: Res<PlayerConfig>,
) {
    // in seconds
    let max_coyote_time = config.max_coyote_time;
    for (
        entity,
        ray_cast_info,
        action_state,
        liner_vel,
        mut position,
        mut transitions,
        coyote_time,
        mut jump_count,
    ) in query.iter_mut()
    {
        let mut time_of_impact = 0.0;
        let is_falling = ray_cast_info
            .cast(&spatial_query, &position, Some(entity))
            .iter()
            .any(|x| {
                time_of_impact = x.1.toi;
                x.1.toi > GROUNDED_THRESHOLD + 0.01
            });
        // Ensures player character lands at the expected x height every time.
        if !is_falling && time_of_impact != 0.0 {
            position.translation.y = position.translation.y - time_of_impact + GROUNDED_THRESHOLD;
        }
        let mut in_c_time = false;
        if let Some(mut c_time) = coyote_time {
            if !is_falling {
                // resets the c_time every time ground gets close again.
                c_time.0 = 0.0;
            } else {
                c_time.0 += (1.0 / time.hz) as f32;
            }
            if c_time.0 < max_coyote_time {
                in_c_time = true;
            }
        };

        //just pressed seems to get missed sometimes... but we need it because pressed makes you
        //jump continuously if held
        //known issue https://github.com/bevyengine/bevy/issues/6183
        if action_state.just_pressed(&PlayerAction::Jump) {
            jump_count.0 = 1;
            transitions.push(Grounded::new_transition(Jumping))
        } else if is_falling {
            if !in_c_time {
                jump_count.0 = 1;
                transitions.push(Grounded::new_transition(Falling))
            }
        }
    }
}

fn player_falling(
    spatial_query: Res<PhysicsWorld>,
    mut query: Query<
        (
            Entity,
            &mut Transform,
            &mut LinearVelocity,
            &ActionState<PlayerAction>,
            &ShapeCaster,
            &mut TransitionQueue,
            &mut JumpCount,
        ),
        (
            With<Player>,
            With<Falling>,
            Without<Dashing>,
        ),
    >,
    time: Res<GameTime>,
    config: Res<PlayerConfig>,
) {
    for (entity, mut transform, mut velocity, action_state, hits, mut transitions, mut jump_count) in
        query.iter_mut()
    {
        let fall_accel = config.fall_accel;
        let mut falling = true;
        if let Some((hit_entity, toi)) = hits.cast(&spatial_query, &transform, Some(entity)) {
            //if we are ~touching the ground
            if (toi.toi + velocity.y * (1.0 / time.hz) as f32) < GROUNDED_THRESHOLD {
                transitions.push(Falling::new_transition(Grounded));
                //stop falling
                velocity.y = 0.0;
                transform.translation.y = transform.translation.y - toi.toi + GROUNDED_THRESHOLD;
                if action_state.pressed(&PlayerAction::Move) {
                    transitions.push(Falling::new_transition(Running));
                } else {
                    transitions.push(Falling::new_transition(Idle));
                }
                falling = false;
            }
        }
        if falling {
            if action_state.just_pressed(&PlayerAction::Jump) && jump_count.0 > 0 {
                velocity.y = 0.0;
                jump_count.0 -= 1;
                
                //println!("air jump: {}", jump_count.0);
                transitions.push(Falling::new_transition(Jumping))

            }   
            if velocity.y > 0.0 {
                velocity.y = velocity.y / 1.2;
            }
            velocity.y -= fall_accel;
            velocity.y = velocity.y.clamp(
                -config.max_fall_vel,
                config.jump_vel_init,
            );
        }
    }
}

fn player_sliding(
    mut commands: Commands,
    mut query: Query<(
        Entity,
        &Gent,
        &ActionState<PlayerAction>,
        &mut TransitionQueue,
        &mut WallSlideTime,
        &mut LinearVelocity,
        &mut JumpCount,
    )>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
    config: Res<PlayerConfig>,
) {
    for (entity, gent, action_state, mut transitions, mut wall_slide_time, mut lin_vel, mut jump_count) in query.iter_mut()
    {
        let mut direction: f32 = 0.0;
        if action_state.pressed(&PlayerAction::Move) {
            direction = action_state.value(&PlayerAction::Move);
        }
        if wall_slide_time.sliding(&config) {
            jump_count.0 = 1;
        }
        if let Ok(player) = gfx_query.get_mut(gent.e_gfx) {
            if wall_slide_time.sliding(&config) && action_state.just_pressed(&PlayerAction::Jump) {
                
                let jump_direction = direction * if wall_slide_time.strict_sliding(&config) {-1.0} else {1.0};

                wall_slide_time.0 = f32::MAX;
                // Move away from the wall a bit so that friction stops
                lin_vel.x = -direction * config.move_accel_init;
                // Give a little boost for the frame that it takes for input to be received
                lin_vel.y = config.fall_accel;

                commands.entity(entity).insert(PlayerPushback::new(
                    jump_direction,
                    Vec2::new(config.wall_pushback, 0.),
                    config.wall_pushback_ticks,
                ));

                jump_count.0 = 2;
                transitions.push(Falling::new_transition(Jumping))
            }
        }
    }
}

fn add_attack(
    mut query: Query<
        (
            &mut TransitionQueue,
            &ActionState<PlayerAction>,
            Option<&CanAttack>,
        ),
        (
            Without<Attacking>,
            Without<Dashing>,
            With<Player>,
        ),
    >,
) {
    for (mut transitions, action_state, maybe_immediate) in query.iter_mut() {
        if action_state.just_pressed(&PlayerAction::Attack) {
            transitions.push(CanAttack::new_transition(
                Attacking::default(),
            ));
        } else if let Some(can_attack) = maybe_immediate {
            if can_attack.immediate {
                transitions.push(CanAttack::new_transition(
                    Attacking::default(),
                ));
            }
        }
    }
}

fn player_attack(
    mut query: Query<
        (
            Entity,
            &Gent,
            &Facing,
            &mut Attacking,
            &mut TransitionQueue,
            &ActionState<PlayerAction>,
            Option<&mut WhirlAbility>,
            Option<&Stealthing>,
        ),
        (With<Player>),
    >,
    mut sprites: Query<&mut Sprite>,
    mut commands: Commands,
    config: Res<PlayerConfig>,
) {
    for (
        entity,
        gent,
        facing,
        mut attacking,
        mut transitions,
        action_state,
        mut whirl,
        maybe_stealthing,
    ) in query.iter_mut()
    {
        let whirl_active = if let Some(whirl) = &whirl {
            whirl.active
        } else {
            false
        };
        if attacking.ticks == 0 || (whirl_active && whirl.as_ref().unwrap().attack_entity.is_none())
        {
            let id = commands
                .spawn((
                    TransformBundle::from_transform(Transform::from_xyz(0.0, 0.0, 0.0)),
                    AnimationCollider(gent.e_gfx),
                    Collider::empty(InteractionGroups::new(
                        PLAYER_ATTACK,
                        ENEMY_HURT,
                    )),
                    if whirl_active {
                        Attack {
                            current_lifetime: 0,
                            max_lifetime: u32::MAX,
                            damage: 20,
                            max_targets: 6,
                            attacker: entity,
                            damaged: Vec::new(),
                            collided: Default::default(),
                            damaged_set: Default::default(),
                            new_group: false,
                            stealthed: maybe_stealthing.is_some(),
                            pushback: None,
                            pushback_applied: false,
                        }
                    } else {
                        let mut att = Attack::new(16, entity); 
                        att.pushback = Some(PlayerPushback::new(
                            -facing.direction(),
                            Vec2::new(config.melee_pushback, 0.),
                            config.melee_pushback_ticks,
                        ));
                        
                        if maybe_stealthing.is_some() {
                            att.damage = att.damage * 2;
                            att.stealthed = true;
                        }

                        att
                    },
                ))
                .set_parent(entity)
                .id();
            if !whirl_active {
                commands.entity(id).insert(Pushback {
                    direction: facing.direction(),
                    strength: 5.,
                });
//                commands.entity(entity).insert(PlayerPushback::new(
//                    -facing.direction(),
//                    Vec2::new(config.melee_pushback, 0.),
//                config.melee_pushback_ticks,
//                ));

            }
            if let Some(mut whirl) = whirl {
                if whirl.active {
                    whirl.attack_entity = Some(id)
                }
            }
            if maybe_stealthing.is_some() {
                let mut sprite = sprites.get_mut(gent.e_gfx).unwrap();
                sprite.color = sprite.color.with_a(1.0);
                transitions.push(Stealthing::new_transition(
                    CanStealth::new(&config),
                ));
            }
        }
        attacking.ticks += 1;
        //if we are in the later half of attacking and another attack input was pressed,
        //indicate an immediate follow up on animation end
        if attacking.ticks >= Attacking::MAX * 8 - 8
            && action_state.just_pressed(&PlayerAction::Attack)
        {
            attacking.followup = true;
        }
        if attacking.ticks == Attacking::MAX * 8 {
            // Keep attacking if whirl is ongoing
            if whirl_active {
                continue;
            }
            if attacking.followup {
                transitions.push(Attacking::new_transition(CanAttack {
                    immediate: true,
                }));
            } else {
                transitions.push(Attacking::new_transition(
                    CanAttack::default(),
                ));
            }
        }
    }
}


fn player_pushback(
    mut query: Query<(
        Entity,
        &mut PlayerPushback,
        &mut LinearVelocity,
    )>,
    mut commands: Commands,
) {
    for (entity, mut knockback, mut velocity) in query.iter_mut() {
        knockback.ticks += 1;

        if knockback.is_added() {

            velocity.x = knockback.x_direction * knockback.strength.x;
            velocity.y = knockback.strength.y;
        }
            
        if knockback.ticks > knockback.max_ticks {
            velocity.x = 0.;
            //velocity.y = 0.;
            
            commands.entity(entity).remove::<PlayerPushback>();
        }
    }
}