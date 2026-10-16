//! Bounded-output compiler probe; intentionally does not model world residency or gameplay.
use super::*;
use crate::levelgen::{compiler, ground_shading, mapgen, surface, CompiledLevel, LevelPngConfig};
use std::time::Instant;

#[test]
#[ignore = "Measures 1x/10x/100x/1000x occupied output chunks, discarding each image after consumption."]
fn occupied_chunk_compiler_throughput() {
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read("assets/levels/tiles/ldtk_rules/level.01_test.ldtk").unwrap()).unwrap();
    let layer = json["defs"]["layers"].as_array().unwrap().iter().filter_map(parse_layer).find(|layer| layer.identifier == "Main").unwrap();
    let image = mapgen::load_png(std::path::Path::new("assets/levels/tiles/ldtk_rules/MainTilesetAdditions.png")).unwrap();
    for multiplier in [1, 10, 100, 1000] {
        let started = Instant::now();
        let mut output_bytes = 0usize;
        let mut checksum = 0u64;
        for index in 0..8 * multiplier {
            let (bytes, value) = compile_output_chunk(&layer, &image, index);
            output_bytes += bytes;
            checksum = checksum.wrapping_add(value);
        }
        println!("compiler_throughput: {}", serde_json::json!({
            "occupied_chunks": 8 * multiplier, "elapsed_ms": started.elapsed().as_secs_f64() * 1000.0,
            "generated_rgba_bytes": output_bytes, "retained_output_bytes": 0,
            "max_output_bytes": 96 * 96 * 6 * 6 * 4, "checksum": checksum,
            "scope": "sequential PNG palette + distance categories + authored LDtk + native raster + Euclidean shading; bounded sink; excludes mosaic rules, placement, backgrounds, darkness, GPU",
        }));
    }
}

fn compile_output_chunk(layer: &LdtkAutoLayer, image: &Image, variant: u32) -> (usize, u64) {
    let level = compiler::compile_pixels(96, 96, &LevelPngConfig::default(), (0..96 * 96).map(|index| {
        let position = UVec2::new(index % 96, index / 96);
        let terrain = position.y < 4 || position.y > 90 || (position.y + variant % 13) % 32 < 15;
        (position, if terrain { [0, 0, 0, 255] } else { [0, 0, 0, 0] })
    }));
    let grid = generate_wall_categories_from_distance(&level.terrain, 96, 96);
    let placements = RuleProgram::terrain(&grid, UVec2::splat(96), layer, u64::from(variant)).cells();
    let surface = surface::TileSurface { size: UVec2::splat(96), tile_px: 6,
        sources: vec![(image, image.width() / 6)], cells: placements.into_iter().map(|tile| tile.map(|tile| surface::TilePaint { tile, source: 0 })).collect() };
    consume_chunk(&surface, &level)
}

fn consume_chunk(surface: &surface::TileSurface<'_>, level: &CompiledLevel) -> (usize, u64) {
    let mut image = surface.bake(URect::from_corners(UVec2::ZERO, UVec2::splat(96)));
    let stride = image.width() as usize * 4;
    let data = image.data.as_mut().unwrap();
    ground_shading::shade(data, stride, UVec2::ZERO, level, 6);
    let checksum = data.iter().map(|&value| u64::from(value)).sum();
    (data.len(), std::hint::black_box(checksum))
}
