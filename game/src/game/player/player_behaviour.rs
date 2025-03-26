use bevy::sprite::Sprite;

//TODO: remove, remove passive check in jumps
use crate::game::player::Passive;
use bevy::transform::TransformSystem::TransformPropagate;
use glam::{Vec2, Vec2Swizzles, Vec3Swizzles};
use leafwing_input_manager::action_state::ActionState;
use rapier2d::geometry::{Group, InteractionGroups};
use rapier2d::parry::query::TOIStatus;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::{
    into_vec2, update_sprite_colliders, AnimationCollider, Collider,
    LinearVelocity, PhysicsWorld, ShapeCaster, ENEMY, ENEMY_HURT, ENEMY_INSIDE,
    GROUND, PLAYER, PLAYER_ATTACK,
};
use theseeker_engine::script::ScriptPlayer;

use super::arc_attack::{Arrow, Projectile};
use super::player_weapon::{
    PlayerCombatStyle, PlayerMeleeWeapon, PushbackValues,
};
use super::{
    dash_icon_fx, player_dash_fx, AttackBundle, CanStealth, DashIcon,
    DashStrike, DashType, JumpCount, Knockback, Passives, PlayerStats,
    Pushback, StatType, Stealthing, Whirling,
};
use crate::game::attack::{
    Attack, DownwardAttack, Hit, SelfPushback, Stealthed,
};
use crate::game::enemy::Enemy;
use crate::game::gentstate::{Facing, TransitionQueue, Transitionable};
use crate::game::pickups::{
    PassiveDescriptionNode, PassiveEntity, PickupDrop, PickupHint, PickupType,
    PICKUP_RANGE_SQUARED,
};
use crate::game::player::{
    Attacking, CanAttack, CanDash, CoyoteTime, Dashing, Falling, Grounded,
    HitFreezeTime, Idle, Jumping, Player, PlayerAction, PlayerConfig,
    PlayerGfx, PlayerStateSet, Running, WallSlideTime, WhirlAbility,
};
use crate::prelude::*;
use crate::ui::popup::{PopupTimer, PopupUi};
use crate::StateDespawnMarker;
use crate::{camera::CameraShake, game::player::PlayerStatMod};

/// Player behavior systems.
/// Do stuff here in states and add transitions to other states by pushing
/// to a TransitionQueue.
pub(crate) struct PlayerBehaviorPlugin;

impl Plugin for PlayerBehaviorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                (
                    player_pickup_interact
                        .run_if(any_with_component::<PickupDrop>),
                    player_idle.run_if(any_with_component::<Idle>),
                    add_attack,
                    player_stealth,
                    player_whirl_charge.before(player_whirl),
                    (
                        player_whirl,
                        bow_auto_aim.after(player_move).run_if(
                            resource_equals(PlayerCombatStyle::Ranged),
                        ),
                    )
                        .before(player_attack),
                    player_attack.run_if(any_with_component::<Attacking>),
                    player_restore_velocity
                        .after(player_attack)
                        .run_if(any_with_component::<CanAttack>),
                    player_move,
                    player_can_dash.run_if(any_with_component::<CanDash>),
                    player_can_stealth.run_if(any_with_component::<CanStealth>),
                    player_run.run_if(any_with_component::<Running>),
                    player_jump.run_if(any_with_component::<Jumping>),
                    player_dash_strike.run_if(any_with_component::<DashStrike>),
                    (
                        player_dash,
                        player_dash_fx.after(player_dash),
                    )
                        .run_if(any_with_component::<Dashing>),
                    dash_icon_fx
                        .after(player_dash_fx)
                        .run_if(any_with_component::<DashIcon>),
                    player_grounded.run_if(any_with_component::<Grounded>),
                    (
                        player_falling,
                        player_sliding.before(player_jump),
                    )
                        .run_if(any_with_component::<Falling>),
                    reset_player_jump_on_downward_attack.after(player_falling),
                    crate::game::physics::knockback
                        .run_if(any_with_component::<Knockback>)
                        .before(player_jump)
                        .after(player_sliding),
                )
                    .in_set(PlayerStateSet::Behavior)
                    .before(update_sprite_colliders),
                // consider a set for all movement/systems modify velocity, then collisions/move
                // moves based on velocity
                (
                    // hitfreeze,
                    set_movement_slots,
                    player_collisions,
                )
                    .chain()
                    .before(TransformPropagate)
                    .in_set(PlayerStateSet::Collisions),
            )
                .chain()
                .run_if(in_state(AppState::InGame)),
        );
    }
}

fn reset_player_jump_on_downward_attack(
    mut p_query: Query<&mut JumpCount, With<Player>>,
    query: Query<&Attack, (Added<Hit>, With<DownwardAttack>)>,
) {
    for attack in query.iter() {
        if let Ok(mut jump_count) = p_query.get_mut(attack.attacker) {
            jump_count.reset();
        }
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
            sprite.color = sprite.color.with_alpha(0.5);
        } else {
            stealthing.duration += 1.0 / time.hz as f32;
            if stealthing.duration > config.stealth_duration {
                sprite.color = sprite.color.with_alpha(1.);

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
            &PlayerStatMod,
            &Gent,
        ),
        (With<Player>, With<Gent>),
    >,
    mut sprites: Query<&mut Sprite, With<PlayerGfx>>,
    time: Res<GameTime>,
    mut commands: Commands,
) {
    for (action_state, mut can_stealth, mut transition_queue, statmod, gent) in
        q_gent.iter_mut()
    {
        can_stealth.remaining_cooldown -= statmod.cdr / time.hz as f32;
        // Return to base sprite color when exiting stealth
        if can_stealth.is_added() {
            let mut sprite = sprites.get_mut(gent.e_gfx).unwrap();
            sprite.color = sprite.color.with_alpha(1.0);
        }
        if action_state.just_pressed(&PlayerAction::Stealth) {
            if can_stealth.remaining_cooldown <= 0.0 {
                transition_queue.push(CanStealth::new_transition(
                    Stealthing::default(),
                ));
            } else {
                commands.insert_resource(CameraShake::new(2.0, 1.0, 5.0));
            }
        }
    }
}

// TODO: change to using Added<attack::Hit>
fn _hitfreeze(
    mut player_q: Query<
        (
            Entity,
            &mut HitFreezeTime,
            &mut LinearVelocity,
        ),
        With<Player>,
    >,
    attack_q: Query<(Entity, &Attack)>,
    config: Res<PlayerConfig>,
) {
    // Track if we need to initialize a hitfreeze affect
    for (attack_entity, attack) in attack_q.iter() {
        if !attack.damaged_set.is_empty() {
            // Make sure the entity doing the attack is actually the player
            if let Ok((_, mut hitfreeze, _)) = player_q.get_mut(attack.attacker)
            {
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

    for (_, mut hitfreeze, mut linear_vel) in player_q.iter_mut() {
        hitfreeze.0 = hitfreeze.0.saturating_add(1);
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
        // check for direction input
        if action_state.clamped_value(&PlayerAction::Move) != 0.0 {
            transitions.push(Idle::new_transition(Running));
        }
    }
}

fn player_move(
    mut q_gent: Query<
        (
            &PlayerStats,
            &PlayerStatMod,
            &mut LinearVelocity,
            &ActionState<PlayerAction>,
            &mut Facing,
            Has<Grounded>,
            Has<Stealthing>,
            Has<Dashing>,
            Has<DashStrike>,
        ),
        (Without<Knockback>, With<Player>),
    >,
) {
    for (
        stats,
        stat_mod,
        mut velocity,
        action_state,
        mut facing,
        is_grounded,
        is_stealth,
        is_dashing,
        is_dash_strike,
    ) in q_gent.iter_mut()
    {
        // Uses high starting acceleration, to emulate "shoving" off the ground/start
        // Acceleration is per game tick.
        let initial_accel = stats.get(StatType::MoveAccelInit);
        let accel = stats.get(StatType::MoveAccel);

        // What "%" does our character get slowed down per game tick.
        // Todo: Have this value be determined by tile type at some point?
        let ground_friction = 0.7;
        let stealth_boost = get_stealth_boost(is_stealth);
        let controllable = !is_dash_strike;
        let direction = action_state.clamped_value(&PlayerAction::Move);
        // FIXME: lwim new version wont allow just_pressed for Axis
        // let new_vel = if action_state.just_pressed(&PlayerAction::Move) &&
        let new_vel = if action_state.clamped_value(&PlayerAction::Move) != 0.0
            && controllable
        {
            velocity.x
                + initial_accel
                    * direction
                    * ground_friction
                    * stealth_boost
                    * stat_mod.speed
        // } else if action_state.pressed(&PlayerAction::Move)
        //     && action_state.clamped_value(&PlayerAction::Move) != 0.0
        //     && controllable
        // {
        //     velocity.x
        //         + accel
        //             * direction
        //             * ground_friction
        //             * stealth_boost
        //             * stat_mod.speed
        } else {
            // de-acceleration profile
            if is_grounded {
                velocity.x + ground_friction * -velocity.x
            } else {
                // airtime de-acceleration profile
                // if action_state.just_released(&PlayerAction::Move) {
                if direction == 0. {
                    velocity.x
                        + initial_accel
                            * 0.5
                            * action_state.clamped_value(&PlayerAction::Move)
                } else {
                    let max_vel = velocity.x.abs();
                    (velocity.x + accel * -velocity.x.signum())
                        .clamp(-max_vel, max_vel)
                }
            }
        };

        if !is_dashing {
            velocity.x = new_vel.clamp(
                -stats.get(StatType::MoveVelMax)
                    * stealth_boost
                    * stat_mod.speed,
                stats.get(StatType::MoveVelMax)
                    * stealth_boost
                    * stat_mod.speed,
            );
        }
        if direction > 0.0 {
            *facing = Facing::Right;
        } else if direction < 0.0 {
            *facing = Facing::Left;
        }
    }
}

fn get_stealth_boost(stealth: bool) -> f32 {
    if stealth {
        1.15
    } else {
        1.0
    }
}

fn set_movement_slots(
    mut q_gent: Query<(&LinearVelocity, &Gent), With<Player>>,
    mut q_gfx_player: Query<
        &mut ScriptPlayer<SpriteAnimation>,
        With<PlayerGfx>,
    >,
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
        if action_state.clamped_value(&PlayerAction::Move) == 0.0
            && action_state.released(&PlayerAction::Jump)
        {
            transitions.push(Running::new_transition(Idle));
        }
    }
}

pub fn player_jump(
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
    for (action_state, mut velocity, mut jumping, mut transitions) in
        query.iter_mut()
    {
        let deaccel_rate = config.jump_fall_accel;

        if jumping.is_added() {
            velocity.y += config.jump_vel_init;
        } else {
            if (velocity.y - deaccel_rate < 0.0)
                || action_state.released(&PlayerAction::Jump)
            {
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
            &PlayerStatMod,
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
    mut commands: Commands,
) {
    for (
        action_state,
        statmod,
        facing,
        mut can_dash,
        mut velocity,
        mut transition_queue,
        hitfreeze,
    ) in q_gent.iter_mut()
    {
        can_dash.remaining_cooldown -= statmod.cdr / time.hz as f32;
        if action_state.just_pressed(&PlayerAction::Dash) {
            if can_dash.remaining_cooldown <= 0.0 {
                let dash_action = Dashing::from_action_state(action_state);
                dash_action.set_player_velocity(&mut velocity, facing, &config);
                transition_queue.push(CanDash::new_transition(dash_action));

                if let Some(mut hitfreeze) = hitfreeze {
                    *hitfreeze = HitFreezeTime(u32::MAX, None)
                }
            } else {
                commands.insert_resource(CameraShake::new(2.0, 1.0, 5.0));
            }
        }
    }
}

pub fn player_dash_strike(
    mut query: Query<
        (
            Entity,
            &Gent,
            &Facing,
            &mut DashStrike,
            &mut TransitionQueue,
            Has<Grounded>,
            Has<Stealthing>,
            &PlayerStatMod,
        ),
        With<Player>,
    >,
    mut commands: Commands,
    config: Res<PlayerConfig>,
    melee_weapon: Res<PlayerMeleeWeapon>,
) {
    for (
        entity,
        gent,
        facing,
        mut strike,
        mut transitions,
        is_grounded,
        is_stealthed,
        player_stat_mod,
    ) in query.iter_mut()
    {
        if strike.ticks == 10 {
            add_dash_strike_collider(
                &mut commands,
                entity,
                gent,
                facing,
                is_stealthed,
                player_stat_mod.attack,
                melee_weapon.pushback_values(&config),
            );
        }
        strike.ticks += 1;
        if strike.ticks == DashStrike::MAX * 8 {
            transitions.push(DashStrike::new_transition(
                CanDash::new(&config, &DashType::Downward),
            ));
            exit_dash(&mut transitions, is_grounded);
        }
    }
}

pub fn player_dash(
    mut query: Query<
        (
            &PlayerStats,
            &Facing,
            &mut LinearVelocity,
            &mut Dashing,
            &mut TransitionQueue,
            Has<Grounded>,
            Has<Stealthing>,
            Option<&mut HitFreezeTime>,
        ),
        With<Player>,
    >,
    config: Res<PlayerConfig>,
    time: Res<GameTime>,
) {
    for (
        stats,
        facing,
        mut velocity,
        mut dashing,
        mut transitions,
        is_grounded,
        is_stealth,
        hitfreeze,
    ) in query.iter_mut()
    {
        if dashing.is_added() {
            dashing.set_player_velocity(&mut velocity, facing, &config);
            if let Some(mut hitfreeze) = hitfreeze {
                *hitfreeze = HitFreezeTime(u32::MAX, None)
            }
        } else {
            dashing.duration += 1.0 / time.hz as f32;
            if dashing.duration > dashing.dash_duration(&config) {
                dashing.duration = 0.0;
                // slow our velocity to the players normal max velocity, without adjusting our trajectory
                let stealth_boost = get_stealth_boost(is_stealth);
                let max_x_vel = stats.get(StatType::MoveVelMax) * stealth_boost;

                if velocity.x.abs() > max_x_vel {
                    let slowdown_factor = max_x_vel / velocity.x.abs();
                    velocity.x *= slowdown_factor;
                    velocity.y *= slowdown_factor;
                }

                if dashing.hit {
                    // stop all movement when we slam into the ground, but leave our velocity when we hit through an enemy
                    if dashing.hit_ground {
                        velocity.x = 0.0;
                        velocity.y = 0.0;
                    }
                    transitions.push(Dashing::new_transition(
                        DashStrike::default(),
                    ));
                } else {
                    transitions.push(Dashing::new_transition(CanDash::new(
                        &config,
                        &dashing.dash_type,
                    )));
                    exit_dash(&mut transitions, is_grounded);
                }
            }
        }
    }
}

fn exit_dash(transitions: &mut TransitionQueue, is_grounded: bool) {
    if is_grounded {
        transitions.push(Running::new_transition(Idle));
    } else {
        transitions.push(Running::new_transition(Falling));
    }
    transitions.push(Attacking::new_transition(
        CanAttack::default(),
    ));
}

fn trigger_dash_strike(
    mut commands: &mut Commands,
    mut dashing: &mut Dashing,
    grounded: bool,
) {
    dashing.duration = f32::MAX;
    dashing.hit = true;
    dashing.hit_ground = grounded;
    commands.insert_resource(CameraShake::new(3.5, 0.4, 2.0));
}

fn add_dash_strike_collider(
    mut commands: &mut Commands,
    entity: Entity,
    gent: &Gent,
    facing: &Facing,
    stealthed: bool,
    attack_mod: f32,
    pushback_values: PushbackValues,
) {
    let PushbackValues {
        self_pushback,
        self_pushback_ticks,
        pushback,
        pushback_ticks,
    } = pushback_values;

    let attack = commands
        .spawn((
            TransformBundle::from_transform(Transform::from_xyz(0.0, 0.0, 0.0)),
            AnimationCollider(gent.e_gfx),
            // TODO: ? ColliderMeta
            Collider::empty(InteractionGroups::new(
                PLAYER_ATTACK,
                ENEMY_HURT,
            )),
            Attack::new(16, entity, 69. * attack_mod),
            SelfPushback(Knockback::new(
                Vec2::new(self_pushback * -facing.direction(), 0.),
                self_pushback_ticks,
            )),
            Pushback(Knockback::new(
                Vec2::new(facing.direction() * pushback, 0.),
                pushback_ticks,
            )),
        ))
        .set_parent(entity)
        .id();

    if stealthed {
        commands.entity(attack).insert(Stealthed);
    };
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
            Has<DashStrike>,
            Option<&mut Whirling>,
        ),
        With<Player>,
    >,
    mut q_enemy: Query<(Entity, &mut Collider), (With<Enemy>, Without<Player>)>,
    mut commands: Commands,
    time: Res<GameTime>,
    config: Res<PlayerConfig>,
) {
    for (
        entity,
        mut pos,
        mut linear_velocity,
        collider,
        slide,
        mut dashing,
        is_dash_strike,
        whirling,
    ) in q_gent.iter_mut()
    {
        let mut shape = collider.0.shared_shape().clone();
        let mut original_pos = pos.translation.xy();
        let mut possible_pos = pos.translation.xy();
        let z = pos.translation.z;
        let mut projected_velocity = linear_velocity.xy();
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
        // if we are not moving, we can not shapecast in direction of movement
        while let Ok(shape_dir) = Dir2::new(projected_velocity) {
            if let Some((e, first_hit)) = spatial_query.shape_cast(
                possible_pos,
                shape_dir,
                &*shape,
                projected_velocity.length() / time.hz as f32 + 0.5,
                interaction,
                Some(entity),
            ) {
                // If we are colliding with an enemy
                if let Ok((enemy, mut collider)) = q_enemy.get_mut(e) {
                    // change collision groups to only include ground so on the next loop we can
                    // ignore enemies/check our ground collision
                    interaction = InteractionGroups {
                        memberships: PLAYER,
                        filter: GROUND,
                    };
                    match first_hit.status {
                        // if we are not yet inside the enemy, collide, but not if we are falling
                        // from above
                        TOIStatus::Converged | TOIStatus::OutOfIterations => {
                            // If we are dashing downwards, immediately cancel the dash, shake the camera, and start the attack animation, but don't do anything else.
                            if let Some(mut dashing) = dashing.as_mut() {
                                if dashing.is_down_dash() {
                                    // currently, this feels out of place, as the strike stops just short of the ground.
                                    // we currently do not have any flying enemies, so disabling dash strikes for enemies.

                                    // trigger_dash_strike(
                                    //     &mut commands,
                                    //     dashing,
                                    //     false,
                                    // );
                                }
                            }

                            // if we are also dashing, or whirling, ignore the collision
                            if dashing.is_none()
                                && whirling.is_none()
                                && !is_dash_strike
                            {
                                let sliding_plane =
                                    into_vec2(first_hit.normal1);
                                // configurable threshold for collision normal/sliding plane in case of physics instability
                                let threshold = 0.000001;
                                if !(1. - threshold..=1. + threshold)
                                    .contains(&sliding_plane.y)
                                {
                                    projected_velocity.x = linear_velocity.x
                                        - sliding_plane.x
                                            * linear_velocity
                                                .xy()
                                                .dot(sliding_plane);
                                }
                            }
                        },
                        // if we are already inside, modify the enemies collision group and add
                        // Inside so next frame we dont collide with them
                        TOIStatus::Penetrating => {
                            collider.0.set_collision_groups(
                                InteractionGroups {
                                    memberships: ENEMY_INSIDE,
                                    filter: Group::all(),
                                },
                            );
                            commands
                                .entity(enemy)
                                .insert(crate::game::enemy::Inside);
                        },
                        // maybe failed never happens?
                        TOIStatus::Failed => {
                            println!("player/enemy collision failed")
                        },
                    }
                // otherwise we are colliding with the ground
                } else {
                    match first_hit.status {
                        TOIStatus::Converged | TOIStatus::OutOfIterations => {
                            // Applies a very small amount of bounce, as well as sliding to the character
                            // the bounce helps prevent the player from getting stuck.
                            let sliding_plane = into_vec2(first_hit.normal1);

                            // If we are dashing downwards, immediately cancel the dash, shake the camera, and start the attack animation
                            if let Some(mut dashing) = dashing.as_mut() {
                                if dashing.is_down_dash() {
                                    trigger_dash_strike(
                                        &mut commands,
                                        dashing,
                                        true,
                                    );
                                }
                            }

                            let bounce_coefficient =
                                if dashing.is_some() || is_dash_strike {
                                    0.0
                                } else {
                                    0.05
                                };
                            let bounce_force = -sliding_plane
                                * linear_velocity.dot(sliding_plane)
                                * bounce_coefficient;

                            projected_velocity = linear_velocity.xy()
                                - sliding_plane
                                    * linear_velocity.xy().dot(sliding_plane);

                            // Applies downward friction only when player tries to push
                            // against the wall while falling. Ignores x component.
                            let friction_coefficient = config.sliding_friction;
                            let friction_force = if projected_velocity.y < -0.0
                            {
                                // make sure at least 1/2 of player is against the wall
                                // (because it looks weird to have the character hanging by their head)
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
                                        InteractionGroups {
                                            memberships: PLAYER,
                                            filter: GROUND,
                                        },
                                        Some(entity),
                                    )
                                    .is_some()
                                {
                                    wall_slide = true;
                                    -(projected_velocity.y
                                        * friction_coefficient)
                                } else {
                                    0.0
                                }
                            } else {
                                0.0
                            };
                            let friction_vec = Vec2::new(0.0, friction_force);

                            projected_velocity += friction_vec + bounce_force;

                            possible_pos = pos.translation.xy()
                                + (shape_dir.xy() * (first_hit.toi - 0.01));
                        },
                        TOIStatus::Penetrating => {
                            let depenetration = -linear_velocity.0;
                            projected_velocity += depenetration;
                            possible_pos = original_pos;
                        },
                        TOIStatus::Failed => {
                            println!("player/ground collision failed")
                        },
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

        pos.translation = (pos.translation.xy()
            + linear_velocity.xy() * (1.0 / time.hz as f32))
            .extend(z);

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
            &mut Transform,
            &mut TransitionQueue,
            Option<&mut CoyoteTime>,
            &mut JumpCount,
            &Passives,
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
        mut position,
        mut transitions,
        coyote_time,
        mut jump_count,
        passives,
    ) in query.iter_mut()
    {
        let ray_cast =
            ray_cast_info.cast(&spatial_query, &position, Some(entity));
        let falling_toi = ray_cast
            .iter()
            .find(|x| x.1.toi > GROUNDED_THRESHOLD + 0.01);
        let is_falling = falling_toi.is_some();
        let time_of_impact = falling_toi.map_or(0.0, |x| x.1.toi);

        // Adjust position if TOI is nonzero.
        if !is_falling && time_of_impact != 0.0 {
            position.translation.y =
                position.translation.y - time_of_impact + GROUNDED_THRESHOLD;
        }
        let mut in_c_time = false;
        if let Some(mut c_time) = coyote_time {
            if !is_falling {
                // resets the coyote timer every time ground gets close again.
                c_time.0 = 0.0;
            } else {
                c_time.0 += 1.0 / time.hz as f32;
            }
            if c_time.0 < max_coyote_time {
                in_c_time = true;
            };
        }
        // When the player is grounded (i.e. not falling), fully restore available jumps.
        if !is_falling {
            jump_count.0 = 0;
        }

        if action_state.just_pressed(&PlayerAction::Jump) {
            let allowed_jumps = if passives.contains(&Passive::RabbitsFoot) {
                3
            } else {
                2
            };
            if jump_count.0 < allowed_jumps {
                jump_count.0 += 1;
                transitions.push(Grounded::new_transition(Jumping));
            }
        } else if is_falling && !in_c_time {
            transitions.push(Grounded::new_transition(Falling));
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
            &Passives,
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
    for (
        entity,
        mut transform,
        mut velocity,
        action_state,
        hits,
        mut transitions,
        mut jump_count,
        passives,
    ) in query.iter_mut()
    {
        let fall_accel = config.fall_accel;
        let mut falling = true;
        if let Some((_, toi)) =
            hits.cast(&spatial_query, &transform, Some(entity))
        {
            // if we are ~touching the ground
            if (toi.toi + velocity.y * (1.0 / time.hz) as f32)
                < GROUNDED_THRESHOLD
            {
                transitions.push(Falling::new_transition(Grounded));
                velocity.y = 0.0;
                transform.translation.y =
                    transform.translation.y - toi.toi + GROUNDED_THRESHOLD;
                if action_state.clamped_value(&PlayerAction::Move) != 0.0 {
                    transitions.push(Falling::new_transition(Running));
                } else {
                    transitions.push(Falling::new_transition(Idle));
                }
                falling = false;
            }
        }
        if falling {
            if action_state.just_pressed(&PlayerAction::Jump) {
                let allowed_jumps = if passives.contains(&Passive::RabbitsFoot)
                {
                    3
                } else {
                    2
                };
                if jump_count.0 < allowed_jumps {
                    velocity.y = 0.0;
                    jump_count.0 += 1;
                    transitions.push(Falling::new_transition(Jumping));
                }
            }
            if velocity.y > 0.0 {
                velocity.y /= 1.2;
            }
            velocity.y -= fall_accel;
            velocity.y = velocity.y.clamp(
                -config.max_fall_vel,
                config.jump_vel_init,
            );
        }
    }
}

pub fn player_sliding(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &ActionState<PlayerAction>,
            &mut TransitionQueue,
            &mut WallSlideTime,
            &mut LinearVelocity,
            &mut JumpCount,
        ),
        With<Gent>,
    >,
    config: Res<PlayerConfig>,
) {
    for (
        entity,
        action_state,
        mut transitions,
        mut wall_slide_time,
        mut lin_vel,
        mut jump_count,
    ) in query.iter_mut()
    {
        let direction = action_state.clamped_value(&PlayerAction::Move);
        if wall_slide_time.sliding(&config) {
            jump_count.0 = 1;
        }
        if wall_slide_time.sliding(&config)
            && action_state.just_pressed(&PlayerAction::Jump)
        {
            let jump_direction = direction
                * if wall_slide_time.strict_sliding(&config) {
                    -1.0
                } else {
                    1.0
                };

            wall_slide_time.0 = f32::MAX;
            // Move away from the wall a bit so that friction stops
            lin_vel.x = -direction * config.move_accel_init;
            // Give a little boost for the frame that it takes for input to be received
            lin_vel.y = config.fall_accel;

            commands.entity(entity).insert(Knockback::new(
                Vec2::new(
                    config.wall_pushback * jump_direction,
                    0.,
                ),
                config.wall_pushback_ticks,
            ));

            jump_count.0 = 2;
            transitions.push(Falling::new_transition(Jumping))
        }
    }
}

fn add_attack(
    mut query: Query<
        (
            &mut TransitionQueue,
            &ActionState<PlayerAction>,
            Option<&CanAttack>,
            Option<&WhirlAbility>,
            Has<Grounded>,
        ),
        (
            Without<Attacking>,
            Without<Whirling>,
            Without<Dashing>,
            Without<DashStrike>,
            With<Player>,
        ),
    >,
    player_config: Res<PlayerConfig>,
    time: Res<GameTime>,
) {
    for (
        mut transitions,
        action_state,
        maybe_immediate,
        maybe_whirl_ability,
        is_grounded,
    ) in query.iter_mut()
    {
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
        if let Some(whirl) = maybe_whirl_ability {
            if whirl.energy
                - (Whirling::MIN_TICKS as f32 * player_config.whirl_cost
                    / time.hz as f32)
                > 0.0
                && is_grounded
                && action_state.pressed(&PlayerAction::Whirl)
            {
                transitions.push(CanAttack::new_transition(
                    Whirling::default(),
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
            &Transform,
            Option<&WallSlideTime>,
            &mut PlayerStats,
            &mut Attacking,
            &mut TransitionQueue,
            &ActionState<PlayerAction>,
            &PlayerStatMod,
            Has<Stealthing>,
            Has<Grounded>,
        ),
        (With<Player>, Without<Whirling>),
    >,
    mut commands: Commands,
    config: Res<PlayerConfig>,
    combat_style: Res<PlayerCombatStyle>,
    melee_weapon: Res<PlayerMeleeWeapon>,
    time: Res<GameTime>,
) {
    for (
        entity,
        gent,
        facing,
        transform,
        wall_slide_time,
        mut player_stats,
        mut attacking,
        mut transitions,
        action_state,
        stat_mod,
        stealthed,
        grounded,
    ) in query.iter_mut()
    {
        if attacking.ticks == 0 {
            let attack = match *combat_style {
                PlayerCombatStyle::Ranged => {
                    let mut animation: ScriptPlayer<SpriteAnimation> =
                        ScriptPlayer::default();
                    animation.play_key("anim.player.BowBasicArrow");
                    animation.set_slot("Start", true);

                    let is_player_pressed_against_wall = wall_slide_time
                        .is_some_and(|s| s.is_pressed_against_wall(&time));
                    let arrow_direction = if is_player_pressed_against_wall {
                        -facing.direction()
                    } else {
                        facing.direction()
                    };
                    // Used for the collider length and the starting position offset
                    let arrow_length = 30.0;
                    let vel = LinearVelocity(
                        Vec2::X * arrow_direction * config.arrow_velocity,
                    );
                    let mut arrow_transform = *transform;
                    // Add an offset to avoid arrows immediately colliding with walls when fired
                    let arrow_pos_offset = arrow_length * 0.5 * arrow_direction;
                    arrow_transform.translation.x += arrow_pos_offset;

                    if !is_player_pressed_against_wall {
                        commands.entity(entity).insert(Knockback::new(
                            Vec2::new(
                                -facing.direction() * config.bow_self_pushback,
                                0.,
                            ),
                            config.bow_self_pushback_ticks,
                        ));
                    }

                    commands
                        .spawn((
                            Arrow,
                            SpriteBundle {
                                transform: arrow_transform,
                                ..Default::default()
                            },
                            Projectile { vel },
                            Collider::cuboid(
                                //TODO: temp fix of extending collider length to account for
                                //tunneling
                                arrow_length,
                                3.0,
                                InteractionGroups::new(
                                    PLAYER_ATTACK,
                                    ENEMY_HURT | GROUND,
                                ),
                            ),
                            Attack::new(
                                192,
                                entity,
                                config.bow_attack_damage * stat_mod.attack,
                            )
                            .with_max_targets(1),
                            Pushback(Knockback::new(
                                Vec2::new(
                                    facing.direction() * config.bow_pushback,
                                    0.,
                                ),
                                config.bow_pushback_ticks,
                            )),
                            animation,
                            StateDespawnMarker,
                        ))
                        .id()
                },
                PlayerCombatStyle::Melee => {
                    let (
                        damage,
                        self_pushback,
                        self_pushback_ticks,
                        pushback,
                        pushback_ticks,
                    ) = match *melee_weapon {
                        PlayerMeleeWeapon::Hammer => (
                            config.hammer_attack_damage,
                            config.hammer_self_pushback,
                            config.hammer_self_pushback_ticks,
                            config.hammer_pushback,
                            config.hammer_pushback_ticks,
                        ),
                        PlayerMeleeWeapon::Sword => (
                            config.sword_attack_damage,
                            config.sword_self_pushback,
                            config.sword_self_pushback_ticks,
                            config.sword_pushback,
                            config.sword_pushback_ticks,
                        ),
                    };

                    // Slow the player down when they attack with the Hammer
                    if let PlayerMeleeWeapon::Hammer = *melee_weapon {
                        player_stats.set(
                            StatType::MoveVelMax,
                            config.hammer_max_move_vel,
                        );
                    }

                    let is_attacking_downwards =
                        !grounded && action_state.pressed(&PlayerAction::Fall);
                    let self_knockback_strength = if is_attacking_downwards {
                        Vec2::new(0.0, self_pushback * 2.0)
                    } else {
                        Vec2::new(self_pushback * -facing.direction(), 0.)
                    };
                    let mut attack_entity_commands = commands.spawn((
                        TransformBundle::from_transform(Transform::from_xyz(
                            0.0, 0.0, 0.0,
                        )),
                        AnimationCollider(gent.e_gfx),
                        // TODO: ? ColliderMeta
                        Collider::empty(InteractionGroups::new(
                            PLAYER_ATTACK,
                            ENEMY_HURT,
                        )),
                        Attack::new(16, entity, damage * stat_mod.attack),
                        SelfPushback(Knockback::new(
                            self_knockback_strength,
                            self_pushback_ticks,
                        )),
                    ));
                    attack_entity_commands.set_parent(entity);

                    if !is_attacking_downwards {
                        attack_entity_commands.insert(Pushback(
                            Knockback::new(
                                Vec2::new(facing.direction() * pushback, 0.),
                                pushback_ticks,
                            ),
                        ));
                    } else {
                        attack_entity_commands.insert(DownwardAttack);
                    }

                    attack_entity_commands.id()
                },
            };

            if stealthed {
                commands.entity(attack).insert(Stealthed);
            };
        }

        attacking.ticks += 1;
        let maximum_ticks = (((Attacking::MAX * 8) as f32 / stat_mod.cdr)
            as u32)
            .clamp(Attacking::MIN * 8, Attacking::MAX * 8);

        // if we are in the later half of attacking and another attack input was pressed,
        // indicate an immediate follow up on animation end
        if attacking.ticks >= maximum_ticks - 8
            && action_state.just_pressed(&PlayerAction::Attack)
        {
            attacking.followup = true;
        }

        // leave attacking state
        if attacking.ticks >= maximum_ticks {
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

/// Restores the player movement velocity after attacking.
fn player_restore_velocity(
    mut query: Query<&mut PlayerStats, (With<Player>, Added<CanAttack>)>,
) {
    for mut stats in query.iter_mut() {
        stats.reset_stat(StatType::MoveVelMax);
    }
}

pub fn player_whirl_charge(
    mut query: Query<(&mut WhirlAbility, &PlayerStatMod), Without<Whirling>>,
    config: Res<PlayerConfig>,
    time: Res<GameTime>,
) {
    for (mut whirl, statmod) in query.iter_mut() {
        whirl.energy = (whirl.energy
            + (config.whirl_regen * statmod.cdr) / time.hz as f32)
            .clamp(0.0, config.max_whirl_energy);
    }
}

pub fn player_whirl(
    mut gent_query: Query<
        (
            Entity,
            &ActionState<PlayerAction>,
            &mut TransitionQueue,
            &mut Whirling,
            &mut WhirlAbility,
            &mut PlayerStats,
            &PlayerStatMod,
            Has<Stealthing>,
            &Gent,
            &Passives,
        ),
        (
            With<Player>,
            Without<Dashing>,
            Without<DashStrike>,
        ),
    >,
    // attacks which have had their collider changed by the AnimationCollider system
    // TODO: need to not change collider unless there is a collider?
    attack_query: Query<
        &Attack,
        (
            With<AnimationCollider>,
            // Changed<Collider>,
        ),
    >,
    mut commands: Commands,
    melee_weapon: Res<PlayerMeleeWeapon>,
    config: Res<PlayerConfig>,
    time: Res<GameTime>,
) {
    for (
        entity,
        action_state,
        mut transitions,
        mut whirling,
        mut whirl_ability,
        mut player_stats,
        stat_mod,
        is_stealthing,
        gent,
        passives,
    ) in gent_query.iter_mut()
    {
        whirling.ticks += 1;
        whirl_ability.energy -= config.whirl_cost / time.hz as f32;
        if action_state.pressed(&PlayerAction::Whirl)
            || whirling.ticks < Whirling::MIN_TICKS
        {
            // Slow the player down when they attack with the Hammer
            if let PlayerMeleeWeapon::Hammer = *melee_weapon {
                player_stats.set(
                    StatType::MoveVelMax,
                    config.hammer_max_move_vel,
                );
            }

            if let Some(attack_entity) = whirling.attack_entity {
                // if the attack entities collider was changed, set the attack to none
                if attack_query.get(attack_entity).is_err() {
                    whirling.attack_entity = None;
                }
            // if there is no attack, spawn a new one
            } else {
                let lifetime = melee_weapon.attack_lifetime(passives);
                let damage = melee_weapon.base_damage() * stat_mod.attack;
                let new_attack = commands
                    .spawn((
                        AttackBundle {
                            attack: Attack::new(lifetime, entity, damage),
                            collider: Collider::empty(InteractionGroups::new(
                                PLAYER_ATTACK,
                                ENEMY_HURT,
                            )),
                        },
                        TransformBundle::from_transform(Transform::from_xyz(
                            0.0, 0.0, 0.0,
                        )),
                        AnimationCollider(gent.e_gfx),
                    ))
                    .set_parent(entity)
                    .id();
                if is_stealthing {
                    commands.entity(new_attack).insert(Stealthed);
                }
                whirling.attack_entity = Some(new_attack);
            }
        } else {
            // leave whirling state if button is not pressed and we are past min ticks
            if whirling.ticks >= Whirling::MIN_TICKS {
                transitions.push(Whirling::new_transition(
                    CanAttack::default(),
                ));
            }
        }
        if whirl_ability.energy < 0. {
            transitions.push(Whirling::new_transition(
                CanAttack::default(),
            ));
        }
    }
}

pub fn bow_auto_aim(
    mut q_gent: Query<(&mut Facing, &Transform), (With<Player>, With<Gent>)>,
    q_enemy: Query<Entity, With<Enemy>>,
    spatial_query: Res<PhysicsWorld>,
) {
    for (mut facing, transform) in q_gent.iter_mut() {
        let is_facing_enemies = spatial_query
            .ray_cast(
                transform.translation.xy(),
                Vec2::X * facing.direction(),
                f32::MAX,
                true,
                InteractionGroups {
                    memberships: PLAYER,
                    filter: ENEMY | GROUND,
                },
                None,
            )
            .is_some_and(|(entity, _)| q_enemy.contains(entity));

        let is_facing_away_from_enemy = spatial_query
            .ray_cast(
                transform.translation.xy(),
                Vec2::NEG_X * facing.direction(),
                f32::MAX,
                true,
                InteractionGroups {
                    memberships: PLAYER,
                    filter: ENEMY | GROUND,
                },
                None,
            )
            .is_some_and(|(entity, _)| q_enemy.contains(entity));

        if !is_facing_enemies && is_facing_away_from_enemy {
            *facing = facing.invert();
        }
    }
}

fn player_pickup_interact(
    mut query: Query<
        (
            &Transform,
            &ActionState<PlayerAction>,
            &mut Passives,
        ),
        With<Player>,
    >,
    pickup_query: Query<(Entity, &PickupDrop, &Transform)>,
    passive_descriptions: Query<
        (Entity, &PassiveEntity),
        With<PassiveDescriptionNode>,
    >,
    pickup_hint: Query<Entity, With<PickupHint>>,
    mut commands: Commands,
) {
    for (p_transform, action_state, mut passives) in query.iter_mut() {
        if action_state.just_pressed(&PlayerAction::Interact) {
            // Get Pickups in Range and pick up a single one

            let p_pos = p_transform.translation.truncate();

            for (entity, pickup, transform) in pickup_query.iter() {
                let dist =
                    p_pos.distance_squared(transform.translation.truncate());

                println!("dist: {}", dist);

                if dist <= PICKUP_RANGE_SQUARED {
                    match &pickup.p_type {
                        PickupType::PassiveDrop(passive) => {
                            println!("passive pickup!!!");
                            passives.add_passive(passive.clone());
                            // Despawn the passive description UI node for the picked up passive.
                            if let Some((passive_description, _)) =
                                passive_descriptions.iter().find(
                                    |(_, passive_entity)| {
                                        passive_entity.get() == entity
                                    },
                                )
                            {
                                commands
                                    .entity(passive_description)
                                    .despawn_recursive();
                            }
                        },
                        PickupType::Seed(_, (_, word)) => {
                            commands
                                .popup()
                                .insert(PopupTimer::default())
                                .with_children(|popup| {
                                    popup.row().with_children(|row| {
                                        row.text(word);
                                    });
                                });
                        },
                    }
                    // Despawn the Pickup popup hint
                    for entity in &pickup_hint {
                        commands.entity(entity).despawn_recursive();
                    }
                    // Despawn the PickupDrop entity from the map
                    commands.entity(entity).despawn_recursive();
                    break;
                }
            }
        }
    }
}
