use crate::prelude::*;

/// A simple plugin for applying parallax to entities.
/// Use by adding this plugin, and attaching the Parallax
/// component to target entities.
pub struct ParallaxPlugin;

impl Plugin for ParallaxPlugin {
    fn build(&self, app: &mut App) {
        // We run in post update so that changes are applied after any camera
        // transformations.
        app.add_systems(
            PostUpdate,
            apply_parallax,
        );
    }
}

#[derive(Clone, PartialEq, Debug, Default, Component)]
pub struct Parallax {
    /// How far away from the camera is the layer?
    /// 0 is on top of the camera, 1.0  is "normal distance"
    /// and larger numbers are background.
    pub(crate) depth: f32,
}

/// System to perform initial setup when entering the gameplay state, load the starting level.
fn apply_parallax() {
    // Attach layer trackers
}