use super::*;

fn small_room(name: String) -> SecretRoomRule {
    let interior = [(1, 0), (2, 0), (1, 1), (2, 1)].map(|(x, y)| (IVec2::new(x, y), RoomTile::Interior));
    SecretRoomRule { name, room: [(IVec2::ZERO, RoomTile::BreakableWall)].into_iter().chain(interior).collect(),
        require_terrain: vec![], forbid_terrain: vec![IVec2::NEG_X], ruleflip_x: true }
}

fn library() -> SecretRoomLibrary {
    SecretRoomLibrary { rules: (0..5).map(|index| small_room(format!("small_{index}"))).collect(), loaded: true }
}

/// Solid terrain around the open cave `cave` picks out.
fn cave_level(size: UVec2, cave: impl Fn(u32) -> bool) -> CompiledLevel {
    let len = (size.x * size.y) as usize;
    CompiledLevel { width: size.x, height: size.y, terrain: (0..size.x * size.y).map(|i| !cave(i % size.x)).collect(),
        spike_tiles: vec![false; len], optional_spike_tiles: vec![false; len],
        bounds: Rect::new(0.0, 0.0, size.x as f32 * 6.0, size.y as f32 * 6.0), ..default() }
}

/// The distance, along the longer axis, between the nearest of two sets of tiles.
fn distance(a: impl Iterator<Item = IVec2> + Clone, b: impl Iterator<Item = IVec2>) -> i32 {
    b.flat_map(|b| a.clone().map(move |a| (a - b).abs().max_element())).min().unwrap_or(i32::MAX)
}

fn two_borders() -> i32 {
    2 * cover_border_tiles() as i32
}

#[test]
fn cover_border_reaches_past_edge_shading() {
    assert_eq!(cover_border_tiles(), 10);
}

#[test]
fn rooms_keep_two_cover_borders_apart_in_either_orientation() {
    for flip in [false, true] { check_room_spacing(flip); }
}

fn check_room_spacing(flip: bool) {
    let level = cave_level(UVec2::new(24, 120), |x| if flip { x >= 20 } else { x < 4 });
    let rooms = carve_secret_rooms(&level, &library(), &[]).placements;
    assert!(rooms.len() >= 2, "rooms fit along the cave");
    let wall_x = if flip { 19 } else { 4 };
    assert!(rooms.iter().all(|room| room.tiles_of(RoomTile::BreakableWall).all(|wall| wall.x == wall_x)));
    let pairs = rooms.iter().enumerate().flat_map(|(index, a)| rooms[index + 1..].iter().map(move |b| (a, b)));
    for (a, b) in pairs {
        assert!(distance(a.tiles(), b.tiles()) > two_borders(), "{} and {} are too close", a.name, b.name);
    }
}

#[test]
fn rooms_keep_two_cover_borders_from_optional_spikes_and_the_crater() {
    let mut level = cave_level(UVec2::new(24, 120), |x| x < 4);
    let spikes: Vec<_> = (4..24).map(|x| UVec2::new(x, 70)).collect();
    let crater = [UVec2::new(8, 30)];
    let changing: Vec<_> = spikes.iter().chain(&crater).map(|tile| tile.as_ivec2()).collect();
    let near = |rooms: &[SecretRoomPlacement]| rooms.iter().any(|room| distance(room.tiles(), changing.iter().copied()) <= two_borders());
    assert!(near(&carve_secret_rooms(&level, &library(), &[]).placements), "fixture places a room near a change");
    for tile in &spikes {
        level.optional_spike_tiles[(tile.y * 24 + tile.x) as usize] = true;
    }
    let rooms = carve_secret_rooms(&level, &library(), &crater).placements;
    assert!(!rooms.is_empty() && !near(&rooms));
}

#[test]
fn dilation_matches_square_distance_at_level_edges() {
    let size = UVec2::new(48, 37);
    let marked = [IVec2::new(0, 0), IVec2::new(7, 6), IVec2::new(8, 6), IVec2::new(47, 36)];
    let tiles = || (0..size.x * size.y).map(|i| IVec2::new((i % size.x) as i32, (i / size.x) as i32));
    let mask: Vec<_> = tiles().map(|tile| marked.contains(&tile)).collect();
    for radius in [0, 1, 10] {
        let expected: Vec<_> = tiles().map(|tile| distance(marked.into_iter(), std::iter::once(tile)) <= radius).collect();
        assert_eq!(dilate(&mask, size, radius as u32), expected);
    }
}

fn authored_room_fixture(rule: &SecretRoomRule) -> CompiledLevel {
    let mut level = cave_level(UVec2::new(193, 99), |_| false);
    for tile in rule.forbid_terrain.iter().map(|&offset| IVec2::new(96, 49) + offset) {
        level.terrain[(tile.y * 193 + tile.x) as usize] = false;
    }
    level
}

#[test]
fn authored_room_walls_receive_mosaics_after_carving() {
    let mut images = Assets::default();
    let rules = catalog::load_test_mosaics(&mut images, "levels/tiles/ice_biome/main_platforms");
    for number in 1..=7 { check_authored_room_mosaics(number, &rules, &mut images); }
}

fn check_authored_room_mosaics(number: u32, rules: &[catalog::MosaicRules; 3], images: &mut Assets<Image>) {
    let rule = load_rule(&Path::new(SECRET_ROOMS_DIR).join(format!("{number}_ruleflip-x.png"))).unwrap();
    let name = rule.name.clone();
    let level = authored_room_fixture(&rule);
    let before = mosaic::run_mosaic_generative_layers(&level, rules, images);
    let library = SecretRoomLibrary { rules: vec![rule], loaded: true };
    let geometry = terrain::prepare_terrain_geometry(&level, &library, &[], rules, images);
    assert_eq!(geometry.rooms.len(), 1, "{name}");
    let expected = mosaic::run_mosaic_generative_layers(&geometry.level, rules, images);
    assert_eq!(geometry.mosaics, expected, "mosaics must use carved geometry");
    let changed = geometry.level.terrain.iter().enumerate().filter(|&(i, &solid)| solid
        && (0..3).any(|layer| geometry.mosaics[layer][i] && !before[layer][i])).count();
    assert!(changed > 0, "{name} needs new interior mosaic cells");
    let region = surface::expand(geometry.rooms[0].bounds(), 10, UVec2::new(level.width, level.height));
    inspect_room_tiles(&geometry, &before, images, region, number);
}

fn inspect_room_tiles(geometry: &terrain::TerrainGeometry<'_>, before: &[Vec<bool>; 3],
    images: &mut Assets<Image>, region: URect, number: u32,
) {
    let rules = ldtk_rules::load_test_artwork(images);
    let surface = terrain::prepare_main_surface(&geometry.level, &geometry.mosaics, &rules, images, 6, None).unwrap();
    let stale = terrain::prepare_main_surface(&geometry.level, before, &rules, images, 6, None).unwrap();
    let current = surface::bake_ground(&surface, &geometry.level, region, &[], images).color;
    let previous = surface::bake_ground(&stale, &geometry.level, region, &[], images).color;
    assert_ne!(current.data, previous.data, "room {number}: corrected mosaic categories must change actual wall artwork");
    if let Ok(directory) = std::env::var("SEEKER_ROOM_FIXTURE_OUTPUT") {
        fs::create_dir_all(&directory).unwrap();
        current.try_into_dynamic().unwrap().save(Path::new(&directory).join(format!("room-{number}.png"))).unwrap();
        previous.try_into_dynamic().unwrap().save(Path::new(&directory).join(format!("room-{number}-before.png"))).unwrap();
    }
}
