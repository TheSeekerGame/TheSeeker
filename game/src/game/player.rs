use bevy::{ecs::component::SparseStorage, math::bool};
use leafwing_input_manager::{orientation::Direction, prelude::*};
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
        // FIXME: ordering
        app.add_systems(
            Update,
            (
                setup_player,
                // player_control,
                player_idle,
                player_move,
                player_gravity,
                player_collisions,
            ),
        )
        .add_plugins((
            InputManagerPlugin::<PlayerAction>::default(),
            PlayerStatePlugin,
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

#[derive(Component, Default)]
pub struct PlayerGent;

#[derive(Component, Default)]
pub struct PlayerGfx;

#[derive(Actionlike, PartialEq, Eq, Clone, Copy, Hash, Debug, Reflect)]
pub enum PlayerAction {
    MoveLeft,
    MoveRight,
    Jump,
}

impl PlayerAction {
    const DIRECTIONS: [Self; 2] = [PlayerAction::MoveLeft, PlayerAction::MoveRight];

    fn direction(self) -> Option<Direction> {
        match self {
            PlayerAction::MoveLeft => Some(Direction::WEST),
            PlayerAction::MoveRight => Some(Direction::EAST),
            _ => None,
        }
    }
}

fn setup_player(q: Query<(&Transform, Entity), Added<PlayerBlueprint>>, mut commands: Commands) {
    for (xf_gent, e_gent) in q.iter() {
        let e_gfx = commands.spawn(()).id();
        commands.entity(e_gent).insert((
            PlayerGentBundle {
                marker: PlayerGent,
                phys: GentPhysicsBundle {
                    rb: RigidBody::Kinematic,
                    collider: Collider::cuboid(10.0, 12.0),
                },
            },
            InputManagerBundle::<PlayerAction> {
                action_state: ActionState::default(),
                input_map: InputMap::new([
                    (KeyCode::A, PlayerAction::MoveLeft),
                    (KeyCode::D, PlayerAction::MoveRight),
                    (KeyCode::Space, PlayerAction::Jump),
                ]),
            },
            Idle,
            Falling,
        ));
        commands.entity(e_gfx).insert((PlayerGfxBundle {
            marker: PlayerGfx,
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

struct PlayerStatePlugin;

impl Plugin for PlayerStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                // test_trans_int,
                // transition::<Idle, Grounded>,
                transition::<Falling, Grounded>,
                transition::<Idle, Running>,
                transition::<Running, Idle>,
                apply_deferred,
            ),
        );
        // add state systems
        // add state transition systems
    }
}

#[derive(Component)]
pub struct PlayerStateTransition<F: PlayerState, N: PlayerState> {
    current: F,
    //if we dont need it can add phantomdata
    next: N, //remove self?
             //also can add a bool for remove current
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
        PlayerStateTransition::<Self::C, N>::new(*self, next)
    }
}

// fn testin() {
//     let idle = Idle;
//     idle.new_transition(Grounded);
// }

impl<F: PlayerState, N: PlayerState> PlayerStateTransition<F, N> {
    fn new(current: F, next: N) -> Self {
        PlayerStateTransition { current, next }
    }
}

fn transition<T: PlayerState, N: PlayerState>(
    query: Query<(Entity, &PlayerStateTransition<T, N>)>,
    mut commands: Commands,
) {
    for (entity, transition) in query.iter() {
        commands
            .entity(entity)
            .insert(transition.next)
            .remove::<T>()
            .remove::<PlayerStateTransition<T, N>>();
    }
}

//======================================================================
pub trait PlayerState: Component<Storage = SparseStorage> + Copy + Clone {}
//could make derive macro to derive it?

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

// #[derive(Component, Default, Copy, Clone)]
// #[component(storage = "SparseSet")]
// pub struct Jumping;
// impl PlayerState for Jumping {}

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

fn player_idle(
    mut query: Query<
        (
            Entity,
            &ActionState<PlayerAction>,
            &Idle,
        ),
        (With<PlayerGent>),
    >,
    mut q_gfx_player: Query<(&mut ScriptPlayer<SpriteAnimation>), With<PlayerGfx>>,
    mut commands: Commands,
) {
    for mut player in q_gfx_player.iter_mut() {
        if player.is_stopped() {
            player.play_key("anim.player.Idle");
        }
    }
    for (g_ent, action_state, idle) in query.iter() {
        if action_state.just_pressed(PlayerAction::MoveLeft) {
            commands.entity(g_ent).insert(idle.new_transition(Running));
        }
    }
}

fn player_move(
    time: Res<Time>,
    mut q_gent: Query<
        (
            &mut LinearVelocity,
            &ActionState<PlayerAction>,
            Ref<Running>,
        ),
        (With<PlayerGent>),
    >,
    mut q_gfx_player: Query<(&mut ScriptPlayer<SpriteAnimation>), With<PlayerGfx>>,
) {
    for (mut velocity, action_state, running) in q_gent.iter_mut() {
        if running.is_added() {
            let mut player = q_gfx_player.single_mut();
            player.play_key("anim.player.Run");
            player.set_slot("DirectionLeft", true);
        }
        let mut direction_vector = Vec2::ZERO;
        for input_direction in PlayerAction::DIRECTIONS {
            if action_state.pressed(input_direction) {
                if let Some(direction) = input_direction.direction() {
                    direction_vector += Vec2::from(direction);
                }
            }
        }
        //TODO: normalize?
        velocity.x += direction_vector.x as f64 * time.delta_seconds_f64();
    }
}

// TODO:
fn player_gravity(
    time: Res<Time>,
    mut q_gent: Query<&mut LinearVelocity, (With<PlayerGent>, Without<Grounded>)>,
) {
    for mut velocity in q_gent.iter_mut() {
        velocity.y -= 10. * time.delta_seconds_f64();
        println!("gravity applied")
    }
}

//TODO
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

        //maybe have to account for children, do colliders get added directly or as children to
        //ldtk entities?

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
                let falling = Falling;
                commands
                    .entity(g_ent)
                    .insert(falling.new_transition(Grounded));
                // commands.entity(g_ent).insert(TransEvent::<Grounded>::new());
                *linear_velocity = LinearVelocity::ZERO
            }

            //skip max_slope_angle
            //
            //add grounded check to remove jitter
        }
    }
}

// TODO: Player Spawn? does it go in Level?

// TODO: Player State
// TODO: Player Movement
// TODO: Player Input

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
