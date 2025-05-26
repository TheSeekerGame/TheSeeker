use bevy::prelude::*;

/// A module which contains tilemap components.
pub mod map;
#[cfg(feature = "render")]
pub(crate) mod render;
/// A module which contains tile components.
pub mod tiles;

pub use crate::map::TilemapBundle;
use crate::map::TilesetTexture;

pub struct TilemapPlugin;

impl Plugin for TilemapPlugin {
    fn build(&self, app: &mut App) {
        use crate::map::*;
        use crate::tiles::*;

        app.add_systems(Update, set_texture_to_copy_src);
        app.add_systems(PreUpdate, clear_all_dirty_bitmaps);

        app.register_type::<TilemapId>()
            .register_type::<TilemapSize>()
            .register_type::<TilesetTexture>()
            .register_type::<TilemapTileSize>()
            .register_type::<TilemapGridSize>()
            .register_type::<TilemapSpacing>()
            .register_type::<TilemapTextureSize>()
            .register_type::<TilemapType>()
            .register_type::<TilePos>()
            .register_type::<TileTextureIndex>()
            .register_type::<TileColor>()
            .register_type::<TileVisible>()
            .register_type::<TileFlip>()
            .register_type::<TileStorage>()
            .register_type::<TilePosOld>();

        app.add_plugins((crate::map::plugin, crate::tiles::plugin));
        #[cfg(feature = "render")]
        app.add_plugins((
            //crate::chunk::plugin,
            crate::render::TileMapRendererPlugin,
        ));
    }
}

pub fn set_texture_to_copy_src(
    mut images: ResMut<Assets<Image>>,
    texture_query: Query<&TilesetTexture>,
) {
    // quick and dirty, run this for all textures anytime a texture component is created.
    for texture in texture_query.iter() {
        texture.set_images_to_copy_src(&mut images)
    }
}
