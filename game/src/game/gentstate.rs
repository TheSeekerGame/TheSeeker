use crate::prelude::*;

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

/// Horizontal facing of an entity. `Copy` for cheap propagation in hot paths.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq)]
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

    /// Returns the opposite direction.
    pub fn invert(&self) -> Self {
        match self {
            Facing::Right => Facing::Left,
            Facing::Left => Facing::Right,
        }
    }
}

/// Marker trait for logical states applied as components.
/// Multiple states can coexist (e.g. locomotion + skill).
pub trait GentState: Component {}

// common states
// Note: Idle is now defined in player/states/mod.rs as a player-specific locomotion state

/// Terminal state. Player is despawned; enemies shed gameplay components,
/// play their death animation, then despawn.
#[derive(Component, Default, Debug)]
pub struct Dead {
    pub ticks: u32,
}

// Legacy state markers kept for reference during refactor; intentionally removed.
