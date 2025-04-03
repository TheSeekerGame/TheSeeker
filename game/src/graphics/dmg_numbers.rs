use bevy::color::palettes;
use bevy::prelude::*;
use theseeker_engine::physics::Collider;
use theseeker_engine::prelude::{GameTickUpdate, GameTime};

use crate::camera::MainCamera;
use crate::game::attack::{apply_attack_damage, DamageInfo};
use crate::game::player::Player;
use crate::prelude::Update;
use crate::StateDespawnMarker;

pub struct DmgNumbersPlugin;

impl Plugin for DmgNumbersPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            instance.after(apply_attack_damage),
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
    enemy_query: Query<(&GlobalTransform, Option<&Collider>), Without<Player>>,
    player_query: Query<(&GlobalTransform, Option<&Collider>), With<Player>>,
    mut damage_events: EventReader<DamageInfo>,
    game_time: Res<GameTime>,
    q_cam: Query<(&GlobalTransform, &Camera), With<MainCamera>>,
) {
    let Some((camera_transform, camera)) = q_cam.iter().next() else {
        return;
    };

    for damage_info in damage_events.read() {
        // Try the enemy query first:
        let (transform, collider, text_color) =
            if let Ok((transform, collider)) =
                enemy_query.get(damage_info.target)
            {
                (
                    transform,
                    collider,
                    if damage_info.crit && damage_info.stealthed {
                        palettes::css::PURPLE.into()
                    } else if damage_info.crit {
                        (palettes::css::YELLOW * 1.1).into()
                    } else if damage_info.stealthed {
                        palettes::css::PINK.into()
                    } else {
                        Color::WHITE
                    },
                )
            } else if let Ok((transform, collider)) =
                player_query.get(damage_info.target)
            {
                // For player, force red color
                (
                    transform,
                    collider,
                    palettes::css::RED.into(),
                )
            } else {
                continue;
            };

        let mut world_position = transform.translation();

        // Offset from collider (above hitbox) if available.
        world_position += match collider {
            Some(collider) => {
                let above_hb_offset = 11.0;
                let collider_height =
                    collider.0.compute_aabb().half_extents().y;
                Vec3::new(
                    1.0,
                    collider_height + above_hb_offset,
                    0.0,
                )
            },
            None => Vec3::ZERO,
        };

        let screen_position = camera
            .world_to_viewport(camera_transform, world_position)
            .unwrap_or_default();

        commands.spawn((
            DmgNumber(
                world_position,
                game_time.tick() as f64 / game_time.hz
                    + game_time.last_update().as_secs_f64(),
            ),
            Text(format!("{:.1}", damage_info.amount)),
            TextColor(text_color),
            TextFont {
                font: asset_server.load("font/Tektur-Regular.ttf"),
                font_size: 42.0,
                ..Default::default()
            },
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(screen_position.x),
                top: Val::Px(screen_position.y),
                ..default()
            },
            StateDespawnMarker,
        ));
    }
}

fn update_number(
    mut commands: Commands,
    mut dmg_number_q: Query<(
        Entity,
        &mut DmgNumber,
        &mut Node,
        &mut TextColor,
    )>,
    q_cam: Query<(&GlobalTransform, &Camera), With<MainCamera>>,
    game_time: Res<GameTime>,
) {
    let Some((camera_transform, camera)) = q_cam.iter().next() else {
        return;
    };
    let max_time = 6.0;

    for (entity, mut dmg_number, mut style, mut text_color) in
        dmg_number_q.iter_mut()
    {
        // This way the floating text position is dependent on the gametick time,
        // so if the game is paused, the floating numbers will pause as well.
        let elapsed_time = game_time.tick() as f64 / game_time.hz
            + game_time.last_update().as_secs_f64()
            - dmg_number.1;

        // apply a little wobble affect, and start each with a random different phase
        let global_pos =
            dmg_number.0 + Vec3::new(0.0, 3.0 * elapsed_time as f32, 0.0);
        let screen_position = camera
            .world_to_viewport(camera_transform, global_pos)
            .unwrap();

        // Fades the floating number out after waiting 4 seconds
        let a = text_color
            .alpha()
            .lerp(
                0.0,
                (elapsed_time as f32 - 1.0) / (max_time - 1.0),
            )
            .clamp(0.0, 1.0);

        text_color.set_alpha(a);

        style.left = Val::Px(screen_position.x);
        style.top = Val::Px(screen_position.y);
        style.position_type = PositionType::Absolute;

        if elapsed_time as f32 > max_time {
            commands.entity(entity).despawn();
        }
    }
}
