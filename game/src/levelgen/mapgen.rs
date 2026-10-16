//! Connector-compatible chunk assembly into shared logical world data.

use super::BiomeKind;
use crate::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
use bevy::prelude::{IVec2, Image};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const MAP_CHUNK_COLOR: [u8; 3] = [0x00, 0x00, 0x00];
const MAP_SPAWN_COLOR: [u8; 3] = [0xFF, 0xFF, 0xFF];
const MAP_GEYSER_COLOR: [u8; 3] = [0x45, 0xFF, 0x00];
const CHUNK_SPAWN_COLOR: [u8; 3] = [0xFF, 0xFF, 0x00];

pub const CHUNK_IMAGE_SIZE: u32 = 96;
pub(crate) const TUNDRA_TRANSITION_START_CHUNKS: u32 = 4;
pub(crate) const TUNDRA_TRANSITION_END_CHUNKS: u32 = 6;

pub const CHUNKS_DIR: &str = "assets/levels/chunks";
pub const MAPS_DIR: &str = "assets/levels/maps";
pub const GENERATED_LEVEL_PREFIX: &str = "generated_map:";

const DIR_UP: u8 = 1 << 0;
const DIR_RIGHT: u8 = 1 << 1;
const DIR_DOWN: u8 = 1 << 2;
const DIR_LEFT: u8 = 1 << 3;

#[derive(Clone, Debug)]
pub struct ChunkTemplate {
    pub name: String,
    pub path: PathBuf,
    connectors: u8,
    pub is_spawn: bool,
    footprint: u32,
}

#[derive(Resource, Default)]
pub struct ChunkLibrary {
    templates: Vec<ChunkTemplate>,
    loaded: bool,
}

impl ChunkLibrary {
    pub fn ensure_loaded(&mut self) -> Result<(), String> {
        if self.loaded {
            return Ok(());
        }
        let dir = Path::new(CHUNKS_DIR);
        let read_dir = fs::read_dir(dir).map_err(|err| {
            format!(
                "failed to read {}: {err}",
                dir.display()
            )
        })?;
        let mut templates: Vec<ChunkTemplate> = read_dir
            .flatten()
            .filter_map(chunk_template_from_entry)
            .collect();
        if templates.is_empty() {
            return Err("no chunk templates found".into());
        }
        let spawn_count = templates.iter().filter(|t| t.is_spawn).count();
        if spawn_count != 1 {
            return Err(format!(
                "expected exactly one spawn chunk, found {spawn_count}"
            ));
        }
        templates.sort_by(|a, b| a.name.cmp(&b.name));
        debug!(
            "Loaded {} chunk templates",
            templates.len()
        );
        self.templates = templates;
        self.loaded = true;
        Ok(())
    }

    pub fn templates(&self) -> &[ChunkTemplate] {
        &self.templates
    }

    pub fn spawn_index(&self) -> Option<usize> {
        self.templates.iter().position(|t| t.is_spawn)
    }
}

#[derive(Clone)]
pub struct MapEntry {
    name: String,
    path: PathBuf,
}

#[derive(Resource, Default)]
pub struct MapLibrary {
    maps: Vec<MapEntry>,
    loaded: bool,
}

impl MapLibrary {
    pub fn ensure_loaded(&mut self) -> Result<(), String> {
        if self.loaded {
            return Ok(());
        }
        let dir = Path::new(MAPS_DIR);
        let read_dir = fs::read_dir(dir).map_err(|err| {
            format!(
                "failed to read {}: {err}",
                dir.display()
            )
        })?;
        let mut maps: Vec<MapEntry> =
            read_dir.flatten().filter_map(map_entry_from_entry).collect();
        if maps.is_empty() {
            return Err("no map definitions found".into());
        }
        maps.sort_by(|a, b| a.name.cmp(&b.name));
        debug!("Loaded {} map definitions", maps.len());
        self.maps = maps;
        self.loaded = true;
        Ok(())
    }
}

pub struct GeneratedMap {
    pub compiled: super::CompiledLevel,
    pub seed: u64,
    pub selected_templates: Vec<(String, IVec2, bool)>,
    pub map_name: String,
    pub synthetic_path: String,
    pub width: u32,
    pub height: u32,
    pub biomes: Vec<BiomeKind>,
    pub chunks_w: u32,
    pub chunks_h: u32,
    pub chunk_colors: Vec<[u8; 3]>,
    pub chunk_occupied: Vec<bool>,
    pub pre_impact: Option<super::impact_crater::PreImpactTerrain>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SlotKind {
    Spawn,
    Regular,
}

#[derive(Clone, Debug)]
struct MapSlot {
    grid_pos: IVec2,
    adjacency_mask: u8,
    kind: SlotKind,
    map_color: [u8; 3],
}

struct MapDescription {
    slots: Vec<MapSlot>,
    bounds: MapBounds,
}

#[derive(Clone, Copy, Default, Debug)]
struct MapBounds {
    min: IVec2,
    max: IVec2,
}

impl MapBounds {
    fn single(pos: IVec2) -> Self {
        Self { min: pos, max: pos }
    }

    fn include(&mut self, pos: IVec2) {
        self.min = IVec2::new(self.min.x.min(pos.x), self.min.y.min(pos.y));
        self.max = IVec2::new(self.max.x.max(pos.x), self.max.y.max(pos.y));
    }

    fn width_chunks(&self) -> u32 {
        (self.max.x - self.min.x + 1).max(0) as u32
    }

    fn height_chunks(&self) -> u32 {
        (self.max.y - self.min.y + 1).max(0) as u32
    }
}

/// The chunk template a map slot uses, and whether it is mirrored.
#[derive(Clone, Copy, Debug, PartialEq)]
struct SlotTemplate {
    template: usize,
    flip: bool,
    source_origin: UVec2,
}

pub fn generate_map(chunk_lib: &mut ChunkLibrary, map_lib: &mut MapLibrary,
    request: &super::pipeline::GenerationRequest,
) -> Result<GeneratedMap, String> {
    let _timer = super::diagnostics::StageTimer::new("assemble_map");
    chunk_lib.ensure_loaded()?;
    map_lib.ensure_loaded()?;
    let spawn_index = chunk_lib.spawn_index().ok_or("spawn chunk missing")?;
    let mut rng = StdRng::seed_from_u64(request.seed);
    let entry = match &request.map {
        Some(name) => map_lib.maps.iter().find(|map| &map.name == name).ok_or("requested map not found")?,
        None => &map_lib.maps[rng.random_range(0..map_lib.maps.len())],
    };
    let map = load_map_description(entry)?;
    let placed = assign_compatible_templates(&map.slots, chunk_lib.templates(), spawn_index, &mut rng)?;
    let chunks_w = map.bounds.width_chunks();
    let chunks_h = map.bounds.height_chunks();
    let width = chunks_w.checked_mul(CHUNK_IMAGE_SIZE).ok_or("map width overflow")?;
    let height = chunks_h.checked_mul(CHUNK_IMAGE_SIZE).ok_or("map height overflow")?;
    let mut decoded = HashMap::new();
    for &SlotTemplate { template: index, .. } in &placed {
        if let std::collections::hash_map::Entry::Vacant(entry) = decoded.entry(index) {
            let template = &chunk_lib.templates[index];
            entry.insert(load_chunk(template)?);
        }
    }
    let pixels = map.slots.iter().zip(&placed).flat_map(|(slot, &SlotTemplate { template: index, flip, source_origin })| {
        let origin = (slot.grid_pos - map.bounds.min).as_uvec2() * CHUNK_IMAGE_SIZE;
        let template = &chunk_lib.templates[index];
        let data = decoded[&index].data.as_deref().unwrap_or_default();
        (0..CHUNK_IMAGE_SIZE * CHUNK_IMAGE_SIZE).filter_map(move |i| {
            let local = UVec2::new(i % CHUNK_IMAGE_SIZE, i / CHUNK_IMAGE_SIZE);
            let source = source_origin + local;
            let rgba = chunk_source_pixel(data, source.x, source.y, template.footprint * CHUNK_IMAGE_SIZE, flip)?;
            (!should_clear_template_pixel(template, rgba)).then_some((origin + local, rgba))
        })
    });
    let compiled = super::compiler::compile_pixels(width, height, &request.config, pixels);
    let pre_impact = map.slots.iter().zip(&placed).find(|(_, placement)| placement.template == spawn_index)
        .map(|(slot, _)| {
            let origin = (slot.grid_pos - map.bounds.min).as_uvec2() * CHUNK_IMAGE_SIZE;
            super::impact_crater::PreImpactTerrain::load(&chunk_lib.templates[spawn_index].path, origin)
        })
        .transpose()?
        .flatten();
    let mut chunk_colors = vec![[0; 3]; (chunks_w * chunks_h) as usize];
    let mut chunk_occupied = vec![false; chunk_colors.len()];
    for slot in &map.slots {
        let pos = (slot.grid_pos - map.bounds.min).as_uvec2();
        let index = (pos.y * chunks_w + pos.x) as usize;
        chunk_colors[index] = slot.map_color;
        chunk_occupied[index] = true;
    }
    let selected_templates = map.slots.iter().zip(&placed).filter(|(_, placement)| placement.source_origin == UVec2::ZERO).map(|(slot, placement)| {
        (chunk_lib.templates[placement.template].name.clone(), slot.grid_pos - map.bounds.min, placement.flip)
    }).collect();
    let spawn_row = map.slots.iter().find(|slot| slot.kind == SlotKind::Spawn).map_or(0, |slot| (slot.grid_pos.y - map.bounds.min.y) as u32);
    let biomes = depth_biomes(width, height, spawn_row);
    Ok(GeneratedMap { compiled, seed: request.seed, selected_templates, map_name: entry.name.clone(),
        synthetic_path: format!("{}{}:{:016x}", GENERATED_LEVEL_PREFIX, entry.name, request.seed),
        width, height, chunks_w, chunks_h, biomes,
        chunk_colors, chunk_occupied, pre_impact })
}

/// Coarse deterministic patches blend the depth bands without scattering isolated biome tiles.
fn depth_biomes(width: u32, height: u32, spawn_row: u32) -> Vec<BiomeKind> {
    let start = (spawn_row + TUNDRA_TRANSITION_START_CHUNKS) * CHUNK_IMAGE_SIZE;
    let end = (spawn_row + TUNDRA_TRANSITION_END_CHUNKS) * CHUNK_IMAGE_SIZE;
    (0..width * height).map(|index| {
        let (x, y) = (index % width, index / width);
        let roll = super::hash::tile_hash((x / 8) as i32, (y / 8) as i32, 0, 0xB10A_E123) % u64::from(end - start);
        if y >= end || (y >= start && roll < u64::from(y - start)) { BiomeKind::Tundra } else { BiomeKind::Ice }
    }).collect()
}

/// One template per slot, in slot order.
fn assign_compatible_templates(slots: &[MapSlot], templates: &[ChunkTemplate], spawn: usize, rng: &mut StdRng)
    -> Result<Vec<SlotTemplate>, String> {
    let mut pools: [Vec<(usize, bool)>; 16] = std::array::from_fn(|mask| {
        compatible_orientations(templates, mask as u8, 1)
    });
    for pool in &mut pools { pool.shuffle(rng); }
    let mut used = vec![false; templates.len()];
    let mut cursors = [0usize; 16];
    let mut placed = assign_large_templates(slots, templates, rng, &mut used);
    for (slot, placement) in slots.iter().zip(&mut placed) {
        if placement.is_some() {
            continue;
        }
        let (template, flip) = choose_template(slot, templates, spawn, &pools, &mut used, &mut cursors)?;
        *placement = Some(SlotTemplate { template, flip, source_origin: UVec2::ZERO });
    }
    placed.into_iter().map(|placement| placement.ok_or_else(|| "unassigned map slot".into())).collect()
}

fn compatible_orientations(templates: &[ChunkTemplate], mask: u8, footprint: u32) -> Vec<(usize, bool)> {
    templates.iter().enumerate().filter(|(_, template)| !template.is_spawn && template.footprint == footprint).flat_map(|(index, template)| {
        [false, true].into_iter().filter_map(move |flip| {
            let connectors = [template.connectors, swap_left_right(template.connectors)][usize::from(flip)];
            (connectors == mask).then_some((index, flip))
        })
    }).collect()
}

/// Reserve complete rooms before single-cell assignment. A large room only exposes the middle
/// of each edge; any external neighbor at a corner makes that block ineligible.
fn assign_large_templates(slots: &[MapSlot], templates: &[ChunkTemplate], rng: &mut StdRng, used: &mut [bool])
    -> Vec<Option<SlotTemplate>> {
    let lookup: HashMap<_, _> = slots.iter().enumerate().map(|(index, slot)| (slot.grid_pos, index)).collect();
    let mut placed = vec![None; slots.len()];
    let mut pools: [Vec<(usize, bool)>; 16] = std::array::from_fn(|mask| compatible_orientations(templates, mask as u8, 3));
    for pool in &mut pools { pool.shuffle(rng); }
    for slot in slots {
        let Some((members, mask)) = large_room_slots(slot.grid_pos, slots, &lookup) else { continue };
        if members.iter().any(|&index| placed[index].is_some()) { continue; }
        let Some((template, flip)) = pools[mask as usize].iter().copied().find(|&(index, _)| !used[index]) else { continue };
        used[template] = true;
        for (local, index) in members.into_iter().enumerate() {
            let source_origin = UVec2::new(local as u32 % 3, local as u32 / 3) * CHUNK_IMAGE_SIZE;
            placed[index] = Some(SlotTemplate { template, flip, source_origin });
        }
    }
    placed
}

fn large_room_slots(origin: IVec2, slots: &[MapSlot], lookup: &HashMap<IVec2, usize>) -> Option<([usize; 9], u8)> {
    let mut members = [0; 9];
    for (local, member) in members.iter_mut().enumerate() {
        *member = *lookup.get(&(origin + IVec2::new(local as i32 % 3, local as i32 / 3)))?;
        if slots[*member].kind == SlotKind::Spawn { return None; }
    }
    let mut mask = 0;
    for (delta, direction) in [(IVec2::new(0, -1), DIR_UP), (IVec2::new(3, 0), DIR_RIGHT),
        (IVec2::new(0, 3), DIR_DOWN), (IVec2::new(-1, 0), DIR_LEFT)] {
        mask |= room_boundary_mask(origin + delta, direction, lookup)?;
    }
    (mask != 0).then_some((members, mask))
}

fn room_boundary_mask(origin: IVec2, direction: u8, lookup: &HashMap<IVec2, usize>) -> Option<u8> {
    let mut mask = 0;
    for edge in 0..3 {
        let occupied = lookup.contains_key(&(origin + room_edge_offset(edge, direction)));
        if occupied && edge != 1 { return None; }
        if occupied { mask = direction; }
    }
    Some(mask)
}

fn room_edge_offset(edge: i32, direction: u8) -> IVec2 {
    if direction == DIR_UP || direction == DIR_DOWN { IVec2::new(edge, 0) } else { IVec2::new(0, edge) }
}

fn choose_template(slot: &MapSlot, templates: &[ChunkTemplate], spawn: usize, pools: &[Vec<(usize, bool)>; 16],
    used: &mut [bool], cursors: &mut [usize; 16],
) -> Result<(usize, bool), String> {
    if slot.kind == SlotKind::Spawn {
        return (templates[spawn].connectors == slot.adjacency_mask).then_some((spawn, false))
            .ok_or_else(|| "spawn connectors do not match map".into());
    }
    let mask = slot.adjacency_mask as usize;
    let pool = &pools[mask];
    let choice = pool.iter().copied().find(|&(index, _)| !used[index])
        .or_else(|| pool.get(cursors[mask] % pool.len().max(1)).copied())
        .ok_or_else(|| format!("no compatible template for connector mask {mask:04b}"))?;
    used[choice.0] = true;
    cursors[mask] += 1;
    Ok(choice)
}

fn chunk_template_from_entry(entry: fs::DirEntry) -> Option<ChunkTemplate> {
    let path = entry.path();
    let stem = png_file_stem(&path)?;
    let name = stem.to_string();
    let is_spawn = stem.starts_with("player_spawn");
    let connectors = parse_connector_suffix(stem);
    let footprint = if stem.starts_with("large_chunk_3x3_") { 3 } else { 1 };
    Some(ChunkTemplate {
        name,
        path,
        connectors,
        is_spawn,
        footprint,
    })
}

fn map_entry_from_entry(entry: fs::DirEntry) -> Option<MapEntry> {
    let path = entry.path();
    let stem = png_file_stem(&path)?;
    let name = stem.to_string();
    Some(MapEntry { name, path })
}

fn png_file_stem(path: &Path) -> Option<&str> {
    is_png(path)
        .then(|| path.file_stem().and_then(|stem| stem.to_str()))
        .flatten()
}

fn is_png(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("png"))
        .unwrap_or(false)
}

fn parse_connector_suffix(stem: &str) -> u8 {
    let mut mask = 0u8;
    for ch in stem.chars().rev() {
        match ch {
            'U' => mask |= DIR_UP,
            'D' => mask |= DIR_DOWN,
            'L' => mask |= DIR_LEFT,
            'R' => mask |= DIR_RIGHT,
            _ => break,
        }
    }
    mask
}

fn load_map_description(entry: &MapEntry) -> Result<MapDescription, String> {
    let image = load_png(&entry.path)?;
    
    let data = image.data.as_ref().ok_or("map image missing data")?;
    let (mut slots, spawn_count, bounds) = collect_map_slots(data, image.width());
    if spawn_count != 1 {
        return Err(format!(
            "map {} must contain exactly one white spawn pixel (found {})",
            entry.name, spawn_count
        ));
    }
    let Some(bounds) = bounds else {
        return Err(format!("map {} contains no chunks", entry.name));
    };

    assign_adjacency_masks(&mut slots);
    Ok(MapDescription { slots, bounds })
}

fn collect_map_slots(data: &[u8], width: u32) -> (Vec<MapSlot>, usize, Option<MapBounds>) {
    let mut slots = Vec::new();
    let mut spawn_count = 0usize;
    let mut bounds = None;

    for (pixel_idx, pixel) in data
        .as_chunks::<4>().0.iter()
        .enumerate()
    {
        let Some(slot) = slot_from_map_pixel(pixel_idx, pixel, width) else {
            continue;
        };
        if slot.kind == SlotKind::Spawn {
            spawn_count += 1;
        }
        include_slot_bounds(&mut bounds, slot.grid_pos);
        slots.push(slot);
    }

    (slots, spawn_count, bounds)
}

fn slot_from_map_pixel(pixel_idx: usize, pixel: &[u8], width: u32) -> Option<MapSlot> {
    if pixel[3] < 128 {
        return None;
    }

    let color = [pixel[0], pixel[1], pixel[2]];
    map_slot_from_color(map_pixel_position(pixel_idx, width), color)
}

fn map_pixel_position(pixel_idx: usize, width: u32) -> IVec2 {
    let x = pixel_idx % width as usize;
    let y = pixel_idx / width as usize;
    IVec2::new(x as i32, y as i32)
}

fn map_slot_from_color(pos: IVec2, color: [u8; 3]) -> Option<MapSlot> {
    let kind = match color {
        MAP_CHUNK_COLOR | MAP_GEYSER_COLOR => SlotKind::Regular,
        MAP_SPAWN_COLOR => SlotKind::Spawn,
        _ => return None,
    };

    Some(MapSlot {
        grid_pos: pos,
        adjacency_mask: 0,
        kind,
        map_color: color,
    })
}

fn include_slot_bounds(bounds: &mut Option<MapBounds>, pos: IVec2) {
    let Some(bounds) = bounds else {
        *bounds = Some(MapBounds::single(pos));
        return;
    };

    bounds.include(pos);
}

fn assign_adjacency_masks(slots: &mut [MapSlot]) {
    let lookup: HashMap<IVec2, ()> =
        slots.iter().map(|slot| (slot.grid_pos, ())).collect();
    for slot in slots {
        slot.adjacency_mask = adjacency_mask(slot.grid_pos, &lookup);
    }
}

fn adjacency_mask(pos: IVec2, lookup: &HashMap<IVec2, ()>) -> u8 {
    let neighbor_offsets = [
        (IVec2::new(0, -1), DIR_UP),
        (IVec2::new(1, 0), DIR_RIGHT),
        (IVec2::new(0, 1), DIR_DOWN),
        (IVec2::new(-1, 0), DIR_LEFT),
    ];
    let mut mask = 0u8;
    for (delta, bit) in neighbor_offsets {
        if lookup.contains_key(&(pos + delta)) {
            mask |= bit;
        }
    }
    mask
}

fn chunk_source_pixel(
    src: &[u8],
    x: u32,
    y: u32,
    size: u32,
    flip: bool,
) -> Option<[u8; 4]> {
    let src_x = if flip { size - 1 - x } else { x };
    let src_idx = ((y * size + src_x) * 4) as usize;
    if src_idx + 3 >= src.len() {
        return None;
    }

    Some([
        src[src_idx],
        src[src_idx + 1],
        src[src_idx + 2],
        src[src_idx + 3],
    ])
}

fn should_clear_template_pixel(template: &ChunkTemplate, pixel: [u8; 4]) -> bool {
    !template.is_spawn && pixel_matches_rgb(pixel, CHUNK_SPAWN_COLOR)
}

fn pixel_matches_rgb(pixel: [u8; 4], color: [u8; 3]) -> bool {
    pixel[0] == color[0] && pixel[1] == color[1] && pixel[2] == color[2]
}

fn swap_left_right(mask: u8) -> u8 {
    let mut swapped = mask & (DIR_UP | DIR_DOWN);
    if mask & DIR_LEFT != 0 {
        swapped |= DIR_RIGHT;
    }
    if mask & DIR_RIGHT != 0 {
        swapped |= DIR_LEFT;
    }
    swapped
}

pub(super) fn load_png(path: &Path) -> Result<Image, String> {
    let bytes = fs::read(path).map_err(|err| {
        format!(
            "failed to read {}: {err}",
            path.display()
        )
    })?;
    let mut image = Image::from_buffer(
        &bytes,
        ImageType::Extension("png"),
        CompressedImageFormats::default(),
        true,
        ImageSampler::Default,
        RenderAssetUsages::default(),
    )
    .map_err(|err| {
        format!(
            "failed to decode {}: {err}",
            path.display()
        )
    })?;
    image.sampler = ImageSampler::nearest();
    Ok(image)
}


fn load_chunk(template: &ChunkTemplate) -> Result<Image, String> {
    let image = load_png(&template.path)?;
    let size = template.footprint * CHUNK_IMAGE_SIZE;
    if image.width() != size || image.height() != size {
        return Err(format!("chunk {} must be {size}x{size} pixels", template.name));
    }
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template(name: &str, mask: u8, spawn: bool) -> ChunkTemplate {
        ChunkTemplate { name: name.into(), path: PathBuf::new(), connectors: mask, is_spawn: spawn, footprint: 1 }
    }

    #[test]
    fn arbitrary_rectangular_map_keeps_row_coordinates_and_connectors() {
        let mut pixels = vec![0u8; 31 * 2 * 4];
        pixels[30 * 4..31 * 4].copy_from_slice(&[255; 4]);
        pixels[61 * 4..62 * 4].copy_from_slice(&[0, 0, 0, 255]);
        let (mut slots, spawns, bounds) = collect_map_slots(&pixels, 31);
        assign_adjacency_masks(&mut slots);
        assert_eq!(spawns, 1);
        assert_eq!(slots[0].grid_pos, IVec2::new(30, 0));
        assert_eq!(slots[1].adjacency_mask, DIR_UP);
        assert_eq!(bounds.unwrap().height_chunks(), 2);
    }

    #[test]
    fn assignment_exhausts_both_orientations_before_reusing_and_is_reproducible() {
        let templates = vec![template("spawn", DIR_DOWN, true), template("right", DIR_RIGHT, false), template("left", DIR_LEFT, false)];
        let slots: Vec<_> = (0..20).map(|i| MapSlot { grid_pos: IVec2::new(i, 0), adjacency_mask: DIR_RIGHT,
            kind: SlotKind::Regular, map_color: MAP_CHUNK_COLOR }).collect();
        let run = || assign_compatible_templates(&slots, &templates, 0, &mut StdRng::seed_from_u64(8)).unwrap();
        let placed = run();
        assert_ne!(placed[0].template, placed[1].template);
        assert_eq!(placed, run());
        assert!(placed.iter().all(|placement| placement.template != 0));
        assert!(placed.iter().all(|placement| {
            let connectors = templates[placement.template].connectors;
            [connectors, swap_left_right(connectors)][usize::from(placement.flip)] == DIR_RIGHT
        }));
    }

    #[test]
    fn assignment_reports_incompatible_spawn_and_missing_pool() {
        let templates = vec![template("spawn", DIR_DOWN, true)];
        let slot = MapSlot { grid_pos: IVec2::ZERO, adjacency_mask: DIR_RIGHT, kind: SlotKind::Spawn,
            map_color: MAP_SPAWN_COLOR };
        assert!(assign_compatible_templates(&[slot], &templates, 0, &mut StdRng::seed_from_u64(1)).is_err());
    }

    fn two_large_rooms() -> Vec<MapSlot> {
        let mut positions: Vec<_> = [0, 4].into_iter().flat_map(|origin| {
            (0..3).flat_map(move |y| (0..3).map(move |x| IVec2::new(origin + x, y)))
        }).collect();
        positions.extend([IVec2::new(-1, 1), IVec2::new(3, 1), IVec2::new(7, 1)]);
        positions.sort_by_key(|pos| (pos.y, pos.x));
        let mut slots: Vec<_> = positions.into_iter().filter_map(|pos| map_slot_from_color(pos, MAP_GEYSER_COLOR)).collect();
        assign_adjacency_masks(&mut slots);
        slots
    }

    #[test]
    fn large_rooms_consume_unique_templates_then_fall_back_to_individual_cells() {
        let slots = two_large_rooms();
        let mut templates = vec![template("spawn", DIR_DOWN, true)];
        templates.extend((1..16).map(|mask| template("small", mask, false)));
        let mut large = template("large", DIR_LEFT | DIR_RIGHT, false);
        large.footprint = 3;
        templates.push(large);
        let run = || assign_compatible_templates(&slots, &templates, 0, &mut StdRng::seed_from_u64(9)).unwrap();
        let placed = run();
        assert_eq!(placed, run());
        assert_eq!(placed.iter().filter(|placement| templates[placement.template].footprint == 3).count(), 9);
        assert_eq!(placed.iter().filter(|placement| placement.source_origin == UVec2::ZERO).count(), slots.len() - 8);
        assert!(slots.iter().all(|slot| slot.map_color == MAP_GEYSER_COLOR));
    }

    #[test]
    fn large_room_rejects_corner_connections_and_player_spawn() {
        let mut slots = two_large_rooms();
        slots.push(map_slot_from_color(IVec2::new(-1, 0), MAP_CHUNK_COLOR).unwrap());
        let lookup: HashMap<_, _> = slots.iter().enumerate().map(|(index, slot)| (slot.grid_pos, index)).collect();
        assert!(large_room_slots(IVec2::ZERO, &slots, &lookup).is_none());
        let index = lookup[&IVec2::new(5, 1)];
        slots[index].kind = SlotKind::Spawn;
        assert!(large_room_slots(IVec2::new(4, 0), &slots, &lookup).is_none());
    }

    #[test]
    fn depth_biomes_leave_surface_ice_and_make_deep_tiles_tundra() {
        let width = 16;
        let cells = depth_biomes(width, 8 * CHUNK_IMAGE_SIZE, 1);
        assert!(cells[..(5 * CHUNK_IMAGE_SIZE * width) as usize].iter().all(|&biome| biome == BiomeKind::Ice));
        assert!(cells[(7 * CHUNK_IMAGE_SIZE * width) as usize..].iter().all(|&biome| biome == BiomeKind::Tundra));
        let transition = &cells[(5 * CHUNK_IMAGE_SIZE * width) as usize..(7 * CHUNK_IMAGE_SIZE * width) as usize];
        assert!(transition.contains(&BiomeKind::Ice) && transition.contains(&BiomeKind::Tundra));
    }

    #[test]
    fn shipped_demo_assembles_three_unique_large_rooms_without_losing_map_markers() {
        let request = super::super::pipeline::GenerationRequest { seed: 20_261_001, map: Some("map1".into()), config: default() };
        let generated = generate_map(&mut ChunkLibrary::default(), &mut MapLibrary::default(), &request).unwrap();
        assert_eq!((generated.width, generated.height), (12 * CHUNK_IMAGE_SIZE, 16 * CHUNK_IMAGE_SIZE));
        assert_eq!(generated.chunk_occupied.iter().filter(|&&occupied| occupied).count(), 64);
        assert_eq!(generated.chunk_colors.iter().filter(|&&color| color == MAP_GEYSER_COLOR).count(), 4);
        let rooms: std::collections::HashSet<_> = generated.selected_templates.iter()
            .filter(|(name, _, _)| name.starts_with("large_chunk_3x3_")).map(|(name, _, _)| name).collect();
        assert_eq!(rooms.len(), 3);
        assert_eq!(generated.compiled.spawns.iter().filter(|spawn| spawn.kind == super::super::SpawnKind::Player).count(), 1);
        assert!(generated.pre_impact.is_some());
    }
}
