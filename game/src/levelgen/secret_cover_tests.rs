use super::*;
use std::borrow::Cow;

/// A secret room fixture: the level before carving, its carved geometry, and decorations
/// inside the first room and in the cave.
struct Fixture {
    original: CompiledLevel,
    geometry: TerrainGeometry<'static>,
    decorations: Vec<DecorationPlacement>,
}

/// Two copies of an authored room, one `apart` tiles below the other, with their entrances
/// in a cave on the right.
fn fixture(images: &mut Assets<Image>, apart: i32) -> Fixture {
    let rule = secret_rooms::load_rule(&Path::new(secret_rooms::SECRET_ROOMS_DIR).join("1_ruleflip-x.png")).unwrap();
    let (min, max) = rule.room.iter().fold((IVec2::MAX, IVec2::MIN), |(min, max), &(offset, _)| (min.min(offset), max.max(offset)));
    let first = IVec2::splat(12) - min;
    let anchors = [first, first + IVec2::new(0, max.y - min.y + apart)];
    let size = (anchors[1] + max + 13).as_uvec2();
    let reach = rule.require_terrain.iter().chain(rule.room.iter().map(|(offset, _)| offset)).map(|offset| offset.x).max().unwrap();
    let cave = first.x + reach + 1;
    let entrances: Vec<_> = anchors.iter().flat_map(|&anchor| rule.forbid_terrain.iter().map(move |&offset| anchor + offset)).collect();
    let rock = |tile: IVec2| tile.x < cave && !entrances.contains(&tile);
    let original = CompiledLevel { width: size.x, height: size.y,
        terrain: (0..size.x * size.y).map(|index| rock(UVec2::new(index % size.x, index / size.x).as_ivec2())).collect(),
        bounds: Rect::new(0.0, 0.0, size.x as f32 * 6.0, size.y as f32 * 6.0), ..default() };
    let rooms: Vec<_> = anchors.map(|anchor| SecretRoomPlacement { name: format!("room at {anchor}"),
        room: rule.room.iter().map(|&(offset, kind)| (anchor + offset, kind)).collect() }).into();
    let carved = carve(&original, rooms.iter().flat_map(SecretRoomPlacement::tiles));
    let mosaics = run_mosaic_generative_layers(&carved, &catalog::load_test_mosaics(images, "levels/tiles/ice_biome/main_platforms"), images);
    let art = images.add(solid(UVec2::new(40, 9)));
    let decoration = |tile: IVec2| DecorationPlacement { handle: art.clone(), tile_x: tile.x as u32, tile_y: tile.y as u32,
        flip_x: false, flip_y: false, kind: "test".into(), active: true, offset_px: IVec2::ZERO, rule_key: 0 };
    let walls: Vec<_> = rooms[0].tiles_of(RoomTile::BreakableWall).collect();
    // Grown beside the first room's wall, wide enough to reach past it into the cave.
    let beside_wall = rooms[0].room.iter().filter(|&&(tile, kind)| kind != RoomTile::BreakableWall && walls.contains(&(tile + IVec2::X)))
        .map(|&(tile, _)| tile).max_by_key(|tile| tile.y).unwrap();
    let decorations = vec![decoration(beside_wall), decoration(IVec2::new(cave + 1, anchors[1].y + 1)),
        decoration(IVec2::new(cave + 3, anchors[0].y - 8))];
    Fixture { original, geometry: TerrainGeometry { level: Cow::Owned(carved), rooms, mosaics }, decorations }
}

fn carve(level: &CompiledLevel, tiles: impl Iterator<Item = IVec2>) -> CompiledLevel {
    let mut carved = level.clone();
    for tile in tiles {
        carved.terrain[(tile.y as u32 * level.width + tile.x as u32) as usize] = false;
    }
    carved
}

fn solid(size: UVec2) -> Image {
    let mut image = surface::image(size);
    image.data.as_mut().unwrap().as_chunks_mut::<4>().0.fill([200, 40, 160, 255]);
    image
}

impl Fixture {
    /// The level with only the `open` rooms carved, and what has grown there.
    fn truth(&self, open: &[usize], decorations: &[DecorationPlacement], rules: &ldtk_rules::LdtkRuleData, images: &Assets<Image>) -> Image {
        let rooms = &self.geometry.rooms;
        let level = carve(&self.original, open.iter().flat_map(|&room| rooms[room].tiles()));
        let closed_room = |tile: IVec2| rooms.iter().enumerate().any(|(index, room)| !open.contains(&index) && room.tiles().any(|t| t == tile));
        let grown: Vec<_> = decorations.iter().filter(|d| !closed_room(IVec2::new(d.tile_x as i32, d.tile_y as i32))).cloned().collect();
        let surface = prepare_main_surface(&level, &self.geometry.mosaics, rules, images, GROUND_TILE_ART_SIZE_PX, None).unwrap();
        let everything = URect::from_corners(UVec2::ZERO, UVec2::new(level.width, level.height));
        surface::bake_ground(&surface, &level, everything, &grown, images).color
    }
}

/// What the player sees: the ground under each room's cover and wall, or under its contents
/// once it is open.
fn seen(ground: &Image, covers: &[RoomCover], open: &[usize]) -> Image {
    let mut seen = ground.clone();
    for (index, cover) in covers.iter().enumerate() {
        let layers = if open.contains(&index) {
            vec![cover.contents.as_ref().map(|image| (image, cover.region))]
        } else {
            vec![cover.wall.as_ref().map(|(image, tiles)| (image, *tiles)), Some((&cover.cover, cover.region))]
        };
        for (image, tiles) in layers.into_iter().flatten() {
            paste(&mut seen, image, tiles.min * GROUND_TILE_ART_SIZE_PX);
        }
    }
    seen
}

fn paste(target: &mut Image, image: &Image, at: UVec2) {
    let (width, source_width) = (target.width(), image.width());
    let pixels = target.data.as_mut().unwrap().as_chunks_mut::<4>().0;
    for (index, &pixel) in image.data.as_ref().unwrap().as_chunks::<4>().0.iter().enumerate().filter(|(_, pixel)| pixel[3] != 0) {
        let position = at + UVec2::new(index as u32 % source_width, index as u32 / source_width);
        pixels[(position.y * width + position.x) as usize] = pixel;
    }
}

/// The first pixel where `open` rooms show differently from the level they stand for.
fn first_difference(fixture: &Fixture, open: &[usize], rules: &ldtk_rules::LdtkRuleData, images: &Assets<Image>) -> Option<UVec2> {
    let mut decorations = fixture.decorations.clone();
    let covers = bake(&fixture.original, &fixture.geometry, rules, None, &mut decorations, images);
    let rooms: Vec<_> = (0..covers.len()).collect();
    let ground = fixture.truth(&rooms, &decorations, rules, images);
    let expected = fixture.truth(open, &fixture.decorations, rules, images);
    let seen = seen(&ground, &covers, open);
    let width = seen.width();
    let mut pixels = seen.data.as_ref().unwrap().as_chunks::<4>().0.iter().zip(expected.data.as_ref().unwrap().as_chunks::<4>().0);
    pixels.position(|(seen, expected)| seen != expected).map(|index| UVec2::new(index as u32 % width, index as u32 / width))
}

#[test]
fn covers_show_the_level_whichever_rooms_are_open() {
    let mut images = Assets::default();
    let rules = ldtk_rules::load_test_artwork(&mut images);
    let fixture = fixture(&mut images, 2 * secret_rooms::cover_border_tiles() as i32 + 1);
    for open in [&[][..], &[0], &[1], &[0, 1]] {
        assert_eq!(first_difference(&fixture, open, &rules, &images), None, "rooms {open:?} open");
    }
}

#[test]
fn rooms_closer_than_two_borders_would_show_a_cover() {
    let mut images = Assets::default();
    let rules = ldtk_rules::load_test_artwork(&mut images);
    let fixture = fixture(&mut images, secret_rooms::cover_border_tiles() as i32 + 5);
    assert!(first_difference(&fixture, &[0], &rules, &images).is_some());
}
