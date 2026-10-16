//! Shared velocity evolution; pickup, damage, and target ownership stay with each effect.
use crate::prelude::*;

#[derive(Clone, Copy)]
pub struct ScatterHomingProfile {
    pub ticks: u32,
    pub damping_start: f32,
    pub damping_end: f32,
    pub max_speed: f32,
    pub steering: f32,
}
#[derive(Debug, Clone, Copy)]
pub struct ScatterHoming {
    ticks_left: u32,
    peak_speed: f32,
}
impl ScatterHoming {
    pub fn new(profile: ScatterHomingProfile) -> Self {
        Self { ticks_left: profile.ticks, peak_speed: 0.0 }
    }
    /// None after scattering; Some marks whether this step completed it.
    pub fn scatter(&mut self, velocity: &mut Vec2, profile: ScatterHomingProfile) -> Option<bool> {
        if self.ticks_left == 0 { return None; }
        let t = 1.0 - self.ticks_left as f32 / profile.ticks as f32;
        *velocity *= profile.damping_start + (profile.damping_end - profile.damping_start) * t;
        self.ticks_left -= 1;
        Some(self.ticks_left == 0)
    }
    pub fn steer(&mut self, velocity: &mut Vec2, direction: Vec2, distance: f32, profile: ScatterHomingProfile) {
        self.peak_speed = self.peak_speed.max((distance * 0.9).min(profile.max_speed));
        let target_velocity = direction * self.peak_speed;
        let t = 1.0 - self.ticks_left as f32 / profile.ticks as f32;
        let homing_scale = t * t * (3.0 - 2.0 * t);
        *velocity = velocity.lerp(target_velocity, profile.steering * homing_scale);
    }
}
