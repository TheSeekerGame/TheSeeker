use std::marker::PhantomData;

use bevy::ecs::component::SparseStorage;

use crate::prelude::*;

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

pub trait Transitionable<T: GentState + Default> {
    fn new_transition(_next: T) -> Box<dyn Fn(Entity, &mut Commands) + Send + Sync + 'static> {
        Box::new(|entity, commands| {
            commands
                .entity(entity)
                .insert(GentStateBundle::<T>::default());
        })
    }
}

//common state
#[derive(Component, Deref, DerefMut, Default)]
pub struct TransitionsFrom<T> {
    t: PhantomData<T>,
    #[deref]
    pub transitions: Vec<Box<dyn Fn(Entity, &mut Commands) + Send + Sync>>,
}

#[derive(Bundle, Default)]
pub struct GentStateBundle<T: GentState + Default> {
    state: T,
    transitions: TransitionsFrom<T>,
}

// States
// states are components which are added to the entity on transition.
// an entity can be in multiple states at once, eg Grounded and Running/Idle
// Impl Playerstate for each state
// Impl Transitionable<T: GentState> for each state that that should be able to be transitioned
// from by a state
pub trait GentState: Component<Storage = SparseStorage> + Clone {}

#[derive(Component, Default, Copy, Clone, Debug)]
#[component(storage = "SparseSet")]
pub struct Idle;
impl GentState for Idle {}
impl Transitionable<Running> for Idle {}

#[derive(Component, Default, Copy, Clone, Debug)]
#[component(storage = "SparseSet")]
pub struct Running;
impl GentState for Running {}
impl Transitionable<Idle> for Running {}

#[derive(Component, Default, Copy, Clone, Debug)]
#[component(storage = "SparseSet")]
pub struct Falling;
impl GentState for Falling {}
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
impl GentState for Jumping {}
impl Transitionable<Falling> for Jumping {}
impl Transitionable<Grounded> for Jumping {}

#[derive(Component, Default, Copy, Clone, Debug)]
#[component(storage = "SparseSet")]
pub struct Grounded;
impl GentState for Grounded {}
//cant be Idle or Running if not Grounded
impl Transitionable<Jumping> for Grounded {
    fn new_transition(
        _next: Jumping,
    ) -> Box<dyn Fn(Entity, &mut Commands) + Send + Sync + 'static> {
        Box::new(|entity, commands| {
            commands
                .entity(entity)
                .insert(GentStateBundle::<Jumping>::default())
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
                .insert(GentStateBundle::<Falling>::default())
                .remove::<(Idle, Running)>();
        })
    }
}

#[derive(Component, Default, Copy, Clone, Debug)]
#[component(storage = "SparseSet")]
pub struct Attacking;
impl GentState for Attacking {}

#[derive(Component, Default, Copy, Clone, Debug)]
#[component(storage = "SparseSet")]
pub struct Hitstun;
impl GentState for Hitstun {}
