use bevy::reflect::TypePath;

use crate::prelude::*;

#[derive(Asset, Debug, Clone)]
#[derive(Serialize, Deserialize)]
#[derive(TypePath)]
pub struct ProjectileDescriptor {
    #[serde(default)]
    pub kind: ProjectileDescriptorKind,
    pub lifetime_ticks: u32,
    #[serde(default)]
    pub ground_despawn: bool,
    #[serde(default)]
    pub collider: ProjectileColliderDescriptor,
    #[serde(default)]
    pub movement: BallisticProjectileDescriptor,
    #[serde(default)]
    pub hit_effects: Vec<ProjectileHitEffect>,
    #[serde(default)]
    pub direct: Option<DirectProjectileDescriptor>,
    #[serde(default)]
    pub sprite: Option<SpriteProjectileDescriptor>,
    #[serde(default)]
    pub pop: Option<ProjectilePopDescriptor>,
}

impl ProjectileDescriptor {
    pub fn standard_ball_defaults() -> Self {
        Self {
            kind: ProjectileDescriptorKind::Ballistic,
            lifetime_ticks: 480,
            ground_despawn: true,
            collider: ProjectileColliderDescriptor {
                half_width: 2.5,
                half_height: 2.5,
            },
            movement: BallisticProjectileDescriptor::arc_defaults(),
            hit_effects: vec![ProjectileHitEffect::Chilled {
                slow_multiplier: 0.5,
                duration_ticks: 192,
            }],
            direct: None,
            sprite: None,
            pop: None,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.lifetime_ticks == 0 {
            return Err("lifetime_ticks must be greater than zero".to_owned());
        }
        self.collider.validate()?;

        match self.kind {
            ProjectileDescriptorKind::Ballistic => self.validate_ballistic()?,
            ProjectileDescriptorKind::DirectSpritePop => {
                self.validate_direct_sprite_pop()?;
            },
        }
        Ok(())
    }

    fn validate_ballistic(&self) -> Result<(), String> {
        self.movement.validate()?;
        if self.direct.is_none() && self.sprite.is_none() && self.pop.is_none() {
            return Ok(());
        }

        Err(
            "ballistic projectiles cannot define direct, sprite, or pop sections"
                .to_owned(),
        )
    }

    fn validate_direct_sprite_pop(&self) -> Result<(), String> {
        self.direct
            .as_ref()
            .ok_or("direct_sprite_pop projectiles require [direct]")?
            .validate()?;
        self.sprite
            .as_ref()
            .ok_or("direct_sprite_pop projectiles require [sprite]")?
            .validate()?;
        self.pop
            .as_ref()
            .ok_or("direct_sprite_pop projectiles require [pop]")?
            .validate()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectileDescriptorKind {
    #[default]
    Ballistic,
    DirectSpritePop,
}

#[derive(Debug, Clone, Copy)]
#[derive(Serialize, Deserialize)]
pub struct ProjectileColliderDescriptor {
    pub half_width: f32,
    pub half_height: f32,
}

impl Default for ProjectileColliderDescriptor {
    fn default() -> Self {
        Self {
            half_width: 2.5,
            half_height: 2.5,
        }
    }
}

impl ProjectileColliderDescriptor {
    pub fn validate(&self) -> Result<(), String> {
        if !self.half_width.is_finite() || self.half_width <= 0.0 {
            return Err("collider.half_width must be finite and positive".to_owned());
        }
        if !self.half_height.is_finite() || self.half_height <= 0.0 {
            return Err("collider.half_height must be finite and positive".to_owned());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
#[derive(Serialize, Deserialize)]
pub struct BallisticProjectileDescriptor {
    pub gravity: f32,
    pub initial_speed: f32,
    pub speed_attempts: u32,
    pub speed_multiplier: f32,
    pub fallback_x_speed: f32,
    pub fallback_y_speed: f32,
}

impl Default for BallisticProjectileDescriptor {
    fn default() -> Self {
        Self::arc_defaults()
    }
}

impl BallisticProjectileDescriptor {
    pub fn arc_defaults() -> Self {
        Self {
            gravity: 432.0,
            initial_speed: 200.0,
            speed_attempts: 10,
            speed_multiplier: 1.15,
            fallback_x_speed: 134.0,
            fallback_y_speed: 151.0,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.gravity.is_finite() || self.gravity <= 0.0 {
            return Err("movement.gravity must be finite and positive".to_owned());
        }
        if !self.initial_speed.is_finite() || self.initial_speed <= 0.0 {
            return Err("movement.initial_speed must be finite and positive".to_owned());
        }
        if self.speed_attempts == 0 {
            return Err("movement.speed_attempts must be greater than zero".to_owned());
        }
        if !self.speed_multiplier.is_finite() || self.speed_multiplier < 1.0 {
            return Err("movement.speed_multiplier must be finite and at least 1.0".to_owned());
        }
        if !self.fallback_x_speed.is_finite() || self.fallback_x_speed < 0.0 {
            return Err("movement.fallback_x_speed must be finite and non-negative".to_owned());
        }
        if !self.fallback_y_speed.is_finite() {
            return Err("movement.fallback_y_speed must be finite".to_owned());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
#[derive(Serialize, Deserialize)]
pub struct DirectProjectileDescriptor {
    pub speed: f32,
    #[serde(default)]
    pub horizontal: bool,
}

impl DirectProjectileDescriptor {
    pub fn validate(&self) -> Result<(), String> {
        if !self.speed.is_finite() || self.speed <= 0.0 {
            return Err("direct.speed must be finite and positive".to_owned());
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
pub struct SpriteProjectileDescriptor {
    pub idle_anim_key: String,
    pub pop_anim_key: String,
    #[serde(default)]
    pub z: Option<f32>,
}

impl SpriteProjectileDescriptor {
    pub fn validate(&self) -> Result<(), String> {
        if self.idle_anim_key.trim().is_empty() {
            return Err("sprite.idle_anim_key must not be empty".to_owned());
        }
        if self.pop_anim_key.trim().is_empty() {
            return Err("sprite.pop_anim_key must not be empty".to_owned());
        }
        if self.z.is_some_and(|z| !z.is_finite()) {
            return Err("sprite.z must be finite when provided".to_owned());
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
pub struct ProjectilePopDescriptor {
    #[serde(default = "default_pop_trigger_radius")]
    pub trigger_radius: f32,
    #[serde(default = "default_true")]
    pub ignore_owner: bool,
    #[serde(default, alias = "ignored_damage_arches")]
    pub damage_ignore_arches: Vec<String>,
}

fn default_pop_trigger_radius() -> f32 {
    0.0
}

fn default_true() -> bool {
    true
}

impl ProjectilePopDescriptor {
    pub fn validate(&self) -> Result<(), String> {
        if !self.trigger_radius.is_finite() || self.trigger_radius < 0.0 {
            return Err("pop.trigger_radius must be finite and non-negative".to_owned());
        }
        if self
            .damage_ignore_arches
            .iter()
            .any(|arch| arch.trim().is_empty())
        {
            return Err("pop.damage_ignore_arches entries must not be empty".to_owned());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProjectileHitEffect {
    Chilled {
        slow_multiplier: f32,
        duration_ticks: u32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_standard_ball_defaults() {
        assert!(ProjectileDescriptor::standard_ball_defaults().validate().is_ok());
    }

    #[test]
    fn validates_direct_sprite_pop_descriptor() {
        let descriptor: ProjectileDescriptor = toml::from_str(
            r#"
            kind = "direct_sprite_pop"
            lifetime_ticks = 192

            [collider]
            half_width = 5.0
            half_height = 5.0

            [direct]
            speed = 220.0
            horizontal = true

            [sprite]
            idle_anim_key = "anim.test.Idle"
            pop_anim_key = "anim.test.Pop"

            [pop]
            trigger_radius = 8.0
            damage_ignore_arches = ["test"]
            "#,
        )
        .unwrap();

        assert!(descriptor.validate().is_ok());
    }

    #[test]
    fn rejects_invalid_projectile_descriptor_values() {
        let mut descriptor = ProjectileDescriptor::standard_ball_defaults();
        descriptor.movement.speed_attempts = 0;
        assert!(descriptor.validate().is_err());

        descriptor = ProjectileDescriptor::standard_ball_defaults();
        descriptor.collider.half_width = 0.0;
        assert!(descriptor.validate().is_err());

        descriptor = ProjectileDescriptor::standard_ball_defaults();
        descriptor.lifetime_ticks = 0;
        assert!(descriptor.validate().is_err());
    }
}
