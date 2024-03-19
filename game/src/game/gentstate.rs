use std::marker::PhantomData;

use bevy::ecs::component::SparseStorage;

use crate::prelude::*;

pub fn transition_from<T: Component>(
    mut query: Query<(Entity, &mut TransitionsFrom<T>)>,
    mut commands: Commands,
) {
    for (entity, mut trans) in query.iter_mut() {
        if !&trans.transitions.is_empty() {
            let transitions = std::mem::take(&mut trans.transitions);
            for transition in transitions {
                transition(entity, &mut commands);
            }
            commands.entity(entity).remove::<T>();
        }
    }
}

pub trait Transitionable<T: GentState> {
    fn new_transition(next: T) -> Box<dyn FnOnce(Entity, &mut Commands) + Send + Sync> {
        Box::new(move |entity, commands| {
            commands.entity(entity).insert(GentStateBundle::<T> {
                state: next,
                transitions: TransitionsFrom::<T>::default(),
            });
        })
    }
}

//common state
#[derive(Component, Deref, DerefMut)]
pub struct TransitionsFrom<T> {
    pub marker: PhantomData<T>,
    #[deref]
    pub transitions: Vec<Box<dyn FnOnce(Entity, &mut Commands) + Send + Sync>>,
}

impl<T: GentState> Default for TransitionsFrom<T> {
    fn default() -> Self {
        Self {
            marker: PhantomData::<T>::default(),
            transitions: Default::default(),
        }
    }
}

#[derive(Bundle, Default)]
pub struct GentStateBundle<T: GentState> {
    pub state: T,
    pub transitions: TransitionsFrom<T>,
}

#[derive(Component, Default)]
pub enum Facing {
    #[default]
    Right,
    Left,
}

impl Facing {
    pub fn direction(&self) -> f32 {
        match self {
            Facing::Right => 1.,
            Facing::Left => -1.,
        }
    }
}

// States
// states are components which are added to the entity on transition.
// an entity can be in multiple states at once, eg Grounded and Running/Idle
// Impl Playerstate for each state
// Impl Transitionable<T: GentState> for each state that that should be able to be transitioned
// from by a state
pub trait GentState: Component<Storage = SparseStorage> {}

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct Idle;
impl GentState for Idle {}
// impl Transitionable<Running> for Idle {}

// #[derive(Component, Default, Debug)]
// #[component(storage = "SparseSet")]
// pub struct Running;
// impl GentState for Running {}
// impl Transitionable<Idle> for Running {}

// #[derive(Component, Default, Debug)]
// #[component(storage = "SparseSet")]
// pub struct Falling;
// impl GentState for Falling {}
// impl Transitionable<Grounded> for Falling {}
// impl Transitionable<Running> for Falling {}
// impl Transitionable<Idle> for Falling {}

// #[derive(Component, Debug)]
// #[component(storage = "SparseSet")]
// pub struct Jumping {
//     current_air_ticks: u32,
//     max_air_ticks: u32,
// }
//
// impl Default for Jumping {
//     fn default() -> Self {
//         Jumping {
//             current_air_ticks: 0,
//             max_air_ticks: 30,
//         }
//     }
// }
// impl GentState for Jumping {}
// impl Transitionable<Falling> for Jumping {}
// impl Transitionable<Grounded> for Jumping {}

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct Grounded;
impl GentState for Grounded {}
//cant be Idle or Running if not Grounded
// impl Transitionable<Jumping> for Grounded {
//     fn new_transition(
//         _next: Jumping,
//     ) -> Box<dyn FnOnce(Entity, &mut Commands) + Send + Sync> {
//         Box::new(|entity, commands| {
//             commands
//                 .entity(entity)
//                 .insert(GentStateBundle::<Jumping>::default())
//                 .remove::<(Idle, Running)>();
//         })
//     }
// }
// //cant be Idle or Running if not Grounded
// impl Transitionable<Falling> for Grounded {
//     fn new_transition(
//         _next: Falling,
//     ) -> Box<dyn FnOnce(Entity, &mut Commands) + Send + Sync> {
//         Box::new(|entity, commands| {
//             commands
//                 .entity(entity)
//                 .insert(GentStateBundle::<Falling>::default())
//                 .remove::<(Idle, Running)>();
//         })
//     }
// }

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct Attacking;
impl GentState for Attacking {}

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct Hitstun;
impl GentState for Hitstun {}
