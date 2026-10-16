//! Compare selective evaluation with full passes before biome selection.
use super::*;
use crate::levelgen::{background, surface, terrain, BiomeKind, CompiledLevel, LevelBiomeMask};

const SEED: u64 = 0x1234_5678_9ABC_DEF0;

struct Fixture {
    rules: LdtkRuleData,
    images: Assets<Image>,
    level: CompiledLevel,
    mosaics: [Vec<bool>; 3],
    biomes: LevelBiomeMask,
}

impl Fixture {
    fn new(size: UVec2) -> Self {
        let json: serde_json::Value = serde_json::from_slice(&std::fs::read("assets/levels/tiles/ldtk_rules/level.01_test.ldtk").unwrap()).unwrap();
        let mut rules = LdtkRuleData::default();
        parse_ldtk_defs(&json, &mut rules);
        let mut images = Assets::default();
        for &uid in rules.tilesets.keys() { rules.tileset_images.insert(uid, images.add(surface::image(UVec2::ONE))); }
        let len = (size.x * size.y) as usize;
        let level = CompiledLevel { width: size.x, height: size.y,
            terrain: (0..len).map(|i| (i / size.x as usize) % 32 < 20).collect(),
            spike_tiles: (0..len).map(|i| i % 73 == 0).collect(), ..default() };
        let mosaics = std::array::from_fn(|layer| (0..len).map(|i| (i / 16) % 5 == layer).collect());
        let biomes = LevelBiomeMask { width: size.x, height: size.y, tundra_inner: vec![],
            cells: (0..len).map(|i| [BiomeKind::Ice, BiomeKind::Tundra][(i / 31) % 2]).collect() };
        Self { rules, images, level, mosaics, biomes }
    }

    fn terrain_grid(&self) -> Vec<i32> {
        let mut grid = generate_wall_categories(&self.level.terrain, &self.mosaics[0], &self.mosaics[1], &self.mosaics[2]);
        terrain::mark_spike_int_grid_cells(&mut grid, &self.level.spike_tiles);
        grid
    }

    fn new_terrain(&self) -> Vec<Option<surface::TilePaint>> {
        terrain::prepare_main_surface(&self.level, &self.mosaics, &self.rules, &self.images, 6, Some(&self.biomes)).unwrap().cells
    }

}

fn reference_terrain(fixture: &Fixture) -> Vec<Option<surface::TilePaint>> {
    let grid = fixture.terrain_grid();
    let (w, h) = (fixture.level.width, fixture.level.height);
    let ice = &fixture.rules.layers["Main"];
    let tundra = fixture.rules.layers.get("TundraMain")
        .filter(|_| fixture.biomes.cells.iter().any(|cell| cell.is_tundra()));
    let size = UVec2::new(w, h);
    let main = RuleProgram::terrain(&grid, size, ice, SEED).cells();
    let alternate = tundra.map(|layer| RuleProgram::terrain(&grid, size, layer, SEED + 0x2222).cells());
    let grass = tundra.map(|layer| RuleProgram::grass(&grid, size, layer, SEED + 0x3333).cells());
    let fallback = discover_fallback_tiles(ice);
    let tundra_fallback = tundra.map(discover_fallback_tiles);
    grid.iter().enumerate().map(|(index, &value)| {
        let use_tundra = value != MAIN_SPIKES_INT_GRID_VALUE && tundra.is_some() && fixture.biomes.cells[index].is_tundra();
        let tile = if use_tundra {
            alternate.as_ref().unwrap()[index].clone().or_else(|| grass.as_ref().unwrap()[index].clone())
        } else { main[index].clone() };
        let fallbacks = if use_tundra { tundra_fallback.as_ref().unwrap() } else { &fallback };
        tile.or_else(|| fallback_tile(value, fallbacks.get(value)))
            .map(|tile| surface::TilePaint { tile, source: usize::from(use_tundra) })
    }).collect()
}

fn reference_background(fixture: &Fixture, grid: &[i32], mask: &[bool]) -> Vec<Option<surface::TilePaint>> {
    let ice = &fixture.rules.layers["NearBackground"];
    let tundra = fixture.rules.layers.get("TundraNearBackground")
        .filter(|_| fixture.biomes.cells.iter().any(|cell| cell.is_tundra()));
    let size = UVec2::new(fixture.level.width, fixture.level.height);
    let main = RuleProgram::background(grid, size, ice, SEED + 0x1111).cells();
    let alternate = tundra.map(|layer| RuleProgram::background(grid, size, layer, SEED + 0x1121).cells());
    let fallback = discover_bg_fallback_tiles(ice);
    let tundra_fallback = tundra.map(discover_bg_fallback_tiles);
    grid.iter().enumerate().map(|(index, &value)| {
        if !mask[index] { return None; }
        let use_tundra = tundra.is_some() && fixture.biomes.cells[index].is_tundra();
        let (tile, fallback) = if use_tundra {
            (alternate.as_ref().unwrap()[index].clone(), tundra_fallback.as_ref().unwrap())
        } else { (main[index].clone(), &fallback) };
        tile.or_else(|| fallback_tile(value, fallback.get(value)))
            .map(|tile| surface::TilePaint { tile, source: usize::from(use_tundra) })
    }).collect()
}

fn fallback_tile(value: i32, tile_id: Option<u32>) -> Option<TilePlacement> {
    tile_id.filter(|_| value != 0).map(|tile_id| TilePlacement { tile_id, flip_x: false, flip_y: false })
}

#[test]
fn selective_biome_rules_match_full_passes_and_missing_biome_fallbacks() {
    let mut fixture = Fixture::new(UVec2::new(193, 99));
    compare_surfaces(&fixture);
    fixture.biomes.cells.fill(BiomeKind::Ice);
    compare_surfaces(&fixture);
    fixture.biomes.cells.fill(BiomeKind::Tundra);
    compare_surfaces(&fixture);
    fixture.rules.layers.remove("TundraMain");
    fixture.rules.layers.remove("TundraNearBackground");
    compare_surfaces(&fixture);
}

fn compare_surfaces(fixture: &Fixture) {
    assert_eq!(fixture.new_terrain(), reference_terrain(fixture));
    let grid: Vec<_> = (0..fixture.level.width * fixture.level.height).map(|i| ((i / 6) % 5) as i32).collect();
    let mask: Vec<_> = grid.iter().enumerate().map(|(i, _)| i % 7 != 0).collect();
    let actual = background::prepare_background_surface(&fixture.level, &grid, &mask, Some(&fixture.biomes), &fixture.rules, &fixture.images).unwrap();
    assert_eq!(actual.cells, reference_background(fixture, &grid, &mask));
}

#[test]
#[ignore = "Measures terrain tile-rule selection only; excludes rasterization and full generation."]
fn selective_biome_rule_cost() {
    let fixture = Fixture::new(UVec2::splat(384));
    assert_eq!(fixture.new_terrain(), reference_terrain(&fixture));
    for run in 0..5 {
        let start = std::time::Instant::now();
        std::hint::black_box(reference_terrain(&fixture));
        let full_ms = start.elapsed().as_secs_f64() * 1000.0;
        let start = std::time::Instant::now();
        std::hint::black_box(fixture.new_terrain());
        println!("selective_biome_rules: {}", serde_json::json!({ "run": run,
            "full_passes_ms": full_ms, "selective_ms": start.elapsed().as_secs_f64() * 1000.0,
            "tiles": 384 * 384, "scope": "terrain rule selection only, synthetic mixed-biome grid with authored rules" }));
    }
}
