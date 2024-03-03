use crate::camera::MainCamera;
use crate::prelude::*;
use bevy::prelude::shape::Quad;
use bevy::render::render_resource::{AsBindGroup, ShaderRef};
use bevy::sprite::{Material2d, Material2dPlugin, MaterialMesh2dBundle};
use bevy_ecs_ldtk::GridCoords;

pub struct GraphicsFxPlugin;

impl Plugin for GraphicsFxPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(Material2dPlugin::<FogMaterial>::default());
        app.add_systems(OnEnter(AppState::InGame), setup_fog);
        app.add_systems(OnExit(AppState::InGame), cleanup_fog);
        app.add_systems(Startup, spawn_test_fog);
        app.add_systems(Update, update_fog);
    }
}

pub fn spawn_test_fog(mut commands: Commands) {
    #[cfg(not(feature = "dev"))]
    commands.spawn((
        Transform::from_translation(Vec3::new(10.0, 400.0, 0.0)),
        FogEmitter { dist: 200.0 },
    ));
    #[cfg(feature = "dev")]
    commands.spawn((
        Transform::from_translation(Vec3::new(10.0, 100.0, 0.0)),
        FogEmitter { dist: 200.0 },
    ));
}

#[derive(Component, Default)]
pub struct FogLayer;

/// marker component for tracking position of a fog emitter
/// any transform with this component will emit fog.
/// Distance is
#[derive(Component, Default)]
pub struct FogEmitter {
    dist: f32,
}

pub fn setup_fog(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<FogMaterial>>,
) {
    // Spawn four layers
    let depths = [1.25f32, 1.15, 1.05, 0.95, 0.85];
    // the last needs to be high to draw on top of everything, layers 5-15 are the various overlays
    let zs = [2.5f32, 3.5, 4.5, 16.0, 17.0];

    let alpha = 0.85 / depths.len() as f32;

    for (depth, z) in depths.into_iter().zip(zs.into_iter()) {
        commands.spawn((
            FogLayer,
            MaterialMesh2dBundle {
                mesh: meshes
                    .add(Mesh::from(Quad::new(Vec2::splat(
                        1000.0,
                    ))))
                    .into(),
                transform: Transform::default()
                    .with_scale(Vec3::splat(1.0))
                    .with_translation(Vec3::new(0.0, 0.0, z)),
                material: materials.add(FogMaterial {
                    depth,
                    alpha,
                    color: Color::rgba(0.87, 0.86, 1.0, alpha),
                    emitter1: Vec4::new(0.0, 400.0, 50.0, 0.0),
                }),
                ..default()
            },
        ));
    }
}

pub fn cleanup_fog(mut commands: Commands, mut fog_layers: Query<Entity, With<FogLayer>>) {
    for layer in &fog_layers {
        commands.entity(layer).despawn()
    }
}

pub fn update_fog(
    mut fog_bundle_query: Query<
        (
            &mut Transform,
            &Handle<FogMaterial>,
            &mut Visibility,
        ),
        With<FogLayer>,
    >,
    mut emitters: Query<(&Transform, &FogEmitter), Without<FogLayer>>,
    mut q_cam: Query<&Transform, (With<MainCamera>, Without<FogLayer>)>,
    mut materials: ResMut<Assets<FogMaterial>>,
) {
    let Some(cam_trnsfrm) = q_cam.iter().next() else {
        return;
    };
    // get the nearest emitter; currently only supports one emitter
    // on screen at a time;
    let mut mat_data = None;
    let mut sqr_dst = f32::MAX;
    for (transfrm, emitter) in emitters.iter() {
        let new_dist = transfrm
            .translation
            .distance_squared(cam_trnsfrm.translation);
        if new_dist < sqr_dst {
            sqr_dst = new_dist;
            mat_data = Some((transfrm, emitter));
        }
    }
    for (mut fog_trnsfrm, handle, mut visibility) in fog_bundle_query.iter_mut() {
        let Some(material) = materials.get_mut(handle) else {
            error!(
                "{}:{}: fog material missing from assets!",
                file!(),
                line!()
            );
            continue;
        };
        if let Some((emitter_trnsfrm, emitter)) = mat_data {
            material.emitter1.x = emitter_trnsfrm.translation.x;
            material.emitter1.y = emitter_trnsfrm.translation.y;
            material.emitter1.z = emitter.dist;
            fog_trnsfrm.translation.x = cam_trnsfrm.translation.x;
            fog_trnsfrm.translation.y = cam_trnsfrm.translation.y;

            *visibility = Visibility::Inherited
        } else {
            *visibility = Visibility::Hidden
        }
    }
}

/// each emitter field: (x, y, max_fog, max_dist)
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct FogMaterial {
    /// depth is used for the parallax affect, note that this is independent
    /// of actual transform depth
    #[uniform(0)]
    depth: f32,
    #[uniform(0)]
    alpha: f32,
    #[uniform(0)]
    color: Color,
    #[uniform(0)]
    /// x, y, are the coordinates of the emitter;
    /// z is distance
    emitter1: Vec4,
}

impl Material2d for FogMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/fog.wgsl".into()
    }
}
