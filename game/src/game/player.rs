use std::{any::Any, collections::VecDeque};

use bevy::{
    ecs::{
        component::{ComponentStorage, SparseStorage},
        entity,
        query::WorldQuery,
        reflect::ReflectCommandExt,
    },
    math::bool,
    reflect::FromType,
};
use bevy_xpbd_2d::parry::utils::Array1;
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
                player_control,
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
                    // rb: RigidBody::Dynamic,
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
            // StateTransitionIntents(VecDeque::from([Box::new(dyn )])),
            // StateTransitionIntents::default(),
            // Idle,
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
                apply_deferred,
            ),
        );
        // add state systems
        // add state transition systems
    }
}

//could also add enum id for each state
//would make the comparisions easier

// #[derive(Bundle)]
// pub struct PlayerStateBundle {
//     transition_intents: StateTransitionIntents,
// }

//============================ gat attempt?? ======================

// pub trait Pst8<T: PlayerState, N: PlayerState> {
//     // fn transition(&self, transition: PlayerStateTransition<T, N>) {}
// }

// #[derive(Component)]
// pub struct TransEvent<T: PlayerState + Default> {
//     state: T,
// }

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
        let something = Self::C::default();
        PlayerStateTransition::<Self::C, T>::new(something, next)
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
        // let something = Self::C::default();
        PlayerStateTransition::<Self::C, N>::new(*self, next)
    }
}

//now system which checks each transition and applys them

fn testin() {
    let idle = Idle;
    idle.new_transition(Grounded);
}

// pub struct StateWrap<T: PlayerState + Default>(T);

// impl<T: Default + PlayerState> PlayerSTrans for StateWrap<T> {
//     type C = T;
//
//     type N = T;
//
//     fn new(&self, n: Self::N) -> PlayerStateTransition<Self::C, Self::N>
//     where
//         Self: PlayerState,
//     {
//         let something = Self::C::default();
//         PlayerStateTransition::<Self::C, Self::N>::new(something, n)
//     }
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

//playerstate should be able to make a transition to any other state
// impl Command for TransEvent<T> {
//     fn apply(self, world: &mut World) {}
// }

// pub trait Something {
//     type Trans: PlayerState
//     where
//         Self: PlayerState;
//
//     // fn apply<F: PlayerState>(&self, f: F) -> PlayerStateTransition<, F> {}
// }

//======================================================================

// impl<T: PlayerState + Default> TransEvent<T> {
//     fn new() -> TransEvent<T>
//     where
//         T: PlayerState + Default,
//     {
//         let state = T::default();
//         Self { state }
//     }
//     fn state_label(&self) -> StateLabel {
//         T::state_label()
//     }

// fn apply(commands) -> FnMut
//     where T: PlayerState + Default,
// {
//
//
// }
// }

//might be able to get rid of reflect stuff?

// impl StateTransition {
//     pub fn new<T>(state: T, priority: i32) -> Self
//     where
//         T: PlayerState + Default,
//     {
//         let reflect_component = <ReflectComponent as FromType<T>>::from_type();
//         let state_name = T::state_label();
//         let state_data = Box::new(state) as _;
//         Self {
//             reflect_component,
//             state_data,
//             state_name,
//             priority,
//         }
//     }
//
//     //this should maybe take entitycommands
//     //optionally remove the current state
//     //
//     pub fn apply<CurrentState: PlayerState>(self, entity: Entity, commands: &mut Commands) {
//         commands.add(move |world: &mut World| {
//             self.reflect_component.insert(
//                 &mut world.entity_mut(entity),
//                 self.state_data.as_reflect(),
//             );
//         })
//     }
// }

// #[derive(WorldQuery)]
// struct StateTransQuery {
//     idle: Option<&'static TransEvent<Idle>>,
//     falling: Option<&'static TransEvent<Falling>>,
//     grounded: Option<&'static TransEvent<Grounded>>,
// }

// impl<'w> StateTransQueryItem<'w> {
//     fn state_labels(&self) -> Option<Vec<StateLabel>> {
//         let mut labels: Vec<StateLabel> = Vec::new();
//         if let Some(idle) = self.idle {
//             labels.push(idle.state_label());
//         }
//         if let Some(falling) = self.falling {
//             labels.push(falling.state_label());
//         }
//         if let Some(grounded) = self.grounded {
//             labels.push(grounded.state_label());
//         }
//         if labels.is_empty() {
//             None
//         } else {
//             Some(labels)
//         }
//     }
// }

// #[derive(WorldQuery)]
// struct StateQuery {
//     idle: Option<&'static Idle>,
//     falling: Option<&'static Falling>,
//     grounded: Option<&'static Grounded>,
// }

// impl<'w> StateQueryItem<'w> {
//     fn transition_priorities(&self) -> Option<Vec<StateLabel>> {
//         let mut priorities: Vec<StateLabel> = Vec::new();
//         if let Some(Idle) = self.idle {
//             priorities.extend(Idle::transition_priorities());
//         }
//         if let Some(Falling) = self.falling {
//             priorities.extend(Falling::transition_priorities());
//         }
//         if let Some(Grounded) = self.grounded {
//             priorities.extend(Grounded::transition_priorities());
//         }
//         priorities.sort_unstable();
//         priorities.dedup();
//         if priorities.is_empty() {
//             None
//         } else {
//             Some(priorities)
//         }
//
//         // let _ = Idle::transition_priorities().extend(Falling::transition_priorities())
//     }
// }

// #[derive(WorldQuery)]
// struct StateFilter {
//     _t: Or<(
//         With<TransEvent<Idle>>,
//         With<TransEvent<Falling>>,
//         With<TransEvent<Grounded>>,
//     )>,
//     _s: Or<(
//         With<Idle>,
//         With<Falling>,
//         With<Grounded>,
//     )>,
// }
//probably also add AnyOf() fiter so that entities with none of them get filtered out

// impl StateQuery {
//     state
//     want way to filter all states not in transition priorities
//     collect them into hashmap, check if in for each trans?
// }

// #[derive(WorldQuery)]
// struct AnyStateTransQuery {
//     states: AnyOf<StateTransQuery>,
// }

//now how do i get instances of states to transition to....

//generic transition system, add for each state
//this removes the state if there is a transition intent that matches one of the states priorities
// fn transition(
//     query: Query<(Entity, StateQuery, StateTransQuery), StateFilter>,
//     mut commands: Commands,
// ) {
//     for (entity, current_states, next_states) in query.iter() {
//         //add states
//         if let Some(prio) = current_states.transition_priorities() {
//             if let Some(n_states) = next_states.state_labels() {
//                 println!("matched some entities...");
//                 // let apply: Vec<StateLabel> = prio
//                 let mut apply = prio
//                     .iter()
//                     .filter(|&x| n_states.iter().any(|n| n == x))
//                     .map(|x| *x);
//                 // .collect();
//                 if next_states.grounded.is_some() {
//                     if apply.any(|s| s == Grounded::state_label()) {
//                         commands.entity(entity).insert(Grounded);
//                         println!("added grounded");
//                     }
//                 }
//                 if next_states.idle.is_some() {
//                     if apply.any(|s| s == Idle::state_label()) {
//                         commands.entity(entity).insert(Idle);
//                     }
//                 }
//                 if next_states.falling.is_some() {
//                     if apply.any(|s| s == Falling::state_label()) {
//                         commands.entity(entity).insert(Falling);
//                     }
//                 }
//                 // for label in apply {
//                 //     commands.entity(entity).insert()
//                 // }
//             }
//         }
//         //remove states if a transition in their priorities has been made
//
//         //remove all state transitions
//
//         // if let Some(TransEvent { state: Grounded }) = next_states.grounded {
//         //     commands.entity(entity).insert(Grounded);
//         // }
//         // if current_states
//         commands.entity(entity).remove::<(
//             TransEvent<Idle>,
//             TransEvent<Falling>,
//             TransEvent<Grounded>,
//         )>();
//     }
// for (state, entity) in query.iter() {
//     for prio in state.transition_priorities().iter() {
//         for transition in transition_intents.iter() {
//             if *prio == transition.state_name {
//                 println!(
//                     "{:?}, {:?}",
//                     prio, transition.state_name
//                 );
//
//                 // commands
//                 //     .entity(entity)
//                 //     .insert(transition.state_data.as_reflect().from_reflect());
//
//                 let trans = TransEvent::<T>::new();
//                 commands.entity(entity).insert(trans);
//                 // transition.apply::<T>(entity, &mut commands);
//                 // commands
//                 //     .entity(entity)
//                 //     .insert(transition.state_data.as_reflect())
//             }
//             //transition.apply
//             //continue?
//
//             //if state is finished what do we do
//             //animation finished?
//
//             // if let Some(inner) = FromReflect::from_reflect(**prio) {}
//             // prio.unwrap()
//             // if FromReflect::from_reflect(prio).get_represented_type_info()
//             //     == FromReflect::from_reflect(transition)
//             // {}
//             // **prio.from_refect()
//             // let something = T::from_reflect(prio);
//             // state.transition(commands, entity);
//         }
//     }
//pseudocode
//check if there is a state transition event to a state on the transition prio list, if so
//remove this state
// state.transition_priorities();
// }
// }

// theres a type for that?
// struct StateTransitionPriorities(Vec<&'static dyn PlayerState>);
// impls StateTransitionPriorities {
// new() -> Self
// }

//trait reflection for states
//try to get rid of reflection
//i think i can just use any
//
//

//might be nice to hold a reference to the state its name of also
//StateTag?
//consider switching this enum out for using ComponentId, could store a vec of ComponentIds in
//PlayerState

// #[derive(Debug, PartialEq, Eq, Ord, PartialOrd, Copy, Clone)]
// pub enum StateLabel {
//     Idle,
//     Falling,
//     Attacking,
//     Grounded,
// }

// pub trait StateLike {}
// impl StateLike for StateLabel {}

//181

pub trait PlayerState: Component<Storage = SparseStorage> + Reflect + Copy + Clone {
    // type StateLike: StateLike;
    // fn state_label() -> StateLabel;
    //cancellable?
    //box or &?
    //i dont really want to have to keep instances of states around for each other state?
    //maybe keep priorities as their own thing as part of a transition
    //could try returning vec of typeids?
    //or just add enum of type names....
    // fn transition_priorities(&self) -> Vec<&'static dyn PlayerState>;
    // fn transition_priorities(&self) -> Vec<Box<dyn PlayerState>>;
    // fn transition_priorities() -> Vec<StateLabel>;

    // fn add_trans<T: PlayerState, N: PlayerState + Default>(
    //     &self,
    //     entity: Entity,
    //     // trans: PlayerStateTransition<T, N>,
    //     mut commands: Commands,
    // ) {
    //     let trans = PlayerStateTransition::<T, N>::new(*self, N::default());
    //     commands.entity(entity).insert(trans);
    // }

    // fn transition(
    //     &self,
    //     priorities: Vec<&'static dyn PlayerState>,
    //     commands: Commands,
    //     entity: Entity,
    // );

    //fn behavior?
}

#[derive(Reflect, Component, Default, Copy, Clone)]
#[component(storage = "SparseSet")]
pub struct Idle;

impl PlayerState for Idle {
    // fn state_label() -> StateLabel {
    //     StateLabel::Idle
    // }
    // fn transition_priorities(&self) -> Vec<Box<dyn PlayerState>> {
    // fn transition_priorities() -> Vec<StateLabel> {
    //     vec![StateLabel::Idle, StateLabel::Falling, StateLabel::Grounded]
    // }
    // fn add_trans<T: PlayerState, N: PlayerState>(
    //     &self,
    //     entity: Entity,
    //     // trans: PlayerStateTransition<T, N>,
    //     mut commands: Commands,
    // ) {
    //     // let new = PlayerStateTransition<T, N>::new();
    // }
}

#[derive(Reflect, Component, Default, Copy, Clone)]
#[component(storage = "SparseSet")]
pub struct Falling;

impl PlayerState for Falling {
    // fn state_label() -> StateLabel {
    //     StateLabel::Falling
    // }
    // fn transition_priorities() -> Vec<StateLabel> {
    //     vec![StateLabel::Grounded]
    // }
}

#[derive(Reflect, Component, Default, Copy, Clone)]
#[component(storage = "SparseSet")]
pub struct Grounded;

impl PlayerState for Grounded {
    // fn state_label() -> StateLabel {
    //     StateLabel::Grounded
    // }
    // fn transition_priorities() -> Vec<StateLabel> {
    //     vec![StateLabel::Idle]
    // }
}

//
//TODO: refactor, controller plugin, physics plugin, state plugin

fn player_move(
    time: Res<Time>,
    mut q_gent: Query<
        (
            &mut LinearVelocity,
            &ActionState<PlayerAction>,
        ),
        With<PlayerGent>,
    >,
) {
    for (mut velocity, action_state) in q_gent.iter_mut() {
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
