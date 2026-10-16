//! Secret room covers: the ground as it was before carving, over a room and the border its
//! carving reshades, until the room's breakable wall falls.
//!
//! Rooms keep two borders apart (see `carve_secret_rooms`), so each cover shows exactly what
//! the ground would show there, whichever rooms are open. What grows inside a room stays
//! hidden until it opens: art in its entrance would otherwise show past the cover.
use super::*;
use secret_rooms::{RoomTile, SecretRoomPlacement};

/// Between the ground (0) and the player (~0.000015).
const COVER_Z: f32 = 0.000_01;
/// Below the cover, which leaves the wall's tiles open.
const WALL_Z: f32 = 0.000_000_011;
/// Just above the ground the room's decorations grow on.
const CONTENTS_Z: f32 = 0.000_000_001;

/// A room's art, cut from the level baked before carving.
pub(super) struct RoomCover {
    /// The tiles the cover spans: the room and its border.
    region: URect,
    /// The room and its border before carving, with the level's decorations outside rooms.
    cover: Image,
    /// The decorations anchored in the room, shown once it opens.
    contents: Option<Image>,
    /// The breakable wall, and the tiles it spans.
    wall: Option<(Image, URect)>,
}

/// Bakes each room's cover, taking the decorations anchored in rooms out of `decorations`
/// so the ground grows them only once their room opens.
pub(super) fn bake(
    original: &CompiledLevel, geometry: &TerrainGeometry<'_>, rules: &ldtk_rules::LdtkRuleData,
    biomes: Option<&LevelBiomeMask>, decorations: &mut Vec<DecorationPlacement>, images: &Assets<Image>,
) -> Vec<RoomCover> {
    let rooms = &geometry.rooms;
    if rooms.is_empty() {
        return Vec::new();
    }
    let _timer = diagnostics::StageTimer::new("secret_cover_preparation");
    let width = original.width;
    let mut owner = vec![None; (width * original.height) as usize];
    let tiles = rooms.iter().enumerate().flat_map(|(index, room)| room.tiles().map(move |tile| (tile, index)));
    for (tile, room) in tiles {
        owner[(tile.y as u32 * width + tile.x as u32) as usize] = Some(room);
    }
    let contents: Vec<Vec<_>> = (0..rooms.len()).map(|room| decorations
        .extract_if(.., |decoration| owner[(decoration.tile_y * width + decoration.tile_x) as usize] == Some(room))
        .collect()).collect();
    let Some(surface) = prepare_main_surface(original, &geometry.mosaics, rules, images, GROUND_TILE_ART_SIZE_PX, biomes)
    else {
        return Vec::new();
    };
    let size = UVec2::new(width, original.height);
    rooms.iter().zip(contents).map(|(room, contents)| {
        let region = surface::expand(room.bounds(), secret_rooms::cover_border_tiles(), size);
        let art = surface::bake_ground(&surface, original, region, decorations, images).color;
        RoomCover::cut(room, region, art, &contents, images)
    }).collect()
}

impl RoomCover {
    /// Cuts the cover and the wall out of `art`, the level before carving baked over
    /// `region`, and draws the room's own decorations for when it opens.
    fn cut(room: &SecretRoomPlacement, region: URect, mut cover: Image,
        contents: &[DecorationPlacement], images: &Assets<Image>,
    ) -> Self {
        let tile_px = GROUND_TILE_ART_SIZE_PX;
        let size = region.size();
        let local = |tile: IVec2| tile.as_uvec2() - region.min;
        let mut footprint = vec![false; (size.x * size.y) as usize];
        for tile in room.tiles().map(local) {
            footprint[(tile.y * size.x + tile.x) as usize] = true;
        }
        let border = secret_rooms::dilate(&footprint, size, secret_rooms::cover_border_tiles());
        let wall_tiles: HashSet<_> = room.tiles_of(RoomTile::BreakableWall).map(local).collect();
        let wall = secret_rooms::tile_rect(room.tiles_of(RoomTile::BreakableWall)).map(|tiles| {
            let min = tiles.min - region.min;
            let mut image = surface::crop(&cover, URect::from_corners(min * tile_px, (tiles.max - region.min) * tile_px));
            keep_tiles(&mut image, min, |tile| wall_tiles.contains(&tile));
            (image, tiles)
        });
        keep_tiles(&mut cover, UVec2::ZERO, |tile| border[(tile.y * size.x + tile.x) as usize] && !wall_tiles.contains(&tile));
        let contents = (!contents.is_empty()).then(|| {
            let mut image = surface::image(size * tile_px);
            surface::decorate(&mut image, region.min * tile_px, contents, images, tile_px);
            image
        });
        Self { region, cover, contents, wall }
    }
}

/// Clears the pixels of `image`, whose top-left tile is `origin`, in the tiles `keep` rejects.
fn keep_tiles(image: &mut Image, origin: UVec2, keep: impl Fn(UVec2) -> bool) {
    let width = image.width();
    let pixels = image.data.as_mut().expect("cover pixels").as_chunks_mut::<4>().0;
    for (index, pixel) in pixels.iter_mut().enumerate() {
        let tile = origin + UVec2::new(index as u32 % width, index as u32 / width) / GROUND_TILE_ART_SIZE_PX;
        if !keep(tile) {
            *pixel = [0; 4];
        }
    }
}

/// Spawns each room's cover, contents and breakable wall, with the collision tiles that
/// keep the room closed until the wall breaks.
pub(super) fn spawn(commands: &mut Commands, parent: Entity, rooms: &[SecretRoomPlacement], covers: Vec<RoomCover>,
    images: &mut Assets<Image>, level_height: u32,
) {
    let tile_px = GROUND_TILE_ART_SIZE_PX as f32;
    let place = |tiles: URect, z: f32| {
        let center = tiles.as_rect().center() * tile_px;
        Transform::from_xyz(center.x, level_height as f32 * tile_px - center.y, z)
    };
    for (room, RoomCover { region, cover, contents, wall }) in rooms.iter().zip(covers) {
        info!("Spawned cover for secret room '{}' over tiles {region:?}", room.name);
        let cover = commands.spawn((Sprite::from_image(images.add(cover)), place(region, COVER_Z),
            pixel_perfect_layers(), StateDespawnMarker, breakable_wall::SecretRoomCover,
            CoverCollisionTiles { tiles: room.tiles().collect() },
            Name::new(format!("SecretRoomCover_{}", room.name)), ChildOf(parent))).id();
        let contents = contents.map(|image| commands.spawn((Sprite::from_image(images.add(image)),
            place(region, CONTENTS_Z), Visibility::Hidden, pixel_perfect_layers(), StateDespawnMarker,
            Name::new(format!("SecretRoomContents_{}", room.name)), ChildOf(parent))).id());
        let Some((image, tiles)) = wall else { continue; };
        let size = tiles.size().as_vec2() * tile_px;
        commands.spawn((Sprite::from_image(images.add(image)), place(tiles, WALL_Z),
            Collider::cuboid(size.x * 0.5, size.y * 0.5), StaticCollider,
            // Attacks that hit enemies hit the wall; the cover's collision tiles block movement.
            CollisionGroups::new(ENEMY, Group::all()), pixel_perfect_layers(), StateDespawnMarker,
            breakable_wall::BreakableWall { hits_remaining: breakable_wall::BREAKABLE_WALL_HITS, cover, contents },
            breakable_wall::SecretRoomCover, Name::new(format!("BreakableWallCover_{}", room.name)), ChildOf(parent)));
    }
}

#[cfg(test)]
#[path = "secret_cover_tests.rs"]
mod tests;
