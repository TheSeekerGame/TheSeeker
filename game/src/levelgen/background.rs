//! background generation and publication.
use super::*;
use background_synthesis::{Context, Exemplar};

/// Separates the tundra's detailing rolls from the ice's.
const TUNDRA_DETAIL_SALT: u64 = 0x7D5A_11E3;

pub(super) fn build_background_biome_allowed_masks(
    biome_mask: Option<&LevelBiomeMask>,
    len: usize,
) -> (Vec<bool>, Vec<bool>) {
    let mut tundra_allowed = vec![false; len];
    let mut ice_allowed = vec![true; len];
    let Some(biome_mask) = biome_mask else {
        return (tundra_allowed, ice_allowed);
    };

    ice_allowed.fill(false);
    for (idx, cell) in biome_mask.cells.iter().enumerate() {
        mark_background_biome_allowed_cell(
            &mut tundra_allowed,
            &mut ice_allowed,
            idx,
            cell.is_tundra(),
        );
    }
    (tundra_allowed, ice_allowed)
}

pub(super) fn mark_background_biome_allowed_cell(
    tundra_allowed: &mut [bool],
    ice_allowed: &mut [bool],
    idx: usize,
    is_tundra: bool,
) {
    if is_tundra {
        tundra_allowed[idx] = true;
        return;
    }
    ice_allowed[idx] = true;
}

pub(super) fn combine_background_biome_near_grids(
    len: usize,
    tundra_allowed: &[bool],
    ice_near_grid: &[i32],
    tundra_near_grid: Option<&[i32]>,
) -> Vec<i32> {
    let mut near_grid = vec![0i32; len];
    for (idx, cell) in near_grid.iter_mut().enumerate() {
        *cell = background_biome_near_grid_value(
            idx,
            tundra_allowed,
            ice_near_grid,
            tundra_near_grid,
        );
    }
    near_grid
}

pub(super) fn background_biome_near_grid_value(
    idx: usize,
    tundra_allowed: &[bool],
    ice_near_grid: &[i32],
    tundra_near_grid: Option<&[i32]>,
) -> i32 {
    if !tundra_allowed.get(idx).copied().unwrap_or(false) {
        return ice_near_grid.get(idx).copied().unwrap_or(0);
    }
    tundra_near_grid
        .and_then(|grid| grid.get(idx).copied())
        .unwrap_or(0)
}

#[derive(Clone, Copy)]
pub(super) struct NearBackgroundLayerInputs<'a> {
    pub compiled: &'a CompiledLevel,
    pub biome_mask: Option<&'a LevelBiomeMask>,
    pub ldtk_data: &'a ldtk_rules::LdtkRuleData,
    pub images: &'a Assets<Image>,
    pub blur_settings: &'a BackgroundBlurSettings,
    pub near_depth: f32,
}

pub(super) fn build_near_background_layer(
    near_grid: &[i32],
    inputs: NearBackgroundLayerInputs,
) -> Option<background_surface::BackgroundBake> {
    if !near_grid.iter().any(|&v| v != 0) {
        return None;
    }

    let (near_grid, biome_mask_warped) = warp_bg_int_grid_for_parallax(
        inputs.compiled.width,
        inputs.compiled.height,
        near_grid,
        inputs.biome_mask,
        inputs.near_depth,
    );
    let biome_mask_for_bake = biome_mask_warped.as_ref().or(inputs.biome_mask);
    let render_mask: Vec<bool> = near_grid.iter().map(|&v| v != 0).collect();
    let surface = prepare_background_surface(
        inputs.compiled,
        &near_grid,
        &render_mask,
        biome_mask_for_bake,
        inputs.ldtk_data,
        inputs.images,
    )?;

    let cutout = background_surface::SpawnChunkCutout::new(inputs.compiled, surface.tile_px);
    Some(background_surface::bake(&surface, inputs.blur_settings.blur_for_group(0),
        cutout, Vec2::new(inputs.compiled.bounds.width(), inputs.compiled.bounds.height())))
}

pub(super) fn finalize_background_group_image(group_image: &mut Image, blur_px: f32) {
    if blur_px > 0.0001 {
        apply_background_blur(group_image, blur_px);
        group_image.sampler = ImageSampler::linear();
        return;
    }
    group_image.sampler = ImageSampler::nearest();
}

#[expect(clippy::too_many_arguments, reason = "Bevy system dependencies are expressed as explicit system parameters.")]
pub(super) fn build_background_layers(
    mut commands: Commands,
    compiled: Option<Res<CompiledLevel>>,
    root: Option<Res<LevelRootEntity>>,
    biomes: Option<Res<LevelBiomeMask>>,
    libraries: Res<GenerationLibraries>,
    cfg: Res<LevelPngConfig>,
    request: Option<Res<pipeline::GenerationRequest>>,
    ldtk_data: Res<ldtk_rules::LdtkRuleData>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut color_materials: ResMut<Assets<ColorMaterial>>,
    mut built_flag: ResMut<pipeline::GenerationBuild>,
    blur_settings: Res<BackgroundBlurSettings>,
    mut darkness_masks: Option<ResMut<DarknessBakeMaskAtlas>>,

) {
    if built_flag.phase != pipeline::BuildPhase::Ground {
        return;
    }
    let Some(compiled) = compiled else { return; };
    let root_e = match root {
        Some(r) => r.0,
        None => return,
    };
    let biome_mask = biomes.as_ref().and_then(|b| {
        if b.matches_dims(compiled.width, compiled.height) {
            Some(b.as_ref())
        } else {
            warn!(
                "Biome mask dimensions {}x{} do not match compiled level {}x{}",
                b.width, b.height, compiled.width, compiled.height
            );
            None
        }
    });
    if !ldtk_ready(&ldtk_data, &images) || !libraries.loaded(&images) {
        return;
    }

    let len = (compiled.width * compiled.height) as usize;
    let manual_near_mask: Option<&[bool]> = if compiled
        .manual_near_bg_mask
        .len()
        == len
    {
        Some(compiled.manual_near_bg_mask.as_slice())
    } else if compiled.manual_near_bg_mask.is_empty() {
        None
    } else {
        warn!(
                "Manual near background mask has length {}, expected {} ({}x{}); ignoring manual markers",
                compiled.manual_near_bg_mask.len(),
                len,
                compiled.width,
                compiled.height
            );
        None
    };
    let (tundra_allowed, ice_allowed) =
        build_background_biome_allowed_masks(biome_mask, len);
    let tundra_present = tundra_allowed.iter().any(|&b| b);

    let seed = request.as_ref().map_or(0, |r| r.seed);
    let banks = match background_bank::load_pair(&cfg, &compiled, seed) {
        Ok(banks) => banks,
        Err(error) => { built_flag.fail(error); return; }
    };

    if let Some(mask_atlas) = darkness_masks.as_mut() {
        mask_atlas.clear_background_layers();
    }

    let level_world_w = compiled.bounds.width();
    let level_world_h = compiled.bounds.height();
    let level_center = Vec2::new(level_world_w * 0.5, level_world_h * 0.5);

    let near_depth = background_surface::parallax_depth(0);

    let shared_images = images.as_ref();
    let exemplar = |handle: &Handle<Image>| shared_images.get(handle).and_then(Exemplar::from_image);
    let (Some(ice), Some(tundra)) = (exemplar(&libraries.backgrounds.ice), exemplar(&libraries.backgrounds.tundra)) else {
        built_flag.fail("Near-background exemplars are missing or unreadable".into());
        return;
    };
    // One near wall for every biome; each biome details it after its own exemplar.
    let size = UVec2::new(compiled.width, compiled.height);
    let mut wall = background_openings::near_wall(&compiled.terrain, size, seed);
    for (covered, &marked) in wall.iter_mut().zip(manual_near_mask.unwrap_or_default()) {
        *covered |= marked;
    }
    let target = Context::new(wall, compiled.terrain.clone(), size);
    let ice_near_grid = background_synthesis::synthesize(&target, &ice, |index| ice_allowed[index], seed);
    let tundra_near_grid = tundra_present.then(|| {
        background_synthesis::synthesize(&target, &tundra, |index| tundra_allowed[index], seed ^ TUNDRA_DETAIL_SALT)
    });
    let near_grid = combine_background_biome_near_grids(len, &tundra_allowed, &ice_near_grid, tundra_near_grid.as_deref());
    let near_layer = build_near_background_layer(&near_grid, NearBackgroundLayerInputs {
        compiled: &compiled,
        biome_mask,
        ldtk_data: &ldtk_data,
        images: shared_images,
        blur_settings: &blur_settings,
        near_depth,
    });
    let [mid_layer, far_layer] = banks;

    for (group, layer) in [near_layer, Some(mid_layer), Some(far_layer)].into_iter().enumerate() {
        let Some(layer) = layer else { continue; };
        background_surface::publish(&mut commands, root_e, group, layer, level_center,
            darkness_masks.as_deref_mut(), &mut images, &mut meshes, &mut color_materials);
    }

    built_flag.phase = pipeline::BuildPhase::Surfaces;
}

pub(super) fn warp_bg_int_grid_for_parallax(
    width: u32,
    height: u32,
    grid: &[i32],
    biome_mask: Option<&LevelBiomeMask>,
    parallax_depth: f32,
) -> (Vec<i32>, Option<LevelBiomeMask>) {
    let len = (width * height) as usize;
    if width == 0 || height == 0 || grid.len() != len {
        return (grid.to_vec(), None);
    }
    if !parallax_depth.is_finite() || parallax_depth <= 0.0 {
        return (grid.to_vec(), None);
    }

    let f = 1.0 / parallax_depth;
    if !f.is_finite() || f <= 0.0 || (f - 1.0).abs() < 0.0001 {
        return (grid.to_vec(), None);
    }

    let origin_x = width as f32 * 0.5;
    let origin_y = height as f32 * 0.5;
    let w_i = width as i32;
    let h_i = height as i32;

    let src_cells = biome_mask.and_then(|b| {
        if b.matches_dims(width, height) {
            Some(b.cells.as_slice())
        } else {
            None
        }
    });
    let mut warped_cells = src_cells.map(|_| vec![BiomeKind::default(); len]);
    let src_inner = biome_mask.and_then(|b| {
        if b.matches_dims(width, height) && b.tundra_inner.len() == len {
            Some(b.tundra_inner.as_slice())
        } else {
            None
        }
    });
    let mut warped_inner = src_inner.map(|_| vec![false; len]);

    let mut warped_grid = vec![0i32; len];
    let sources = BgParallaxWarpSources {
        width,
        origin_x,
        origin_y,
        scale_factor: f,
        max_x: w_i - 1,
        max_y: h_i - 1,
        grid,
        src_cells,
        src_inner,
    };
    for y_out in 0..height {
        warp_bg_int_grid_row(
            y_out,
            &sources,
            &mut warped_grid,
            &mut warped_cells,
            &mut warped_inner,
        );
    }

    let warped_biome_mask = warped_cells.map(|cells| LevelBiomeMask {
        width,
        height,
        cells,
        tundra_inner: warped_inner.unwrap_or_default(),
    });
    (warped_grid, warped_biome_mask)
}

pub(super) struct BgParallaxWarpSources<'a> {
    pub width: u32,
    pub origin_x: f32,
    pub origin_y: f32,
    pub scale_factor: f32,
    pub max_x: i32,
    pub max_y: i32,
    pub grid: &'a [i32],
    pub src_cells: Option<&'a [BiomeKind]>,
    pub src_inner: Option<&'a [bool]>,
}

pub(super) fn warp_bg_int_grid_row(
    y_out: u32,
    sources: &BgParallaxWarpSources,
    warped_grid: &mut [i32],
    warped_cells: &mut Option<Vec<BiomeKind>>,
    warped_inner: &mut Option<Vec<bool>>,
) {
    for x_out in 0..sources.width {
        warp_bg_int_grid_cell(
            x_out,
            y_out,
            sources,
            warped_grid,
            warped_cells,
            warped_inner,
        );
    }
}

pub(super) fn warp_bg_int_grid_cell(
    x_out: u32,
    y_out: u32,
    sources: &BgParallaxWarpSources,
    warped_grid: &mut [i32],
    warped_cells: &mut Option<Vec<BiomeKind>>,
    warped_inner: &mut Option<Vec<bool>>,
) {
    let src_idx = parallax_warp_source_index(x_out, y_out, sources);
    let dst_idx = (y_out * sources.width + x_out) as usize;
    warped_grid[dst_idx] = sources.grid[src_idx];
    copy_warped_biome_cell(sources.src_cells, warped_cells, src_idx, dst_idx);
    copy_warped_inner_cell(sources.src_inner, warped_inner, src_idx, dst_idx);
}

pub(super) fn parallax_warp_source_index(
    x_out: u32,
    y_out: u32,
    sources: &BgParallaxWarpSources,
) -> usize {
    let pos_out_x = x_out as f32 + 0.5;
    let pos_out_y = y_out as f32 + 0.5;
    let pos_src_x =
        sources.origin_x + (pos_out_x - sources.origin_x) / sources.scale_factor;
    let pos_src_y =
        sources.origin_y + (pos_out_y - sources.origin_y) / sources.scale_factor;

    let src_x = ((pos_src_x - 0.5).round() as i32).clamp(0, sources.max_x) as u32;
    let src_y = ((pos_src_y - 0.5).round() as i32).clamp(0, sources.max_y) as u32;
    (src_y * sources.width + src_x) as usize
}

pub(super) fn copy_warped_biome_cell(
    src_cells: Option<&[BiomeKind]>,
    warped_cells: &mut Option<Vec<BiomeKind>>,
    src_idx: usize,
    dst_idx: usize,
) {
    let (Some(src_cells), Some(dst_cells)) = (src_cells, warped_cells.as_mut()) else {
        return;
    };
    dst_cells[dst_idx] = src_cells[src_idx];
}

pub(super) fn copy_warped_inner_cell(
    src_inner: Option<&[bool]>,
    warped_inner: &mut Option<Vec<bool>>,
    src_idx: usize,
    dst_idx: usize,
) {
    let (Some(src_inner), Some(dst_inner)) = (src_inner, warped_inner.as_mut()) else {
        return;
    };
    dst_inner[dst_idx] = src_inner[src_idx];
}

pub(super) fn prepare_background_surface<'a>(
    compiled: &CompiledLevel, int_grid: &[i32], render_mask: &[bool], biome_mask: Option<&LevelBiomeMask>,
    ldtk_data: &'a ldtk_rules::LdtkRuleData, images: &'a Assets<Image>,
) -> Option<surface::TileSurface<'a>> {
    let _timer = diagnostics::StageTimer::new("bake_background_group_ldtk_from_intgrid");
    let size = UVec2::new(compiled.width, compiled.height);
    let assets = biome_layers(ldtk_data, images, None, biome_mask, ["NearBackground", "TundraNearBackground"])?;
    let tile_px = assets[0].tile_size;
    let rules: Vec<_> = assets.iter().enumerate().map(|(index, asset)| {
        let seed = 0x1234_5678_9ABC_DEF0u64.wrapping_add(0x1111 + index as u64 * 0x10);
        (ldtk_rules::RuleProgram::background(int_grid, size, asset.layer, seed),
            ldtk_rules::discover_bg_fallback_tiles(asset.layer))
    }).collect();
    let cells = int_grid.iter().enumerate().map(|(index, &value)| {
        if !render_mask[index] { return None; }
        let source = usize::from(cell_uses_tundra(biome_mask, assets.len(), index));
        let (program, fallback) = &rules[source];
        let tile = program.at(index).or_else(|| {
            (value != 0).then(|| fallback.get(value)).flatten()
                .map(|tile_id| ldtk_rules::TilePlacement { tile_id, flip_x: false, flip_y: false })
        });
        tile.map(|tile| surface::TilePaint { tile, source })
    }).collect();
    let sources = assets.into_iter().map(|asset| (asset.image, asset.tiles_per_row)).collect();
    Some(surface::TileSurface { size, tile_px, sources, cells })
}
