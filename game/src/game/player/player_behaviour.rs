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

use bevy::transform::TransformSystem::TransformPropagate;
use glam::{Vec2, Vec2Swizzles, Vec3Swizzles};
use leafwing_input_manager::action_state::ActionState;
use rapier2d::geometry::{Group, InteractionGroups};
use rapier2d::parry::query::TOIStatus;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::condition::any_matching;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::{
    into_vec2, AnimationCollider, Collider, LinearVelocity, PhysicsWorld, ShapeCaster, ENEMY,
    GROUND, PLAYER, PLAYER_ATTACK,
};
use theseeker_engine::prelude::{GameTickUpdate, GameTime};
use theseeker_engine::script::ScriptPlayer;

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
                    player_whirl.before(player_attack),
                    player_attack.run_if(any_with_component::<Attacking>),
                    player_move,
                    player_can_dash.run_if(any_with_component::<CanDash>),
                    player_run.run_if(any_with_component::<Running>),
                    player_jump.run_if(any_with_component::<Jumping>),
                    player_dash.run_if(any_with_component::<Dashing>),
                    player_grounded.run_if(any_with_component::<Grounded>),
                    player_falling.run_if(any_with_component::<Falling>),
                    player_sliding
                        .before(player_jump)
                        .run_if(any_matching::<(With<Falling>,)>()),
                )
                    .in_set(PlayerStateSet::Behavior),
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

pub fn player_whirl(
    mut q_gent: Query<
        (
            &ActionState<PlayerAction>,
            &mut WhirlAbility,
            &mut TransitionQueue,
            Option<&Grounded>,
            Option<&Attacking>,
        ),
        (With<Player>, With<Gent>),
    >,
    time: Res<GameTime>,
    config: Res<PlayerConfig>,
) {
    for (action_state, mut whirl, mut transition_queue, grounded, attacking) in q_gent.iter_mut() {
        if action_state.pressed(&PlayerAction::Whirl) {
            if whirl.energy > 0.0 && grounded.is_some() {
                if attacking.is_none() {
                    transition_queue.push(CanAttack::new_transition(
                        Attacking::default(),
                    ));
                }
                whirl.active = true;
                whirl.energy += 1.0 / time.hz as f32;
            } else {
                whirl.active = false;
            }
        } else {
            whirl.active = false;
            whirl.energy += 1.0 / time.hz as f32;
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
            &Gent,
            Option<&Dashing>,
        ),
        (With<Player>),
    >,
) {
    for (mut velocity, action_state, mut facing, grounded, gent, dashing) in q_gent.iter_mut() {
        let mut direction: f32 = 0.0;
        // Uses high starting acceleration, to emulate "shoving" off the ground/start
        // Acceleration is per game tick.
        let initial_accel = config.move_accel_init;
        let accel = config.move_accel;

        // What "%" does our character get slowed down per game tick.
        // Todo: Have this value be determined by tile type at some point?
        let ground_friction = 0.7;

        direction = action_state.value(&PlayerAction::Move);
        let new_vel = if action_state.just_pressed(&PlayerAction::Move) {
            velocity.x + accel * direction * ground_friction
        } else if action_state.pressed(&PlayerAction::Move) {
            velocity.x + initial_accel * direction * ground_friction
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
                -config.max_move_vel,
                config.max_move_vel,
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
                }
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
        ),
        (With<Player>),
    >,
    mut q_enemy: Query<Entity, With<Enemy>>,
    time: Res<GameTime>,
    config: Res<PlayerConfig>,
) {
    for (entity, mut pos, mut linear_velocity, collider, slide, dashing) in q_gent.iter_mut() {
        let mut shape = collider.0.shared_shape().clone();
        // let mut tries = 0;
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
                if q_enemy.get(e).is_ok() {
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
                            // if we are also dashing, ignore the collision entirely
                            if dashing.is_none() {
                                let sliding_plane = into_vec2(first_hit.normal1);
                                //configurable theshold for collision normal/sliding plane in case of physics instability
                                let threshold = 0.000001;
                                if !(1. - threshold..=1. + threshold).contains(&sliding_plane.y) {
                                    projected_velocity.x = linear_velocity.x
                                        - sliding_plane.x * linear_velocity.xy().dot(sliding_plane);
                                }
                            }
                        },
                        //if we are already inside, do nothing
                        TOIStatus::Penetrating => {},
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
                            println!("converged");
                        },
                        TOIStatus::Penetrating => {
                            let depenetration = -linear_velocity.0;
                            projected_velocity += depenetration;
                            dbg!(projected_velocity);
                            println!("penetrating");
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
            transitions.push(Grounded::new_transition(Jumping))
        } else if is_falling {
            if !in_c_time {
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
    for (entity, mut transform, mut velocity, action_state, hits, mut transitions) in
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
    mut query: Query<(
        &Gent,
        &ActionState<PlayerAction>,
        &mut TransitionQueue,
        &mut WallSlideTime,
        &mut LinearVelocity,
    )>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
    config: Res<PlayerConfig>,
) {
    for (gent, action_state, mut transitions, mut wall_slide_time, mut lin_vel) in query.iter_mut()
    {
        let mut direction: f32 = 0.0;
        if action_state.pressed(&PlayerAction::Move) {
            direction = action_state.value(&PlayerAction::Move);
        }
        if let Ok(player) = gfx_query.get_mut(gent.e_gfx) {
            if wall_slide_time.sliding(&config) && action_state.just_pressed(&PlayerAction::Jump) {
                wall_slide_time.0 = f32::MAX;
                // Move away from the wall a bit so that friction stops
                lin_vel.x = -direction * config.move_accel_init;
                // Give a little boost for the frame that it takes for input to be received
                lin_vel.y = config.fall_accel;
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
        ),
        (Without<Attacking>, With<Player>),
    >,
) {
    for (mut transitions, action_state) in query.iter_mut() {
        if action_state.pressed(&PlayerAction::Attack) {
            transitions.push(CanAttack::new_transition(
                Attacking::default(),
            ));
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
            Option<&WhirlAbility>,
        ),
        (With<Player>),
    >,
    mut commands: Commands,
) {
    for (entity, gent, facing, mut attacking, mut transitions, whirl) in query.iter_mut() {
        if attacking.ticks == Attacking::STARTUP * 8 {
            commands
                .spawn((
                    TransformBundle::from_transform(Transform::from_xyz(0.0, 0.0, 0.0)),
                    AnimationCollider(gent.e_gfx),
                    Collider::empty(InteractionGroups::new(
                        PLAYER_ATTACK,
                        ENEMY,
                    )),
                    Attack::new(16, entity),
                    Pushback {
                        direction: facing.direction(),
                        strength: 10.,
                    },
                ))
                .set_parent(entity);
        }
        attacking.ticks += 1;
        if attacking.ticks == Attacking::MAX * 8 {
            // Keep attacking if whirl is ongoing
            if let Some(whirl) = whirl {
                if whirl.active {
                    continue;
                }
            }
            transitions.push(Attacking::new_transition(CanAttack));
        }
    }
}
