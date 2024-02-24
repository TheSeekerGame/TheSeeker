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
        app.add_systems(Startup, setup_fog);
        app.add_systems(Startup, spawn_test_fog);
        app.add_systems(Update, update_fog);
    }
}

pub fn spawn_test_fog(
    mut commands: Commands,
) {
    commands.spawn((
        Transform::from_translation(Vec3::new(10.0, 400.0, 0.0)),
        FogEmitter { dist: 200.0 }
    ));
}

#[derive(Component, Default)]
pub struct FogLayer;

/// marker component for tracking position of a fog emitter
/// any transform with this component will emit fog.
#[derive(Component, Default)]
pub struct FogEmitter {
    dist: f32,
}

pub fn setup_fog(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<FogMaterial>>,
) {
    // Spawn three layers
    let depths = [0.7, 0.95, 1.1];
    let alpha = 0.2;
    for depth in depths {
        commands.spawn((
            FogLayer,
            MaterialMesh2dBundle {
                mesh: meshes
                    .add(Mesh::from(Quad::new(Vec2::splat(
                        1000.0,
                    ))))
                    .into(),
                transform: Transform::default().with_scale(Vec3::splat(1.0)),
                material: materials.add(FogMaterial {
                    depth: depth,
                    alpha: alpha / depths.len() as f32,
                    color: Color::rgba(0.87, 0.86, 1.0, 1.0),
                    emitter1: Vec4::new(0.0, 400.0, 50.0, 0.0),
                    emitter2: Default::default(),
                    emitter3: Default::default(),
                }),
                ..default()
            },
        ));
    }
    println!("fog spawned");
}

pub fn update_fog(
    mut commands: Commands,
    mut fog_bundle_query: Query<(&mut Transform, &Handle<FogMaterial>, &mut Visibility), With<FogLayer>>,
    mut emitters: Query<(&Transform, &FogEmitter), Without<FogLayer>>,
    mut q_cam: Query<&Transform, (With<MainCamera>, Without<FogLayer>)>,
    mut materials: ResMut<Assets<FogMaterial>>,
) {
    let Some(cam_trnsfrm) = q_cam.iter().next() else {
        return;
    };
    // get the nearest emitter; currently only supports one emitter
    // on screen at a time
    let mut mat_data = None;
    let mut sqr_dst = f32::MAX;
    for (transfrm, emitter) in emitters.iter() {
        let new_dist = transfrm.translation.distance_squared(cam_trnsfrm.translation);
        if new_dist < sqr_dst {
            sqr_dst = new_dist;
            mat_data = Some((transfrm, emitter));
        }
    }
    for (mut fog_trnsfrm, handle, mut visibility) in fog_bundle_query.iter_mut() {
        let Some(material) = materials.get_mut(handle) else {
            error!("{}:{}: fog material missing from assets!", file!(), line!());
            continue;
        };
        if let Some((emitter_trnsfrm, emitter)) = mat_data {
            material.emitter1.x = emitter_trnsfrm.translation.x;
            material.emitter1.y = emitter_trnsfrm.translation.y;
            material.emitter1.z = emitter.dist;
            fog_trnsfrm.translation = cam_trnsfrm.translation;
            fog_trnsfrm.translation.z = 100.0;

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
    #[uniform(0)]
    emitter2: Vec4,
    #[uniform(0)]
    emitter3: Vec4,
}

impl Material2d for FogMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/fog.wgsl".into()
    }
}
