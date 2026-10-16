use super::*;

fn fixture() -> CompiledLevel {
    let width = 193;
    let height = 99;
    let terrain = (0..width * height).map(|index| {
        let x = index % width;
        let y = index / width;
        x < 3 || y < 4 || (x > 90 && x < 109 && y > 20) || (y > 49 && y < 80 && x < 170)
    }).collect();
    CompiledLevel { width, height, terrain, spike_tiles: vec![false; (width * height) as usize],
        bounds: Rect::new(0.0, 0.0, width as f32 * 6.0, height as f32 * 6.0), ..default() }
}

fn artwork(size: UVec2) -> Image {
    let mut image = surface::image(size);
    for (index, pixel) in image.data.as_mut().unwrap().as_chunks_mut::<4>().0.iter_mut().enumerate() {
        pixel.copy_from_slice(&[(index % 233) as u8, 160, 244, [0, 48, 255][index % 3]]);
    }
    image
}

#[test]
fn terrain_shading_is_identical_across_internal_borders_and_corners() {
    let level = fixture();
    let original = artwork(UVec2::new(level.width, level.height) * 6);
    let mut reference = original.clone();
    ground_shading::shade(reference.data.as_mut().unwrap(), level.width as usize * 24, UVec2::ZERO, &level, 6);
    for region in surface::chunk_regions(UVec2::new(level.width, level.height)) {
        let padded = surface::expand(region, ground_shading::SUPPORT_PX.div_ceil(6), UVec2::new(level.width, level.height));
        let mut patch = surface::crop(&original, URect { min: padded.min * 6, max: padded.max * 6 });
        let stride = patch.width() as usize * 4;
        ground_shading::shade(patch.data.as_mut().unwrap(), stride, padded.min * 6, &level, 6);
        let result = surface::crop(&patch, URect { min: (region.min - padded.min) * 6, max: (region.max - padded.min) * 6 });
        let expected = surface::crop(&reference, URect { min: region.min * 6, max: region.max * 6 });
        assert_eq!(result.data, expected.data, "region {region:?}");
    }
}

#[test]
fn gaussian_regions_match_whole_image_with_transparency_and_filter_gutters() {
    let original = artwork(UVec2::new(193, 99));
    for radius in [0.0, 2.0, 3.0, 5.0] { compare_blur_regions(&original, radius); }
}

fn compare_blur_regions(original: &Image, radius: f32) {
    let mut reference = original.clone();
    finalize_background_group_image(&mut reference, radius);
    for region in surface::chunk_regions(UVec2::new(original.width(), original.height())) {
        let padded = surface::expand(region, crate::graphics::background_blur::support(radius) + 3,
            UVec2::new(original.width(), original.height()));
        let mut patch = surface::crop(original, padded);
        finalize_background_group_image(&mut patch, radius);
        let actual = surface::crop(&patch, URect { min: region.min - padded.min, max: region.max - padded.min });
        assert_eq!(actual.data, surface::crop(&reference, region).data, "radius {radius}, region {region:?}");
    }
}

#[test]
fn placement_index_matches_exhaustive_spacing_across_negative_cell_boundaries() {
    let positions: Vec<_> = (0..50).map(|i| IVec2::new(i * 17 % 233 - 100, i * 31 % 197 - 80)).collect();
    let mut index = spatial::PlacementIndex::default();
    for &position in &positions { index.insert(position); }
    for i in 0..300 {
        let position = IVec2::new(i * 43 % 301 - 130, i * 73 % 301 - 140);
        let radius = i % 70;
        assert_eq!(index.clear_square(position, radius), positions.iter().all(|&p| (p - position).abs().max_element() > radius));
    }
}

#[test]
#[ignore = "Diagnostic fixture selection reads authored maps and templates."]
fn find_adaptation_fixture() {
    let mut chunks = mapgen::ChunkLibrary::default();
    let mut maps = mapgen::MapLibrary::default();
    for seed in 1..100 {
        let request = pipeline::GenerationRequest { seed, map: None, config: LevelPngConfig::default() };
        let map = mapgen::generate_map(&mut chunks, &mut maps, &request).unwrap();
        let count = map.compiled.optional_spike_tiles.iter().filter(|&&v| v).count();
        if count > 0 { println!("optional fixture seed={seed} tiles={count}"); return; }
    }
    println!("Authored chunk library has no optional-spike markers; synthetic fixtures cover adaptation preparation.");
}

#[test]
fn prepared_adaptation_commits_once_and_keeps_secret_cover_collision() {
    use bevy::ecs::system::RunSystemOnce;
    use bevy::sprite_render::MeshMaterial2d;
    let mut world = World::new();
    let level = fixture();
    let mut collision = TileCollisionWorld::default();
    collision.rebuild(level.width, level.height, 6.0, &level.terrain);
    let covered = IVec2::new(1, 1);
    collision.add_dynamic_tiles(&[covered, covered]);
    let index = (covered.y as u32 * level.width + covered.x as u32) as usize;
    let visual = world.spawn(MeshMaterial2d::<ColorMaterial>::default()).id();
    world.insert_resource(level);
    world.insert_resource(collision);
    let mut prepared = adaptation::PreparedAdaptation::default();
    prepared.states[1].tiles.push(adaptation::TileDelta { index, terrain: false, hazard: true });
    prepared.states[1].visuals.push((visual, Handle::default()));
    world.insert_resource(TestAdaptation(prepared));
    world.run_system_once(commit_test_adaptation).unwrap();
    world.run_system_once(commit_test_adaptation).unwrap();
    assert!(!world.resource::<CompiledLevel>().terrain[index]);
    assert!(world.resource::<CompiledLevel>().spike_tiles[index]);
    let mut collision = world.resource_mut::<TileCollisionWorld>();
    assert!(collision.tile_solid_info(1, 1).is_some());
    collision.remove_dynamic_tiles(&[covered]);
    assert!(collision.tile_solid_info(1, 1).is_some());
    collision.remove_dynamic_tiles(&[covered]);
    assert!(collision.tile_solid_info(1, 1).is_none());
}

#[derive(Resource)]
struct TestAdaptation(adaptation::PreparedAdaptation);

fn commit_test_adaptation(mut commands: Commands, mut prepared: ResMut<TestAdaptation>,
    mut level: ResMut<CompiledLevel>, mut collision: ResMut<TileCollisionWorld>,
    mut visuals: Query<&mut bevy::sprite_render::MeshMaterial2d<ColorMaterial>>,
) {
    let expected = usize::from(!prepared.0.is_active());
    assert_eq!(prepared.0.activate(&mut commands, &mut level, &mut collision, &mut visuals), expected);
}

#[test]
fn restart_discards_world_products_and_rejects_stale_publication() {
    use bevy::ecs::system::RunSystemOnce;
    let mut world = World::new();
    let old = pipeline::GenerationBuild::default();
    let old_id = old.id;
    world.insert_resource(old);
    world.insert_resource(fixture());
    world.insert_resource(LevelBiomeMask { width: 1, height: 1, cells: vec![BiomeKind::Ice], tundra_inner: vec![] });
    world.insert_resource(publication::Publication { id: old_id, images: Vec::new(),
        presented: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)) });
    world.run_system_once(|mut commands: Commands| reset_level_runtime(&mut commands)).unwrap();
    assert!(world.get_resource::<CompiledLevel>().is_none());
    assert!(world.get_resource::<LevelBiomeMask>().is_none());
    assert!(!world.resource::<TileCollisionWorld>().is_ready());
    assert_ne!(world.resource::<pipeline::GenerationBuild>().id, old_id);
    assert!(!publication::ready(&world, old_id));
}
