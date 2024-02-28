use bevy::transform::TransformSystem::TransformPropagate;
use crate::camera::MainCamera;
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
            apply_parallax.before(TransformPropagate),
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

/// Applies parallax transformations
fn apply_parallax(
    mut query: Query<(&mut Transform, &Parallax, &GlobalTransform), Without<MainCamera>>,
    q_cam: Query<&Transform, (With<MainCamera>)>,
    mut last_cam_pos: Local<Vec2>,
    mut runs: Local<u32>,
) {
    let Some(cam_trnsfrm) = q_cam.iter().next() else {
        return;
    };
    // Todo: figure out how to either do the camera offset parallax (find center point of bg)
    //  or figure out how to compensate for all the inititial spawning movements
    //  (maybe have a global offset tracker for everything with parallax and deterministically
    //  shift that based on camera's total movements? Wait no, still problem of center not being known...
    let mut a = false;
    if *runs < 2 {
        // Skip the first frame to give time for the transform hierarchies to propagate once.
        *runs += 1;
        *last_cam_pos = cam_trnsfrm.translation.xy();
        return;
    } else {
        let delta = cam_trnsfrm.translation.xy() - *last_cam_pos;
        for (mut transform, parallax, global_transform) in query.iter_mut() {
            let delta = delta / (parallax.depth);
            if !a {
                println!("globl: {}, local: {}", global_transform.translation().xy(), transform.translation.xy());
                println!("cam: {}", cam_trnsfrm.translation.xy());
                a = true;
            }
            //let pos_final = cam_trnsfrm.translation.xy() + delta;
            transform.translation.x += delta.x;
            transform.translation.y += delta.y;
        }
        *last_cam_pos = cam_trnsfrm.translation.xy();
    }
}