//! Immediate stretch followed by cubic recovery starting on tick one.
use crate::prelude::*;

#[derive(Debug, Clone, Copy)]
pub struct ScaleRecovery {
    current_tick: u32,
    base_scale: Vec3,
}
impl ScaleRecovery {
    pub fn start(transform: &mut Transform, existing: Option<Self>, peak: Vec2) -> Self {
        let base_scale = existing.map_or(transform.scale, |recovery| recovery.base_scale);
        transform.scale = Vec3::new(base_scale.x * peak.x, base_scale.y * peak.y, base_scale.z);
        Self { current_tick: 1, base_scale }
    }
    /// Returns true on the same tick that restores the base scale.
    pub fn tick(&mut self, transform: &mut Transform, duration: u32, peak: Vec2) -> bool {
        let t = (self.current_tick as f32 / duration as f32).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - t).powi(3);
        let strength = 1.0 - eased;
        transform.scale = Vec3::new(
            self.base_scale.x * (1.0 - (1.0 - peak.x) * strength),
            self.base_scale.y * (1.0 + (peak.y - 1.0) * strength), self.base_scale.z,
        );
        self.current_tick += 1;
        if self.current_tick > duration {
            transform.scale = self.base_scale;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retrigger_uses_original_scale_and_finishes_on_the_last_tick() {
        let base = Vec3::new(2.0, 3.0, 4.0);
        let peak = Vec2::new(0.5, 1.5);
        let mut transform = Transform::from_scale(base);
        let mut recovery = ScaleRecovery::start(&mut transform, None, peak);
        assert!(!recovery.tick(&mut transform, 3, peak));
        recovery = ScaleRecovery::start(&mut transform, Some(recovery), peak);
        assert_eq!(transform.scale, Vec3::new(1.0, 4.5, 4.0));
        assert!(!recovery.tick(&mut transform, 3, peak));
        assert!(!recovery.tick(&mut transform, 3, peak));
        assert!(recovery.tick(&mut transform, 3, peak));
        assert_eq!(transform.scale, base);
    }
}
