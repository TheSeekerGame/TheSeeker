use bevy::ecs::component::SparseStorage;
use leafwing_input_manager::{
    axislike::VirtualAxis, common_conditions::action_pressed, orientation::Direction, prelude::*,
};
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
        // FIXME: ordering, add behavior systems to set and animation systems to set, apply
        // deffered between
        app.add_systems(
            Update,
            (
                setup_player,
                player_idle.run_if(any_with_component::<Idle>()),
                player_run.run_if(any_with_component::<Running>()),
                player_jump.run_if(any_with_component::<Jumping>()),
                player_move,
                player_collisions,
                player_grounded.run_if(any_with_component::<Grounded>()),
                player_falling.run_if(any_with_component::<Falling>()),
            )
                .in_set(PlayerStateSet::Behavior)
                .run_if(in_state(AppState::InGame)),
        )
        .add_plugins((
            InputManagerPlugin::<PlayerAction>::default(),
            PlayerStatePlugin,
            PlayerAnimationPlugin,
        ));
    }
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
//where is this added/spawned, currently debug spawned only
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

fn setup_player(q: Query<(&Transform, Entity), Added<PlayerBlueprint>>, mut commands: Commands) {
    for (xf_gent, e_gent) in q.iter() {
        let e_gfx = commands.spawn(()).id();
        commands.entity(e_gent).insert((
            PlayerGentBundle {
                marker: PlayerGent { e_gfx },
                phys: GentPhysicsBundle {
                    rb: RigidBody::Kinematic,
                    collider: Collider::cuboid(6.0, 10.0),
                    shapecast: ShapeCaster::new(
                        Collider::cuboid(10.0, 12.0),
                        Vec2::ZERO.into(),
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
            Falling,
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
        println!("player spawned")
    }
}

#[derive(SystemSet, Clone, PartialEq, Eq, Debug, Hash)]
pub enum PlayerStateSet {
    Behavior,
    Transition,
    Animation,
}

struct PlayerStatePlugin;

impl Plugin for PlayerStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                transition::<Falling, Grounded>.run_if(any_with_component::<
                    PlayerStateTransition<Falling, Grounded>,
                >()),
                transition::<Grounded, Falling>.run_if(any_with_component::<
                    PlayerStateTransition<Grounded, Falling>,
                >()),
                transition::<Falling, Running>.run_if(any_with_component::<
                    PlayerStateTransition<Falling, Running>,
                >()),
                transition::<Grounded, Jumping>.run_if(any_with_component::<
                    PlayerStateTransition<Grounded, Jumping>,
                >()),
                transition::<Jumping, Falling>.run_if(any_with_component::<
                    PlayerStateTransition<Jumping, Falling>,
                >()),
                transition::<Idle, Running>.run_if(any_with_component::<
                    PlayerStateTransition<Idle, Running>,
                >()),
                transition::<Running, Idle>.run_if(any_with_component::<
                    PlayerStateTransition<Running, Idle>,
                >()),
            )
                .in_set(PlayerStateSet::Transition)
                .after(PlayerStateSet::Behavior)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

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

#[derive(Component)]
pub struct PlayerStateTransition<C: PlayerState, N: PlayerState> {
    current: C,
    //if we dont need it can add phantomdata
    next: N,
}

impl<C: PlayerState, N: PlayerState> PlayerStateTransition<C, N> {
    fn new(current: C, next: N) -> Self {
        PlayerStateTransition { current, next }
    }
}

pub trait Transitionable<T: PlayerState> {
    type C: PlayerState + Default;

    fn new_transition(&self, next: T) -> PlayerStateTransition<Self::C, T>
    where
        Self: PlayerState,
    {
        PlayerStateTransition::<Self::C, T>::new(Self::C::default(), next)
    }
}

impl<T: PlayerState, N: PlayerState> Transitionable<N> for T
where
    T: PlayerState + Default,
{
    type C = Self;

    fn new_transition(&self, next: N) -> PlayerStateTransition<Self::C, N>
    where
        Self: PlayerState,
    {
        PlayerStateTransition::<Self::C, N>::new(self.clone(), next)
    }
}

fn transition<T: PlayerState, N: PlayerState>(
    query: Query<(Entity, &PlayerStateTransition<T, N>)>,
    mut commands: Commands,
) {
    for (entity, transition) in query.iter() {
        commands
            .entity(entity)
            .insert(transition.next.clone())
            .remove::<T>()
            .remove::<PlayerStateTransition<T, N>>();
    }
}

//======================================================================
pub trait PlayerState: Component<Storage = SparseStorage> + Clone {}

#[derive(Component, Default, Copy, Clone)]
#[component(storage = "SparseSet")]
pub struct Idle;
impl PlayerState for Idle {}

#[derive(Component, Default, Copy, Clone)]
#[component(storage = "SparseSet")]
pub struct Running;
impl PlayerState for Running {}

#[derive(Component, Default, Copy, Clone)]
#[component(storage = "SparseSet")]
pub struct Falling;
impl PlayerState for Falling {}

#[derive(Component, Default, Clone)]
#[component(storage = "SparseSet")]
pub struct Jumping {
    //TODO:
    //can this be frames/ticks instead?
    airtime: Timer,
}
impl Jumping {
    fn new() -> Self {
        Jumping {
            airtime: Timer::from_seconds(0.5, TimerMode::Once),
        }
    }
}
impl PlayerState for Jumping {}

#[derive(Component, Default, Copy, Clone)]
#[component(storage = "SparseSet")]
pub struct Grounded;
impl PlayerState for Grounded {}

#[derive(Component, Default, Copy, Clone)]
#[component(storage = "SparseSet")]
pub struct Attacking;
impl PlayerState for Attacking {}

//=======================================================================
//
//TODO: refactor, controller plugin, physics plugin, state plugin

//can only be idle when grounded and no input...
fn player_idle(
    mut query: Query<
        (
            Entity,
            &PlayerGent,
            &ActionState<PlayerAction>,
            &Idle,
        ),
        With<Grounded>,
    >,
    mut commands: Commands,
) {
    for (g_ent, _g_marker, action_state, idle) in query.iter() {
        println!("is idle");
        // check for direction input
        let mut direction: f32 = 0.0;
        if action_state.pressed(PlayerAction::Move) {
            direction = action_state.value(PlayerAction::Move);
            println!("moving??")
        }
        if direction != 0.0 {
            commands.entity(g_ent).insert(idle.new_transition(Running));
            // } else if direction_vector.x == 0.0 {
            //     commands.entity(g_ent).insert(Idle);
        }
    }
}

//seprate run and fall/jump movement? y/n?
//split into run and generic player_move
fn player_move(
    time: Res<Time>,
    mut q_gent: Query<
        (
            Entity,
            &mut LinearVelocity,
            &ActionState<PlayerAction>,
        ),
        (With<PlayerGent>),
    >,
    mut q_gfx_player: Query<(&mut ScriptPlayer<SpriteAnimation>), With<PlayerGfx>>,
) {
    for (g_ent, mut velocity, action_state) in q_gent.iter_mut() {
        //switch to getting by entity from gent
        let mut player = q_gfx_player.single_mut();
        let mut direction: f32 = 0.0;
        if action_state.pressed(PlayerAction::Move) {
            //use .clamped_value()?
            direction = action_state.value(PlayerAction::Move);
        }

        velocity.x = 0.0;
        velocity.x += direction as f64 * time.delta_seconds_f64() * 5000.0;

        //this doesnt work for the jump animation, need to look into further
        if direction > 0.0 {
            player.set_slot("DirectionRight", true);
            player.set_slot("DirectionLeft", false);
        } else if direction < 0.0 {
            player.set_slot("DirectionRight", false);
            player.set_slot("DirectionLeft", true);
        }
    }
}

fn player_run(
    time: Res<Time>,
    mut q_gent: Query<
        (
            Entity,
            &mut LinearVelocity,
            &ActionState<PlayerAction>,
            &Running,
        ),
        (With<PlayerGent>),
    >,
    mut commands: Commands,
) {
    for (g_ent, mut velocity, action_state, running) in q_gent.iter_mut() {
        println!("{:?} is running", g_ent);
        let mut direction: f32 = 0.0;
        if action_state.pressed(PlayerAction::Move) {
            direction = action_state.value(PlayerAction::Move);
        }

        //should it account for decel and only transition to idle when player stops completely?
        if direction == 0.0 {
            commands.entity(g_ent).insert(running.new_transition(Idle));
            velocity.x = 0.0;
        }
    }
}

//TODO: Coyote time, impulse/gravity damping at top, double jump
fn player_jump(
    time: Res<Time>,
    mut query: Query<
        (
            Entity,
            &ActionState<PlayerAction>,
            &mut LinearVelocity,
            &mut Jumping,
        ),
        (With<PlayerGent>),
    >,
    mut commands: Commands,
) {
    for (entity, action_state, mut velocity, mut jumping) in query.iter_mut() {
        if jumping.airtime.tick(time.delta()).finished() {
            commands
                .entity(entity)
                .insert(jumping.new_transition(Falling));
            velocity.y = 200.0 * time.delta_seconds_f64();
        }
        if action_state.just_released(PlayerAction::Jump) {
            commands
                .entity(entity)
                .insert(jumping.new_transition(Falling));
            velocity.y = 200.0 * time.delta_seconds_f64();
        }

        velocity.y += 100. * time.delta_seconds_f64();
        // if action_state.just_pressed(PlayerAction::Jump) {
        //     //we actually want to apply a jump implulse instead of this?
        //     //and when held? do what..
        //     velocity.y += 1000. * time.delta_seconds_f64();
        // }
        if jumping.is_added() {
            velocity.y += 3000. * time.delta_seconds_f64();
        }
    }
}

//TODO
//add shapecasting forward/in movement direction to check for collisions
fn player_collisions(
    collisions: Res<Collisions>,
    mut q_gent: Query<
        (
            Entity,
            &RigidBody,
            &mut Position,
            &Rotation,
            &mut LinearVelocity,
        ),
        With<PlayerGent>,
    >,
    mut commands: Commands,
) {
    for contacts in collisions.iter() {
        if !contacts.during_current_substep {
            continue;
        }

        let is_first: bool;
        let (g_ent, rb, mut position, rotation, mut linear_velocity) =
            if let Ok(player) = q_gent.get_mut(contacts.entity1) {
                is_first = true;
                player
            } else if let Ok(player) = q_gent.get_mut(contacts.entity2) {
                is_first = false;
                player
            } else {
                continue;
            };

        //skipping check for kinematic

        for manifold in contacts.manifolds.iter() {
            let normal = if is_first {
                -manifold.global_normal1(rotation)
            } else {
                -manifold.global_normal2(rotation)
            };

            for contact in manifold.contacts.iter().filter(|c| c.penetration > 0.0) {
                position.0 += normal * contact.penetration;
                // let falling = Falling;
                // commands
                //     .entity(g_ent)
                //     .insert(falling.new_transition(Grounded));
                // commands.entity(g_ent).insert(TransEvent::<Grounded>::new());
                *linear_velocity = LinearVelocity::ZERO
            }

            //skip max_slope_angle
            //
            //add grounded check to remove jitter
        }
    }
}

fn player_grounded(
    mut query: Query<
        (
            Entity,
            //probably should be option
            &ShapeHits,
            &Grounded,
            &ActionState<PlayerAction>,
        ),
        (With<PlayerGent>),
    >,
    mut commands: Commands,
) {
    for (entity, hits, grounded, action_state) in query.iter_mut() {
        let is_falling = hits.iter().any(|x| x.time_of_impact > 0.1);
        if action_state.just_pressed(PlayerAction::Jump) {
            commands
                .entity(entity)
                .insert(grounded.new_transition(Jumping::new()));
        } else if is_falling {
            commands
                .entity(entity)
                .insert(grounded.new_transition(Falling));
            //TODO switch this to direction vector, this not correct and causes no idle if space
            //held after landing from jump
        } else if action_state.get_pressed().len() == 0 {
            commands.entity(entity).insert(Idle);
        }
    }
}

fn player_falling(
    time: Res<Time>,
    mut query: Query<
        (
            Entity,
            &mut LinearVelocity,
            &ActionState<PlayerAction>,
            &ShapeHits,
            &Falling,
        ),
        With<PlayerGent>,
    >,
    mut commands: Commands,
) {
    for (entity, mut velocity, action_state, hits, falling) in query.iter_mut() {
        println!("{:?} is falling", entity);
        velocity.y -= (300. * time.delta_seconds_f64()).clamp(0., 13.);
        for hit in hits.iter() {
            if hit.time_of_impact < 0.001 {
                commands
                    .entity(entity)
                    .insert(falling.new_transition(Grounded));
                println!("{:?} should be grounded", entity);
                //stop falling
                velocity.y = 0.0;
                if action_state.pressed(PlayerAction::Move) {
                    commands
                        .entity(entity)
                        .insert(falling.new_transition(Running));
                    println!("{:?} should be running", entity)
                }
            }
        }
        // }
    }
}

// fn player_idle_animation(
//     i_query: Query<
//         &PlayerGent,
//         Or<(
//             Added<Idle>,
//             // (Added<Grounded>, With<Idle>),
//         )>,
//     >,
//     mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
// ) {
//     for (gent) in i_query.iter() {
//         if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
//             player.play_key("anim.player.Idle")
//         }
//     }
// }

//without idle state...
fn player_idle_animation(
    i_query: Query<
        &PlayerGent,
        Or<(
            (Added<Grounded>, Without<Running>),
            (
                With<Grounded>,
                With<PlayerStateTransition<Running, Idle>>,
            ),
        )>,
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for (gent) in i_query.iter() {
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
    for (gent) in f_query.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            player.play_key("anim.player.Fall")
        }
    }
}

fn player_jumping_animation(
    f_query: Query<&PlayerGent, Added<Jumping>>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for (gent) in f_query.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            player.play_key("anim.player.Jump")
        }
    }
}

fn player_running_animation(
    r_query: Query<
        (&PlayerGent),
        Or<(
            Added<Running>,
            (With<Running>, Added<Grounded>),
        )>,
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for (gent) in r_query.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            player.play_key("anim.player.Run")
        }
    }
}

// TODO: this is temporary
fn player_control(
    q_gent_player: Query<(), With<PlayerGent>>,
    mut q_gfx_player: Query<(&mut ScriptPlayer<SpriteAnimation>), With<PlayerGfx>>,
    input: Res<Input<KeyCode>>,
) {
    for mut player in &mut q_gfx_player {
        if player.is_stopped() {
            player.play_key("anim.player.Idle");
        }
        if input.just_pressed(KeyCode::Left) {
            player.play_key("anim.player.Run");
            player.set_slot("DirectionLeft", true);
        }
        if input.just_released(KeyCode::Left) {
            player.play_key("anim.player.Idle");
            player.set_slot("DirectionLeft", false);
        }
        if input.just_pressed(KeyCode::Right) {
            player.play_key("anim.player.Run");
            player.set_slot("DirectionRight", true);
        }
        if input.just_released(KeyCode::Right) {
            player.play_key("anim.player.Idle");
            player.set_slot("DirectionRight", false);
        }
        if input.just_pressed(KeyCode::Up) {
            player.play_key("anim.player.IdleLookUp");
        }
        if input.just_pressed(KeyCode::Down) {
            player.play_key("anim.player.IdleLookDown");
        }
        if input.just_pressed(KeyCode::W) {
            player.play_key("anim.player.SwordWhirling");
        }
        if input.just_pressed(KeyCode::Q) {
            player.play_key("anim.player.SwordAirDown");
        }
        if input.just_pressed(KeyCode::E) {
            player.play_key("anim.player.SwordAirFrontA");
        }
        if input.just_pressed(KeyCode::R) {
            player.play_key("anim.player.SwordAirFrontB");
        }
        if input.just_pressed(KeyCode::T) {
            player.play_key("anim.player.SwordFrontA");
        }
        if input.just_pressed(KeyCode::Y) {
            player.play_key("anim.player.SwordFrontB");
        }
        if input.just_pressed(KeyCode::U) {
            player.play_key("anim.player.SwordFrontC");
        }
        if input.just_pressed(KeyCode::I) {
            player.play_key("anim.player.SwordRunA");
        }
        if input.just_pressed(KeyCode::O) {
            player.play_key("anim.player.SwordRunB");
        }
        if input.just_pressed(KeyCode::P) {
            player.play_key("anim.player.SwordUp");
        }
        if input.just_pressed(KeyCode::A) {
            player.play_key("anim.player.Jump");
        }
        if input.just_pressed(KeyCode::S) {
            player.play_key("anim.player.JumpForward");
        }
        if input.just_pressed(KeyCode::D) {
            player.play_key("anim.player.Fly");
        }
        if input.just_pressed(KeyCode::F) {
            player.play_key("anim.player.FlyForward");
        }
        if input.just_pressed(KeyCode::G) {
            player.play_key("anim.player.Fall");
        }
        if input.just_pressed(KeyCode::H) {
            player.play_key("anim.player.FallForward");
        }
        if input.just_pressed(KeyCode::J) {
            player.play_key("anim.player.FlyFallTransition");
        }
        if input.just_pressed(KeyCode::K) {
            player.play_key("anim.player.FlyFallForwardTransition");
        }
        if input.just_pressed(KeyCode::L) {
            player.play_key("anim.player.Land");
        }
        if input.just_pressed(KeyCode::Z) {
            player.play_key("anim.player.LandForward");
        }
        if input.just_pressed(KeyCode::X) {
            player.play_key("anim.player.Dash");
        }
        if input.just_pressed(KeyCode::C) {
            player.play_key("anim.player.Roll");
        }
        if input.just_pressed(KeyCode::V) {
            player.play_key("anim.player.WallSlide");
        }
        if input.just_pressed(KeyCode::Space) {
            player.set_slot("Damage", true);
        }
        if input.just_released(KeyCode::Space) {
            player.set_slot("Damage", false);
        }
    }
}
