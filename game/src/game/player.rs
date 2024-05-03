use bevy::transform::TransformSystem::TransformPropagate;
use leafwing_input_manager::{axislike::VirtualAxis, prelude::*};
use rapier2d::geometry::{Group, InteractionGroups};
use rapier2d::parry::query::TOIStatus;
use theseeker_engine::assets::config::{update_field, DynamicConfig};
use theseeker_engine::gent::{Gent, GentPhysicsBundle};
use theseeker_engine::physics::{
    into_vec2, Collider, LinearVelocity, PhysicsWorld, ShapeCaster, ENEMY, GROUND, PLAYER,
    PLAYER_ATTACK,
};
use theseeker_engine::{
    animation::SpriteAnimationBundle, assets::animation::SpriteAnimation,
    gent::TransformGfxFromGent, script::ScriptPlayer,
};

use crate::game::{attack::*, gentstate::*};
use crate::prelude::*;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (setup_player.run_if(in_state(GameState::Playing)))
                .before(PlayerStateSet::Transition)
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            OnEnter(GameState::Paused),
            (
                debug_player,
                crate::game::enemy::debug_enemy,
            )
                .chain(),
        )
        .add_plugins((
            InputManagerPlugin::<PlayerAction>::default(),
            PlayerBehaviorPlugin,
            PlayerTransitionPlugin,
            PlayerAnimationPlugin,
        ));

        #[cfg(feature = "dev")]
        app.add_systems(
            GameTickUpdate,
            debug_player_states
                .run_if(in_state(GameState::Playing))
                .after(PlayerStateSet::Transition),
        );
    }
}

///set to order the player behavior, state transitions, and animations relative to eachother
#[derive(SystemSet, Clone, PartialEq, Eq, Debug, Hash)]
pub enum PlayerStateSet {
    Behavior,
    Transition,
    Animation,
}

//TODO: change to player spawnpoint
#[derive(Bundle, LdtkEntity, Default)]
pub struct PlayerBlueprintBundle {
    marker: PlayerBlueprint,
}

#[derive(Bundle)]
pub struct PlayerGentBundle {
    player: Player,
    marker: Gent,
    phys: GentPhysicsBundle,
    coyote_time: CoyoteTime,
}

#[derive(Bundle)]
pub struct PlayerGfxBundle {
    marker: PlayerGfx,
    gent2gfx: TransformGfxFromGent,
    sprite: SpriteSheetBundle,
    animation: SpriteAnimationBundle,
}

#[derive(Component, Default)]
pub struct PlayerBlueprint;

#[derive(Component)]
pub struct Player;

#[derive(Component)]
pub struct PlayerGfx {
    pub e_gent: Entity,
}

#[derive(Actionlike, PartialEq, Eq, Clone, Copy, Hash, Debug, Reflect)]
pub enum PlayerAction {
    Move,
    Jump,
    Attack,
}

fn debug_player_states(
    query: Query<
        AnyOf<(
            Ref<Running>,
            Ref<Idle>,
            Ref<Falling>,
            Ref<Jumping>,
            Ref<Grounded>,
        )>,
        With<Player>,
    >,
) {
    for states in query.iter() {
        // println!("{:?}", states);
        let (running, idle, falling, jumping, grounded) = states;
        let mut states_string: String = String::new();
        if let Some(running) = running {
            if running.is_added() {
                states_string.push_str("added running, ");
            }
        }
        if let Some(idle) = idle {
            if idle.is_added() {
                states_string.push_str("added idle, ");
            }
        }
        if let Some(falling) = falling {
            if falling.is_added() {
                states_string.push_str("added falling, ");
            }
        }
        if let Some(jumping) = jumping {
            if jumping.is_added() {
                states_string.push_str("added jumping, ");
            }
        }
        if let Some(grounded) = grounded {
            if grounded.is_added() {
                states_string.push_str("added grounded, ");
            }
        }
        if !states_string.is_empty() {
            println!("{}", states_string);
        }

        // let components = entity.archetype().sparse_set_components();
        // for item in components {
        // print!("{:?}", item);
        // }
    }
}

// fn debug_player(world: &World, query: Query<Entity, With<PlayerGfx>>) {
fn debug_player(world: &World, query: Query<Entity, With<Player>>) {
    for entity in query.iter() {
        let components = world.inspect_entity(entity);
        for component in components.iter() {
            println!("{:?}", component.name());
        }
    }
}

fn setup_player(
    mut q: Query<(&mut Transform, Entity), Added<PlayerBlueprint>>,
    mut commands: Commands,
) {
    for (mut xf_gent, e_gent) in q.iter_mut() {
        //TODO: proper way of ensuring z is correct
        //why is this getting changed? xpbd?
        xf_gent.translation.z = 15.;
        println!("{:?}", xf_gent);
        let e_gfx = commands.spawn(()).id();
        commands.entity(e_gent).insert((
            Name::new("Player"),
            PlayerGentBundle {
                player: Player,
                marker: Gent { e_gfx },
                phys: GentPhysicsBundle {
                    collider: Collider::cuboid(
                        4.0,
                        10.0,
                        InteractionGroups {
                            memberships: PLAYER,
                            //should be more specific
                            filter: Group::all(),
                        },
                    ),
                    shapecast: ShapeCaster {
                        shape: Collider::cuboid(4.0, 10.0, InteractionGroups::none())
                            .0
                            .shared_shape()
                            .clone(),
                        origin: Vec2::new(0.0, 0.0),
                        max_toi: f32::MAX,
                        direction: Direction2d::NEG_Y,
                        //something seems sus here
                        interaction: InteractionGroups {
                            memberships: PLAYER,
                            filter: GROUND,
                        },
                    },
                    linear_velocity: LinearVelocity(Vec2::ZERO),
                    // layers: CollisionLayers::new([Layer::Player], [Layer::EnemyAttack]),
                },
                coyote_time: Default::default(),
            },
            Facing::Right,
            Health {
                current: 600,
                max: 600,
            },
            //have to use builder here *i think* because of different types between keycode and
            //axis
            InputManagerBundle::<PlayerAction> {
                action_state: ActionState::default(),
                input_map: InputMap::default()
                    .insert(PlayerAction::Jump, KeyCode::Space)
                    .insert(PlayerAction::Jump, KeyCode::KeyW)
                    .insert(PlayerAction::Jump, KeyCode::ArrowUp)
                    .insert(
                        PlayerAction::Move,
                        VirtualAxis::from_keys(KeyCode::KeyA, KeyCode::KeyD),
                    )
                    .insert(
                        PlayerAction::Move,
                        VirtualAxis::from_keys(KeyCode::ArrowLeft, KeyCode::ArrowRight),
                    )
                    .insert(PlayerAction::Attack, KeyCode::Enter)
                    .insert(PlayerAction::Attack, KeyCode::KeyJ)
                    .build(),
            },
            Falling,
            TransitionQueue::default(),
            AddQueue::default(),
        ));
        commands.entity(e_gfx).insert((PlayerGfxBundle {
            marker: PlayerGfx { e_gent },
            //marker: Gfxent { e_gent},
            gent2gfx: TransformGfxFromGent {
                pixel_aligned: false,
                gent: e_gent,
            },
            sprite: SpriteSheetBundle {
                transform: *xf_gent,
                ..Default::default()
            },
            animation: Default::default(),
        },));
        // println!("player spawned")
    }
}

struct PlayerTransitionPlugin;

impl Plugin for PlayerTransitionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (transition.run_if(any_with_component::<TransitionQueue>),)
                // .chain()
                .in_set(PlayerStateSet::Transition)
                .after(PlayerStateSet::Behavior)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

// States
// states are components which are added to the entity on transition.
// an entity can be in multiple states at once, eg Grounded and Running/Idle
// Impl Playerstate for each state
// Impl Transitionable<T: GentState> for each state that that should be able to be transitioned
// from by a state
// pub trait GentState: Component<Storage = SparseStorage> {}

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct Idle;
impl GentState for Idle {}
impl GenericState for Idle {}

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct Running;
impl GentState for Running {}
impl GenericState for Running {}

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct Falling;
impl GentState for Falling {}
impl GenericState for Falling {}

#[derive(Component, Debug)]
#[component(storage = "SparseSet")]
pub struct Jumping;
impl GentState for Jumping {}
impl GenericState for Jumping {}

#[derive(Component, Default, Debug)]
pub struct CoyoteTime(f32);

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct Grounded;
impl GentState for Grounded {}
//cant be Idle or Running if not Grounded
impl Transitionable<Jumping> for Grounded {
    type Removals = (Grounded, Idle, Running);
}
//cant be Idle or Running if not Grounded
impl Transitionable<Falling> for Grounded {
    type Removals = (Grounded, Idle, Running);
}

#[derive(Component, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct Attacking {
    ticks: u32,
}
impl Attacking {
    const STARTUP: u32 = 1;
    const MAX: u32 = 5;
}
impl GentState for Attacking {}

impl Transitionable<CanAttack> for Attacking {
    type Removals = (Attacking);
}

#[derive(Component, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct CanAttack;
impl GentState for CanAttack {}

impl Transitionable<Attacking> for CanAttack {
    type Removals = (CanAttack);
}

///Player behavior systems.
///Do stuff here in states and add transitions to other states by pushing
///to a TransitionQueue.
struct PlayerBehaviorPlugin;

impl Plugin for PlayerBehaviorPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PlayerConfig::default());
        app.add_systems(GameTickUpdate, load_player_config);
        app.add_systems(
            GameTickUpdate,
            (
                player_idle.run_if(any_with_component::<Idle>),
                add_attack,
                player_attack.run_if(any_with_component::<Attacking>),
                player_move,
                player_run.run_if(any_with_component::<Running>),
                player_jump.run_if(any_with_component::<Jumping>),
                player_grounded.run_if(any_with_component::<Grounded>),
                player_falling.run_if(any_with_component::<Falling>),
                //consider a set for all movement/systems modify velocity, then collisions/move
                //moves based on velocity
                player_collisions
                    .after(player_move)
                    .after(player_grounded)
                    .after(player_jump)
                    .after(player_falling)
                    .before(TransformPropagate),
            )
                .in_set(PlayerStateSet::Behavior),
        );
    }
}

#[derive(Resource, Debug, Default)]
pub struct PlayerConfig {
    /// The maximum horizontal velocity the player can move at.
    ///
    /// (in pixels/second)
    max_move_vel: f32,

    /// The maximum downward velocity the player can fall at.
    ///
    /// (in pixels/second)
    max_fall_vel: f32,

    /// The initial acceleration applied to the player for the first tick they start moving.
    ///
    /// (in pixels/second^2)
    move_accel_init: f32,

    /// The acceleration applied to the player while they continue moving horizontally.
    ///
    /// (in pixels/second^2)
    move_accel: f32,

    /// How much velocity does the player have at the moment they jump?
    ///
    /// (in pixels/second)
    jump_vel_init: f32,

    /// How fast does the player accelerate downward while holding down the jump button?
    ///
    /// (in pixels/second^2)
    jump_fall_accel: f32,

    /// How fast does the player accelerate downward while in the falling state?
    /// (ie: after releasing the jump key)
    ///
    /// (in pixels/second^2)
    fall_accel: f32,

    /// How many seconds does our characters innate hover boots work?
    max_coyote_time: f32,
}

fn load_player_config(
    mut ev_asset: EventReader<AssetEvent<DynamicConfig>>,
    cfgs: Res<Assets<DynamicConfig>>,
    preloaded: Res<PreloadedAssets>,
    mut player_config: ResMut<PlayerConfig>,
    mut commands: Commands,
    mut initialized_config: Local<bool>,
) {
    // convert from asset key string to bevy handle
    let Some(cfg_handle) = preloaded.get_single_asset::<DynamicConfig>("cfg.player") else {
        return;
    };
    // The reason we do this here instead of in an AssetEvent::Added match arm, is because
    // the Added match arm fires before preloaded updates with the asset key; as a result
    // you can't tell what specific DynamicConfig loaded in like that.
    if !*initialized_config {
        if let Some(cfg) = cfgs.get(cfg_handle.clone()) {
            update_player_config(&mut player_config, cfg);
            println!("init:");
            dbg!(&player_config);
        }
        *initialized_config = true;
    }
    for ev in ev_asset.read() {
        match ev {
            AssetEvent::Modified { id } => {
                if let Some(cfg) = cfgs.get(*id) {
                    if cfg_handle.id() == *id {
                        println!("before:");
                        dbg!(&player_config);
                        update_player_config(&mut player_config, cfg);
                        println!("after:");
                        dbg!(&player_config);
                    }
                }
            },
            _ => {},
        }
    }
}

#[rustfmt::skip]
fn update_player_config(config: &mut PlayerConfig, cfg: &DynamicConfig) {
    let mut errors = Vec::new();

    update_field(&mut errors, &cfg.0, "max_move_vel", |val| config.max_move_vel = val);
    update_field(&mut errors, &cfg.0, "max_fall_vel", |val| config.max_fall_vel = val);
    update_field(&mut errors, &cfg.0, "move_accel_init", |val| config.move_accel_init = val);
    update_field(&mut errors, &cfg.0, "move_accel", |val| config.move_accel = val);
    update_field(&mut errors, &cfg.0, "jump_vel_init", |val| config.jump_vel_init = val);
    update_field(&mut errors, &cfg.0, "jump_fall_accel", |val| config.jump_fall_accel = val);
    update_field(&mut errors, &cfg.0, "fall_accel", |val| config.fall_accel = val);
    update_field(&mut errors, &cfg.0, "max_coyote_time", |val| config.max_coyote_time = val);

   for error in errors{
       warn!("failed to load player cfg value: {}", error);
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
    time: Res<GameTime>,
    config: Res<PlayerConfig>,
    mut q_gent: Query<
        (
            &mut LinearVelocity,
            &ActionState<PlayerAction>,
            &mut Facing,
            Option<&Grounded>,
            &Gent,
        ),
        (With<Player>),
    >,
    mut q_gfx_player: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for (mut velocity, action_state, mut facing, grounded, gent) in q_gent.iter_mut() {
        let mut direction: f32 = 0.0;
        // Uses high starting acceleration, to emulate "shoving" off the ground/start
        // Acceleration is per game tick.
        let initial_accel = config.move_accel_init;
        let accel = config.move_accel;

        // What "%" does our character get slowed down per game tick.
        // Todo: Have this value be determined by tile type at some point?
        let ground_friction = 0.7;

        let new_vel = if action_state.just_pressed(&PlayerAction::Move) {
            direction = action_state.value(&PlayerAction::Move);
            velocity.x + accel * direction * ground_friction
        } else if action_state.pressed(&PlayerAction::Move) {
            direction = action_state.value(&PlayerAction::Move);
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
        velocity.x = new_vel.clamp(
            -config.max_move_vel,
            config.max_move_vel,
        );

        if let Ok(mut player) = q_gfx_player.get_mut(gent.e_gfx) {
            // if direction > 0.0 {
            //     player.set_slot("DirectionRight", true);
            //     player.set_slot("DirectionLeft", false);
            // } else if direction < 0.0 {
            //     player.set_slot("DirectionRight", false);
            //     player.set_slot("DirectionLeft", true);
            // }
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
        if direction > 0.0 {
            *facing = Facing::Right;
        } else if direction < 0.0 {
            *facing = Facing::Left;
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

//TODO: load jump properties from script/animation (velocity/accel + ticks/frames)
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

fn player_collisions(
    spatial_query: Res<PhysicsWorld>,
    mut q_gent: Query<
        (
            Entity,
            &mut Transform,
            &mut LinearVelocity,
            &Collider,
        ),
        (With<Player>),
    >,
    time: Res<GameTime>,
) {
    for (entity, mut pos, mut linear_velocity, collider) in q_gent.iter_mut() {
        let mut shape = collider.0.shared_shape().clone();
        let mut tries = 0;
        let mut original_pos = pos.translation.xy();

        // We loop over the shape cast operation to check if the new trajectory might *also* collide.
        // This can happen in a corner for example, where the first collision is on one wall, and
        // so the velocity is only stopped in the x direction, but not the y, so without the extra
        // check with the new velocity and position, the y might clip the player through the roof
        // of the corner.
        //if we are not moving, we can not shapecast in direction of movement
        while let Ok(shape_dir) = Direction2d::new(linear_velocity.0) {
            if let Some((e, first_hit)) = spatial_query.shape_cast(
                pos.translation.xy(),
                shape_dir,
                &*shape,
                linear_velocity.length() / time.hz as f32 + 0.5,
                InteractionGroups {
                    memberships: PLAYER,
                    filter: GROUND,
                },
                Some(entity),
            ) {
                if first_hit.status != TOIStatus::Penetrating {
                    // Applies a very small amount of bounce, as well as sliding to the character
                    // the bounce helps prevent the player from getting stuck.
                    let sliding_plane = into_vec2(first_hit.normal1);

                    let bounce_coefficient = 0.05;
                    let bounce_force =
                        -sliding_plane * linear_velocity.dot(sliding_plane) * bounce_coefficient;

                    let projected_velocity = linear_velocity.xy()
                        - sliding_plane * linear_velocity.xy().dot(sliding_plane);
                    linear_velocity.0 = projected_velocity + bounce_force;

                    let new_pos = pos.translation.xy() + (shape_dir.xy() * (first_hit.toi - 0.01));
                    pos.translation.x = new_pos.x;
                    pos.translation.y = new_pos.y;
                } else if tries > 1 {
                    // If we tried a few times and still penetrating, just abort the whole movement
                    // thing entirely. This scenario rarely occurs, so stopping movement is fine.
                    pos.translation.x = original_pos.x;
                    pos.translation.y = original_pos.y;
                    linear_velocity.0 = Vec2::ZERO;
                    break;
                }
                tries += 1;
            } else {
                break;
            }
            if tries > 5 {
                break;
            }
        }
        let z = pos.translation.z;
        pos.translation =
            (pos.translation.xy() + linear_velocity.xy() * (1.0 / time.hz as f32)).extend(z);
    }
}
// }

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
        (With<Player>, With<Grounded>),
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
            .cast(&*spatial_query, &position, Some(entity))
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
        (With<Player>, With<Falling>),
    >,
    time: Res<GameTime>,
    config: Res<PlayerConfig>,
) {
    for (entity, mut transform, mut velocity, action_state, hits, mut transitions) in
        query.iter_mut()
    {
        let fall_accel = config.fall_accel;
        let mut falling = true;
        if let Some((hit_entity, toi)) = hits.cast(
            &*spatial_query,
            &transform,
            Some(entity),
        ) {
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
            &Facing,
            &mut Attacking,
            &mut TransitionQueue,
        ),
        (With<Player>),
    >,
    mut commands: Commands,
) {
    for (entity, facing, mut attacking, mut transitions) in query.iter_mut() {
        if attacking.ticks == Attacking::STARTUP * 8 {
            commands
                .spawn((
                    TransformBundle::from_transform(Transform::from_xyz(
                        10. * facing.direction(),
                        0.,
                        0.,
                    )),
                    Collider::cuboid(
                        10.,
                        10.,
                        InteractionGroups::new(PLAYER_ATTACK, ENEMY),
                    ),
                    Attack::new(16),
                ))
                .set_parent(entity);
        }
        attacking.ticks += 1;
        if attacking.ticks == Attacking::MAX * 8 {
            transitions.push(Attacking::new_transition(CanAttack));
        }
    }
    //3 attack chain,
}

///play animations here, run after transitions
struct PlayerAnimationPlugin;

impl Plugin for PlayerAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                player_idle_animation,
                player_falling_animation,
                player_jumping_animation,
                player_running_animation,
                player_attacking_animation,
                sprite_flip,
            )
                .in_set(PlayerStateSet::Animation)
                .after(PlayerStateSet::Transition)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

fn player_idle_animation(
    i_query: Query<
        &Gent,
        Or<(
            (Added<Idle>, Without<Attacking>),
            (With<Idle>, Added<CanAttack>),
        )>,
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for gent in i_query.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            player.play_key("anim.player.Idle")
        }
    }
}

//TODO: add FallForward
fn player_falling_animation(
    f_query: Query<
        &Gent,
        Or<(
            (Added<Falling>, Without<Attacking>),
            (With<Falling>, Added<CanAttack>),
        )>,
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for gent in f_query.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            player.play_key("anim.player.Fall")
        }
    }
}

fn player_jumping_animation(
    f_query: Query<
        &Gent,
        Or<(
            (Added<Jumping>, Without<Attacking>),
            (With<Jumping>, Added<CanAttack>),
        )>,
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for gent in f_query.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            player.play_key("anim.player.Jump")
        }
    }
}

fn player_running_animation(
    r_query: Query<
        &Gent,
        Or<(
            (Added<Running>, Without<Attacking>),
            (With<Running>, Added<CanAttack>),
        )>,
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for gent in r_query.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            player.play_key("anim.player.Run")
        }
    }
}

fn player_attacking_animation(
    r_query: Query<(&Gent, Has<Falling>, Has<Jumping>, Has<Running>), Added<Attacking>>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for (gent, is_falling, is_jumping, is_running) in r_query.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            if is_falling || is_jumping {
                player.play_key("anim.player.SwordBasicAir")
            } else if is_running {
                player.play_key("anim.player.SwordBasicRun")
            } else {
                player.play_key("anim.player.SwordBasicIdle")
            }
        }
    }
    //have to check if first or 2nd attack, play diff anim
    //also check for up attack?
}

fn sprite_flip(
    query: Query<(&Facing, &Gent)>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
    mut current_direction: Local<bool>,
    mut old_direction: Local<bool>,
) {
    for (facing, gent) in query.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            *old_direction = *current_direction;
            match facing {
                Facing::Right => {
                    //TODO: toggle facing script action
                    player.set_slot("DirectionRight", true);
                    player.set_slot("DirectionLeft", false);
                    *current_direction = true;   
                },
                Facing::Left => {
                    player.set_slot("DirectionRight", false);
                    player.set_slot("DirectionLeft", true);
                    *current_direction = false;
                },
            }
            
            // lazy change detection cause I can't be asked to learn proper bevy way lel ~c12
            if *old_direction != *current_direction {
                player.set_slot("DirectionChanged", true);
            } else {
                player.set_slot("DirectionChanged", false);
            }
        
        }
    }
}
