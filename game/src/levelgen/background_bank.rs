//! Load-only adapter for the paired backgrounds produced by tools/backgrounds/generate.py.
use super::*;
use serde::Deserialize;
use bevy::tasks::{ComputeTaskPool, TaskPool};

const BANK_DIR: &str = "assets/levels/background_bank";
const FORMAT_VERSION: u32 = 2;
const PADDING: u32 = 2;

#[derive(Deserialize)]
struct Manifest {
    version: u32,
    map_hash: u64,
    size: [u32; 2],
    tile_px: u32,
    patches: Vec<Patch>,
}

#[derive(Deserialize)]
struct Patch {
    min: [u32; 2],
    max: [u32; 2],
    texture_min: [u32; 2],
    texture_max: [u32; 2],
}

pub(super) fn load_pair(config: &LevelPngConfig, level: &CompiledLevel, seed: u64) -> Result<[background_surface::BackgroundBake; 2], String> {
    let _timer = diagnostics::StageTimer::new("shipped_background_decode");
    let (name, seed) = selection(&config.path, seed)?;
    let variant = std::env::var("SEEKER_BACKGROUND_VARIANT").ok()
        .and_then(|value| value.parse::<usize>().ok()).unwrap_or(seed as usize % 3);
    if variant >= 3 { return Err("Background variant must be 0, 1, or 2".into()); }
    let size = UVec2::new(level.width, level.height) + UVec2::splat(PADDING * 2);
    let source = if config.path.starts_with(mapgen::GENERATED_LEVEL_PREFIX) {
        Path::new("assets/levels/maps").join(format!("{name}.png"))
    } else { Path::new("assets").join(&config.path) };
    let map_hash = fs::read(&source).map_err(|err| format!("{}: {err}", source.display()))?
        .into_iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3));
    let load = |group| load_layer(&Path::new(BANK_DIR).join(&name).join(format!("{variant}-{group}")), size, config.tile_size, map_hash);
    let pair = [load(1)?, load(2)?];
    info!("levelgen: selected shipped background pair {name}/{variant}");
    Ok(pair)
}

fn selection(path: &str, seed: u64) -> Result<(String, u64), String> {
    if let Some(rest) = path.strip_prefix(mapgen::GENERATED_LEVEL_PREFIX) {
        let (name, seed) = rest.rsplit_once(':').ok_or("Invalid generated-map identity")?;
        return Ok((name.into(), u64::from_str_radix(seed, 16).map_err(|err| err.to_string())?));
    }
    let name = Path::new(path).file_stem().and_then(|stem| stem.to_str()).ok_or("Invalid level filename")?;
    Ok((format!("custom/{name}"), seed))
}

fn load_layer(root: &Path, size: UVec2, tile_size: f32, map_hash: u64) -> Result<background_surface::BackgroundBake, String> {
    let path = root.join("manifest.json");
    let data = fs::read(&path).map_err(|err| format!("{}: {err}. Build this map's assets with tools/backgrounds/generate.py", path.display()))?;
    let manifest: Manifest = serde_json::from_slice(&data).map_err(|err| format!("{}: {err}", path.display()))?;
    if manifest.version != FORMAT_VERSION || manifest.map_hash != map_hash || UVec2::from_array(manifest.size) != size || manifest.tile_px != GROUND_TILE_ART_SIZE_PX {
        return Err(format!("{}: background dimensions or format changed; regenerate its assets", path.display()));
    }
    let patches = load_patches(root, &manifest)?;
    Ok(background_surface::BackgroundBake { patches, size, tile_px: manifest.tile_px, world_size: size.as_vec2() * tile_size })
}

fn load_patches(root: &Path, manifest: &Manifest) -> Result<Vec<background_surface::BackgroundPatch>, String> {
    if manifest.patches.is_empty() { return Ok(Vec::new()); }
    let pool = ComputeTaskPool::get_or_init(TaskPool::new);
    let batch_size = manifest.patches.len().div_ceil(pool.thread_num().clamp(1, 4));
    let batches = pool.scope(|scope| {
        for (batch_index, batch) in manifest.patches.chunks(batch_size).enumerate() {
            scope.spawn(load_patch_batch(root, manifest, batch, batch_index * batch_size));
        }
    });
    let mut ordered: Vec<_> = batches.into_iter().flatten().collect();
    ordered.sort_by_key(|(index, _)| *index);
    ordered.into_iter().map(|(_, patch)| patch).collect()
}

async fn load_patch_batch(root: &Path, manifest: &Manifest, batch: &[Patch], start: usize)
    -> Vec<(usize, Result<background_surface::BackgroundPatch, String>)> {
    batch.iter().enumerate().map(|(offset, patch)| {
        let index = start + offset;
        (index, read_patch(root, index, patch, manifest))
    }).collect()
}

fn read_patch(root: &Path, index: usize, patch: &Patch, manifest: &Manifest) -> Result<background_surface::BackgroundPatch, String> {
    let region = URect { min: UVec2::from_array(patch.min), max: UVec2::from_array(patch.max) };
    let texture_region = URect { min: UVec2::from_array(patch.texture_min), max: UVec2::from_array(patch.texture_max) };
    validate_region(region, texture_region, manifest)?;
    let mut color = mapgen::load_png(&root.join(format!("{index}.png")))?;
    color.sampler = ImageSampler::linear();
    let coverage = mapgen::load_png(&root.join(format!("{index}-coverage.png")))?;
    if UVec2::new(color.width(), color.height()) != texture_region.size()
        || UVec2::new(coverage.width(), coverage.height()) != region.size() * manifest.tile_px {
        return Err(format!("{}: texture {index} has incorrect dimensions", root.display()));
    }
    let coverage = coverage.data.as_ref().ok_or("Missing background coverage pixels")?
        .as_chunks::<4>().0.iter().map(|px| u8::from(px[0] != 0)).collect();
    Ok(background_surface::BackgroundPatch { region, texture_region, color, coverage })
}

fn validate_region(region: URect, texture: URect, manifest: &Manifest) -> Result<(), String> {
    let size = UVec2::from_array(manifest.size);
    if !region.min.cmplt(region.max).all() || !region.max.cmple(size).all()
        || !region.min.x.is_multiple_of(mapgen::CHUNK_IMAGE_SIZE) || !region.min.y.is_multiple_of(mapgen::CHUNK_IMAGE_SIZE)
        || !region.size().cmple(UVec2::splat(mapgen::CHUNK_IMAGE_SIZE)).all() {
        return Err("Invalid background chunk bounds".into());
    }
    let pixels = URect { min: region.min * manifest.tile_px, max: region.max * manifest.tile_px };
    if texture != surface::expand(pixels, 1, size * manifest.tile_px) {
        return Err("Background texture must include its one-pixel filtering gutter".into());
    }
    Ok(())
}
