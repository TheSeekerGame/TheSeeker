use bevy::prelude::*;

use seek_ecs_tilemap::TilemapParallaxConfig;

/// A plugin for applying shader-based parallax to tilemap entities.
/// 
/// This plugin uses a shader-based approach where the parallax offset
/// is calculated in the vertex shader, eliminating per-frame Transform
/// updates and improving performance.
pub struct ParallaxPlugin;

impl Plugin for ParallaxPlugin {
    fn build(&self, app: &mut App) {
        // Register the component type for reflection
        app.register_type::<ParallaxLayer>();
        
        // System to sync ParallaxLayer components with the TilemapParallaxConfig resource
        app.add_systems(
            PostUpdate,
            sync_parallax_to_tilemap_config,
        );
    }
}

/// Component that defines parallax behavior for a tilemap layer.
/// 
/// The scale values control how much the layer moves relative to the camera:
/// - (1.0, 1.0) = normal movement (sticks to world)
/// - (0.0, 0.0) = no movement (screen-fixed, like a skybox)
/// - (0.5, 0.5) = moves at half speed (classic parallax background)
#[derive(Component, Debug, Clone, Copy, Default, Reflect)]
#[reflect(Component)]
pub struct ParallaxLayer {
    /// Parallax scale factors for X and Y axes
    pub scale: Vec2,
}

impl ParallaxLayer {
    /// Creates a new ParallaxLayer with the given scale factors.
    /// Values are clamped to the range [0.0, 1.0].
    pub fn new(scale: Vec2) -> Self {
        Self {
            scale: scale.clamp(Vec2::ZERO, Vec2::ONE),
        }
    }
    
    /// Creates a uniform parallax layer (same scale for both axes)
    pub fn uniform(scale: f32) -> Self {
        Self::new(Vec2::splat(scale))
    }
    
    /// Creates a screen-fixed layer (no parallax movement)
    pub fn screen_fixed() -> Self {
        Self::new(Vec2::ZERO)
    }
    
    /// Creates a normal world-space layer (no parallax effect)
    pub fn world_space() -> Self {
        Self::new(Vec2::ONE)
    }
}

/// System that syncs ParallaxLayer components to the TilemapParallaxConfig resource
fn sync_parallax_to_tilemap_config(
    mut tilemap_config: ResMut<TilemapParallaxConfig>,
    // Query for added or changed ParallaxLayer components
    parallax_query: Query<(Entity, &ParallaxLayer), Or<(Added<ParallaxLayer>, Changed<ParallaxLayer>)>>,
    // Query for removed components
    mut removed: RemovedComponents<ParallaxLayer>,
) {
    // Update config for new or changed ParallaxLayer components
    for (entity, parallax) in parallax_query.iter() {
        tilemap_config.set_scale(entity, parallax.scale);
    }
    
    // Remove from config when ParallaxLayer is removed
    for entity in removed.read() {
        tilemap_config.remove(entity);
    }
}

// Re-export old component names as deprecated aliases for backwards compatibility
#[deprecated(
    since = "0.15.0",
    note = "Use ParallaxLayer instead. The Parallax component using depth values is deprecated in favor of scale-based parallax."
)]
pub type Parallax = ParallaxLayer;

#[deprecated(
    since = "0.15.0", 
    note = "ParallaxOrigin is no longer needed with shader-based parallax. Remove this component."
)]
#[derive(Component, Default)]
pub struct ParallaxOrigin(pub Vec2);

#[deprecated(
    since = "0.15.0",
    note = "ParallaxOffset is no longer needed with shader-based parallax. Remove this component."
)]
#[derive(Component, Default)]
pub struct ParallaxOffset(pub Vec2);
