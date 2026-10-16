//! Parallax: entities at a depth other than 1 shift against the camera as if nearer or farther.
use bevy::transform::TransformSystems::Propagate;

use crate::camera::MainCamera;
use crate::prelude::*;

/// Places parallaxed entities around the camera each frame, once it has moved.
pub struct ParallaxPlugin;

impl Plugin for ParallaxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, (init_parallax, apply_parallax).chain().before(Propagate));
    }
}

/// How far from the camera an entity lies: 1 is the world's own distance, less is nearer and
/// more is farther.
#[derive(Clone, PartialEq, Debug, Default, Component)]
pub struct Parallax {
    pub depth: f32,
}

/// Where a parallaxed entity lies at depth 1, taken from its first translation. Move the entity
/// by changing this: parallax overwrites its translation, which must therefore be in world
/// space, so the entity's parent must sit at the world origin.
#[derive(Clone, PartialEq, Debug, Default, Component)]
pub struct ParallaxOrigin(pub Vec2);

/// The point, relative to the origin, whose offset from the camera the depth scales, when it
/// is not the origin itself.
#[derive(Clone, PartialEq, Debug, Default, Component)]
pub struct ParallaxOffset(pub Vec2);

fn init_parallax(
    mut commands: Commands,
    entities: Query<(Entity, &Transform), (With<Parallax>, Without<ParallaxOrigin>, Without<MainCamera>)>,
) {
    for (entity, transform) in &entities {
        commands.entity(entity).insert(ParallaxOrigin(transform.translation.xy()));
    }
}

fn apply_parallax(
    mut entities: Query<(&mut Transform, &Parallax, &ParallaxOrigin, Option<&ParallaxOffset>), Without<MainCamera>>,
    camera: Single<&Transform, With<MainCamera>>,
) {
    let camera = camera.translation.xy();
    for (mut transform, parallax, origin, offset) in &mut entities {
        let offset = offset.map_or(Vec2::ZERO, |offset| offset.0);
        // Deriving the position from the origin each frame, rather than moving it with the
        // camera, keeps floating-point error from accumulating into drift.
        let center = camera - (camera - (origin.0 + offset)) / parallax.depth;
        let position = center - offset;
        transform.translation.x = position.x;
        transform.translation.y = position.y;
    }
}
