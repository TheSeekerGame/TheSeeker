use std::time::Duration;

// anyhow no longer required after refactor
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::*;
use glam::Vec2;
use theseeker_engine::physics::{Collider, ColliderShapeAccess};

use crate::appstate::StateDespawnMarker;
use crate::camera::MainCamera;
use crate::game::attack::Health;
use crate::game::enemy::Enemy;
use crate::game::gentstate::Dead;
use crate::prelude::Update;

const BACKGROUND_COLOR: Color = Color::NONE;
const ANIMATION_DELAY_IN_MILLIS: u64 = 600;
const ANIMATION_SPEED: f32 = 0.2;
const SPARK_DURATION_MILLIS: u64 = 400;
// UI constants
const BAR_WIDTH: f32 = 80.0;  // Base width (actual visible width will be less due to slant)
const BAR_HEIGHT: f32 = 16.0;  // Increased to allow spark to extend beyond visible bar
const BAR_SLOPE_FACTOR: f32 = 0.05; // Should match shader SLOPE constant

pub struct EnemyHpBarPlugin;

impl Plugin for EnemyHpBarPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiMaterialPlugin::<Material>::default());
        app
            .add_systems(Update, instance)
            .add_systems(Update, update_positions)
            .add_systems(Update, update_hp)
            .add_systems(Update, update_visibility)
            .add_systems(Update, despawn);
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
    /// Delay before the white trailing bar starts shrinking.
    delay: Timer,
    /// Lifetime of the sharp "spark" line at the damage frontier.
    spark: Timer,
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Copy, Debug)]
pub struct Material {
    /// A number between `0` and `1` indicating the current health amount (red part).
    #[uniform(0)]
    health: f32,
    /// The current position (in UV space) of the trailing damage indicator (white part).
    #[uniform(1)]
    damage: f32,
    /// Visibility / alpha for the transient "spark" line that marks recent damage.
    /// Ranges from `1` (fully visible) to `0` (invisible).
    #[uniform(2)]
    spark: f32,
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
                    // Root node of the health bar
                    Node {
                        width: Val::Px(BAR_WIDTH),
                        height: Val::Px(BAR_HEIGHT),
                        padding: UiRect::all(Val::Px(0.0)),
                        position_type: PositionType::Absolute,
                        ..default()
                    },
                    BackgroundColor(BACKGROUND_COLOR),
                    Visibility::Hidden,
                    Root { parent: entity },
                    StateDespawnMarker,
                ))
                .with_children(|parent| {
                    parent.spawn((
                        // The filled portion of the bar driven by custom material
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            align_self: AlignSelf::Center,
                            ..default()
                        },
                        MaterialNode(material.add(Material {
                            health: 1.0,
                            damage: 1.0,
                            spark: 0.0,
                        })),
                        Bar { parent: entity },
                        DamageAnimation {
                            delay: Timer::new(
                                Duration::from_millis(ANIMATION_DELAY_IN_MILLIS),
                                TimerMode::Once,
                            ),
                            spark: {
                                let mut t = Timer::new(
                                    Duration::from_millis(SPARK_DURATION_MILLIS),
                                    TimerMode::Once,
                                );
                                // Start in the finished state so the spark is invisible until first damage.
                                t.tick(Duration::from_millis(SPARK_DURATION_MILLIS));
                                t
                            },
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
    mut hp_root_q: Query<(&Root, &mut Node)>,
    mut camera_q: Query<(&GlobalTransform, &Camera), With<MainCamera>>,
) {
    let Ok((camera_transform, camera)) = camera_q.single() else { return; };

    for (hp_root, mut style) in hp_root_q.iter_mut() {
        let Ok((global_transform, collider)) = enemy_q.get(hp_root.parent) else {
            continue;
        };
        let mut world_position = global_transform.translation();

        // Makes the health bar float above the collider, if it exists
        world_position += match collider {
            Some(collider) => {
                let collider_height =
                    collider.shape().compute_local_aabb().half_extents().y;
                Vec3::new(0.0, collider_height, 0.0)
            },
            None => Vec3::ZERO,
        };

        let Ok(screen_position) = camera.world_to_viewport(camera_transform, world_position) else {
            // Off-screen or projection failed; nothing to do for this bar this frame.
            continue;
        };

        let width = match style.width {
            Val::Px(value) => value,
            _ => 100.0,
        };

        // center the bar, and make it hover above the collider
        let offset = Vec2::ZERO + Vec2::new(-width * 0.5, -60.0);

        // Update the position of the health bar UI
        style.left = Val::Px(screen_position.x + offset.x);
        style.top = Val::Px(screen_position.y + offset.y);
    }
}

fn update_hp(
    enemy_q: Query<&Health, With<Enemy>>,
    mut hp_bar_q: Query<(
        &Bar,
        &MaterialNode<Material>,
        &mut DamageAnimation,
    )>,
    mut materials: ResMut<Assets<Material>>,
    time: Res<Time>,
) {
    for (hp_bar, matnode, mut damage_animation) in &mut hp_bar_q {
        // Tick timers first so their state is up-to-date for this frame.
        damage_animation.delay.tick(time.delta());
        damage_animation.spark.tick(time.delta());

        let Ok(health) = enemy_q.get(hp_bar.parent) else {
            continue;
        };

        let Some(material) = materials.get_mut(&matnode.0) else {
            // Should never happen, but play safe.
            continue;
        };

        let health_factor = (health.current as f32) / (health.max as f32);

        // Detect change (damage) and restart animations when it happens
        if (material.health - health_factor).abs() > f32::EPSILON {
            damage_animation.delay.reset();
            damage_animation.spark.reset();
            material.spark = 1.0; // fully visible spark
        }

        // Animate trailing white bar once delay elapsed
        if damage_animation.delay.finished() {
            material.damage = material.damage.lerp(health_factor, ANIMATION_SPEED);
        }

        // Fade spark over its lifetime
        if !damage_animation.spark.finished() {
            // spark visibility decays from 1.0 -> 0.0 over the timer duration
            material.spark = 1.0 - damage_animation.spark.fraction();
        } else {
            material.spark = 0.0;
        }

        material.health = health_factor;
    }
}

// We no longer need a separate system to tick the damage animation; all timing is handled inside `update_hp`.

fn update_visibility(
    enemy_q: Query<Ref<Health>, With<Enemy>>,
    mut hp_root_q: Query<(&Root, &mut Visibility)>,
) {
    for (hp_bar, mut visibility) in hp_root_q.iter_mut() {
        let Ok(health) = enemy_q.get(hp_bar.parent) else {
            continue;
        };
        if health.is_changed() {
            if health.current == health.max {
                *visibility = Visibility::Hidden
            } else {
                *visibility = Visibility::Inherited
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
                commands.entity(hp_entity).despawn()
            },
            Ok(None) => {},
        }
    }
}
