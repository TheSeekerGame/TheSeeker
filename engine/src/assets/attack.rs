use bevy::reflect::TypePath;

use crate::prelude::*;

#[derive(Asset, Debug, Clone)]
#[derive(Serialize, Deserialize)]
#[derive(TypePath)]
pub struct AttackDescriptor {
    pub lifetime_ticks: u32,
    #[serde(default)]
    pub stop_motion: bool,
    #[serde(default)]
    pub collider: AttackColliderDescriptor,
}

impl AttackDescriptor {
    pub fn contact_melee_defaults() -> Self {
        Self {
            lifetime_ticks: 8,
            stop_motion: true,
            collider: AttackColliderDescriptor::default(),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.lifetime_ticks == 0 {
            return Err("lifetime_ticks must be greater than zero".to_owned());
        }
        self.collider.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
#[derive(Serialize, Deserialize)]
pub struct AttackColliderDescriptor {
    #[serde(default)]
    pub source: AttackColliderSource,
    pub half_width: f32,
    pub half_height: f32,
    #[serde(default)]
    pub offset_x: f32,
    #[serde(default)]
    pub offset_y: f32,
}

impl Default for AttackColliderDescriptor {
    fn default() -> Self {
        Self {
            source: AttackColliderSource::AnimationPixels,
            half_width: 8.0,
            half_height: 8.0,
            offset_x: 0.0,
            offset_y: 5.0,
        }
    }
}

impl AttackColliderDescriptor {
    pub fn validate(&self) -> Result<(), String> {
        if !self.half_width.is_finite() || self.half_width <= 0.0 {
            return Err("collider.half_width must be finite and positive".to_owned());
        }
        if !self.half_height.is_finite() || self.half_height <= 0.0 {
            return Err("collider.half_height must be finite and positive".to_owned());
        }
        if !self.offset_x.is_finite() {
            return Err("collider.offset_x must be finite".to_owned());
        }
        if !self.offset_y.is_finite() {
            return Err("collider.offset_y must be finite".to_owned());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum AttackColliderSource {
    #[default]
    AnimationPixels,
    FixedBox,
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_contact_melee_defaults() {
        assert!(AttackDescriptor::contact_melee_defaults().validate().is_ok());
    }

    #[test]
    fn rejects_invalid_attack_descriptor_values() {
        let mut descriptor = AttackDescriptor::contact_melee_defaults();
        descriptor.lifetime_ticks = 0;
        assert!(descriptor.validate().is_err());

        descriptor = AttackDescriptor::contact_melee_defaults();
        descriptor.collider.half_width = 0.0;
        assert!(descriptor.validate().is_err());
    }
}
