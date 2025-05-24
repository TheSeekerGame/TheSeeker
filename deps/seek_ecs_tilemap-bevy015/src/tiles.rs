use bevy::ecs::entity::MapEntities;
use bevy::ecs::reflect::ReflectMapEntities;
use bevy::prelude::*;

use crate::map::*;

pub(crate) fn plugin(_app: &mut App) {}

/// Bundle for tile entities.
#[derive(Bundle, Default, Clone, Copy, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TileBundle {
    pub position: TilePos,
    pub texture_index: TileTextureIndex,
    pub tilemap_id: TilemapId,
    pub visible: TileVisible,
    pub flip: TileFlip,
    pub color: TileColor,
    pub old_position: TilePosOld,
}

#[derive(Component, Reflect, Default, Clone, Copy, Debug)]
#[reflect(Component)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TilePosOld(pub TilePos);

/// A tile position in the tilemap grid.
#[derive(Component, Reflect, Default, Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd)]
#[reflect(Component)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TilePos {
    pub x: u32,
    pub y: u32,
}

impl TilePos {
    pub const fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }

    /// Converts a tile position (2D) into an index in a flattened vector (1D), assuming the
    /// tile position lies in a tilemap of the specified size.
    pub fn to_index(&self, tilemap_size: &TilemapSize) -> usize {
        ((self.y * tilemap_size.x) + self.x) as usize
    }

    /// Checks to see if `self` lies within a tilemap of the specified size.
    pub fn within_map_bounds(&self, map_size: &TilemapSize) -> bool {
        self.x < map_size.x && self.y < map_size.y
    }
}

impl From<TilePos> for UVec2 {
    fn from(pos: TilePos) -> Self {
        UVec2::new(pos.x, pos.y)
    }
}

impl From<&TilePos> for UVec2 {
    fn from(pos: &TilePos) -> Self {
        UVec2::new(pos.x, pos.y)
    }
}

impl From<UVec2> for TilePos {
    fn from(v: UVec2) -> Self {
        Self { x: v.x, y: v.y }
    }
}

impl From<TilePos> for Vec2 {
    fn from(pos: TilePos) -> Self {
        Vec2::new(pos.x as f32, pos.y as f32)
    }
}

impl From<&TilePos> for Vec2 {
    fn from(pos: &TilePos) -> Self {
        Vec2::new(pos.x as f32, pos.y as f32)
    }
}

/// A texture index into the atlas or texture array for a single tile. Indices in an atlas are horizontal based.
#[derive(Component, Reflect, Default, Clone, Copy, Debug, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[reflect(Component)]
pub struct TileTextureIndex(pub u32);

/// A custom color for the tile.
#[derive(Component, Reflect, Default, Clone, Copy, Debug)]
#[reflect(Component)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TileColor(pub Color);

impl From<Color> for TileColor {
    fn from(color: Color) -> Self {
        TileColor(color)
    }
}

/// Hides or shows a tile based on the boolean. Default: True
#[derive(Component, Reflect, Clone, Copy, Debug, Hash)]
#[reflect(Component)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TileVisible(pub bool);

impl Default for TileVisible {
    fn default() -> Self {
        Self(true)
    }
}

/// Flips the tiles texture along the X, Y or diagonal axes
#[derive(Component, Reflect, Default, Clone, Copy, Debug, Hash)]
#[reflect(Component)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TileFlip {
    /// Flip tile along the x axis.
    pub x: bool,
    /// Flip tile along the Y axis.
    pub y: bool,
    pub d: bool, // anti
}

/// Used to store tile entities for fast look up.
/// Tile entities are stored in a grid. The grid is always filled with None.
#[derive(Component, Reflect, Default, Debug, Clone)]
#[reflect(Component, MapEntities)]
pub struct TileStorage {
    tiles: Vec<Entity>,
    pub size: TilemapSize,
}

impl MapEntities for TileStorage {
    fn map_entities<M: EntityMapper>(&mut self, entity_mapper: &mut M) {
        for entity in self.tiles.iter_mut() {
            if *entity != Entity::PLACEHOLDER {
                *entity = entity_mapper.map_entity(*entity);
            }
        }
    }
}

impl TileStorage {
    /// Creates a new tile storage that is empty.
    pub fn empty(size: TilemapSize) -> Self {
        Self {
            tiles: vec![Entity::PLACEHOLDER; size.count()],
            size,
        }
    }

    /// Gets a tile entity for the given tile position, if an entity is associated with that tile
    /// position.
    ///
    /// Panics if the given `tile_pos` doesn't lie within the extents of the underlying tile map.
    pub fn get(&self, tile_pos: &TilePos) -> Option<Entity> {
        let e = self.tiles[tile_pos.to_index(&self.size)];
        if e != Entity::PLACEHOLDER {
            Some(e)
        } else {
            None
        }
    }

    /// Gets a tile entity for the given tile position, if:
    /// 1) the tile position lies within the underlying tile map's extents *and*
    /// 2) there is an entity associated with that tile position;
    /// otherwise it returns `None`.
    pub fn checked_get(&self, tile_pos: &TilePos) -> Option<Entity> {
        if tile_pos.within_map_bounds(&self.size) {
            self.get(tile_pos)
        } else {
            None
        }
    }

    /// Sets a tile entity for the given tile position.
    ///
    /// If there is an entity already at that position, it will be replaced.
    ///
    /// Panics if the given `tile_pos` doesn't lie within the extents of the underlying tile map.
    pub fn set(&mut self, tile_pos: &TilePos, tile_entity: Entity) {
        self.tiles[tile_pos.to_index(&self.size)] = tile_entity;
    }

    /// Sets a tile entity for the given tile position, if the tile position lies within the
    /// underlying tile map's extents.
    ///
    /// If there is an entity already at that position, it will be replaced.
    pub fn checked_set(&mut self, tile_pos: &TilePos, tile_entity: Entity) {
        if tile_pos.within_map_bounds(&self.size) {
            self.set(tile_pos, tile_entity)
        }
    }

    /// Returns an iterator with all of the positions in the grid.
    ///
    /// If an Entity has not been set for a given location, that value
    /// will be `Entity::PLACEHOLDER`.
    pub fn iter(&self) -> impl Iterator<Item = &Entity> {
        self.tiles.iter()
    }

    /// Returns mutable iterator with all of the positions in the grid.
    ///
    /// If an Entity has not been set for a given location, that value
    /// will be `Entity::PLACEHOLDER`.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Entity> {
        self.tiles.iter_mut()
    }

    /// Remove entity at the given tile position, if there was one.
    ///
    /// Panics if the given `tile_pos` doesn't lie within the extents of the underlying tile map.
    pub fn remove(&mut self, tile_pos: &TilePos) {
        self.tiles[tile_pos.to_index(&self.size)] = Entity::PLACEHOLDER;
    }

    /// Remove any stored entity at the given tile position, if the given `tile_pos` does lie within
    /// the extents of the underlying map.
    ///
    /// Otherwise, nothing is done.
    pub fn checked_remove(&mut self, tile_pos: &TilePos) {
        if tile_pos.within_map_bounds(&self.size) {
            self.tiles[tile_pos.to_index(&self.size)] = Entity::PLACEHOLDER;
        }
    }
}
