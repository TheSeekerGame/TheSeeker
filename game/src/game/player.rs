use std::marker::PhantomData;

use bevy::ecs::component::SparseStorage;
use bevy_xpbd_2d::{SubstepSchedule, SubstepSet};
use leafwing_input_manager::{axislike::VirtualAxis, prelude::*};
use theseeker_engine::{
    animation::SpriteAnimationBundle,
    assets::animation::SpriteAnimation,
    gent::{GentPhysicsBundle, TransformGfxFromGent},
    script::ScriptPlayer,
};

use crate::prelude::*;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (setup_player.run_if(in_state(GameState::Playing)),)
                .chain()
                .before(PlayerStateSet::Transition)
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(OnEnter(GameState::Paused), debug_player)
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
    e_gfx: Entity,
}

#[derive(Component)]
pub struct PlayerGfx {
    e_gent: Entity,
}

#[derive(Actionlike, PartialEq, Eq, Clone, Copy, Hash, Debug, Reflect)]
pub enum PlayerAction {
    Move,
    Jump,
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
                    .build(),
            },
            PlayerStateBundle::<Falling>::default(),
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
///State transition plugin
///Add a transition_from::<T: PlayerState>.run_if(any_with_component::<T>()) for each state

struct PlayerTransitionPlugin;

impl Plugin for PlayerTransitionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                transition_from::<Idle>.run_if(any_with_component::<Idle>()),
                transition_from::<Running>.run_if(any_with_component::<Running>()),
                transition_from::<Grounded>.run_if(any_with_component::<Grounded>()),
                transition_from::<Jumping>.run_if(any_with_component::<Jumping>()),
                transition_from::<Falling>.run_if(any_with_component::<Falling>()),
                apply_deferred,
            )
                .chain()
                .in_set(PlayerStateSet::Transition)
                .after(PlayerStateSet::Behavior)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

fn transition_from<T: Component + Send + Sync + 'static>(
    mut query: Query<(Entity, &mut TransitionsFrom<T>)>,
    mut commands: Commands,
) {
    for (entity, mut trans) in query.iter_mut() {
        for transition in &trans.transitions {
            transition(entity, &mut commands);
        }
        //could decide to remove state + transitionsfrom here
        if !&trans.transitions.is_empty() {
            commands.entity(entity).remove::<T>();
            trans.transitions.clear();
        }
    }
}

pub trait Transitionable<T: PlayerState + Default> {
    fn new_transition(_next: T) -> Box<dyn Fn(Entity, &mut Commands) + Send + Sync + 'static> {
        Box::new(|entity, commands| {
            commands
                .entity(entity)
                .insert(PlayerStateBundle::<T>::default());
        })
    }
}

#[derive(Component, Deref, DerefMut, Default)]
struct TransitionsFrom<T> {
    t: PhantomData<T>,
    #[deref]
    transitions: Vec<Box<dyn Fn(Entity, &mut Commands) + Send + Sync>>,
}

#[derive(Bundle, Default)]
pub struct PlayerStateBundle<T: PlayerState + Default> {
    state: T,
    transitions: TransitionsFrom<T>,
}

// States
// states are components which are added to the entity on transition.
// an entity can be in multiple states at once, eg Grounded and Running/Idle
// Impl Playerstate for each state
// Impl Transitionable<T: PlayerState> for each state that that should be able to be transitioned
// from by a state
pub trait PlayerState: Component<Storage = SparseStorage> + Clone {}

#[derive(Component, Default, Copy, Clone, Debug)]
#[component(storage = "SparseSet")]
pub struct Idle;
impl PlayerState for Idle {}
impl Transitionable<Running> for Idle {}

#[derive(Component, Default, Copy, Clone, Debug)]
#[component(storage = "SparseSet")]
pub struct Running;
impl PlayerState for Running {}
impl Transitionable<Idle> for Running {}

#[derive(Component, Default, Copy, Clone, Debug)]
#[component(storage = "SparseSet")]
pub struct Falling;
impl PlayerState for Falling {}
impl Transitionable<Grounded> for Falling {}
impl Transitionable<Running> for Falling {}
impl Transitionable<Idle> for Falling {}

#[derive(Component, Clone, Debug)]
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
impl PlayerState for Jumping {}
impl Transitionable<Falling> for Jumping {}
impl Transitionable<Grounded> for Jumping {}

#[derive(Component, Default, Copy, Clone, Debug)]
#[component(storage = "SparseSet")]
pub struct Grounded;
impl PlayerState for Grounded {}
//cant be Idle or Running if not Grounded
impl Transitionable<Jumping> for Grounded {
    fn new_transition(
        _next: Jumping,
    ) -> Box<dyn Fn(Entity, &mut Commands) + Send + Sync + 'static> {
        Box::new(|entity, commands| {
            commands
                .entity(entity)
                .insert(PlayerStateBundle::<Jumping>::default())
                .remove::<(Idle, Running)>();
        })
    }
}
//cant be Idle or Running if not Grounded
impl Transitionable<Falling> for Grounded {
    fn new_transition(
        _next: Falling,
    ) -> Box<dyn Fn(Entity, &mut Commands) + Send + Sync + 'static> {
        Box::new(|entity, commands| {
            commands
                .entity(entity)
                .insert(PlayerStateBundle::<Falling>::default())
                .remove::<(Idle, Running)>();
        })
    }
}

#[derive(Component, Default, Copy, Clone, Debug)]
#[component(storage = "SparseSet")]
pub struct Attacking;
impl PlayerState for Attacking {}

///player behavior systems.
///do stuff here in states and add transitions to other states by pushing
///to a TransitionsFrom<T: PlayerState> components queue of transitions.
struct PlayerBehaviorPlugin;

impl Plugin for PlayerBehaviorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                player_idle.run_if(any_with_component::<Idle>()),
                player_run.run_if(any_with_component::<Running>()),
                player_jump.run_if(any_with_component::<Jumping>()),
                player_move,
                // player move sets velocity, and player collisions sets vel to zero to avoid collisions
                player_collisions.after(player_move).after(player_jump),
                player_grounded.run_if(any_with_component::<Grounded>()),
                player_falling.run_if(any_with_component::<Falling>()),
            ),
        );
    }
}

fn player_idle(
    mut query: Query<
        (
            &ActionState<PlayerAction>,
            &mut TransitionsFrom<Idle>,
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
    )>,
    mut q_gfx_player: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for (mut velocity, action_state, gent) in q_gent.iter_mut() {
        let mut direction: f32 = 0.0;
        if action_state.pressed(PlayerAction::Move) {
            //use .clamped_value()?
            direction = action_state.value(PlayerAction::Move);
        }
        let new_x_vel = /*velocity.x + */direction as f32 * 100.;

        velocity.x = 0.0;
        velocity.x += new_x_vel;

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
            &mut LinearVelocity,
            &ActionState<PlayerAction>,
            &mut TransitionsFrom<Running>,
        ),
        (
            With<PlayerGent>,
            With<Grounded>,
            With<Running>,
        ),
    >,
) {
    for (mut velocity, action_state, mut transitions) in q_gent.iter_mut() {
        let mut direction: f32 = 0.0;
        if action_state.pressed(PlayerAction::Move) {
            direction = action_state.value(PlayerAction::Move);
        }
        //should it account for decel and only transition to idle when player stops completely?

        //shouldnt be able to transition to idle if we also jump
        if direction == 0.0 && action_state.released(PlayerAction::Jump) {
            transitions.push(Running::new_transition(Idle));
            velocity.x = 0.0;
        }
    }
}

//TODO: Coyote time, impulse/gravity damping/float at top, double jump
//TODO: load jump properties from script/animation (velocity/accel + ticks/frames)
fn player_jump(
    mut query: Query<
        (
            &ActionState<PlayerAction>,
            &mut LinearVelocity,
            &mut Jumping,
            &mut TransitionsFrom<Jumping>,
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
            &mut Transform,
            &mut LinearVelocity,
            &Collider,
        ),
        (With<PlayerGent>, With<RigidBody>),
    >,
    time: Res<GameTime>,
) {
    for (entity, mut transform, mut linear_velocity, collider) in q_gent.iter_mut() {
        if let Some(first_hit) = spatial_query.cast_shape(
            // smaller collider then the players collider to prevent getting stuck
            &Collider::cuboid(3.5, 9.0),
            transform.translation.xy(),
            0.0,
            linear_velocity.normalize(),
            // The 0.1 there is to prevent player from getting stuck
            (linear_velocity.length() / (time.hz) as f32) + 0.1,
            false,
            SpatialQueryFilter::default().without_entities([entity]),
        ) {
            println!("First hit: {:?}", first_hit);
            println!(
                "length: {:?}",
                linear_velocity.length() / time.hz as f32
            );
            // Allows player to slide past surfaces if they are perfectly horizontal or vertical
            if first_hit.normal1.x.abs() > 0.1 {
                linear_velocity.x = 0.0;
            }
            if first_hit.normal1.y.abs() > 0.1 {
                linear_velocity.y = 0.0;
            }
        }
    }
}

fn player_grounded(
    mut query: Query<
        (
            &ShapeHits,
            &ActionState<PlayerAction>,
            &mut TransitionsFrom<Grounded>,
        ),
        (With<PlayerGent>, With<Grounded>),
    >,
) {
    for (hits, action_state, mut transitions) in query.iter_mut() {
        let is_falling = hits.iter().any(|x| x.time_of_impact > 0.1);
        //just pressed seems to get missed sometimes... but we need it because pressed makes you
        //jump continuously if held
        //known issue https://github.com/bevyengine/bevy/issues/6183
        if action_state.just_pressed(PlayerAction::Jump) {
            // if action_state.pressed(PlayerAction::Jump) {
            transitions.push(Grounded::new_transition(
                Jumping::default(),
            ))
        } else if is_falling {
            transitions.push(Grounded::new_transition(Falling))
        }
    }
}

fn player_falling(
    mut query: Query<
        (
            &mut LinearVelocity,
            &ActionState<PlayerAction>,
            &ShapeHits,
            &mut TransitionsFrom<Falling>,
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
            Update,
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
