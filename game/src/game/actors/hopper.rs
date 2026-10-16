//! Hopper decisions retain cadence, balanced direction sampling, and anger memory.

use bevy::prelude::*;

pub struct HopperMemory {
    pub idle_ready_tick: u32,
    pub air_jump_used: bool,
    pub air_jump_checked: bool,
    pub anger_active: bool,
    directions: [f32; 10],
    direction_index: usize,
}

impl Default for HopperMemory {
    fn default() -> Self {
        Self { idle_ready_tick: 0, air_jump_used: false, air_jump_checked: false,
            anger_active: false, directions: [-1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0], direction_index: 10 }
    }
}

pub fn next_random(state: &mut u32) -> u32 {
    *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    *state
}

impl HopperMemory {
    pub fn enter_idle(&mut self, rng: &mut u32, idle_ticks: [u32; 2], player: Option<Vec2>, position: Vec2, escape_distance: f32) -> bool {
        self.air_jump_used = false;
        self.air_jump_checked = false;
        let escaped = !self.anger_active || player.is_none_or(|player| player.distance(position) > escape_distance);
        if escaped {
            self.anger_active = false;
            self.idle_ready_tick = idle_ticks[(next_random(rng) & 1 == 0) as usize];
        } else { self.idle_ready_tick = idle_ticks[0]; }
        !escaped
    }

    pub fn direction(&mut self, rng: &mut u32, target_delta: Option<f32>) -> f32 {
        if let Some(delta) = target_delta.filter(|_| self.anger_active) { return delta.signum(); }
        if self.direction_index >= self.directions.len() { self.refill(rng); }
        let direction = self.directions[self.direction_index];
        self.direction_index += 1;
        direction
    }

    fn refill(&mut self, rng: &mut u32) {
        self.directions[..5].fill(-1.0);
        self.directions[5..].fill(1.0);
        for index in (1..10).rev() {
            let other = (next_random(rng) as usize) % (index + 1);
            self.directions.swap(index, other);
        }
        self.direction_index = 0;
    }

    pub fn try_air_jump(&mut self, rng: &mut u32, falling_ticks: u32) -> bool {
        if self.air_jump_used || self.air_jump_checked || falling_ticks < 12 { return false; }
        self.air_jump_checked = true;
        if falling_ticks >= 24 { return false; }
        let random = next_random(rng);
        let jump = if self.anger_active { ((random >> 24) as u8) < 64 } else { random & 1 == 0 };
        self.air_jump_used = jump;
        jump
    }
}
