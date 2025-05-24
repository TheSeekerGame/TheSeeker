use bevy::diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::texture::BevyDefault;
use bevy::window::WindowMode;
use rand::random;
use seek_ecs_tilemap::map::*;
use seek_ecs_tilemap::tiles::*;
use seek_ecs_tilemap::{TilemapBundle, TilemapPlugin};

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            present_mode: bevy::window::PresentMode::Immediate, // Disable vsync
            mode: WindowMode::Windowed,
            ..default()
        }),
        ..default()
    }));
    app.add_plugins(TilemapPlugin);
    app.add_systems(Startup, setup);
    app.add_systems(Update, updates);
    app.add_systems(Update, updates_entire_map_pos);
    app.add_plugins(LogDiagnosticsPlugin::default())
        .add_plugins(FrameTimeDiagnosticsPlugin::default());
    app.run();
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn(Camera2dBundle {
        camera: Camera {
            hdr: true,
            ..default()
        },
        transform: Transform::from_xyz(420.0, 240.0, 0.0),
        ..Default::default()
    });

    let texture_handle: Handle<Image> = asset_server.load("dirt-tiles.png");
    /* commands.spawn(SpriteBundle {
        texture: texture_handle.clone(),
        transform: Transform::from_xyz(0.0, 0.0, 0.0),
        ..default()
    });*/

    for i in 1..5u32 {
        let width = 25 * i;
        let height = 25 * i; //random::<u8>() as u32 * i;
        let e_tilemap = commands
            .spawn(TilemapBundle {
                grid_size: TilemapGridSize::new(16.0, 16.0),
                map_type: TilemapType::Square,
                size: TilemapSize::new(width, height),
                spacing: Default::default(),
                storage: Default::default(),
                texture: TilesetTexture::Single(texture_handle.clone()),
                tile_size: TilemapTileSize::new(16.0, 16.0),
                chunks: TilemapChunks::default(),
                //transform: Transform::from_scale(Vec3::splat(0.25)),
                transform: Transform::from_scale(Vec3::splat(1.0 / (i as f32)))
                    .with_translation(Vec3::new(0.0, 0.0, i as f32)),
                global_transform: Default::default(),
                visibility: Default::default(),
                inherited_visibility: Default::default(),
                view_visibility: Default::default(),
            })
            .id();
        for y in 0..height {
            for x in 0..width {
                let idx = (x % 24) + ((y % 16) * width);
                //println!("{}", idx);
                commands.spawn(TileBundle {
                    position: TilePos::new(x, y),
                    texture_index: TileTextureIndex(idx),
                    tilemap_id: TilemapId(e_tilemap),
                    //visible: TileVisible(y % 2 == 0 || x % 2 == 0),
                    visible: TileVisible::default(),
                    flip: TileFlip {
                        x: false,
                        y: false,
                        d: false,
                    },
                    //color: TileColor(Srgba::rgb_u8(((x *25) % 256) as u8, ((y * 25) % 256) as u8, 0).into()),
                    color: TileColor(Color::WHITE.into()),
                    old_position: default(),
                });
            }
        }
    }
}

pub fn updates(
    mut q_tile: Query<(
        &TilemapId,
        &TilePos,
        &mut TileColor,
        &mut TileFlip,
        &mut TileVisible,
    )>,
    mut idx: Local<usize>,
) {
    for i in 0..10 {
        let Some((id, pos, mut color, flip, mut visible)) = q_tile.iter_mut().skip(*idx).next()
        else {
            *idx = 0;
            return;
        };

        *color =
            TileColor(Color::rgba(random::<f32>(), random::<f32>(), random::<f32>(), 1.0).into());

        visible.0 = random::<bool>();

        *idx += 1
    }
}

pub fn updates_entire_map_pos(
    mut q_map: Query<&mut Transform, With<TilemapType>>,
    time: Res<Time>,
) {
    for (i, mut map) in q_map.iter_mut().enumerate() {
        let time = time.elapsed_seconds() * (1.1 * i as f32) * 0.1;
        map.translation = Vec3::new(time.sin() * 100.0, time.cos() * 100.0, map.translation.z);
    }
}
