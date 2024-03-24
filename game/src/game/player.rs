use leafwing_input_manager::{axislike::VirtualAxis, prelude::*};
use theseeker_engine::{
    animation::SpriteAnimationBundle,
    assets::animation::SpriteAnimation,
    gent::{GentPhysicsBundle, TransformGfxFromGent},
    script::ScriptPlayer,
};

use crate::game::{attack::Health, gentstate::*};
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

#[derive(Bundle, LdtkEntity, Default)]
pub struct PlayerBlueprintBundle {
    marker: PlayerBlueprint,
}

#[derive(Bundle)]
pub struct PlayerGentBundle {
    marker: PlayerGent,
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
pub struct PlayerGent {
    pub e_gfx: Entity,
}

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
        With<PlayerGent>,
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
fn debug_player(world: &World, query: Query<Entity, With<PlayerGent>>) {
    for entity in query.iter() {
        let components = world.inspect_entity(entity);
        for component in components.iter() {
            println!("{:?}", component.name());
        }
    }
}

fn setup_player(q: Query<(&Transform, Entity), Added<PlayerBlueprint>>, mut commands: Commands) {
    for (xf_gent, e_gent) in q.iter() {
        println!("{:?}", xf_gent);
        let e_gfx = commands.spawn(()).id();
        commands.entity(e_gent).insert((
            PlayerGentBundle {
                marker: PlayerGent { e_gfx },
                phys: GentPhysicsBundle {
                    rb: RigidBody::Kinematic,
                    collider: Collider::cuboid(4.0, 10.0),
                    shapecast: ShapeCaster::new(
                        Collider::cuboid(3.99, 10.0),
                        Vec2::new(0.0, -2.0),
                        0.0,
                        Vec2::NEG_Y.into(),
                    ),
                },
                coyote_time: Default::default(),
            },
            Health {
                current: 100,
                max: 100,
            },
            //have to use builder here *i think* because of different types between keycode and
            //axis
            InputManagerBundle::<PlayerAction> {
                action_state: ActionState::default(),
                input_map: InputMap::default()
                    .insert(KeyCode::Space, PlayerAction::Jump)
                    .insert(
                        VirtualAxis::from_keys(KeyCode::A, KeyCode::D),
                        PlayerAction::Move,
                    )
                    .insert(KeyCode::Return, PlayerAction::Attack)
                    .build(),
            },
            Falling::default(),
            TransitionQueue::default(),
        ));
        commands.entity(e_gfx).insert((PlayerGfxBundle {
            marker: PlayerGfx { e_gent },
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
            (
                (transition.run_if(any_with_component::<TransitionQueue>()),),
                apply_deferred,
            )
                .chain()
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
pub struct Jumping {
    current_air_ticks: u32,
    max_air_ticks: u32,
}

impl Default for Jumping {
    fn default() -> Self {
        Jumping {
            current_air_ticks: 0,
            max_air_ticks: 30,
        }
    }
}
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

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct Attacking;
impl GentState for Attacking {}

///Player behavior systems.
///Do stuff here in states and add transitions to other states by pushing
///to a TransitionQueue.
struct PlayerBehaviorPlugin;

impl Plugin for PlayerBehaviorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                player_idle.run_if(any_with_components::<Idle, PlayerGent>()),
                player_run.run_if(any_with_components::<Running, PlayerGent>()),
                player_jump.run_if(any_with_components::<Jumping, PlayerGent>()),
                player_move,
                player_grounded.run_if(any_with_components::<
                    Grounded,
                    PlayerGent,
                >()),
                player_falling.run_if(any_with_components::<Falling, PlayerGent>()),
                player_collisions
                    .after(player_move)
                    .after(player_jump)
                    .after(player_falling),
            ),
        );
    }
}

fn player_idle(
    mut query: Query<
        (
            &ActionState<PlayerAction>,
            &mut TransitionQueue,
        ),
        (
            With<Grounded>,
            With<Idle>,
            With<PlayerGent>,
        ),
    >,
) {
    for (action_state, mut transitions) in query.iter_mut() {
        // println!("is idle");
        // check for direction input
        let mut direction: f32 = 0.0;
        // println!("idleing, {:?}", action_state.get_pressed());
        if action_state.pressed(PlayerAction::Move) {
            direction = action_state.value(PlayerAction::Move);
            // println!("moving??")
        }
        if direction != 0.0 {
            transitions.push(Idle::new_transition(Running::default()));
        }
    }
}

fn player_move(
    mut q_gent: Query<(
        &mut LinearVelocity,
        &ActionState<PlayerAction>,
        &PlayerGent,
        Option<&Grounded>,
    )>,
    //kinda dont want to do flipping here
    mut q_gfx_player: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for (mut velocity, action_state, gent, grounded) in q_gent.iter_mut() {
        let mut direction: f32 = 0.0;
        // Uses high starting acceleration, to emulate "shoving" off the ground/start
        // Acceleration is per game tick.
        let initial_accel = 45.0;
        let accel = 5.0;

        // What "%" does our character get slowed down per game tick.
        let ground_friction = 0.7;

        let new_vel = if action_state.just_pressed(PlayerAction::Move) {
            direction = action_state.value(PlayerAction::Move);
            velocity.x + accel * direction
        } else if action_state.pressed(PlayerAction::Move) {
            direction = action_state.value(PlayerAction::Move);
            velocity.x + initial_accel * direction
        } else {
            // de-acceleration profile
            if grounded.is_some() {
                velocity.x + ground_friction * -velocity.x
            } else {
                // airtime de-acceleration profile
                if action_state.just_released(PlayerAction::Move) {
                    velocity.x + initial_accel * 0.5 * action_state.value(PlayerAction::Move)
                } else {
                    let max_vel = velocity.x.abs();
                    (velocity.x + accel * -velocity.x.signum()).clamp(-max_vel, max_vel)
                }
            }
        };
        velocity.x = new_vel.clamp(-100.0, 100.0);

        if let Ok(mut player) = q_gfx_player.get_mut(gent.e_gfx) {
            if direction > 0.0 {
                player.set_slot("DirectionRight", true);
                player.set_slot("DirectionLeft", false);
            } else if direction < 0.0 {
                player.set_slot("DirectionRight", false);
                player.set_slot("DirectionLeft", true);
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
            With<PlayerGent>,
            With<Grounded>,
            With<Running>,
        ),
    >,
) {
    for (action_state, mut transitions) in q_gent.iter_mut() {
        let mut direction: f32 = 0.0;
        if action_state.pressed(PlayerAction::Move) {
            direction = action_state.value(PlayerAction::Move);
        }
        //should it account for decel and only transition to idle when player stops completely?
        //shouldnt be able to transition to idle if we also jump
        if direction == 0.0 && action_state.released(PlayerAction::Jump) {
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
        With<PlayerGent>,
    >,
) {
    for (action_state, mut velocity, mut jumping, mut transitions) in query.iter_mut() {
        //can enter state and first frame jump not pressed if you tap
        //i think this is related to the fixedtimestep input
        // print!("{:?}", action_state.get_pressed());

        let deaccel_rate = 2.5;

        // Jump should not be limited by number if "ticks" should be physics driven.
        if jumping.is_added() {
            velocity.y += 150.0;
        } else {
            if (velocity.y - deaccel_rate < 0.0) || action_state.released(PlayerAction::Jump) {
                transitions.push(Jumping::new_transition(Falling));
            }
            velocity.y -= deaccel_rate;
        }

        jumping.current_air_ticks += 1;

        velocity.y = velocity.y.clamp(0., 150.);
    }
}

fn player_collisions(
    spatial_query: SpatialQuery,
    mut q_gent: Query<
        (
            Entity,
            &Transform,
            &mut LinearVelocity,
            &Collider,
        ),
        (With<PlayerGent>, With<RigidBody>),
    >,
    time: Res<GameTime>,
) {
    for (entity, transform, mut linear_velocity, collider) in q_gent.iter_mut() {
        let mut collider = collider.clone();
        let mut tries = 0;
        loop {
            if let Some(first_hit) = spatial_query.cast_shape(
                // smaller collider then the players collider to prevent getting stuck
                &collider,
                transform.translation.xy(),
                0.0,
                linear_velocity.normalize(),
                linear_velocity.length() / time.hz as f32,
                false,
                SpatialQueryFilter::default().without_entities([entity]),
            ) {
                // If time of impact is 0.0, it means we are inside the wall,
                // by making the player collider smaller it allows them to attempt escape.
                // Will prevent player from getting stuck unless they are *really* intent on it.
                if first_hit.time_of_impact == 0.0 && tries < 5 {
                    collider = collider.clone();
                    collider.set_scale(collider.scale() * 0.95, 1);
                    tries += 1;
                    continue;
                }

                // Applies a very small amount of bounce, as well as sliding to the character
                // the bounce helps prevent the player from getting stuck.

                let sliding_plane = first_hit.normal1;

                let bounce_coefficient = 0.1;
                let bounce_force =
                    -sliding_plane * linear_velocity.dot(sliding_plane) * bounce_coefficient;

                let sliding_plane = first_hit.normal1;

                let projected_velocity =
                    linear_velocity.xy() - sliding_plane * linear_velocity.dot(sliding_plane);

                linear_velocity.0 = projected_velocity + bounce_force;
            }
            break;
        }
    }
}

fn player_grounded(
    mut query: Query<
        (
            &ShapeHits,
            &ActionState<PlayerAction>,
            &mut TransitionQueue,
            Option<&mut CoyoteTime>,
        ),
        (With<PlayerGent>, With<Grounded>),
    >,
    time: Res<GameTime>,
) {
    // in seconds
    let max_coyote_time = 0.1;
    for (hits, action_state, mut transitions, coyote_time) in query.iter_mut() {
        let is_falling = hits.iter().any(|x| x.time_of_impact > 0.1);

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
        if action_state.just_pressed(PlayerAction::Jump) {
            // if action_state.pressed(PlayerAction::Jump) {
            transitions.push(Grounded::new_transition(
                Jumping::default(),
            ))
        } else if is_falling {
            if !in_c_time {
                transitions.push(Grounded::new_transition(Falling))
            }
        }
    }
}

fn player_falling(
    mut query: Query<
        (
            &mut LinearVelocity,
            &ActionState<PlayerAction>,
            &ShapeHits,
            &mut TransitionQueue,
        ),
        (With<PlayerGent>, With<Falling>),
    >,
) {
    for (mut velocity, action_state, hits, mut transitions) in query.iter_mut() {
        let fall_accel = 2.9;
        let mut falling = true;
        for hit in hits.iter() {
            //if we are ~touching the ground
            if hit.time_of_impact < 0.001 {
                transitions.push(Falling::new_transition(Grounded));
                // println!("{:?} should be grounded", entity);
                //stop falling
                velocity.y = 0.0;
                if action_state.pressed(PlayerAction::Move) {
                    transitions.push(Falling::new_transition(Running));
                    // println!("{:?} should be running", entity)
                } else {
                    transitions.push(Falling::new_transition(Idle));
                }
                falling = false;
            }
        }
        if falling {
            velocity.y -= fall_accel;
            velocity.y = velocity.y.clamp(-100., 0.);
        }
    }
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
            )
                .in_set(PlayerStateSet::Animation)
                .after(PlayerStateSet::Transition)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

fn player_idle_animation(
    i_query: Query<&PlayerGent, Added<Idle>>,
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
    f_query: Query<&PlayerGent, Added<Falling>>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for gent in f_query.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            player.play_key("anim.player.Fall")
        }
    }
}

fn player_jumping_animation(
    f_query: Query<&PlayerGent, Added<Jumping>>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for gent in f_query.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            player.play_key("anim.player.Jump")
        }
    }
}

fn player_running_animation(
    r_query: Query<&PlayerGent, Added<Running>>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for gent in r_query.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            player.play_key("anim.player.Run")
        }
    }
}
