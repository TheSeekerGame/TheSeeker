use super::{mapgen, CompiledLevel, LevelPngConfig, SpawnKind, SpawnSpec};
use crate::prelude::*;

// Color constants in sRGB 8-bit (R,G,B)
const COLOR_TERRAIN: [u8; 3] = [0x00, 0x00, 0x00];
const COLOR_PLAYER: [u8; 3] = [0xFF, 0xFF, 0x00];
const COLOR_SPIKES: [u8; 3] = [0xFF, 0xFF, 0xFF];
/// Optional spike marker. It renders and collides as terrain until activated.
pub const COLOR_OPTIONAL_SPIKES: [u8; 3] = [0x80, 0x80, 0x80];
const COLOR_VAGRANT_YAK_PAIR: [u8; 3] = [0xBB, 0x5E, 0x02];
const COLOR_COLLECTOR: [u8; 3] = [0x45, 0x63, 0x68];
const COLOR_MONK: [u8; 3] = [0xFF, 0xEE, 0xC3];
const COLOR_EXILED: [u8; 3] = [0x65, 0x41, 0x70];
const COLOR_CAPSULE: [u8; 3] = [0x92, 0x8C, 0x8C];
pub const COLOR_ORE_MARKER: [u8; 3] = [0x4D, 0xFB, 0xFF];
pub const COLOR_NEAR_BG_MANUAL: [u8; 3] = [0x5B, 0x66, 0x6F];
pub const COLOR_RULE_REQUIRE: [u8; 3] = [0x6A, 0xBE, 0x30];
pub const COLOR_RULE_FORBID: [u8; 3] = [0xAC, 0x32, 0x32];
pub const MOSAIC_RULE_REQUIRE_PRIMARY_COLOR: [u8; 3] = [0x4B, 0xFF, 0x00];
pub const MOSAIC_RULE_FORBID_PRIMARY_COLOR: [u8; 3] = [0xFF, 0x00, 0x00];
pub const MOSAIC_RULE_REQUIRE_SELF_COLOR: [u8; 3] = [0x35, 0x68, 0x20];
pub const MOSAIC_RULE_FORBID_SELF_COLOR: [u8; 3] = [0x68, 0x20, 0x20];

pub fn compile_level_image(
    image: &Image,
    cfg: &LevelPngConfig,
) -> CompiledLevel {
    let width = image.width();
    let height = image.height();
    let Some(data) = image.data.as_ref() else {
        warn!("Level PNG has no pixel data");
        return CompiledLevel::default();
    };

    compile_pixels(width, height, cfg, data.as_chunks::<4>().0.iter().enumerate().map(|(index, px)| {
        (UVec2::new(index as u32 % width, index as u32 / width), [px[0], px[1], px[2], px[3]])
    }))
}

/// Both authored PNGs and assembled template cells enter the same palette compiler.
pub(super) fn compile_pixels(width: u32, height: u32, cfg: &LevelPngConfig,
    pixels: impl Iterator<Item = (UVec2, [u8; 4])>,
) -> CompiledLevel {
    let mut cells = LevelImageCompilation::new(width, height);
    for (position, rgba) in pixels {
        let Some(kind) = classify_level_image_pixel([rgba[0], rgba[1], rgba[2]], rgba[3]) else { continue; };
        let pixel = LevelImagePixel {
            image_position: position.as_ivec2(),
            position: Vec2::new(position.x as f32 + 0.5, (height - position.y) as f32 - 0.5) * cfg.tile_size,
            spawn_chunk: (position / mapgen::CHUNK_IMAGE_SIZE).as_ivec2(), index: (position.y * width + position.x) as usize,
        };
        cells.apply_pixel_kind(kind, pixel);
    }
    cells.add_capsule_in_player_spawn_chunk();
    CompiledLevel { spawns: cells.spawns,
        bounds: Rect::new(0.0, 0.0, width as f32 * cfg.tile_size, height as f32 * cfg.tile_size), width, height,
        terrain: cells.terrain, spike_tiles: cells.spikes,
        optional_spike_tiles: cells.optional_spikes, manual_near_bg_mask: cells.manual_near_bg, ore_spawn_tiles: cells.ore_spawn_tiles }
}

struct LevelImageCompilation {
    spawns: Vec<SpawnSpec>,
    terrain: Vec<bool>,
    spikes: Vec<bool>,
    optional_spikes: Vec<bool>,
    manual_near_bg: Vec<bool>,
    ore_spawn_tiles: Vec<IVec2>,
    capsule_markers: Vec<SpawnSpec>,

}

impl LevelImageCompilation {
    fn new(width: u32, height: u32) -> Self {
        let len = (width * height) as usize;
        Self {
            spawns: Vec::new(),
            terrain: vec![false; len],
            spikes: vec![false; len],
            optional_spikes: vec![false; len],
            manual_near_bg: vec![false; len],
            ore_spawn_tiles: Vec::new(),
            capsule_markers: Vec::new(),

        }
    }

    fn apply_pixel_kind(
        &mut self,
        kind: LevelImagePixelKind,
        pixel: LevelImagePixel,
    ) {
        match kind {
            LevelImagePixelKind::Spawn(kind) => {
                self.push_spawn(kind, pixel.position, pixel.spawn_chunk);
            },
            LevelImagePixelKind::Capsule => {
                self.capsule_markers.push(SpawnSpec {
                    kind: SpawnKind::Capsule,
                    position: pixel.position,
                    chunk: pixel.spawn_chunk,
                });
            },
            LevelImagePixelKind::Ore => {
                self.ore_spawn_tiles.push(pixel.image_position);

            },
            LevelImagePixelKind::Terrain => self.mark_terrain(pixel.index),
            LevelImagePixelKind::Spikes => self.mark_spike(pixel.index),
            LevelImagePixelKind::OptionalSpikes => {
                self.mark_optional_spike(pixel.index);
            },
            LevelImagePixelKind::ManualNearBg => {
                self.manual_near_bg[pixel.index] = true;

            },
        }
    }

    fn push_spawn(&mut self, kind: SpawnKind, position: Vec2, chunk: IVec2) {
        self.spawns.push(SpawnSpec {
            kind,
            position,
            chunk,
        });
    }

    fn mark_terrain(&mut self, index: usize) {
        self.terrain[index] = true;
    }

    fn mark_spike(&mut self, index: usize) {
        self.spikes[index] = true;
    }

    fn mark_optional_spike(&mut self, index: usize) {
        self.mark_terrain(index);
        self.optional_spikes[index] = true;
    }

    fn add_capsule_in_player_spawn_chunk(&mut self) {
        let Some(player_spawn_chunk) = self.player_spawn_chunk() else {
            return;
        };

        self.push_capsule_in_spawn_chunk(player_spawn_chunk);
        self.warn_ignored_capsules(player_spawn_chunk);
    }

    fn player_spawn_chunk(&self) -> Option<IVec2> {
        self.spawns
            .iter()
            .find(|spawn| spawn.kind == SpawnKind::Player)
            .map(|spawn| spawn.chunk)
    }

    fn push_capsule_in_spawn_chunk(&mut self, player_spawn_chunk: IVec2) {
        let mut capsule_in_spawn_chunk = self
            .capsule_markers
            .iter()
            .filter(|spawn| spawn.chunk == player_spawn_chunk);
        let Some(capsule_spawn) = capsule_in_spawn_chunk.next() else {
            return;
        };

        self.spawns.push(capsule_spawn.clone());
        if capsule_in_spawn_chunk.next().is_some() {
            warn!(
                "levelgen: multiple capsule spawn pixels found in the player spawn chunk; ignoring extras"
            );
        }
    }

    fn warn_ignored_capsules(&self, player_spawn_chunk: IVec2) {
        let ignored_capsules = self
            .capsule_markers
            .iter()
            .filter(|spawn| spawn.chunk != player_spawn_chunk)
            .count();
        if ignored_capsules > 0 {
            warn!(
                "levelgen: ignored {} capsule spawn pixel(s) outside the player spawn chunk",
                ignored_capsules
            );
        }
    }
}

#[derive(Clone, Copy)]
struct LevelImagePixel {
    position: Vec2,
    image_position: IVec2,
    spawn_chunk: IVec2,
    index: usize,
}

#[derive(Clone, Copy)]
enum LevelImagePixelKind {
    Spawn(SpawnKind),
    Capsule,
    Ore,
    Terrain,
    Spikes,
    OptionalSpikes,
    ManualNearBg,
}

fn classify_level_image_pixel(
    color: [u8; 3],
    alpha: u8,
) -> Option<LevelImagePixelKind> {
    if alpha < 128 {
        return None;
    }

    match color {
        COLOR_PLAYER => Some(LevelImagePixelKind::Spawn(SpawnKind::Player)),
        COLOR_VAGRANT_YAK_PAIR => {
            Some(LevelImagePixelKind::Spawn(SpawnKind::VagrantYakPair))
        },
        COLOR_COLLECTOR => {
            Some(LevelImagePixelKind::Spawn(SpawnKind::Collector))
        },
        COLOR_MONK => Some(LevelImagePixelKind::Spawn(SpawnKind::Monk)),
        COLOR_EXILED => Some(LevelImagePixelKind::Spawn(SpawnKind::Exiled)),
        COLOR_CAPSULE => Some(LevelImagePixelKind::Capsule),
        COLOR_ORE_MARKER => Some(LevelImagePixelKind::Ore),
        COLOR_TERRAIN => Some(LevelImagePixelKind::Terrain),
        COLOR_SPIKES => Some(LevelImagePixelKind::Spikes),
        COLOR_OPTIONAL_SPIKES => Some(LevelImagePixelKind::OptionalSpikes),
        COLOR_NEAR_BG_MANUAL => Some(LevelImagePixelKind::ManualNearBg),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_spike_color_has_distinct_classification() {
        assert!(matches!(
            classify_level_image_pixel(COLOR_OPTIONAL_SPIKES, 255),
            Some(LevelImagePixelKind::OptionalSpikes)
        ));
        assert!(classify_level_image_pixel(COLOR_OPTIONAL_SPIKES, 127).is_none());
    }

    #[test]
    fn optional_spike_marker_is_initially_terrain() {
        let mut compilation = LevelImageCompilation::new(1, 1);
        compilation.mark_optional_spike(0);

        assert_eq!(compilation.terrain, vec![true]);
        assert_eq!(compilation.spikes, vec![false]);
        assert_eq!(compilation.optional_spikes, vec![true]);
    }
}
