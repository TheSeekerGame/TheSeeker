use crate::camera::MainCamera;
use crate::game::attack::{attack_damage, Attack};
use crate::prelude::Update;
use bevy::prelude::*;
use theseeker_engine::physics::Collider;
use theseeker_engine::prelude::{GameTickUpdate, GameTime};

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

/// Marker component for a damage number, the vec2 is the starting spawn location
/// in world space, and the f32 is the time it was spawned in
#[derive(Component)]
struct DmgNumber(Vec3, f64);

fn instance(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    entity_with_hp: Query<(&GlobalTransform, Option<&Collider>)>,
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
                if let Ok((transform, collider)) = entity_with_hp.get(*attacked) {
                    let mut world_position = transform.translation();

                    // Makes the number start above the collider, if it exists
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
                        DmgNumber(
                            world_position,
                            game_time.tick() as f64 / game_time.hz
                                + game_time.last_update().as_secs_f64(),
                        ),
                        TextBundle::from_section(
                            format!("{}", attack.damage),
                            TextStyle {
                                font: asset_server.load("font/Tektur-Regular.ttf"),
                                font_size: 24.0,
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
    mut dmg_numer_q: Query<(
        Entity,
        &mut DmgNumber,
        &mut Style,
        &mut Text,
    )>,
    q_cam: Query<(&GlobalTransform, &Camera), With<MainCamera>>,
    game_time: Res<GameTime>,
) {
    let Some((camera_transform, camera)) = q_cam.iter().next() else {
        return;
    };
    let max_time = 6.0;

    for (entity, mut dmg_number, mut style, mut text) in dmg_numer_q.iter_mut() {
        let text_style = &mut text.sections[0].style;
        // This way the floating text position is dependent on the gametick time,
        // so if the game is paused, the floating numbers will pause as well.
        let elapsed_time = game_time.tick() as f64 / game_time.hz
            + game_time.last_update().as_secs_f64()
            - dmg_number.1;

        // apply a little wobble affect, and start each with a random different phase
        let global_pos = dmg_number.0
            + Vec3::new(
                ((elapsed_time + dmg_number.1 * 777777.0).sin() * elapsed_time * 5.0) as f32,
                4.0 * elapsed_time as f32,
                0.0,
            );
        let screen_position = camera
            .world_to_viewport(camera_transform, global_pos)
            .unwrap();

        // Fades the floating number out after waiting 4 seconds
        let a = text_style
            .color
            .a()
            .lerp(
                0.0,
                (elapsed_time as f32 - 4.0) / (max_time - 4.0),
            )
            .clamp(0.0, 1.0);

        text_style.color = Color::rgba(
            text_style.color.r(),
            text_style.color.g(),
            text_style.color.b(),
            a,
        );

        style.left = Val::Px(screen_position.x);
        style.top = Val::Px(screen_position.y);
        style.position_type = PositionType::Absolute;

        if elapsed_time as f32 > max_time {
            commands.entity(entity).despawn();
        }
    }
}
