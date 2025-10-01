use crate::prelude::*;

pub mod ghosting;
pub mod sprite_stretch;

pub use ghosting::{
    FadeCurve, Ghost, GhostColorMode, GhostMovement, GhostingPlugin,
    GhostingSet, GhostingSource, ScaleCurve,
};
pub use sprite_stretch::{SpriteStretch, SpriteStretchPlugin, StretchAnchor};

/// Per-entity sprite scale used by the stretch/squeeze effect.
/// Applied alongside `Sprite` for entities that need stretch/squeeze.
#[derive(Component, Default, Debug, Clone)]
pub struct SpriteScale {
    /// Horizontal scale factor (1.0 = normal)
    pub x: f32,
    /// Vertical scale factor (1.0 = normal)
    pub y: f32,
}

impl SpriteScale {
    pub fn new() -> Self {
        Self { x: 1.0, y: 1.0 }
    }

    pub fn reset(&mut self) {
        self.x = 1.0;
        self.y = 1.0;
    }
}
