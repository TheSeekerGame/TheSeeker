use crate::camera::MainCamera;
use crate::game::attack::{attack_damage, Attack, Health};
use crate::game::player::Player;
use crate::prelude::Update;
use bevy::prelude::*;
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::*;
use bevy::sprite::{MaterialMesh2dBundle, Mesh2dHandle};
use glam::{Vec2, Vec3Swizzles};
use theseeker_engine::physics::Collider;
use theseeker_engine::prelude::{GameTickUpdate, GameTime};

// alright, how to go about the damage number affect...
// I guess just read from any damage components when they do damage?
// similar to what I was doing for the hitfreeze affect.

pub struct DmgNumbersPlugin;

impl Plugin for DmgNumbersPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            instance.after(attack_damage),
        );
        app.add_systems(Update, update_number);
    }
}

/*
#[derive(Component)]
struct HpBar(Entity);
*/
/// Marker component for a damage number, the vec2 is the starting spawn location
/// in world space, and the f32 is the time it was spawned in
#[derive(Component)]
struct DmgNumber(Vec3, f32);

fn instance(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    entity_with_hp: Query<(
        Entity,
        &GlobalTransform,
        Option<&Collider>,
    )>,
    attacks: Query<&Attack, With<GlobalTransform>>,
    game_time: Res<GameTime>,
    q_cam: Query<(&GlobalTransform, &Camera), With<MainCamera>>,
) {
    let Some((camera_transform, camera)) = q_cam.iter().next() else {
        return;
    };

    for attack in attacks.iter() {
        for (attacked, at_tick) in attack.damaged.iter() {
            if *at_tick == game_time.tick() {
                // only spawn in a floating number for a new attack damage instance
                if let Ok((entity, transform, collider)) = entity_with_hp.get(*attacked) {
                    let mut world_position = transform.translation();

                    // Makes the health bar float above the collider, if it exists
                    world_position += match collider {
                        Some(collider) => {
                            let collider_height = collider.0.compute_aabb().half_extents().y;
                            Vec3::new(0.0, collider_height + 10.0, 0.0)
                        },
                        None => Vec3::ZERO,
                    };

                    let screen_position = camera
                        .world_to_viewport(camera_transform, world_position)
                        .unwrap();

                    commands.spawn((
                        DmgNumber(world_position),
                        TextBundle::from_section(
                            format!("{}", attack.damage),
                            TextStyle {
                                font: asset_server.load("font/Tektur-Bold.ttf"),
                                font_size: 20.0,
                                color: Color::RED,
                            },
                        )
                        .with_style(Style {
                            position_type: PositionType::Absolute,
                            left: Val::Px(screen_position.x),
                            top: Val::Px(screen_position.y),
                            ..default()
                        }),
                    ));
                }
            }
        }
    }
}

fn update_number(
    mut commands: Commands,
    mut dmg_numer_q: Query<(Entity, &mut DmgNumber, &mut Style)>,
    q_cam: Query<(&GlobalTransform, &Camera), With<MainCamera>>,
) {
    let Some((camera_transform, camera)) = q_cam.iter().next() else {
        return;
    };

    for (entity, mut dmg_number, mut style) in dmg_numer_q.iter_mut() {
        dmg_number.0 += Vec3::new(0.0, 0.09, 0.0);

        let screen_position = camera
            .world_to_viewport(camera_transform, dmg_number.0)
            .unwrap();

        // Update the position of the health bar UI
        style.left = Val::Px(screen_position.x);
        style.top = Val::Px(screen_position.y);
        style.position_type = PositionType::Absolute;
    }
}
