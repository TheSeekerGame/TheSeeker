use std::time::Duration;

use anyhow::{anyhow, Result};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::*;
use bevy::time::Stopwatch;
use bevy::utils;
use glam::Vec2;
use iyes_bevy_extras::system::any_added_component;
use theseeker_engine::physics::Collider;
use theseeker_engine::time::GameTickUpdate;

use crate::appstate::StateDespawnMarker;
use crate::camera::MainCamera;
use crate::game::attack::Health;
use crate::game::enemy::Enemy;
use crate::game::gentstate::Dead;
use crate::prelude::Update;

const BACKGROUND_COLOR: Color = Color::rgba(0.5, 0.5, 0.5, 0.5);

pub struct EnemyHpBarPlugin;

impl Plugin for EnemyHpBarPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiMaterialPlugin::<Material>::default());
        app.add_systems(
            Update,
            (
                instance,
                update_positions.map(utils::dbg),
                update_hp.map(utils::dbg),
                update_visibility,
                tick_damage_animation,
                despawn,
            ),
        );
    }
}

#[derive(Component)]
pub struct Root {
    pub parent: Entity,
}

#[derive(Component)]
pub struct Bar {
    pub parent: Entity,
}

#[derive(Component)]
pub struct DamageAnimation {
    delay: Timer,
    progress: Stopwatch,
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Copy, Debug)]
pub struct Material {
    /// A number between `0` and `1` indicating the health amount.
    #[uniform(0)]
    health: f32,
    /// The current position of the damage taken indicator.
    #[uniform(1)]
    damage: f32,
}

impl UiMaterial for Material {
    fn fragment_shader() -> ShaderRef {
        "shaders/enemy_hp.wgsl".into()
    }
}

fn instance(
    mut commands: Commands,
    enemy_q: Query<(Entity, Ref<Health>), (With<GlobalTransform>, With<Enemy>)>,
    mut material: ResMut<Assets<Material>>,
) {
    for (entity, health) in enemy_q.iter() {
        if health.is_added() {
            commands
                .spawn((
                    NodeBundle {
                        style: Style {
                            width: Val::Px(60.0),
                            height: Val::Px(10.0),
                            padding: UiRect::horizontal(Val::Px(2.0)),
                            position_type: PositionType::Absolute,
                            ..default()
                        },
                        background_color: BACKGROUND_COLOR.into(),
                        visibility: Visibility::Hidden,
                        ..default()
                    },
                    Root { parent: entity },
                    StateDespawnMarker,
                ))
                .with_children(|parent| {
                    parent.spawn((
                        MaterialNodeBundle {
                            style: Style {
                                width: Val::Percent(100.0),
                                height: Val::Percent(200.0),
                                align_self: AlignSelf::Center,
                                ..default()
                            },
                            material: material.add(Material {
                                health: 1.0,
                                damage: 1.0,
                            }),
                            ..default()
                        },
                        Bar { parent: entity },
                        DamageAnimation {
                            delay: Timer::new(
                                Duration::from_millis(1100),
                                TimerMode::Once,
                            ),
                            progress: Stopwatch::new(),
                        },
                    ));
                });
        }
    }
}

fn update_positions(
    enemy_q: Query<
        (&GlobalTransform, Option<&Collider>),
        (With<Health>, With<Enemy>),
    >,
    mut hp_root_q: Query<(&Root, &mut Style)>,
    mut camera_q: Query<(&GlobalTransform, &Camera), With<MainCamera>>,
) -> Result<()> {
    let (camera_transform, camera) = camera_q.get_single()?;

    for (hp_root, mut style) in hp_root_q.iter_mut() {
        let (global_transform, collider) = enemy_q.get(hp_root.parent)?;
        let mut world_position = global_transform.translation();

        // Makes the health bar float above the collider, if it exists
        world_position += match collider {
            Some(collider) => {
                let collider_height =
                    collider.0.compute_aabb().half_extents().y;
                Vec3::new(0.0, collider_height, 0.0)
            },
            None => Vec3::ZERO,
        };

        let screen_position = camera
            .world_to_viewport(camera_transform, world_position)
            .ok_or(anyhow!(
                "Unable to get screen position from camera."
            ))?;

        let width = match style.width {
            Val::Px(value) => value,
            _ => 100.0,
        };

        // center the bar, and make it hover above the collider
        let offset = Vec2::ZERO + Vec2::new(-width * 0.5, -30.0);

        // Update the position of the health bar UI
        style.left = Val::Px(screen_position.x + offset.x);
        style.top = Val::Px(screen_position.y + offset.y);
    }

    Ok(())
}

fn update_hp(
    enemy_q: Query<&Health, With<Enemy>>,
    mut hp_bar_q: Query<(
        &Bar,
        &Handle<Material>,
        &mut DamageAnimation,
    )>,
    mut material: ResMut<Assets<Material>>,
) -> Result<()> {
    for (hp_bar, handle, mut damage_animation) in &mut hp_bar_q {
        let health = enemy_q.get(hp_bar.parent)?;
        let material = material.get_mut(handle).ok_or(anyhow!(
            "Enemy health bar material not found"
        ))?;

        let health_factor = 1.0 * (health.current as f32 / health.max as f32);

        // Reset the damage animation on taking damage
        if material.health != health_factor {
            damage_animation.delay.reset();
            damage_animation.progress.reset();
        }

        if damage_animation.delay.finished() {
            material.damage = material.damage.lerp(health_factor, 0.1);
        }

        material.health = health_factor;
    }

    Ok(())
}

fn tick_damage_animation(
    mut damage_animation: Query<&mut DamageAnimation>,
    time: Res<Time>,
) {
    for mut damage_animation in &mut damage_animation {
        damage_animation.delay.tick(time.delta());
        if damage_animation.delay.finished() {
            damage_animation.progress.tick(time.delta());
        }
    }
}

fn update_visibility(
    enemy_q: Query<Ref<Health>, With<Enemy>>,
    mut hp_root_q: Query<(&Root, &mut Visibility)>,
) {
    for (hp_bar, mut visibility) in hp_root_q.iter_mut() {
        if let Ok(health) = enemy_q.get(hp_bar.parent) {
            if health.is_changed() {
                if health.current == health.max {
                    *visibility = Visibility::Hidden
                } else {
                    *visibility = Visibility::Inherited
                }
            }
        }
    }
}

fn despawn(
    mut commands: Commands,
    enemy_q: Query<Option<&Dead>, With<Enemy>>,
    hp_root_q: Query<(Entity, &Root)>,
) {
    for (hp_entity, hp_root) in &hp_root_q {
        match enemy_q.get(hp_root.parent) {
            // Despawn if entity can't be found or is marked as dead.
            Ok(Some(_)) | Err(_) => {
                commands.entity(hp_entity).despawn_recursive()
            },
            Ok(None) => {},
        }
    }
}
