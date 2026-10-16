//! Terrain generation and publication.
use super::*;

pub(super) fn mark_spike_int_grid_cells(int_grid: &mut [i32], spike_tiles: &[bool]) {
    for (cell, &spike) in int_grid.iter_mut().zip(spike_tiles) {
        if spike { *cell = ldtk_rules::MAIN_SPIKES_INT_GRID_VALUE; }
    }
}

struct TerrainRules<'a> {
    main: ldtk_rules::RuleProgram<'a>,
    grass: Option<ldtk_rules::RuleProgram<'a>>,
    fallback: ldtk_rules::WallFallbackTiles,
}

impl<'a> TerrainRules<'a> {
    fn new(grid: &'a [i32], size: UVec2, layer: &'a ldtk_rules::LdtkAutoLayer, tundra: bool) -> Self {
        let seed = 0x1234_5678_9ABC_DEF0u64;
        Self { main: ldtk_rules::RuleProgram::terrain(grid, size, layer, seed.wrapping_add(if tundra { 0x2222 } else { 0 })),
            grass: tundra.then(|| ldtk_rules::RuleProgram::grass(grid, size, layer, seed.wrapping_add(0x3333))),
            fallback: ldtk_rules::discover_fallback_tiles(layer) }
    }

    fn at(&self, index: usize, value: i32) -> Option<ldtk_rules::TilePlacement> {
        self.main.at(index).or_else(|| self.grass.as_ref().and_then(|grass| grass.at(index)))
            .or_else(|| (value != 0).then(|| self.fallback.get(value)).flatten().map(|tile_id| {
                ldtk_rules::TilePlacement { tile_id, flip_x: false, flip_y: false }
            }))
    }
}

fn terrain_categories(compiled: &CompiledLevel, mosaics: &[Vec<bool>; 3]) -> Vec<i32> {
    let mut grid = if ldtk_rules::mosaic_masks_empty(&mosaics[0], &mosaics[1], &mosaics[2]) {
        ldtk_rules::generate_wall_categories_from_distance(&compiled.terrain, compiled.width, compiled.height)
    } else {
        ldtk_rules::generate_wall_categories(&compiled.terrain, &mosaics[0], &mosaics[1], &mosaics[2])
    };
    mark_spike_int_grid_cells(&mut grid, &compiled.spike_tiles);
    grid
}

pub(super) fn prepare_main_surface<'a>(
    compiled: &CompiledLevel, mosaic_masks: &[Vec<bool>; 3],
    ldtk_data: &'a ldtk_rules::LdtkRuleData, images: &'a Assets<Image>,
    tile_px: u32, biome_mask: Option<&LevelBiomeMask>,
) -> Option<surface::TileSurface<'a>> {
    let _timer = diagnostics::StageTimer::new("terrain_tile_rules");
    let size = UVec2::new(compiled.width, compiled.height);
    let grid = terrain_categories(compiled, mosaic_masks);
    let assets = biome_layers(ldtk_data, images, Some(tile_px), biome_mask, ["Main", "TundraMain"])?;
    let rules: Vec<_> = assets.iter().enumerate().map(|(index, asset)| {
        TerrainRules::new(&grid, size, asset.layer, index == 1)
    }).collect();
    let cells = grid.iter().enumerate().map(|(index, &value)| {
        let source = usize::from(value != ldtk_rules::MAIN_SPIKES_INT_GRID_VALUE
            && cell_uses_tundra(biome_mask, assets.len(), index));
        rules[source].at(index, value).map(|tile| surface::TilePaint { tile, source })
    }).collect();
    let sources = assets.into_iter().map(|asset| (asset.image, asset.tiles_per_row)).collect();
    Some(surface::TileSurface { size, tile_px, sources, cells })
}

pub(super) fn biome_layers<'a>(rules: &'a ldtk_rules::LdtkRuleData, images: &'a Assets<Image>, tile_px: Option<u32>,
    biomes: Option<&LevelBiomeMask>, names: [&str; 2],
) -> Option<Vec<ldtk_rules::TileLayerAssets<'a>>> {
    let mut layers = vec![rules.resolve_layer(names[0], images, tile_px)?];
    if biomes.is_some_and(|mask| mask.cells.iter().any(|cell| cell.is_tundra())) {
        if let Some(layer) = rules.resolve_layer(names[1], images, Some(layers[0].tile_size)) { layers.push(layer); }
    }
    Some(layers)
}

pub(super) fn cell_uses_tundra(biomes: Option<&LevelBiomeMask>, layer_count: usize, index: usize) -> bool {
    layer_count == 2 && biomes.and_then(|mask| mask.cells.get(index)).is_some_and(|cell| cell.is_tundra())
}

pub(super) struct TerrainGeometry<'a> {
    pub level: std::borrow::Cow<'a, CompiledLevel>,
    pub rooms: Vec<secret_rooms::SecretRoomPlacement>,
    pub mosaics: [Vec<bool>; 3],
}

/// Carves the secret rooms, which keep clear of `crater`, the tiles the intro impact changes.
pub(super) fn prepare_terrain_geometry<'a>(compiled: &'a CompiledLevel, library: &secret_rooms::SecretRoomLibrary,
    crater: &[UVec2], rules: &[MosaicRules; 3], images: &Assets<Image>,
) -> TerrainGeometry<'a> {
    let carved = secret_rooms::carve_secret_rooms(compiled, library, crater);
    let mut level = std::borrow::Cow::Borrowed(compiled);
    if !carved.placements.is_empty() {
        let modified = level.to_mut();
        modified.terrain = carved.terrain;
        modified.manual_near_bg_mask.resize(modified.terrain.len(), false);
        let width = modified.width;
        let near_background = carved.placements.iter().flat_map(|room| room.tiles_of(secret_rooms::RoomTile::NearBackground));
        for tile in near_background {
            modified.manual_near_bg_mask[(tile.y as u32 * width + tile.x as u32) as usize] = true;
        }
        modified.ore_spawn_tiles.extend(carved.placements.iter().flat_map(|room| room.tiles_of(secret_rooms::RoomTile::OreMarker)));
    }
    // Mosaic rules need the actual exposed edges, including room interiors.
    // Covers and adaptations reuse these categories without rerunning the rules.
    let mosaics = run_mosaic_generative_layers(&level, rules, images);
    TerrainGeometry { level, rooms: carved.placements, mosaics }
}

pub(super) fn tile_is_optional_spike(compiled: &CompiledLevel, tile_x: u32, tile_y: u32) -> bool {
    if tile_x >= compiled.width || tile_y >= compiled.height {
        return false;
    }
    compiled
        .optional_spike_tiles
        .get((tile_y * compiled.width + tile_x) as usize)
        .copied()
        .unwrap_or(false)
}

#[expect(clippy::too_many_arguments, reason = "The ground joins the compiled level, its generation inputs, and each asset store it publishes into.")]
pub(super) fn ensure_ground(
    commands: &mut Commands,
    parent: Entity,
    compiled: &CompiledLevel,
    cfg: &LevelPngConfig,
    libraries: &GenerationLibraries,
    ldtk_data: &ldtk_rules::LdtkRuleData,
    secret_rooms_lib: &secret_rooms::SecretRoomLibrary,
    images: &mut Assets<Image>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    foliage_materials: &mut Assets<foliage::FoliageMaterial>,
    foliage_settings: foliage::FoliageSettings,
    mut darkness_masks: Option<&mut DarknessBakeMaskAtlas>,
    biome_mask: Option<&LevelBiomeMask>,
    chunk_colors: Option<&LevelChunkColorMask>,
    occupancy: Option<&LevelChunkOccupancyMask>,
    pre_impact: Option<&impact_crater::PreImpactTerrain>,
) {
    let _timer = diagnostics::StageTimer::new("ground_total");
    let original_compiled = compiled;
    let crater = pre_impact.map(|pre_impact| pre_impact.changed_tiles(compiled)).unwrap_or_default();
    let geometry = prepare_terrain_geometry(compiled, secret_rooms_lib, &crater, &libraries.mosaics, images);
    if !geometry.rooms.is_empty() { commands.insert_resource(geometry.level.as_ref().clone()); }
    let compiled = geometry.level.as_ref();
    let mosaic_masks = &geometry.mosaics;

    let LevelPlacements { biomes: biome_mask_final, ice_allowed: ice_allowed_mask, decorations: mut decoration_placements,
        entities: entity_placements } = place_level(compiled, libraries, images, biome_mask, chunk_colors, occupancy, &geometry.rooms);
    commands.insert_resource(biome_mask_final.clone());
    let tile_px = GROUND_TILE_ART_SIZE_PX;
    let size_px = UVec2::new(compiled.width, compiled.height) * tile_px;
    commands.insert_resource(surface::snow_occlusion(&decoration_placements, size_px, tile_px, images));
    let covers = secret_covers::bake(original_compiled, &geometry, ldtk_data, Some(&biome_mask_final),
        &mut decoration_placements, images);

    let chunk_groups = optional_spikes::prepare_local_variants(compiled);
    let active_compiled = chunk_groups.as_ref().map(|_| optional_spikes::activated_compiled_level(compiled));
    let ground_surface = prepare_main_surface(compiled, mosaic_masks, ldtk_data, images,
        tile_px, Some(&biome_mask_final)).expect("validated terrain tilesets");
    let active_surface = active_compiled.as_ref().and_then(|level| {
        prepare_main_surface(level, mosaic_masks, ldtk_data, images, tile_px, Some(&biome_mask_final))
    });
    let decoration_chunks = surface::decoration_chunks(&decoration_placements, images, tile_px);
    let regions = surface::chunk_regions(UVec2::new(compiled.width, compiled.height));
    let mut patches = surface::bake_jobs(&regions, |region| {
        let chunk = (region.min / mapgen::CHUNK_IMAGE_SIZE).as_ivec2();
        let decorations = decoration_chunks.get(&chunk).map(Vec::as_slice).unwrap_or_default();
        let base = surface::bake_ground(&ground_surface, compiled, region, decorations, images);
        let group = chunk_groups.as_ref().and_then(|plan| plan.chunk_groups.get(&chunk)).copied();
        let active = group.and_then(|_| active_surface.as_ref().zip(active_compiled.as_ref()))
            .map(|(surface, level)| surface::bake_ground(surface, level, region, decorations, images));
        (base, group, active)
    });
    patches.sort_by_key(|(base, _, _)| (base.region.min.y, base.region.min.x));
    drop(ground_surface);
    drop(active_surface);

    if let Some(atlas) = darkness_masks.as_mut() {
        let chunks = patches.iter().map(|(base, _, _)| {
            (base.region.min * tile_px, base.color.width(), base.color.height(), surface::alpha(&base.color))
        }).collect();
        atlas.ground.set_chunks(size_px, chunks, compiled.bounds.center(), compiled.bounds.size(), 1.0);
    }
    let mesh = meshes.add(bevy::prelude::Rectangle::default());
    let mut variants = Vec::new();
    let intact = pre_impact.map(|pre_impact| pre_impact.intact_level(compiled));
    let intact_regions = intact.as_ref().map(|(_, regions)| regions.as_slice()).unwrap_or_default();
    for (base, group, active) in patches {
        let formed_crater = intact_regions.contains(&base.region);
        let entity = publish_ground_patch(commands, parent, compiled, cfg.tile_size,
            base, group, active, &mesh, images, materials, &mut variants);
        if let Some(entity) = entity.filter(|_| formed_crater) {
            commands.entity(entity).insert(impact_crater::ImpactFormed);
        }
    }
    if let Some((intact, regions)) = &intact {
        let inputs = impact_crater::LevelSurfaceInputs { compiled, mosaics: mosaic_masks, ldtk: ldtk_data,
            biomes: &biome_mask_final, decorations: &libraries.decorations, ice_allowed: &ice_allowed_mask,
            tundra_allowed: &biome_mask_final.tundra_inner, tile_size: cfg.tile_size };
        impact_crater::publish_intact_patches(commands, parent, (intact, regions), &inputs, &mesh, images, materials);
    }
    if let Some(plan) = chunk_groups {
        let (_, mut runtime) = plan.into_parts();
        runtime.prepare(commands, parent, compiled, variants);
        commands.insert_resource(runtime);
    }

    if !entity_placements.is_empty() {
        let mut entity_spawn_assets = EntitySpawnAssets {
            images,
            meshes,
            foliage_materials,
            foliage_settings,
        };
        let spawned = spawn_entities(
            commands,
            parent,
            compiled,
            &entity_placements,
            &mut entity_spawn_assets,
        );
        if spawned > 0 {
            info!("levelgen: spawned {} entities", spawned);
        }
    }

    secret_covers::spawn(commands, parent, &geometry.rooms, covers, images, compiled.height);
    spawn_spike_hazards(commands, parent, compiled);
}

#[expect(clippy::too_many_arguments, reason = "Publication joins a prepared chunk with its world geometry and owned render assets.")]
pub(super) fn publish_ground_patch(
    commands: &mut Commands, parent: Entity, compiled: &CompiledLevel, tile_size: f32,
    base: surface::GroundPatch, group: Option<usize>, active: Option<surface::GroundPatch>,
    mesh: &Handle<Mesh>, images: &mut Assets<Image>, materials: &mut Assets<ColorMaterial>, variants: &mut Vec<(usize, Entity, Handle<ColorMaterial>)>,
) -> Option<Entity> {
    let region = base.region;
    let size = region.size().as_vec2() * tile_size;
    let center = region.min.as_vec2() * tile_size + size * 0.5;
    let center = Vec2::new(center.x, compiled.bounds.height() - center.y);
    let (active_color, active_overlay) = active.map(|patch| (Some(patch.color), patch.overlay)).unwrap_or_default();
    let entity = publish_surface_variant(commands, parent, region, center, size, 0.0, pixel_perfect_layers(),
        base.color, active_color, group, mesh, images, materials, variants);
    if base.overlay.is_none() && active_overlay.is_none() { return entity; }
    let overlay = base.overlay.unwrap_or_else(|| surface::image(UVec2::ONE));
    publish_surface_variant(commands, parent, region, center, size, 0.0,
        RenderLayers::layer(SPIKE_LIGHT_OVERLAY_LAYER), overlay, active_overlay, group, mesh, images, materials, variants);
    entity
}

#[expect(clippy::too_many_arguments, reason = "Surface publication names geometry, visibility, variants, and asset ownership.")]
pub(super) fn publish_surface_variant(
    commands: &mut Commands, parent: Entity, region: URect, center: Vec2, size: Vec2, z: f32,
    layers: RenderLayers, base: Image, active: Option<Image>, group: Option<usize>,
    mesh: &Handle<Mesh>, images: &mut Assets<Image>, materials: &mut Assets<ColorMaterial>, variants: &mut Vec<(usize, Entity, Handle<ColorMaterial>)>,
) -> Option<Entity> {
    if !surface::visible(&base) && active.is_none() { return None; }
    let material = materials.add(ColorMaterial::from(images.add(base)));
    let entity = commands.spawn((
        Mesh2d(mesh.clone()), bevy::sprite_render::MeshMaterial2d(material),
        Transform::from_xyz(center.x, center.y, z).with_scale(size.extend(1.0)),
        layers, StateDespawnMarker, bevy::ecs::hierarchy::ChildOf(parent),
        Name::new(format!("SurfaceChunk{}x{}", region.min.x, region.min.y)),
    ));
    let entity = entity.id();
    if let Some((image, group_index)) = active.zip(group) {
        let active_material = materials.add(ColorMaterial::from(images.add(image)));
        variants.push((group_index, entity, active_material));
    }
    Some(entity)
}
