use super::*;

/// A level `tiles` square of 6-pixel tiles, so 64 lightmap pixels of 3 world units for 32.
fn level(tiles: u32) -> CompiledLevel {
    let size = tiles as f32 * 6.0;
    CompiledLevel { width: tiles, height: tiles, terrain: vec![false; (tiles * tiles) as usize],
        bounds: Rect::new(0.0, 0.0, size, size), ..default() }
}

/// A layer over the whole level at `depth`, covering every pixel but those `open` accepts.
fn layer(level: &CompiledLevel, depth: f32, open: impl Fn(UVec2) -> bool) -> DarknessBakeMaskLayer {
    let size = UVec2::new(level.width, level.height) * 6;
    let alpha = (0..size.x * size.y).map(|i| u8::from(!open(UVec2::new(i % size.x, i / size.x)))).collect();
    let mut layer = DarknessBakeMaskLayer::default();
    layer.set_chunks(size, vec![(UVec2::ZERO, size.x, size.y, alpha)], level.bounds.center(), level.bounds.size(), depth);
    layer
}

fn light_at(light: &[u8], x: u32, y: u32) -> u8 {
    light[(y * 64 + x) as usize]
}

#[test]
fn light_fades_from_where_the_sky_shows() {
    let level = level(32);
    // A hole in a layer without parallax, over lightmap pixels 30..34 on both axes.
    let far = layer(&level, 1.0, |pixel| (90..102).contains(&pixel.x) && (90..102).contains(&pixel.y));
    let light = bake(&level, None, &DarknessBakeMaskAtlas { far, ..default() }).unwrap();
    assert_eq!(light_at(&light, 31, 31), 255);
    let row: Vec<_> = (33..=41).map(|x| light_at(&light, x, 31)).collect();
    assert!(row.windows(2).all(|pair| pair[0] > pair[1]), "light fades with distance: {row:?}");
    assert_eq!(light_at(&light, 41, 31), 0, "nothing reaches past the radius");
}

#[test]
fn parallax_shows_a_deep_layer_depth_times_nearer_its_center() {
    let level = level(32);
    // A hole 24 world units right of the center of a layer at depth 2 shows 48 units right of it.
    let far = layer(&level, 2.0, |pixel| (117..123).contains(&pixel.x) && (93..99).contains(&pixel.y));
    let light = bake(&level, None, &DarknessBakeMaskAtlas { far, ..default() }).unwrap();
    assert_eq!(light_at(&light, 48, 32), 255);
    assert_eq!(light_at(&light, 38, 32), 0);
}

#[test]
fn ground_and_unoccupied_chunks_hide_the_sky() {
    let level = level(32);
    let masks = DarknessBakeMaskAtlas { ground: layer(&level, 1.0, |pixel| pixel.x >= 96), ..default() };
    let light = bake(&level, None, &masks).unwrap();
    assert_eq!((light_at(&light, 63, 10), light_at(&light, 10, 10)), (255, 0));
    let unoccupied = LevelChunkOccupancyMask { chunks_w: 1, chunks_h: 1, occupied: vec![false] };
    assert!(bake(&level, Some(&unoccupied), &masks).unwrap().iter().all(|&light| light == 0));
}

#[test]
fn a_level_without_coverage_fails_to_bake() {
    assert!(bake(&level(32), None, &DarknessBakeMaskAtlas::default()).is_none());
}

