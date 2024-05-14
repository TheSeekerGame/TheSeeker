use crate::camera::MainCamera;
use crate::game::attack::Health;
use crate::prelude::Update;
use bevy::prelude::*;
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::*;
use bevy::sprite::{MaterialMesh2dBundle, Mesh2dHandle};
use glam::{Vec2, Vec3Swizzles};
use theseeker_engine::physics::Collider;

pub struct HpBarsPlugin;

impl Plugin for HpBarsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiMaterialPlugin::<HpBarUiMaterial>::default());
        app.add_systems(Update, instance);
        app.add_systems(Update, update_positions);
        app.add_systems(Update, update_hp);
    }
}

#[derive(Component)]
struct HpBar(Entity);
#[derive(Component)]
struct HpBackground(Entity);

/*fn update(time: Res<Time>, mut ui_materials: ResMut<Assets<HpBarUiMaterial>>) {
    for (_, material) in ui_materials.iter_mut() {
        // rainbow color effect
        let new_color = Color::hsl((time.elapsed_seconds() * 60.0) % 360.0, 1., 0.5);
        material.color = new_color.rgba_to_vec4();
    }
}*/

fn instance(
    mut commands: Commands,
    entity_with_hp: Query<(Entity, Ref<Health>), With<GlobalTransform>>,
    mut ui_materials: ResMut<Assets<HpBarUiMaterial>>,
) {
    for ((entity, health)) in entity_with_hp.iter() {
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
                        background_color: Color::rgb(0.5, 0.5, 0.5).into(),
                        //visibility: Visibility::Hidden,
                        ..default()
                    },
                    HpBackground(entity),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        MaterialNodeBundle {
                            style: Style {
                                width: Val::Percent(100.0),
                                height: Val::Percent(100.0),
                                //padding: UiRect::all(Val::Px(3.0)),
                                align_self: AlignSelf::Center,
                                ..default()
                            },
                            material: ui_materials.add(HpBarUiMaterial {
                                factor: 1.0,
                                background_color: Color::rgb(0.1, 0.1, 0.1).into(),
                                filled_color: Color::rgb(0.8, 0.2, 0.2).into(),
                            }),
                            ..default()
                        },
                        HpBar(entity),
                    ));
                });
        }
    }
}

fn update_positions(
    mut commands: Commands,
    entity_with_hp: Query<(&GlobalTransform, Option<&Collider>)>,
    mut hp_bar: Query<(Entity, &HpBackground, &mut Style)>,
    mut q_cam: Query<(&GlobalTransform, &Camera), With<MainCamera>>,
) {
    let Some((camera_transform, camera)) = q_cam.iter().next() else {
        return;
    };

    for (bg_entity, hp_bg, mut style) in hp_bar.iter_mut() {
        if let Ok((global_transform, collider)) = entity_with_hp.get(hp_bg.0) {
            let mut world_position = global_transform.translation();

            // Makes the health bar float above the collider, if it exists
            world_position += match collider {
                Some(collider) => {
                    let collider_height = collider.0.compute_aabb().half_extents().y;
                    Vec3::new(0.0, collider_height, 0.0)
                },
                None => Vec3::ZERO,
            };

            // Calculate the screen position of the entity
            let screen_position = camera
                .world_to_viewport(camera_transform, world_position)
                .unwrap();

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
            //*visibility = Visibility::Visible;
        } else {
            commands.entity(bg_entity).despawn();
            // Hide the health bar UI if the entity is not found
            //visibility = Visibility::Hidden;
        }
    }
}

fn update_hp(
    mut commands: Commands,
    entity_with_hp: Query<&Health>,
    mut hp_bar: Query<(&HpBar, &Handle<HpBarUiMaterial>)>,
    mut ui_materials: ResMut<Assets<HpBarUiMaterial>>,
    //mut hp_bar_bg: Query<&mut Visibility, With<HpBackground>>,
) {
    for (hpbar, ui_mat_handle) in hp_bar.iter() {
        if let Ok(health) = entity_with_hp.get(hpbar.0) {
            if let Some(mat) = ui_materials.get_mut(ui_mat_handle) {
                mat.factor = 1.0 * (health.current as f32 / health.max as f32)
            }
        }
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Copy, Debug)]
pub struct HpBarUiMaterial {
    // A number between `0` and `1` indicating how much of the bar should be filled.
    #[uniform(0)]
    pub factor: f32,
    #[uniform(1)]
    pub background_color: Color,
    #[uniform(2)]
    pub filled_color: Color,
}

impl UiMaterial for HpBarUiMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/hp_bar.wgsl".into()
    }
}
