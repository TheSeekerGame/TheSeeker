use bevy::ecs::component::SparseStorage;
use leafwing_input_manager::orientation::Direction;

use crate::prelude::*;

//todo make generic
pub fn transition(mut query: Query<(Entity, &mut TransitionQueue)>, mut commands: Commands) {
    for (entity, mut trans) in query.iter_mut() {
        if !&trans.is_empty() {
            let transitions = std::mem::take(&mut trans.0);
            for transition in transitions {
                transition(entity, &mut commands);
            }
        }
    }
}

pub trait Transitionable<T: GentState> {
    type Removals;
    fn new_transition(next: T) -> Box<dyn FnOnce(Entity, &mut Commands) + Send + Sync>
    where
        Self: GentState + Component,
        <Self as Transitionable<T>>::Removals: Bundle,
    {
        Box::new(move |entity, commands| {
            commands
                .entity(entity)
                .remove::<Self::Removals>()
                .insert(next);
        })
    }
}

//make not component, make field of state machine
#[derive(Component, Deref, DerefMut, Default)]
pub struct TransitionQueue(Vec<Box<dyn FnOnce(Entity, &mut Commands) + Send + Sync>>);

#[derive(Component, Deref, DerefMut, Default)]
pub struct AddQueue(Vec<Box<dyn FnOnce(Entity, &mut Commands) + Send + Sync>>);

impl AddQueue {
    pub fn add<T: GentState>(&mut self, next: T) {
        self.push(Box::new(move |entity, commands| {
            commands.entity(entity).insert(next);
        }))
    }
}

pub fn add_states(mut query: Query<(Entity, &mut AddQueue)>, mut commands: Commands) {
    for (entity, mut add_states) in query.iter_mut() {
        if !&add_states.is_empty() {
            let additions = std::mem::take(&mut add_states.0);
            for addition in additions {
                addition(entity, &mut commands);
            }
        }
    }
}

#[derive(Component, Debug, Default, Clone)]
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

/// States
/// states are components which are added to the entity on transition.
/// an entity can be in multiple states at once, eg Grounded and Running/Idle
/// Impl GentState for each state
/// Impl Transitionable<T: GentState> for each state that that should be able to be transitioned
/// from by a state
pub trait GentState: Component<Storage = SparseStorage> {}

/// A GenericState has a blanket Transitionable impl for any GentState,
/// it will remove itsself on transition
pub trait GenericState: Component<Storage = SparseStorage> {}

impl<T: GentState, N: GentState + GenericState> Transitionable<T> for N {
    type Removals = (N, Idle);
}

//on leaving some states the state machine should ensure we are not also in other specific states,
//in that case do not implement GenericState for the state and instead implement
//Transitionable with type Removals = (the states to remove)

//common states
#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct Idle;
impl GentState for Idle {}
//
// #[derive(Component, Default, Debug)]
// #[component(storage = "SparseSet")]
// pub struct Grounded;
// impl GentState for Grounded {}
//
// #[derive(Component, Default, Debug)]
// #[component(storage = "SparseSet")]
// pub struct Attacking;
// impl GentState for Attacking {}
//

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct Hitstun;
impl GentState for Hitstun {}
