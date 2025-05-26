//! Level Management
//!
//! This is where we manage everything related to managing the map /
//! the in-game playable space/area.
//! A "level" is one "room" in the game, connected to others.
//!
//! The player should be able to walk between them,
//! and we dynamically switch (load/unload) levels as needed.
//!
//! This module is the framework for level transitions, loading/unloading,
//! general map setup, and other such managerial stuff.
//!
//! Any of the stuff that actually *happens* within the map when you
//! play the game, doesn't belong here. Put that stuff under [`crate::game`].

use seek_ecs_tilemap::tiles::TilePos;

use crate::parallax::ParallaxLayer;
use crate::prelude::*;

pub struct LevelManagerPlugin;

impl Plugin for LevelManagerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(LevelSelection::Identifier(
            "Level_0".into(),
        ));
        app.add_systems(
            OnEnter(AppState::InGame),
            game_level_init,
        );
        app.add_systems(Update, attach_parallax);
        app.add_systems(Update, hide_level_0);
        app.add_systems(
            Update,
            add_despawn_marker_to_entity::<TilePos>,
        );
    }
}

/// System to perform initial setup when entering the gameplay state, load the starting level.
fn game_level_init(mut commands: Commands, preloaded: Res<PreloadedAssets>) {
    // TODO: per-level asset management instead of preloaded assets
    // TODO: when we have save files, use that to choose the level to init at

    commands.spawn((
        LdtkWorldBundle {
            ldtk_handle: LdtkProjectHandle {
                handle: preloaded
                    .get_single_asset("level.01")
                    .expect("Expected asset key 'level.01'"),
            },
            ..Default::default()
        },
        StateDespawnMarker,
    ));

    //#[cfg(not(feature = "dev"))]
    // #[cfg(feature = "dev")]
    // commands.spawn((
    // StateDespawnMarker,
    // LdtkWorldBundle {
    // ldtk_handle: preloaded
    // .get_single_asset("level.dev")
    // .expect("Expected asset key 'level.dev'"),
    // ..Default::default()
    // },
    // ));
}

/// when a specific entity is spawned on level load
fn add_despawn_marker_to_entity<T: Component>(
    mut commands: Commands,
    query: Query<Entity, Added<T>>,
    state: Res<State<AppState>>,
    mut ran: Local<bool>,
) {
    // Ensures we only iterate over all the items once
    if state.is_changed() && *state.get() == AppState::InGame {
        *ran = false;
    }
    if *ran {
        return;
    }
    if query.iter().next().is_some() {
        *ran = true;
    }

    for e in query.iter() {
        commands.entity(e).insert(StateDespawnMarker);
    }
}

/// level_0 is a giant grey object that blocks all our backgrounds, so we hide it.
fn hide_level_0(
    mut commands: Commands,
    mut query: Query<(Entity, &Name, &mut Visibility)>,
    state: Res<State<AppState>>,
    mut ran: Local<bool>,
) {
    if state.is_changed() && *state.get() == AppState::InGame {
        *ran = false;
    }
    if *ran {
        return;
    }
    for (entity, name, mut visbility) in query.iter_mut() {
        if name.as_str() == "Level_0" {
            *visbility = Visibility::Hidden;
            println!("Made 'level_0' invisible");
            *ran = true;
            break;
        }
    }
}

/// An indicator component for when you want the main background without needing
/// to go through all the layers again.
#[derive(Component)]
pub struct MainBackround;

#[derive(Component)]
pub struct OtherBackround;

/// attaches parallax components to the different LdtkWorldBundle layers
fn attach_parallax(
    mut commands: Commands,
    mut query: Query<
        (Entity, &LayerMetadata, &mut Transform),
        (
            Without<ParallaxLayer>,
            Without<OtherBackround>,
            Without<MainBackround>,
        ),
    >,
) {
    for (entity, layer_metadata, mut transform) in query.iter_mut() {
        let mut use_parallax = true;
        // Convert from depth-based (0 = camera, 1 = normal, >1 = background)
        // to scale-based (1 = normal, 0 = fixed, <1 = slower movement)
        // The formula is: scale = 1 / (1 + amount)
        // This gives us the inverse relationship we need
        let (parallax_scale, z_offset) = match &*layer_metadata.identifier {
            "Background" | "TundraBackground" => {
                // depth 1.4 -> scale ~0.71
                (Vec2::splat(1.0 / 1.4), 0.4)
            },
            "FarBackground" | "TundraFarBackground" => {
                // depth 1.3 -> scale ~0.77
                (Vec2::splat(1.0 / 1.3), 0.3)
            },
            "MiddleBackground" | "TundraMiddleBackground" => {
                // depth 1.2 -> scale ~0.83
                (Vec2::splat(1.0 / 1.2), 0.2)
            },
            "NearBackground" | "TundraNearBackground" => {
                // depth 1.1 -> scale ~0.91
                (Vec2::splat(1.0 / 1.1), 0.1)
            },
            "Main" => {
                commands.entity(entity).insert(MainBackround);
                use_parallax = false;
                (Vec2::ONE, -transform.translation.z * 0.000001)
            },
            "Entities" => {
                commands.entity(entity).try_insert(StateDespawnMarker);
                continue;
            },
            _ => {
                commands.entity(entity).insert(OtherBackround);
                use_parallax = false;
                (Vec2::ONE, -transform.translation.z * 0.000001)
            },
        };

        println!(
            "{:?}: parallax_scale = {:?}, z_offset = {}",
            layer_metadata.identifier,
            parallax_scale,
            z_offset
        );
        commands.entity(entity).try_insert(StateDespawnMarker);

        transform.translation.z = 0.0 - z_offset;

        if use_parallax {
            commands.entity(entity).insert(ParallaxLayer {
                scale: parallax_scale,
            });
        }
    }
}
