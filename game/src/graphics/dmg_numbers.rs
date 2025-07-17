use bevy::color::palettes;
use bevy::prelude::*;
use theseeker_engine::physics::{Collider, ColliderShapeAccess};
use theseeker_engine::prelude::{GameTickUpdate, GameTime};
use theseeker_engine::animation::{SpriteAnimationBundle};
use theseeker_engine::script::ScriptPlayer;

use crate::camera::MainCamera;
use crate::game::attack::{apply_attack_damage, DamageInfo};
use crate::game::player::Player;
use crate::prelude::Update;
use crate::StateDespawnMarker;

pub struct DmgNumbersPlugin;

impl Plugin for DmgNumbersPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DamageNumberZOrder>();
        app.add_systems(
            GameTickUpdate,
            instance.after(apply_attack_damage),
        );
        app.add_systems(Update, (update_number, reset_z_order_after_timeout));
    }
}

/// Resource to track z-ordering for damage numbers
#[derive(Resource)]
struct DamageNumberZOrder {
    /// Current z offset for next damage number
    current_z: f32,
    /// Last time a damage number was spawned
    last_spawn_tick: u64,
}

impl Default for DamageNumberZOrder {
    fn default() -> Self {
        Self {
            current_z: 0.0,
            last_spawn_tick: 0,
        }
    }
}

/// Marker component for a damage number group, contains all digit entities
#[derive(Component)]
struct DmgNumberGroup {
    /// World position where the damage number spawned
    start_pos: Vec3,
    /// Time when the damage number was spawned
    spawn_time: f64,
    /// List of entity IDs for all digit sprites in this damage number
    digit_entities: Vec<Entity>,
}

/// Marker component for individual digit sprites
#[derive(Component)]
struct DmgNumberDigit;

fn instance(
    mut commands: Commands,
    enemy_query: Query<(&GlobalTransform, Option<&Collider>), Without<Player>>,
    player_query: Query<(&GlobalTransform, Option<&Collider>), With<Player>>,
    mut damage_events: EventReader<DamageInfo>,
    game_time: Res<GameTime>,
    q_cam: Query<(&GlobalTransform, &Camera), With<MainCamera>>,
    mut z_order: ResMut<DamageNumberZOrder>,
) {
    let Some((_camera_transform, _camera)) = q_cam.iter().next() else {
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
                let above_hb_offset = 15.0;
                let collider_height =
                    collider.shape().compute_local_aabb().half_extents().y;
                Vec3::new(
                    1.0,
                    collider_height + above_hb_offset,
                    0.0,
                )
            },
            None => Vec3::ZERO,
        };

        // Update z-order tracking
        z_order.current_z += 0.0000001;
        z_order.last_spawn_tick = game_time.tick();
        let damage_z = world_position.z + 0.1 + z_order.current_z;

        // Convert damage amount to string to get individual digits (rounded)
        let damage_text = format!("{:.0}", damage_info.amount);
        let digit_count = damage_text.len();
        
        // Calculate total width of the damage number display
        // Each digit is 3 pixels wide, with 1 pixel gap between them
        let total_width = (digit_count * 3 + (digit_count - 1)) as f32;
        
        // Calculate starting x position to center the damage number
        let start_x = world_position.x - total_width / 2.0;
        
        let mut digit_entities = Vec::new();
        
        // Spawn each digit sprite
        for (i, digit_char) in damage_text.chars().enumerate() {
            // Calculate position for this digit
            let digit_x = start_x + (i as f32 * 4.0); // 3 pixels for digit + 1 pixel gap
            let digit_pos = Vec3::new(digit_x, world_position.y, damage_z);
            
            // Get the animation key for this digit
            let anim_key = format!("anim.numbers.{}", digit_char);
            
            // Create the animation player
            let mut player = ScriptPlayer::default();
            player.play_key(&anim_key);
            
            // Spawn the digit entity
            let digit_entity = commands.spawn((
                DmgNumberDigit,
                SpriteAnimationBundle { player },
                Sprite {
                    texture_atlas: Some(TextureAtlas::default()),
                    color: text_color,
                    ..default()
                },
                Transform::from_translation(digit_pos),
                GlobalTransform::default(),
                Visibility::Visible,
                InheritedVisibility::VISIBLE,
                ViewVisibility::default(),
                StateDespawnMarker,
            )).id();
            
            digit_entities.push(digit_entity);
        }
        
        // Create a parent entity to track all the digits
        commands.spawn((
            DmgNumberGroup {
                start_pos: world_position,
                spawn_time: game_time.tick() as f64 / game_time.hz
                    + game_time.last_update().as_secs_f64(),
                digit_entities,
            },
            StateDespawnMarker,
        ));
    }
}

fn update_number(
    mut commands: Commands,
    mut dmg_group_q: Query<(Entity, &mut DmgNumberGroup)>,
    mut digit_q: Query<(&mut Transform, &mut Sprite), With<DmgNumberDigit>>,
    game_time: Res<GameTime>,
) {
    let max_time = 6.0;
    let fade_start_time = 0.5; // Start fading after 0.5 seconds

    for (group_entity, dmg_group) in dmg_group_q.iter_mut() {
        // This way the floating text position is dependent on the gametick time,
        // so if the game is paused, the floating numbers will pause as well.
        let elapsed_time = game_time.tick() as f64 / game_time.hz
            + game_time.last_update().as_secs_f64()
            - dmg_group.spawn_time;

        // Apply movement and fade to all digits
        let vertical_offset = 3.0 * elapsed_time as f32;
        
        // Calculate fade alpha with smooth transition
        let alpha = if elapsed_time as f32 <= fade_start_time {
            // Stay fully opaque for the first fade_start_time seconds
            1.0
        } else {
            // Then fade out smoothly over the remaining time
            let fade_duration = max_time - fade_start_time;
            let fade_elapsed = elapsed_time as f32 - fade_start_time;
            1.0 - (fade_elapsed / fade_duration).min(1.0)
        };
        
        // Update each digit
        for &digit_entity in &dmg_group.digit_entities {
            if let Ok((mut transform, mut sprite)) = digit_q.get_mut(digit_entity) {
                // Update position (only Y changes, X stays the same)
                transform.translation.y = dmg_group.start_pos.y + vertical_offset;
                
                // Update alpha while preserving the color tint
                sprite.color.set_alpha(alpha);
            }
        }

        // Despawn all entities when time is up
        if elapsed_time as f32 > max_time {
            // Despawn all digit entities
            for &digit_entity in &dmg_group.digit_entities {
                commands.entity(digit_entity).despawn();
            }
            // Despawn the group entity
            commands.entity(group_entity).despawn();
        }
    }
}

fn reset_z_order_after_timeout(
    mut z_order: ResMut<DamageNumberZOrder>,
    game_time: Res<GameTime>,
) {
    const RESET_AFTER_TICKS: u64 = 96; // 1 second at 96 Hz
    
    if game_time.tick() > z_order.last_spawn_tick + RESET_AFTER_TICKS {
        // Reset z-order if no damage numbers spawned for 1 second
        z_order.current_z = 0.0;
    }
}
