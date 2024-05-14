// For the hp bar, going to generate a mesh, and have the mesh be attacked to anything with hp.
// At least for the initial approach
// wwaaait, don't really need shaders for this; can just stack some rectangles; ez
// especially since its something so simple

use crate::game::attack::Health;
use crate::prelude::Update;
use bevy::prelude::*;
use bevy::sprite::{MaterialMesh2dBundle, Mesh2dHandle};
use glam::{Vec2, Vec3Swizzles};

pub struct HpBarsPlugin;

impl Plugin for HpBarsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, run);
    }
}

// Yeah, so the plan is to iterate through everything with an hp component,
// and if hp is less then max, spawn in a rectangle entity, attach it to the entity as its child?
// okay I think that works. Yeah and then can use hp_bar component to track the child entity
// and then get the hp_bar entity using the parent child query.
// okay.
// or heck, dont even need an hp bar component on the main entity, since querys filter already

#[derive(Component)]
struct HpBar(Mesh2dHandle);
#[derive(Component)]
struct HpBackground(Mesh2dHandle);

fn run(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    entity_with_hp: Query<(Entity, &Children, &Health), (Without<HpBar>, Without<HpBackground>)>,
    mut hp_bar_child: Query<(Entity, &mut Transform, HpBar), Without<Without<HpBackground>>>,
    mut hp_backround_child: Query<(Entity, &Transform, HpBackground), Without<HpBar>>,
) {
    let hp_bar_width = 15.0;
    let hp_bar_height = 1.0;
    let padding = 0.2;
    for ((entity, children, hp)) in entity_with_hp.iter() {
        let should_display_bar = hp.current != hp.max || hp.current == 0;
        for &child in children.iter() {
            let mut anchor_pt = None;
            if let Ok((hp_bar_entity, transform, bar)) = hp_backround_child.get_mut(child) {
                if !should_display_bar {
                    commands.entity(entity).remove_children(&[hp_bar_entity]);
                    commands.entity(hp_bar_entity).despawn();
                }
                anchor_pt = Some(transform.translation.xy());
            }
            if let Ok((hp_bar_entity, mut transform, bar)) = hp_bar_child.get_mut(child) {
                if !should_display_bar {
                    commands.entity(entity).remove_children(&[hp_bar_entity]);
                    commands.entity(hp_bar_entity).despawn();
                } else {
                    let scale = hp.current as f32 / hp.max as f32;
                    transform.scale.x = scale;
                    transform.translation.x = anchor_pt.unwrap().x - scale * 0.5 + padding;
                }
            }
            // get the health of each child unit
            let health = q_child.get(child);
        }
    }
    let hp_bar_mesh_handle = Mesh2dHandle(meshes.add(Rectangle::new(
        hp_bar_width,
        hp_bar_height,
    )));
    let hp_bg_mesh_handle = Mesh2dHandle(meshes.add(Rectangle::new(
        hp_bar_width,
        hp_bar_height,
    )));
    let bg_color = Color::rgb(0.0, ));
    commands.spawn(MaterialMesh2dBundle {
        mesh: hp_bar_mesh_handle,
        material: materials.add(color),
        transform: Transform::from_xyz(
            // Distribute shapes from -X_EXTENT to +X_EXTENT.
            -X_EXTENT / 2. + i as f32 / (num_shapes - 1) as f32 * X_EXTENT,
            0.0,
            0.0,
        ),
        ..default()
    });
}
