// For the hp bar, going to generate a mesh, and have the mesh be attacked to anything with hp.
// At least for the initial approach
// wwaaait, don't really need shaders for this; can just stack some rectangles; ez
// especially since its something so simple

use crate::prelude::{AppState, Component, OnExit, Update};
use bevy::prelude::{App, Assets, ColorMaterial, Commands, Mesh, Plugin, Rectangle, ResMut};
use bevy::sprite::{Material2dPlugin, Mesh2dHandle};
use theseeker_engine::prelude::{OnEnter, Startup};

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
struct HpBar;
fn run(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let mesh_handle = Mesh2dHandle(meshes.add(Rectangle::new(50.0, 100.0)));
}
