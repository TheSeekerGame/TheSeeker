use bevy::prelude::*;

/// Base movement tuning in pixels per simulation tick at 96 Hz.
#[derive(Component)]
pub struct PlayerStats {
    pub max_move_velocity: f32,
}

impl Default for PlayerStats {
    fn default() -> Self {
        Self {
            max_move_velocity: 0.67,
        }
    }
}

#[derive(Component, Deref, DerefMut)]
pub struct EnemiesNearby(pub u32);

#[derive(Component, Debug)]
pub struct PlayerStatMod {
    pub damage: f32,
    pub defense: f32,
    pub speed: f32,
    pub cdr: f32,
    pub extra_jumps: u8,
}

impl PlayerStatMod {
    pub fn new() -> PlayerStatMod {
        PlayerStatMod {
            damage: 1.0,
            defense: 1.0,
            speed: 1.0,
            cdr: 1.0,
            extra_jumps: 0,
        }
    }
}
