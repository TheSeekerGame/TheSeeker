use bevy::prelude::*;
use seek_ecs_tilemap::map::{TilemapChunks, TilemapId, TilemapSize};
use seek_ecs_tilemap::tiles::{
    TilePos, TileTextureIndex, TileColor, TileFlip, TileVisible,
};

/// Plugin that compresses all tiles of every tilemap into the chunk
/// buffer once, then despawns the tile entities.
pub struct TilemapStaticifyPlugin;

impl Plugin for TilemapStaticifyPlugin {
    fn build(&self, app: &mut App) {
        // Run in PostUpdate so that we can wait until LDtk has spawned all tile
        // entities. The system disables itself automatically after running
        // once.
        app.add_systems(Last, compress_and_despawn_tiles_once);
    }
}

/// Transfer tile component data into the owning `TilemapChunks` and
/// immediately despawn the tile entity.
#[allow(clippy::type_complexity)]
fn compress_and_despawn_tiles_once(
    mut commands: Commands,
    // All components carried by a static tile.
    q_tiles: Query<(
        Entity,
        &TilemapId,
        &TilePos,
        &TileTextureIndex,
        &TileColor,
        &TileFlip,
        &TileVisible,
    )>,
    // Access to map size and chunk buffer on each tilemap entity.
    mut q_maps: Query<(&TilemapSize, &mut TilemapChunks)>,
    mut ran: Local<bool>,
) {
    // If we've already performed the compression, early-exit.
    if *ran {
        return;
    }

    // Wait until tiles actually exist (LDtk finished spawning).
    if q_tiles.is_empty() {
        return;
    }

    // Ensure every tilemap has its chunk buffer initialized before we start
    // pushing tile data.
    for (size, mut chunks) in q_maps.iter_mut() {
        chunks.init((*size).into(), UVec2::new(2048, 2048));
    }

    for (tile_entity, map_id, pos, tex_idx, color, flip, visible) in &q_tiles {
        if let Ok((_size, mut chunks)) = q_maps.get_mut(map_id.0) {
            chunks.set_tiledata_at(pos, tex_idx, color, flip, visible);
        }
        commands.entity(tile_entity).despawn_recursive();
    }

    // Mark as done so we stop scheduling work every frame.
    *ran = true;
} 