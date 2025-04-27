use crate::prelude::*;

// todo make generic
pub fn transition(
    mut query: Query<(Entity, &mut TransitionQueue)>,
    mut commands: Commands,
) {
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
    /// StartingState::new_transition(EndingState)
    fn new_transition(
        next: T,
    ) -> Box<dyn FnOnce(Entity, &mut Commands) + Send + Sync>
    where
        Self: GentState + Component,
        <Self as Transitionable<T>>::Removals: Bundle,
    {
        Box::new(move |entity, commands| {
            if let Some(mut entity_commands) = commands.get_entity(entity) {
                entity_commands.remove::<Self::Removals>().insert(next);
            }
        })
    }
}

// make not component, make field of state machine
#[derive(Component, Deref, DerefMut, Default)]
pub struct TransitionQueue(
    Vec<Box<dyn FnOnce(Entity, &mut Commands) + Send + Sync>>,
);

#[derive(Component, Deref, DerefMut, Default)]
pub struct AddQueue(Vec<Box<dyn FnOnce(Entity, &mut Commands) + Send + Sync>>);

impl AddQueue {
    pub fn add<T: GentState>(&mut self, next: T) {
        self.push(Box::new(move |entity, commands| {
            commands.entity(entity).insert(next);
        }))
    }
}

pub fn add_states(
    mut query: Query<(Entity, &mut AddQueue)>,
    mut commands: Commands,
) {
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

    /// Returns the opposite of a given [`Facing`] variant.
    pub fn invert(&self) -> Self {
        match self {
            Facing::Right => Facing::Left,
            Facing::Left => Facing::Right,
        }
    }
}

/// States
/// states are components which are added to the entity on transition.
/// an entity can be in multiple states at once, eg Grounded and Running/Idle
/// Impl GentState for each state
/// Impl Transitionable<T: GentState> for each state that that should be able to be transitioned
/// from by a state
pub trait GentState: Component {}

/// A GenericState has a blanket Transitionable impl for any GentState,
/// it will remove itself on transition
pub trait GenericState: Component {}

impl<T: GentState, N: GentState + GenericState> Transitionable<T> for N {
    type Removals = (N, Idle);
}

// on leaving some states the state machine should ensure we are not also in other specific states,
// in that case do not implement GenericState for the state and instead implement
// Transitionable with type Removals = (the states to remove)

// common states
#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct Idle;
impl GentState for Idle {}

/// Pseudostate, currently when this is added to player it is despawned,
/// when added to enemy it removes most components, plays a death animation and eventually despawns
#[derive(Component, Default, Debug)]
pub struct Dead {
    pub ticks: u32,
}

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
// #[derive(Component, Default, Debug)]
// #[component(storage = "SparseSet")]
// pub struct Hitstun;
// impl GentState for Hitstun {}



#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct Patrolling;
impl GentState for Patrolling {}
// Patrolling needs custom transition logic later (Patrolling -> Aggroed),
// so we don't implement GenericState for it yet.

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct Chasing;
impl GentState for Chasing {}
impl GenericState for Chasing {} // Uses default transition removal

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct Attacking; // Renamed from RangedAttack state in old code
impl GentState for Attacking {}
impl GenericState for Attacking {} // Uses default transition removal

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct Defending; // Renamed from Defense state in old code
impl GentState for Defending {}
impl GenericState for Defending {} // Uses default transition removal

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct Dying; // This replaces the old Dead state component logic
impl GentState for Dying {}
// Dying might need specific cleanup later, so no GenericState yet.

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct Decaying; // Replaces the old Decay component logic
impl GentState for Decaying {}
// Decaying likely leads to despawn, no GenericState needed.