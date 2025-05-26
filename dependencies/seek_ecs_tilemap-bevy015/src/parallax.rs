//! Parallax configuration for tilemaps
//! 
//! This module provides a resource-based configuration system for 
//! shader-based parallax rendering of tilemaps.

use bevy::prelude::*;
use bevy::utils::HashMap;

/// Resource that stores parallax configuration for tilemap entities.
/// This is used by the tilemap renderer to apply parallax effects in the shader.
#[derive(Resource, Default, Clone)]
pub struct TilemapParallaxConfig {
    /// Maps tilemap entities to their parallax scales
    pub(crate) parallax_scales: HashMap<Entity, Vec2>,
}

impl TilemapParallaxConfig {
    /// Updates the parallax scale for a tilemap entity.
    /// 
    /// The scale values control how much the layer moves relative to the camera:
    /// - (1.0, 1.0) = normal movement (sticks to world)
    /// - (0.0, 0.0) = no movement (screen-fixed, like a skybox)
    /// - (0.5, 0.5) = moves at half speed (classic parallax background)
    pub fn set_scale(&mut self, entity: Entity, scale: Vec2) {
        self.parallax_scales.insert(entity, scale.clamp(Vec2::ZERO, Vec2::ONE));
    }
    
    /// Removes parallax configuration for an entity.
    pub fn remove(&mut self, entity: Entity) {
        self.parallax_scales.remove(&entity);
    }
    
    /// Gets the parallax scale for an entity, defaulting to Vec2::ONE if not set.
    pub fn get_scale(&self, entity: Entity) -> Vec2 {
        self.parallax_scales.get(&entity).copied().unwrap_or(Vec2::ONE)
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<TilemapParallaxConfig>();
} 