use crate::prelude::*;

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
}

/// Terminal state. Player is despawned; enemies shed gameplay components,
/// play their death animation, then despawn.
#[derive(Component, Default, Debug)]
pub struct Dead {
    pub ticks: u32,
}
