//! The spawn crater before and after the landing cinematic's impact.
//!
//! Terrain and collision are always those of the formed crater. A spawn template may have an
//! intact-ground sibling with the same file name in `assets/levels/chunks/pre_impact/`; only its
//! solid (`#000000`) pixels are read. Cave directly below the chunk where the intact ground
//! closes an opening is filled too, far enough that no view before the impact reaches past it. That terrain is baked as alternate, initially
//! hidden surface patches that a cinematic shows until the pod lands.
use std::path::Path;

use super::*;

const PRE_IMPACT_DIR: &str = "pre_impact";
/// Extent of the cave filled below a closed opening, in tiles.
const PIT_FILL_DEPTH: u32 = 12;
const PIT_FILL_HALF_WIDTH: u32 = 16;

/// Intact terrain of the spawn chunk, row-major from the top like `CompiledLevel::terrain`.
#[derive(Resource, Clone)]
pub struct PreImpactTerrain {
    /// Tile position of the spawn chunk's top-left corner.
    origin: UVec2,
    terrain: Vec<bool>,
}

/// Visible only once the crater has formed.
#[derive(Component)]
pub(crate) struct ImpactFormed;

/// Visible only before the impact; spawned hidden.
#[derive(Component)]
pub(crate) struct ImpactIntact;

impl PreImpactTerrain {
    /// Reads the intact sibling of a spawn template placed at `origin`, if one exists.
    pub(super) fn load(template: &Path, origin: UVec2) -> Result<Option<Self>, String> {
        let (Some(dir), Some(name)) = (template.parent(), template.file_name()) else {
            return Ok(None);
        };
        let path = dir.join(PRE_IMPACT_DIR).join(name);
        if !path.exists() {
            return Ok(None);
        }
        let image = mapgen::load_png(&path)?;
        let size = mapgen::CHUNK_IMAGE_SIZE;
        if image.width() != size || image.height() != size {
            return Err(format!("{} must be {size}x{size} pixels", path.display()));
        }
        let pixels = image.data.as_deref().unwrap_or_default();
        let terrain = pixels.as_chunks::<4>().0.iter()
            .map(|pixel| pixel[3] >= 128 && pixel[..3] == [0, 0, 0])
            .collect();
        Ok(Some(Self { origin, terrain }))
    }

    /// The level before the impact, and the chunk regions whose terrain differs.
    pub(super) fn intact_level(&self, level: &CompiledLevel) -> (CompiledLevel, Vec<URect>) {
        let (intact, changed) = self.restore(level);
        let size = mapgen::CHUNK_IMAGE_SIZE;
        let mut regions: Vec<URect> = changed.iter().map(|tile| {
            let min = *tile / size * size;
            URect::from_corners(min, (min + UVec2::splat(size)).min(UVec2::new(level.width, level.height)))
        }).collect();
        regions.sort_by_key(|region| (region.min.y, region.min.x));
        regions.dedup();
        (intact, regions)
    }

    /// The tiles whose terrain the impact changes.
    pub(super) fn changed_tiles(&self, level: &CompiledLevel) -> Vec<UVec2> {
        self.restore(level).1
    }

    fn restore(&self, level: &CompiledLevel) -> (CompiledLevel, Vec<UVec2>) {
        let mut intact = level.clone();
        let size = mapgen::CHUNK_IMAGE_SIZE;
        let mut changed: Vec<UVec2> = self.terrain.iter().enumerate()
            .map(|(index, &solid)| (self.origin + UVec2::new(index as u32 % size, index as u32 / size), solid))
            .filter(|&(tile, solid)| set_terrain(&mut intact, tile, solid))
            .map(|(tile, _)| tile)
            .collect();
        let bottom = self.origin.y + size;
        for column in (0..size).map(|x| self.origin.x + x) {
            changed.extend(fill_pit_below(&mut intact, UVec2::new(column, bottom)));
        }
        (intact, changed)
    }
}

fn terrain_at(level: &CompiledLevel, tile: UVec2) -> Option<bool> {
    (tile.x < level.width && tile.y < level.height).then(|| level.terrain[(tile.y * level.width + tile.x) as usize])
}

/// Returns whether the tile changed.
fn set_terrain(level: &mut CompiledLevel, tile: UVec2, solid: bool) -> bool {
    if terrain_at(level, tile).is_none_or(|current| current == solid) {
        return false;
    }
    level.terrain[(tile.y * level.width + tile.x) as usize] = solid;
    true
}

/// Fills the cave reachable from `start` within the pit bounds when the intact spawn chunk
/// closes it from above.
fn fill_pit_below(level: &mut CompiledLevel, start: UVec2) -> Vec<UVec2> {
    let closed_above = start.y > 0 && terrain_at(level, start - UVec2::Y) == Some(true);
    if !closed_above || terrain_at(level, start) != Some(false) {
        return Vec::new();
    }
    let min = UVec2::new(start.x.saturating_sub(PIT_FILL_HALF_WIDTH), start.y);
    let max = UVec2::new(start.x + PIT_FILL_HALF_WIDTH, start.y + PIT_FILL_DEPTH - 1);
    let mut filled = Vec::new();
    let mut pending = vec![start];
    while let Some(tile) = pending.pop() {
        if !set_terrain(level, tile, true) {
            continue;
        }
        filled.push(tile);
        let neighbors = [tile.saturating_sub(UVec2::X), tile + UVec2::X, tile + UVec2::Y];
        pending.extend(neighbors.into_iter().filter(|next| {
            next.cmpge(min).all() && next.cmple(max).all() && terrain_at(level, *next) == Some(false)
        }));
    }
    filled
}

/// Bakes the intact ground with the formed level's mosaics and publishes it hidden.
pub(super) fn publish_intact_patches(
    commands: &mut Commands, parent: Entity, (intact, regions): (&CompiledLevel, &[URect]), level: &LevelSurfaceInputs<'_>,
    mesh: &Handle<Mesh>, images: &mut Assets<Image>, materials: &mut Assets<ColorMaterial>,
) {
    // Snow caps and edge detail follow the intact ground's own surface.
    let mut placements = Vec::new();
    for (library, allowed) in [(&level.decorations.ice, level.ice_allowed), (&level.decorations.tundra, level.tundra_allowed)] {
        placements.extend(generate_decoration_placements(intact, library, images, Some(allowed)));
    }
    let decorations = surface::decoration_chunks(&placements, images, GROUND_TILE_ART_SIZE_PX);
    let Some(surface) = terrain::prepare_main_surface(intact, level.mosaics, level.ldtk, images,
        GROUND_TILE_ART_SIZE_PX, Some(level.biomes)) else {
        return;
    };
    let patches: Vec<_> = regions.iter().map(|&region| {
        let chunk = (region.min / mapgen::CHUNK_IMAGE_SIZE).as_ivec2();
        let decorations = decorations.get(&chunk).map(Vec::as_slice).unwrap_or_default();
        surface::bake_ground(&surface, intact, region, decorations, images)
    }).collect();
    drop(surface);
    for patch in patches {
        let entity = terrain::publish_ground_patch(commands, parent, level.compiled, level.tile_size,
            patch, None, None, mesh, images, materials, &mut Vec::new());
        let Some(entity) = entity else { continue };
        commands.entity(entity).insert((ImpactIntact, Visibility::Hidden));
    }
}

/// Inputs shared by every bake of the level's main surface.
pub(super) struct LevelSurfaceInputs<'a> {
    pub compiled: &'a CompiledLevel,
    pub mosaics: &'a [Vec<bool>; 3],
    pub ldtk: &'a ldtk_rules::LdtkRuleData,
    pub biomes: &'a LevelBiomeMask,
    pub decorations: &'a Biomes<PlacementLibrary>,
    pub ice_allowed: &'a [bool],
    pub tundra_allowed: &'a [bool],
    pub tile_size: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rows 0-1 solid; a one-tile opening on row 2 into an open room from row 3 down.
    fn pit_level() -> CompiledLevel {
        let (width, height) = (40, 20);
        let terrain = (0..width * height).map(|index| {
            let (x, y) = (index % width, index / width);
            y < 2 || (y == 2 && x != 20)
        }).collect();
        CompiledLevel { width, height, terrain, ..default() }
    }

    #[test]
    fn closed_opening_fills_the_cave_below_within_the_pit_bounds() {
        let mut level = pit_level();
        let filled = fill_pit_below(&mut level, UVec2::new(20, 2));
        assert!(filled.contains(&UVec2::new(20, 2)));
        assert_eq!(terrain_at(&level, UVec2::new(20 - PIT_FILL_HALF_WIDTH, 2 + PIT_FILL_DEPTH - 1)), Some(true));
        assert_eq!(terrain_at(&level, UVec2::new(20 + PIT_FILL_HALF_WIDTH + 1, 5)), Some(false));
        assert_eq!(terrain_at(&level, UVec2::new(20, 2 + PIT_FILL_DEPTH)), Some(false));
    }

    #[test]
    fn opening_still_open_above_is_left_alone() {
        let mut level = pit_level();
        level.terrain[(level.width + 20) as usize] = false;
        assert!(fill_pit_below(&mut level, UVec2::new(20, 2)).is_empty());
    }
}
