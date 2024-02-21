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
        app.add_systems(Startup, (spawn_fog));
        app.add_systems(Update, (update_fog));
    }
}

#[derive(Component, Default)]
pub struct FogLayer;

pub fn spawn_fog(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<FogMaterial>>,
    asset_server: Res<AssetServer>,
) {
    // Spawn two layers
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
                depth: 0.7,
                alpha: 0.066,
                color: Color::BLUE,
                emitter1: Vec4::new(0.0, 400.0, 50.0, 0.0),
                emitter2: Default::default(),
                emitter3: Default::default(),
            }),
            ..default()
        },
    ));
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
                depth: 0.95,
                alpha: 0.066,
                color: Color::BLUE,
                emitter1: Vec4::new(0.0, 400.0, 50.0, 0.0),
                emitter2: Default::default(),
                emitter3: Default::default(),
            }),
            ..default()
        },
    ));
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
                depth: 1.1,
                alpha: 0.066,
                color: Color::BLUE,
                emitter1: Vec4::new(0.0, 400.0, 50.0, 0.0),
                emitter2: Default::default(),
                emitter3: Default::default(),
            }),
            ..default()
        },
    ));
    println!("fog spawned");
}

pub fn update_fog(
    mut commands: Commands,
    mut fog_bundle_query: Query<(&mut Transform), With<FogLayer>>,
    mut q_cam: Query<&Transform, (With<MainCamera>, Without<FogLayer>)>,
) {
    let Some(cam_trnsfrm) = q_cam.iter().next() else {
        return;
    };
    for (mut fog_trnsfrm) in fog_bundle_query.iter_mut() {
        fog_trnsfrm.translation = cam_trnsfrm.translation;
        fog_trnsfrm.translation.z = 100.0;
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
