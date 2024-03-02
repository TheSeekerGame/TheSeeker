use crate::camera::MainCamera;
use crate::prelude::*;
use bevy::transform::TransformSystem::TransformPropagate;

/// A simple plugin for applying parallax to entities.
/// Use by adding this plugin, and attaching the Parallax
/// component to target entities.
///
/// Note that making parallaxed objects the child of another object will distort the calulation,
/// since offset is calculated from difference between cameras position and the objects [`ParallaxOrigin`]
pub struct ParallaxPlugin;

impl Plugin for ParallaxPlugin {
    fn build(&self, app: &mut App) {
        // We run in post update so that changes are applied after any camera
        // transformations.
        app.add_systems(
            PostUpdate,
            init_parallax.before(apply_parallax),
        );
        app.add_systems(
            PostUpdate,
            apply_parallax.before(TransformPropagate),
        );
    }
}

#[derive(Clone, PartialEq, Debug, Default, Component)]
pub struct Parallax {
    /// How far away from the camera is the layer?
    /// 0 is on top of the camera, 1.0  is "normal distance"
    /// and larger numbers are background.
    pub depth: f32,
}

/// Stores the "base" position of the Parallaxed object
/// if you want to move the parallaxed object around, change this instead of the transform.
///
/// Calculated from all parallex entities transform without this component and added to them.
#[derive(Clone, PartialEq, Debug, Default, Component)]
pub struct ParallaxOrigin(pub Vec2);

/// An optional component; add it if the center of the parallax'd objects
/// "center of parallax" is different from its origin. (offset should be set relative to local origin)
///
/// The center of parallax, is the position compared to the cameras position in order
/// to determine the parallax offset amount.
#[derive(Clone, PartialEq, Debug, Default, Component)]
pub struct ParallaxOffset(pub Vec2);

/// Applies parallax transformations
fn init_parallax(
    mut commands: Commands,
    mut query: Query<
        (Entity, &Transform),
        (
            Without<MainCamera>,
            With<Parallax>,
            Without<ParallaxOrigin>,
        ),
    >,
) {
    for (entity, transform) in query.iter_mut() {
        commands.entity(entity).insert(ParallaxOrigin(
            transform.translation.xy(),
        ));
    }
}
/// Applies parallax transformations
fn apply_parallax(
    mut query: Query<
        (
            &mut Transform,
            &Parallax,
            &ParallaxOrigin,
            Option<&ParallaxOffset>,
        ),
        Without<MainCamera>,
    >,
    q_cam: Query<&Transform, (With<MainCamera>)>,
) {
    let Some(cam_trnsfrm) = q_cam.iter().next() else {
        return;
    };
    let mut a = false;
    for (mut transform, parallax, origin, offset) in query.iter_mut() {
        let offset = offset.map(|x| x.0);
        let mut delta = cam_trnsfrm.translation.xy() - (origin.0 + offset.unwrap_or_default());

        delta = delta / (parallax.depth);
        if !a {
            println!(
                "origin: {}, local: {} parallax val: {} offset: {}",
                origin.0,
                transform.translation.xy(),
                parallax.depth,
                offset.unwrap_or_default(),
            );
            println!("cam: {}", cam_trnsfrm.translation.xy());
            a = true;
        }
        let mut pos_final = cam_trnsfrm.translation.xy() - delta - offset.unwrap_or_default();

        transform.translation.x = pos_final.x;
        transform.translation.y = pos_final.y;
    }
}
