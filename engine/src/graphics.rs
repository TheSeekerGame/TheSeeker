use bevy::app::PluginGroupBuilder;
use bevy::prelude::With;
use bevy_ecs_ldtk::{GridCoords, LevelIid};
use bevy_ecs_ldtk::assets::LdtkProject;
use crate::prelude::{Added, App, apply_deferred, Assets, Bundle, Commands, Component, Entity, GameTickMidFlush, GameTickSet, GameTickUpdate, GameTime, Handle, LdtkEntity, Parent, Plugin, Query, Res, Update, Without};
use crate::time::{run_gametickupdate_schedule, update_gametime};

pub struct GraphicsFxPlugin;

impl Plugin for GraphicsFxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (spawn_fog, ),
        );
    }
}

#[derive(Component, Default)]
pub struct FogEmitterMarker;

pub fn spawn_fog(
    mut commands: Commands,
    fog_bundle_query: Query<(&GridCoords), With<FogEmitterMarker>>,
) {
    for (&grid_cords) in fog_bundle_query.iter(){
        println!("fog spawned at location: {grid_cords:?} type: {}", std::any::type_name_of_val(&grid_cords))
    }
}

pub fn update_fog(
    mut commands: Commands,
    fog_bundle_query: Query<(&GridCoords), With<FogEmitterMarker>>,
) {
    for (&grid_cords) in fog_bundle_query.iter(){
        println!("fog spawned at location: {grid_cords:?} type: {}", std::any::type_name_of_val(&grid_cords))
    }
}