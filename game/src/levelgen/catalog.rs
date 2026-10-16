//! Generation asset libraries and the filename conventions that describe them.
//!
//! Rule images are named `<order>_rule…` and apply in ascending order; rules sharing an order keep
//! directory order. A rule name may carry `ruleflip-x`/`ruleflip-y` (the pattern also matches
//! mirrored), `randflip-x`/`randflip-y` (the placed artwork is mirrored at random), `prob<N>` (the
//! percent chance to apply, default 100), `max<N>` (placements per chunk) and size tags such as
//! `smallcorner`. Placement rule directories are named `rules…` and may carry `mindist-<N>` (tiles
//! between placements) and `colorrule-<RRGGBB>` (the chunk color the rules are limited to). Sprite
//! names may carry size tags, `weight<N>` (default 100), `xoffset±<N>`/`yoffset±<N>` and harvest drops.
use std::collections::BTreeMap;
use std::sync::OnceLock;

use super::*;

pub(super) const ENTITY_Z: f32 = -0.000_000_001;
pub(super) const GEYSER_KIND: &str = "geysers";

/// Size tags pairing placement rules with sprites.
const SIZE_TAGS: [&str; 12] = [
    "smallcorner", "mediumcorner", "bigcorner", "massivecorner",
    "smallflat", "mediumflat", "bigflat", "massiveflat",
    "smalledge", "mediumedge", "bigedge", "massiveedge",
];

/// A rule image and the metadata its filename and directory carry.
#[derive(Clone)]
pub(super) struct RuleImage {
    pub order: u32,
    pub handle: Handle<Image>,
    pub tags: Vec<&'static str>,
    pub ruleflip_x: bool,
    pub ruleflip_y: bool,
    pub randflip_x: bool,
    pub randflip_y: bool,
    pub prob_percent: u8,
    pub max_per_chunk: Option<u32>,
    pub min_distance_tiles: u32,
    pub map_color: Option<[u8; 3]>,
}

impl RuleImage {
    /// The metadata in a rule image's filename stem and its directory's name, without an image.
    pub(super) fn named(stem: &str, directory: &str) -> Option<Self> {
        let name = stem.to_ascii_lowercase();
        let directory = directory.to_ascii_lowercase();
        let order = leading(&name, char::is_ascii_digit);
        if order.is_empty() || !name.contains("_rule") {
            return None;
        }
        let flag = |marker: &str| name.contains(marker) || name.contains(&marker.replace('-', "_"));
        Some(Self {
            order: order.parse().unwrap_or(0),
            handle: Handle::default(),
            tags: size_tags(&name),
            ruleflip_x: flag("ruleflip-x"),
            ruleflip_y: flag("ruleflip-y"),
            randflip_x: flag("randflip-x"),
            randflip_y: flag("randflip-y"),
            prob_percent: number_after(&name, "prob").map_or(100, |percent| percent.min(100) as u8),
            max_per_chunk: number_after(&name, "max").map(|max| max.max(1)),
            min_distance_tiles: number_after(&directory, "mindist-").unwrap_or(0),
            map_color: color_after(&directory, "colorrule-"),
        })
    }
}

/// A placeable artwork and the metadata its filename carries.
#[derive(Clone)]
pub(super) struct SpriteImage {
    pub handle: Handle<Image>,
    pub tags: Vec<&'static str>,
    pub weight: u32,
    pub offset_px: IVec2,
    pub harvest: Option<harvestable_materials::HarvestPlantDefinition>,
}

impl SpriteImage {
    fn load(assets: &AssetServer, file: &PngFile) -> Self {
        let name = file.stem.to_ascii_lowercase();
        let offset = |marker| signed_after(&name, marker).unwrap_or(0);
        Self {
            handle: assets.load(&file.path),
            tags: size_tags(&name),
            weight: number_after(&name, "weight").map_or(100, |weight| weight.max(1)),
            offset_px: IVec2::new(offset("xoffset"), offset("yoffset")),
            harvest: harvestable_materials::parse_harvest_drop_from_sprite_name(&file.stem),
        }
    }
}

/// Rules deciding where a kind of decoration or entity goes, and the sprites it may use.
#[derive(Default, Clone)]
pub(super) struct PlacementKind {
    pub rules: Vec<RuleImage>,
    pub sprites: Vec<SpriteImage>,
}

/// Kinds by name; placement passes visit them in name order.
#[derive(Default, Clone)]
pub(super) struct PlacementLibrary {
    pub kinds: BTreeMap<String, PlacementKind>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum OreCategory {
    Ore1,
    Ore2,
    Ore3,
}

impl OreCategory {
    pub(super) const ICE: [Self; 1] = [Self::Ore1];
    pub(super) const TUNDRA: [Self; 2] = [Self::Ore2, Self::Ore3];

    pub(super) fn dir_name(self) -> &'static str {
        match self {
            Self::Ore1 => "ore1",
            Self::Ore2 => "ore2",
            Self::Ore3 => "ore3",
        }
    }

    fn number(self) -> u8 {
        self as u8 + 1
    }
}

#[derive(Clone)]
pub(super) struct OreImage {
    pub handle: Handle<Image>,
    pub definition: harvestable_materials::HarvestOreDefinition,
}

#[derive(Default, Clone)]
pub(super) struct OreLibrary {
    pub sprites_by_category: HashMap<OreCategory, Vec<OreImage>>,
}

impl OreLibrary {
    pub(super) fn sprites_for(&self, category: OreCategory) -> &[OreImage] {
        self.sprites_by_category.get(&category).map_or(&[], Vec::as_slice)
    }
}

/// Ordered mosaic rules and their masks, decoded on first use.
#[derive(Default, Clone)]
pub(super) struct MosaicRules {
    pub rules: Vec<RuleImage>,
    pub decoded: OnceLock<Vec<MosaicDecodedRule>>,
}

/// A mosaic rule decoded from its mask image. Offsets are relative to the mask center.
#[derive(Clone)]
pub(super) struct MosaicDecodedRule {
    pub order: u32,
    /// Conditions on the unclaimed source, the primary terrain, and the layer's own claims.
    pub source: MaskConditions,
    pub primary: MaskConditions,
    pub layer: MaskConditions,
    /// Source cells the rule claims into the layer.
    pub claims: Vec<(i32, i32)>,
    pub ruleflip_x: bool,
    pub ruleflip_y: bool,
    pub randflip_x: bool,
    pub randflip_y: bool,
    pub prob_percent: u8,
}

/// Offsets that must be set, and offsets that must be clear, in one grid.
#[derive(Clone, Default)]
pub(super) struct MaskConditions {
    pub require: Vec<(i32, i32)>,
    pub forbid: Vec<(i32, i32)>,
}

#[derive(Default, Clone)]
pub(super) struct Biomes<T> {
    pub ice: T,
    pub tundra: T,
}

impl<T> Biomes<T> {
    fn load(ice: &str, tundra: &str, load: impl Fn(&str) -> T) -> Self {
        Self { ice: load(ice.trim_matches('/')), tundra: load(tundra.trim_matches('/')) }
    }

    fn each(&self) -> [&T; 2] {
        [&self.ice, &self.tundra]
    }
}

/// Every generation library, read when a game starts.
#[derive(Resource, Default, Clone)]
pub(super) struct GenerationLibraries {
    /// Main-platform mosaic layers 1–3.
    pub mosaics: [MosaicRules; 3],
    pub decorations: Biomes<PlacementLibrary>,
    pub entities: Biomes<PlacementLibrary>,
    pub ores: Biomes<OreLibrary>,
    /// The artist's near background for each biome, which near detailing copies.
    pub backgrounds: Biomes<Handle<Image>>,
}

impl GenerationLibraries {
    /// Whether every image the libraries name has loaded.
    pub(super) fn loaded(&self, images: &Assets<Image>) -> bool {
        let kinds = || self.decorations.each().into_iter().chain(self.entities.each()).flat_map(|library| library.kinds.values());
        let rules = self.mosaics.iter().map(|mosaic| &mosaic.rules)
            .chain(kinds().map(|kind| &kind.rules))
            .flatten()
            .map(|rule| &rule.handle);
        let sprites = kinds().flat_map(|kind| &kind.sprites).map(|sprite| &sprite.handle);
        let ores = self.ores.each().into_iter().flat_map(|library| library.sprites_by_category.values().flatten());
        rules.chain(sprites).chain(ores.map(|ore| &ore.handle)).chain(self.backgrounds.each())
            .all(|handle| images.contains(handle))
    }
}

/// Immutable asset-version snapshot shared by every logical and surface stage.
#[derive(Clone)]
pub(super) struct GenerationCatalog {
    pub libraries: GenerationLibraries,
    pub ldtk: ldtk_rules::LdtkRuleData,
}

#[derive(Resource, Default)]
pub(super) struct PreparedCatalog {
    pub version: u64,
    pub configuration: Option<LevelPngConfig>,
    pub data: Option<std::sync::Arc<GenerationCatalog>>,
}

pub(super) fn load_generation_libraries(
    mut libraries: ResMut<GenerationLibraries>,
    assets: Res<AssetServer>,
    cfg: Res<LevelPngConfig>,
) {
    let assets = assets.as_ref();
    let placements = |dir: &str| placement_library(assets, dir);
    let (ice_entities, tundra_entities) = (&cfg.entities_dir, &cfg.tundra_entities_dir);
    *libraries = GenerationLibraries {
        mosaics: cfg.mosaic_dirs.each_ref().map(|dir| mosaic_rules(assets, &format!("{}/rules", dir.trim_matches('/')))),
        decorations: Biomes::load(&cfg.decorations_dir, &cfg.tundra_decorations_dir, placements),
        entities: Biomes::load(ice_entities, tundra_entities, placements),
        ores: Biomes {
            ice: ore_library(assets, ice_entities.trim_matches('/'), &OreCategory::ICE),
            tundra: ore_library(assets, tundra_entities.trim_matches('/'), &OreCategory::TUNDRA),
        },
        backgrounds: Biomes::load(&cfg.near_bg_dir, &cfg.tundra_near_bg_dir, |dir| assets.load(format!("{dir}/exemplar.png"))),
    };
}

pub(super) fn load_ldtk_rules(
    mut ldtk_data: ResMut<ldtk_rules::LdtkRuleData>,
    mut build: ResMut<pipeline::GenerationBuild>,
    assets: Res<AssetServer>,
    cfg: Res<LevelPngConfig>,
) {
    if ldtk_data.loaded {
        return;
    }
    match ldtk_rules::load_ldtk_rules(&Path::new("assets").join(&cfg.ldtk_rules_path), &assets) {
        Ok(data) => *ldtk_data = data,
        Err(error) => return build.fail(format!("Failed to load LDtk rules: {error}")),
    }
    for (name, layer) in &ldtk_data.layers {
        let rules: usize = layer.rule_groups.iter().map(|group| group.rules.len()).sum();
        if rules > 0 {
            info!("LDtk layer '{name}': {} rule groups, {rules} rules", layer.rule_groups.len());
        }
    }
}

/// Drops the snapshot and decoded masks when a level image changes on disk.
pub(super) fn invalidate_catalog(
    mut events: MessageReader<bevy::asset::AssetEvent<Image>>,
    mut catalog: ResMut<PreparedCatalog>,
    assets: Res<AssetServer>,
    mut libraries: ResMut<GenerationLibraries>,
) {
    let dirty = events.read().any(|event| match event {
        bevy::asset::AssetEvent::Modified { id } | bevy::asset::AssetEvent::Removed { id } =>
            assets.get_path(*id).is_some_and(|path| path.path().starts_with("levels")),
        _ => false,
    });
    if !dirty {
        return;
    }
    catalog.version += 1;
    catalog.data = None;
    for rules in &mut libraries.mosaics {
        rules.decoded.take();
    }
}

/// Kinds under `root`, each a directory of `rules…` directories and a `sprites` directory; a kind
/// missing either is skipped.
fn placement_library(assets: &AssetServer, root: &str) -> PlacementLibrary {
    let kinds = subdirectories(root).filter_map(|kind| {
        let dir = format!("{root}/{kind}");
        let mut rules: Vec<_> = subdirectories(&dir)
            .filter(|name| name.to_ascii_lowercase().starts_with("rules"))
            .flat_map(|name| rule_images(&format!("{dir}/{name}"), |path| assets.load(path.to_owned())))
            .collect();
        rules.sort_by_key(|rule| rule.order);
        let sprites: Vec<_> = sorted_png_files(&format!("{dir}/sprites")).iter().map(|file| SpriteImage::load(assets, file)).collect();
        debug!("levelgen: {dir} has {} rules and {} sprites", rules.len(), sprites.len());
        (!rules.is_empty() && !sprites.is_empty()).then(|| (kind, PlacementKind { rules, sprites }))
    });
    PlacementLibrary { kinds: kinds.collect() }
}

fn ore_library(assets: &AssetServer, root: &str, categories: &[OreCategory]) -> OreLibrary {
    let sprites_by_category = categories.iter().filter_map(|&category| {
        let files = sorted_png_files(&format!("{root}/{}", category.dir_name()));
        let sprites: Vec<_> = files.iter().filter_map(|file| ore_image(assets, file, category)).collect();
        (!sprites.is_empty()).then_some((category, sprites))
    });
    OreLibrary { sprites_by_category: sprites_by_category.collect() }
}

fn ore_image(assets: &AssetServer, file: &PngFile, category: OreCategory) -> Option<OreImage> {
    let definition = harvestable_materials::parse_harvest_ore_from_sprite_name(&file.stem)
        .filter(|definition| definition.category == category.number());
    let Some(definition) = definition else {
        debug!("levelgen: {} is not a {} image", file.path, category.dir_name());
        return None;
    };
    Some(OreImage { handle: assets.load(&file.path), definition })
}

fn mosaic_rules(assets: &AssetServer, dir: &str) -> MosaicRules {
    MosaicRules { rules: rule_images(dir, |path| assets.load(path.to_owned())), decoded: OnceLock::new() }
}

/// A terrain's three mosaic layers, decoded straight into `images` for tests.
#[cfg(test)]
pub(super) fn load_test_mosaics(images: &mut Assets<Image>, dir: &str) -> [MosaicRules; 3] {
    let mut load = |path: &str| images.add(mapgen::load_png(&Path::new("assets").join(path)).expect("mosaic rule image"));
    std::array::from_fn(|index| MosaicRules {
        rules: rule_images(&format!("{dir}/mosaic{}/rules", index + 1), &mut load),
        decoded: OnceLock::new(),
    })
}

/// A directory's rule images, in application order.
fn rule_images(dir: &str, mut load: impl FnMut(&str) -> Handle<Image>) -> Vec<RuleImage> {
    let directory = dir.rsplit('/').next().unwrap_or(dir);
    let rule = |file: &PngFile| RuleImage::named(&file.stem, directory).map(|rule| RuleImage { handle: load(&file.path), ..rule });
    let mut rules: Vec<_> = png_files(dir).iter().filter_map(rule).collect();
    rules.sort_by_key(|rule| rule.order);
    rules
}

struct PngFile {
    name: String,
    stem: String,
    /// The asset path.
    path: String,
}

/// The PNG files in an asset directory, in directory order.
fn png_files(dir: &str) -> Vec<PngFile> {
    asset_directory(dir).filter_map(|entry| {
        let path = entry.path();
        let name = path.file_name()?.to_str()?.to_owned();
        let stem = path.file_stem()?.to_str()?.to_owned();
        let png = path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("png"));
        png.then(|| PngFile { path: format!("{dir}/{name}"), name, stem })
    }).collect()
}

fn sorted_png_files(dir: &str) -> Vec<PngFile> {
    let mut files = png_files(dir);
    files.sort_by(|a, b| a.name.cmp(&b.name));
    files
}

fn subdirectories(dir: &str) -> impl Iterator<Item = String> {
    asset_directory(dir)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
}

fn asset_directory(dir: &str) -> impl Iterator<Item = fs::DirEntry> {
    fs::read_dir(Path::new("assets").join(dir))
        .inspect_err(|error| debug!("levelgen: cannot read assets/{dir} ({error})"))
        .into_iter()
        .flatten()
        .flatten()
}

fn size_tags(name: &str) -> Vec<&'static str> {
    SIZE_TAGS.into_iter().filter(|tag| name.contains(tag)).collect()
}

fn leading(text: &str, keep: fn(&char) -> bool) -> &str {
    &text[..text.find(|character| !keep(&character)).unwrap_or(text.len())]
}

fn after<'a>(name: &'a str, marker: &str) -> Option<&'a str> {
    name.find(marker).map(|start| &name[start + marker.len()..])
}

fn number_after(name: &str, marker: &str) -> Option<u32> {
    leading(after(name, marker)?, char::is_ascii_digit).parse().ok()
}

fn signed_after(name: &str, marker: &str) -> Option<i32> {
    let tail = after(name, marker)?;
    let (sign, digits) = tail.strip_prefix('+').map(|digits| (1, digits))
        .or_else(|| tail.strip_prefix('-').map(|digits| (-1, digits)))?;
    leading(digits, char::is_ascii_digit).parse::<i32>().ok().map(|value| sign * value)
}

fn color_after(name: &str, marker: &str) -> Option<[u8; 3]> {
    let tail = after(name, marker)?;
    let hex = leading(tail.strip_prefix('#').unwrap_or(tail), char::is_ascii_hexdigit);
    let channel = |index: usize| u8::from_str_radix(hex.get(index * 2..index * 2 + 2)?, 16).ok();
    Some([channel(0)?, channel(1)?, channel(2)?])
}
