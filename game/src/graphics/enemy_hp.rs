use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::*;
use glam::Vec2;
use theseeker_engine::physics::Collider;

use crate::appstate::StateDespawnMarker;
use crate::camera::MainCamera;
use crate::game::attack::Health;
use crate::game::enemy::Enemy;
use crate::prelude::Update;

pub struct EnemyHpBarPlugin;

impl Plugin for EnemyHpBarPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiMaterialPlugin::<Material>::default());
        app.add_systems(Update, instance);
        app.add_systems(Update, update_positions);
        app.add_systems(Update, update_hp);
        app.add_systems(Update, update_visibility);
    }
}

#[derive(Component)]
pub struct Root(pub Entity);

#[derive(Component)]
pub struct Bar(pub Entity);

#[derive(Asset, TypePath, AsBindGroup, Clone, Copy, Debug)]
pub struct Material {
    /// A number between `0` and `1` indicating how much of the bar should be filled.
    #[uniform(0)]
    pub factor: f32,
    #[uniform(1)]
    pub background_color: Color,
    #[uniform(2)]
    pub filled_color: Color,
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
                            width: Val::Px(75.0),
                            height: Val::Px(14.0),
                            padding: UiRect::all(Val::Px(3.0)),
                            ..default()
                        },
                        background_color: Color::rgb(0.75, 0.75, 0.75).into(),
                        visibility: Visibility::Hidden,
                        ..default()
                    },
                    Root(entity),
                    StateDespawnMarker,
                ))
                .with_children(|parent| {
                    parent.spawn((
                        MaterialNodeBundle {
                            style: Style {
                                width: Val::Percent(100.0),
                                height: Val::Percent(100.0),
                                align_self: AlignSelf::Center,
                                ..default()
                            },
                            material: material.add(Material {
                                factor: 1.0,
                                background_color: Color::rgb(0.15, 0.15, 0.15)
                                    .into(),
                                filled_color: Color::rgb(0.635, 0.196, 0.306)
                                    .into(),
                            }),
                            ..default()
                        },
                        Bar(entity),
                    ));
                });
        }
    }
}

fn update_positions(
    mut commands: Commands,
    enemy_q: Query<
        (&GlobalTransform, Option<&Collider>),
        (With<Health>, With<Enemy>),
    >,
    mut hp_root_q: Query<(Entity, &Root, &mut Style)>,
    mut camera_q: Query<(&GlobalTransform, &Camera), With<MainCamera>>,
) {
    let Some((camera_transform, camera)) = camera_q.iter().next() else {
        return;
    };

    for (bg_entity, hp_bg, mut style) in hp_root_q.iter_mut() {
        if let Ok((global_transform, collider)) = enemy_q.get(hp_bg.0) {
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
                .unwrap_or_default();

            let width = match style.width {
                Val::Px(value) => value,
                _ => 100.0,
            };

            // center the bar, and make it hover above the collider
            let mut offset = Vec2::ZERO + Vec2::new(-width * 0.5, -30.0);

            // Update the position of the health bar UI
            style.left = Val::Px(screen_position.x + offset.x);
            style.top = Val::Px(screen_position.y + offset.y);
            style.position_type = PositionType::Absolute;
        } else if hp_bg.0 != Entity::PLACEHOLDER {
            commands.entity(bg_entity).despawn();
        }
    }
}

fn update_hp(
    enemy_q: Query<&Health, With<Enemy>>,
    mut hp_bar_q: Query<(&Bar, &Handle<Material>)>,
    mut material: ResMut<Assets<Material>>,
) {
    for (hp_bar, ui_mat_handle) in hp_bar_q.iter() {
        if let Ok(health) = enemy_q.get(hp_bar.0) {
            if let Some(mat) = material.get_mut(ui_mat_handle) {
                mat.factor = 1.0 * (health.current as f32 / health.max as f32)
            }
        } else {
            if let Some(mat) = material.get_mut(ui_mat_handle) {
                mat.factor = 0.0;
            }
        }
    }
}

fn update_visibility(
    enemy_q: Query<Ref<Health>, With<Enemy>>,
    mut hp_root_q: Query<(&Root, &mut Visibility)>,
) {
    for (hp_bar, mut visibility) in hp_root_q.iter_mut() {
        if let Ok(health) = enemy_q.get(hp_bar.0) {
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
