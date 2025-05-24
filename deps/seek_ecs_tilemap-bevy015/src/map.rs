use bevy::ecs::entity::MapEntities;
use bevy::ecs::reflect::ReflectMapEntities;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureUsages};
use bevy::render::sync_world::SyncToRenderWorld;

use crate::tiles::*;

pub(crate) fn plugin(_app: &mut App) {}

#[derive(Bundle, Default)]
pub struct TilemapBundle {
    pub grid_size: TilemapGridSize,
    pub map_type: TilemapType,
    pub size: TilemapSize,
    pub spacing: TilemapSpacing,
    pub storage: TileStorage,
    pub texture: TilesetTexture,
    pub tile_size: TilemapTileSize,
    pub chunks: TilemapChunks,
    pub transform: Transform,
    pub global_transform: GlobalTransform,
    pub visibility: Visibility,
    pub inherited_visibility: InheritedVisibility,
    pub view_visibility: ViewVisibility,
    pub sync: SyncToRenderWorld,
}

/// A component which stores a reference to the tilemap entity.
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[reflect(Component, MapEntities)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TilemapId(pub Entity);

impl MapEntities for TilemapId {
    fn map_entities<M: EntityMapper>(&mut self, entity_mapper: &mut M) {
        self.0 = entity_mapper.map_entity(self.0);
    }
}

impl Default for TilemapId {
    fn default() -> Self {
        Self(Entity::PLACEHOLDER)
    }
}

/// Size of the tilemap in tiles.
#[derive(Component, Reflect, Default, Clone, Copy, Debug, Hash)]
#[reflect(Component)]
pub struct TilemapSize {
    pub x: u32,
    pub y: u32,
}

impl TilemapSize {
    pub const fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }

    pub const fn count(&self) -> usize {
        (self.x * self.y) as usize
    }
}

impl From<TilemapSize> for Vec2 {
    fn from(tilemap_size: TilemapSize) -> Self {
        Vec2::new(tilemap_size.x as f32, tilemap_size.y as f32)
    }
}

impl From<&TilemapSize> for Vec2 {
    fn from(tilemap_size: &TilemapSize) -> Self {
        Vec2::new(tilemap_size.x as f32, tilemap_size.y as f32)
    }
}

impl From<TilemapSize> for UVec2 {
    fn from(size: TilemapSize) -> Self {
        UVec2::new(size.x, size.y)
    }
}

impl From<UVec2> for TilemapSize {
    fn from(vec: UVec2) -> Self {
        TilemapSize { x: vec.x, y: vec.y }
    }
}

#[derive(Component, Reflect, Clone, Debug, Hash, PartialEq, Eq)]
pub enum TilesetTexture {
    /// All textures for tiles are inside a single image asset directly next to each other
    Single(Handle<Image>),
    /// Each tile's texture has its own image asset (each asset must have the same size), so there
    /// is a vector of image assets.
    ///
    /// Each image should have the same size, identical to the provided `TilemapTileSize`. If this
    /// is not the case, a panic will be thrown during the verification when images are being
    /// extracted to the render world.
    Vector(Vec<Handle<Image>>),
    /// The tiles are provided as array layers inside a KTX2 or DDS container.
    TextureContainer(Handle<Image>),
}

impl Default for TilesetTexture {
    fn default() -> Self {
        TilesetTexture::Single(Default::default())
    }
}

impl TilesetTexture {
    pub fn image_handles(&self) -> Vec<&Handle<Image>> {
        match &self {
            TilesetTexture::Single(handle) => vec![handle],
            TilesetTexture::Vector(handles) => handles.iter().collect(),
            TilesetTexture::TextureContainer(handle) => vec![handle],
        }
    }

    pub fn verify_ready(&self, images: &Res<Assets<Image>>) -> bool {
        self.image_handles().into_iter().all(|h| {
            if let Some(image) = images.get(h) {
                image
                    .texture_descriptor
                    .usage
                    .contains(TextureUsages::COPY_SRC)
            } else {
                false
            }
        })
    }

    /// Sets images with the `COPY_SRC` flag.
    pub fn set_images_to_copy_src(&self, images: &mut ResMut<Assets<Image>>) {
        for handle in self.image_handles() {
            // NOTE: We retrieve it non-mutably first to avoid triggering an `AssetEvent::Modified`
            // if we didn't actually need to modify it
            if let Some(image) = images.get(handle) {
                if !image
                    .texture_descriptor
                    .usage
                    .contains(TextureUsages::COPY_SRC)
                {
                    if let Some(image) = images.get_mut(handle) {
                        image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING
                            | TextureUsages::COPY_SRC
                            | TextureUsages::COPY_DST;
                    };
                }
            }
        }
    }

    pub fn clone_weak(&self) -> Self {
        match self {
            TilesetTexture::Single(handle) => TilesetTexture::Single(handle.clone_weak()),
            TilesetTexture::Vector(handles) => {
                TilesetTexture::Vector(handles.iter().map(|h| h.clone_weak()).collect())
            }
            TilesetTexture::TextureContainer(handle) => {
                TilesetTexture::TextureContainer(handle.clone_weak())
            }
        }
    }
}

/// Size of the tiles in pixels
#[derive(Component, Reflect, Default, Clone, Copy, Debug, PartialOrd, PartialEq)]
#[reflect(Component)]
pub struct TilemapTileSize {
    pub x: f32,
    pub y: f32,
}

impl TilemapTileSize {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

impl From<TilemapTileSize> for TilemapGridSize {
    fn from(tile_size: TilemapTileSize) -> Self {
        TilemapGridSize {
            x: tile_size.x,
            y: tile_size.y,
        }
    }
}

impl From<TilemapTileSize> for Vec2 {
    fn from(tile_size: TilemapTileSize) -> Self {
        Vec2::new(tile_size.x, tile_size.y)
    }
}

impl From<&TilemapTileSize> for Vec2 {
    fn from(tile_size: &TilemapTileSize) -> Self {
        Vec2::new(tile_size.x, tile_size.y)
    }
}

impl From<Vec2> for TilemapTileSize {
    fn from(v: Vec2) -> Self {
        let Vec2 { x, y } = v;
        TilemapTileSize { x, y }
    }
}

/// Size of the tiles on the grid in pixels.
/// This can be used to overlay tiles on top of each other.
/// Ex. A 16x16 pixel tile can be overlapped by 8 pixels by using
/// a grid size of 16x8.
#[derive(Component, Reflect, Default, Clone, Copy, Debug, PartialOrd, PartialEq)]
#[reflect(Component)]
pub struct TilemapGridSize {
    pub x: f32,
    pub y: f32,
}

impl TilemapGridSize {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

impl From<TilemapGridSize> for Vec2 {
    fn from(grid_size: TilemapGridSize) -> Self {
        Vec2::new(grid_size.x, grid_size.y)
    }
}

impl From<&TilemapGridSize> for Vec2 {
    fn from(grid_size: &TilemapGridSize) -> Self {
        Vec2::new(grid_size.x, grid_size.y)
    }
}

impl From<Vec2> for TilemapGridSize {
    fn from(v: Vec2) -> Self {
        TilemapGridSize { x: v.x, y: v.y }
    }
}

impl From<&Vec2> for TilemapGridSize {
    fn from(v: &Vec2) -> Self {
        TilemapGridSize { x: v.x, y: v.y }
    }
}

/// Spacing between tiles in pixels inside of the texture atlas.
/// Defaults to 0.0
#[derive(Component, Reflect, Default, Clone, Copy, Debug)]
#[reflect(Component)]
pub struct TilemapSpacing {
    pub x: f32,
    pub y: f32,
}

impl From<TilemapSpacing> for Vec2 {
    fn from(spacing: TilemapSpacing) -> Self {
        Vec2::new(spacing.x, spacing.y)
    }
}

impl TilemapSpacing {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub const fn zero() -> Self {
        Self { x: 0.0, y: 0.0 }
    }
}

/// Size of the atlas texture in pixels.
#[derive(Component, Reflect, Default, Clone, Copy, Debug)]
#[reflect(Component)]
pub struct TilemapTextureSize {
    pub x: f32,
    pub y: f32,
}

impl TilemapTextureSize {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

impl From<TilemapTextureSize> for Vec2 {
    fn from(texture_size: TilemapTextureSize) -> Self {
        Vec2::new(texture_size.x, texture_size.y)
    }
}

impl From<Vec2> for TilemapTextureSize {
    fn from(size: Vec2) -> Self {
        TilemapTextureSize {
            x: size.x,
            y: size.y,
        }
    }
}

impl From<TilemapTileSize> for TilemapTextureSize {
    fn from(tile_size: TilemapTileSize) -> Self {
        let TilemapTileSize { x, y } = tile_size;
        TilemapTextureSize { x, y }
    }
}

#[derive(Component, Reflect, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
#[reflect(Component)]
pub enum TilemapType {
    /// A tilemap with rectangular tiles.
    #[default]
    Square,
}

#[derive(Component, Debug, Default, Clone, PartialEq, Eq)]
pub struct TilemapChunks {
    /// All the chunks.
    /// To be loaded into the GPU as texture array layers.
    pub(crate) chunks: Vec<TilemapChunk>,
    /// Total 2D number of chunks.
    pub(crate) n_chunks: UVec2,
    /// The dimensions of each chunk (max 2048x2048)
    pub(crate) chunk_size: UVec2,
    /// 2D number of 64x64 sub-chunks per chunk.
    /// (This determines the texture size)
    pub(crate) n_subchunks: UVec2,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct TilemapChunk {
    /// This will be copied into the GPU as a "texture", where
    /// each texel represents one tilemap tile.
    ///
    /// TextureFormat: Rgba32Uint. Size: as per `n_subchunks`.
    ///
    /// The per-tile data is packed into the texel's RGBA values as follows:
    ///  - R: TileTextureIndex
    ///  - G: TileColor (as 8bpc RGBA)
    ///  - B: flags bitmask to indicate flips and such
    ///  - A: (currently unused)
    pub(crate) data: Vec<u8>,
    /// Bitmap where each bit corresponds to one 64x64 sub-chunk,
    /// indicating whether any tile within that region has been
    /// changed and thus the data needs to be copied to GPU memory.
    pub(crate) dirty_bitmap: Box<[u32; 32]>,
}

impl TilemapChunks {
    /// Length of a contiguous slice representing one sub-chunk
    /// Sub-chunks are always 64x64 * 8 bytes (TextureFormat::Rg32Uint)
    pub(crate) const SUBCHUNK_DATA_LEN: usize = 64 * 64 * 8;

    pub(crate) fn init(&mut self, map_size: UVec2, chunk_size: UVec2) {
        assert!(chunk_size.x <= 2048 && chunk_size.y <= 2048);
        self.chunk_size = chunk_size.min(map_size);
        self.n_chunks.x = map_size.x.div_ceil(self.chunk_size.x);
        self.n_chunks.y = map_size.y.div_ceil(self.chunk_size.y);
        self.n_subchunks.x = self.chunk_size.x.div_ceil(64);
        self.n_subchunks.y = self.chunk_size.y.div_ceil(64);
        let chunk_data_size =
            Self::SUBCHUNK_DATA_LEN * self.n_subchunks.y as usize * self.n_subchunks.x as usize;
        let n_chunks = self.n_chunks.y as usize * self.n_chunks.x as usize;
        self.chunks = Vec::from_iter((0..n_chunks).map(|_| TilemapChunk {
            data: vec![0; chunk_data_size],
            dirty_bitmap: Box::new([u32::MAX; 32]),
        }));
    }
    pub(crate) fn set_tiledata_at(
        &mut self,
        pos: &TilePos,
        index: &TileTextureIndex,
        color: &TileColor,
        flip: &TileFlip,
        visible: &TileVisible,
    ) {
        let chunk_x = pos.x / self.chunk_size.x;
        let chunk_y = pos.y / self.chunk_size.y;
        let tile_x = pos.x % self.chunk_size.x;
        let tile_y = pos.y % self.chunk_size.y;
        let subchunk_x = tile_x / 64;
        let subchunk_y = tile_y / 64;
        let tile_sub_x = tile_x % 64;
        let tile_sub_y = tile_y % 64;
        let i_chunk = (chunk_y * self.n_chunks.x + chunk_x) as usize;
        let chunk = &mut self.chunks[i_chunk];
        let subchunk_offset = (subchunk_y * self.n_subchunks.x + subchunk_x) as usize * 64 * 64 * 8;
        let tile_offset = subchunk_offset + ((tile_sub_y * 64 + tile_sub_x) * 8) as usize;
        let tile_bytes = &mut chunk.data[tile_offset..(tile_offset + 8)];
        // GPUs are little endian
        let index_bytes = u16::try_from(index.0).unwrap().to_le_bytes();
        let color_bytes = color.0.to_linear().as_u32().to_le_bytes();
        tile_bytes[0..2].copy_from_slice(&index_bytes);
        tile_bytes[4..8].copy_from_slice(&color_bytes);
        tile_bytes[3] = ((visible.0 as u8) << 0)
            | ((flip.x as u8) << 1)
            | ((flip.y as u8) << 2)
            | ((flip.d as u8) << 3);
        chunk.dirty_bitmap[subchunk_y as usize] |= 1 << subchunk_x;
    }

    /// Transfers dirty data efficiently from `source` to `self`
    ///
    /// Sub-chunk data corresponding to bits set in the `source`'s
    /// dirty bitmasks will be copied over, overwriting the respective
    /// data in `self`.
    ///
    /// The dirty bitmasks in `self` will be set equal to those
    /// in `source` (thus indicating which sub-chunks were copied).
    ///
    /// `self` must have equal dimensions and chunk layout as `source`.
    pub(crate) fn copy_dirty(&mut self, source: &Self) {
        debug_assert_eq!(self.chunk_size, source.chunk_size);
        debug_assert_eq!(self.n_chunks, source.n_chunks);
        debug_assert_eq!(self.n_subchunks, source.n_subchunks);

        for (dst_chunk, src_chunk) in self.chunks.iter_mut().zip(source.chunks.iter()) {
            *dst_chunk.dirty_bitmap = *src_chunk.dirty_bitmap;
            let mut data_start = 0;
            for (sc_y, row_bitmap) in src_chunk
                .dirty_bitmap
                .iter()
                .copied()
                .take(self.n_subchunks.y as usize)
                .enumerate()
            {
                for sc_x in (0..32).take(self.n_subchunks.x as usize) {
                    let data_end = data_start + Self::SUBCHUNK_DATA_LEN;
                    if row_bitmap & (1 << sc_x) != 0 {
                        let dst_data = &mut dst_chunk.data[data_start..data_end];
                        let src_data = &src_chunk.data[data_start..data_end];
                        dst_data.copy_from_slice(src_data);
                    }
                    data_start = data_end;
                }
            }
        }
    }
    pub(crate) fn texture_size(&self) -> Extent3d {
        Extent3d {
            width: self.n_subchunks.x * 64,
            height: self.n_subchunks.y * 64,
            depth_or_array_layers: self.n_chunks.y * self.n_chunks.x,
        }
    }
    pub(crate) fn clear_all_dirty_bitmaps(&mut self) {
        for chunk in self.chunks.iter_mut() {
            chunk.dirty_bitmap = default();
        }
    }
}

pub(crate) fn clear_all_dirty_bitmaps(mut q_tm: Query<&mut TilemapChunks>) {
    q_tm.iter_mut().for_each(|mut chunks| {
        chunks.clear_all_dirty_bitmaps();
    });
}
