use bevy::reflect::TypePath;

use super::script::*;
use crate::data::*;
use crate::prelude::*;

/// Sprite Animation Asset type
///
/// Would typically be loaded from TOML files.
#[derive(Asset, Debug, Clone)]
#[derive(Serialize, Deserialize)]
#[derive(TypePath)]
pub struct SpriteAnimation {
    #[serde(default)]
    pub playback: Option<ClipSection>,
    #[serde(default)]
    pub sections: HashMap<String, ClipSection>,
    /// Any customization configs
    #[serde(default)]
    pub config: ScriptConfig,
    /// General animation parameters
    pub settings: ExtendedScriptSettings<SpriteAnimationSettings>,
    /// Optional frame bookmarks to help during scripting
    #[serde(default)]
    pub frame_bookmarks: HashMap<String, FrameId>,
    /// Optional gameplay events emitted by the animation timeline.
    #[serde(default)]
    pub events: Vec<SpriteAnimationEvent>,
    /// Optional movement impulses emitted when specific frames are entered.
    #[serde(default)]
    pub frame_impulse: Vec<SpriteAnimationFrameImpulse>,
    /// Optional "script": list of actions to perform during playback
    #[serde(default)]
    pub script: Vec<
        ExtendedScript<
            SpriteAnimationScriptParams,
            SpriteAnimationScriptRunIf,
            SpriteAnimationScriptAction,
        >,
    >,
}

/// A bounded sprite clip with an explicit end policy.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ClipSection {
    pub start: FrameId,
    pub end: FrameId,
    pub ticks_per_frame: u32,
    pub loop_start: Option<FrameId>,
    #[serde(default)]
    pub mode: ClipMode,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipMode {
    #[default]
    Once,
    Loop,
    Hold,
}

impl ClipSection {
    pub fn validate(&self) -> Result<(), String> {
        if self.start.0 == 0 || self.start > self.end || self.ticks_per_frame == 0 {
            return Err("clip requires positive frames/ticks and start <= end".to_owned());
        }
        if self.loop_start.is_some_and(|frame| frame < self.start || frame > self.end) {
            return Err("loop_start must lie inside the section".to_owned());
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
pub struct SpriteAnimationSettings {
    pub atlas_asset_key: Option<String>,
    pub image_asset_key: Option<String>,
    pub ticks_per_frame: u32,
    /// None inherits the application's animation clock policy.
    #[serde(default)]
    pub frame_clock: Option<FrameClock>,
    pub frame_start: FrameId,
    pub frame_min: FrameId,
    pub frame_max: FrameId,
    #[serde(default)]
    pub play_reversed: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FrameClock { #[default] Local, Global }

#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct AnimationClock(pub FrameClock);

#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
pub struct SpriteAnimationEvent {
    pub on: SpriteAnimationEventTrigger,
    pub emit: String,
}

#[derive(Debug, Clone, Default)]
#[derive(Serialize, Deserialize)]
pub struct SpriteAnimationEventTrigger {
    pub frame_enter: Option<FrameId>,
    pub frame_exit: Option<FrameId>,
    pub bookmark: Option<String>,
    #[serde(default)]
    pub anim_finished: bool,
}

#[derive(Debug, Clone, Copy)]
#[derive(Serialize, Deserialize)]
pub struct SpriteAnimationFrameImpulse {
    pub frame: FrameId,
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub mode: FrameImpulseMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum FrameImpulseMode {
    #[default]
    OnceOnEnter,
}


#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
pub struct SpriteAnimationScriptParams {
    pub frame_bookmark: Option<String>,
    pub if_frame_lt: Option<FrameIndexOrBookmark>,
    pub if_frame_le: Option<FrameIndexOrBookmark>,
    pub if_frame_gt: Option<FrameIndexOrBookmark>,
    pub if_frame_ge: Option<FrameIndexOrBookmark>,
    pub if_frame_is: Option<OneOrMany<FrameIndexOrBookmark>>,
    pub if_frame_is_not: Option<OneOrMany<FrameIndexOrBookmark>>,
    pub if_oldanim_frame_lt: Option<FrameId>,
    pub if_oldanim_frame_le: Option<FrameId>,
    pub if_oldanim_frame_gt: Option<FrameId>,
    pub if_oldanim_frame_ge: Option<FrameId>,
    pub if_oldanim_frame_was: Option<OneOrMany<FrameId>>,
    pub if_oldanim_frame_was_not: Option<OneOrMany<FrameId>>,
    pub if_playing_reversed: Option<bool>,
}

#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
pub enum SpriteAnimationScriptRunIf {
    #[serde(rename = "run_at_frame")]
    Frame(OneOrMany<FrameIndexOrBookmark>),
    #[serde(rename = "run_every_n_frames")]
    FrameQuant(Quant),
}

/// The various actions that can be performed from an animation script
#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum SpriteAnimationScriptAction {
    /// Change playback speed
    SetTicksPerFrame {
        /// The new frame rate
        ticks_per_frame: u32,
        /// Whether the current progress should be hard reset to the new tick rate
        reset_progress: Option<bool>,
    },
    /// Immediately change to the given frame, without waiting for `ticks_per_frame`
    SetFrameNow {
        /// Use this bookmark
        to_frame_bookmark: Option<String>,
        /// The frame index (can be a single value or a list for mapping)
        frame_index: Option<OneOrMany<FrameId>>,
    },
    /// Change the next frame to be displayed, after `ticks_per_frame` elapses.
    SetFrameNext {
        /// Use this bookmark
        to_frame_bookmark: Option<String>,
        /// The frame index (can be a single value or a list for mapping)
        frame_index: Option<OneOrMany<FrameId>>,
    },
    /// Set sprite colorization
    SetSpriteColor {
        /// The new sprite color
        color: ColorRepr,
    },
    /// Set sprite X/Y flip
    SetSpriteFlip {
        /// Set flip on the X axis
        flip_x: Option<bool>,
        /// Set flip on the Y axis
        flip_y: Option<bool>,
    },
    /// Reverse Playback
    ReversePlayback {
        /// Set a specific direction.
        /// If omitted, toggles the current direction.
        reversed: Option<bool>,
    },
    /// Transform: relative translation
    TransformMove {
        x: Option<Frac>,
        y: Option<Frac>,
        z: Option<Frac>,
    },
    /// Transform: set absolute translation
    TransformTeleport { x: Frac, y: Frac, z: Option<Frac> },
    /// Transform: rotate by N turns (1 turn = 360 degrees)
    TransformRotateTurns { turns: Frac },
    /// Transform: rotate by N degrees
    TransformRotateDegrees { degrees: Frac },
    /// Transform: set the rotation to a specific value
    TransformSetRotationTurns { turns: Frac },
    /// Transform: set the rotation to a specific value
    TransformSetRotationDegrees { degrees: Frac },
    /// Transform: set scale
    TransformSetScale { x: Frac, y: Frac },
}

#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum FrameIndexOrBookmark {
    Index(FrameId),
    Bookmark(String),
}

impl SpriteAnimation {
    pub fn resolve_image_atlas(
        &self,
        preloaded: &PreloadedAssets,
        anim_key: Option<&str>,
    ) -> Option<(
        Handle<Image>,
        Handle<TextureAtlasLayout>,
    )> {
        let mut default_image_key;
        let image_key =
            if let Some(key) = &self.settings.extended.image_asset_key {
                key
            } else {
                default_image_key = anim_key?.to_owned();
                default_image_key.push_str(".image");
                &default_image_key
            };
        let mut default_layout_key;
        let layout_key =
            if let Some(key) = &self.settings.extended.atlas_asset_key {
                key
            } else {
                default_layout_key = anim_key?.to_owned();
                default_layout_key.push_str(".atlas");
                &default_layout_key
            };
        Some((
            preloaded.get_single_asset(image_key)?,
            preloaded.get_single_asset(layout_key)?,
        ))
    }
}

/// Because c12 wants 1-based indexing
/// and i want to use the type system to ensure no off-by-1 errors
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[derive(Serialize, Deserialize)]
#[serde(transparent)]
pub struct FrameId(pub u32);

impl FrameId {
    pub fn as_sprite_index(self) -> usize {
        debug_assert!(self.0 != 0);
        (self.0 - 1) as usize
    }

    pub fn from_sprite_index(i: usize) -> Self {
        FrameId(i as u32 + 1)
    }
}

impl Default for FrameId {
    fn default() -> Self {
        FrameId(1)
    }
}

impl std::ops::Add<FrameId> for FrameId {
    type Output = FrameId;

    fn add(self, rhs: FrameId) -> Self::Output {
        FrameId(self.0 + rhs.0 - 1)
    }
}

impl std::ops::Add<u32> for FrameId {
    type Output = FrameId;

    fn add(self, rhs: u32) -> Self::Output {
        FrameId(self.0 + rhs)
    }
}

impl std::ops::Sub<FrameId> for FrameId {
    type Output = FrameId;

    fn sub(self, rhs: FrameId) -> Self::Output {
        FrameId(self.0 - (rhs.0 - 1))
    }
}

impl std::ops::Sub<u32> for FrameId {
    type Output = FrameId;

    fn sub(self, rhs: u32) -> Self::Output {
        FrameId(self.0 - rhs)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationPngMetadata {
    pub base_name: String,
    pub rows: u32,
    pub columns: u32,
    pub cell_width: u32,
    pub cell_height: u32,
}

/// Parse atlas defaults from `Name__r1c5__36x36`.
pub fn parse_animation_png_metadata(stem: &str) -> Option<AnimationPngMetadata> {
    let (prefix, size) = stem.rsplit_once("__")?;
    let (base_name, grid) = prefix.rsplit_once("__")?;
    let grid = grid.strip_prefix('r')?;
    let (rows, columns) = grid.split_once('c')?;
    let (cell_width, cell_height) = size.split_once('x')?;

    let rows = rows.parse().ok()?;
    let columns = columns.parse().ok()?;
    let cell_width = cell_width.parse().ok()?;
    let cell_height = cell_height.parse().ok()?;

    if base_name.is_empty()
        || rows == 0
        || columns == 0
        || cell_width == 0
        || cell_height == 0
    {
        return None;
    }

    Some(AnimationPngMetadata {
        base_name: base_name.to_string(),
        rows,
        columns,
        cell_width,
        cell_height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_generated_animation_png_metadata() {
        let parsed =
            parse_animation_png_metadata("Fly__r1c5__36x36").unwrap();

        assert_eq!(parsed.base_name, "Fly");
        assert_eq!(parsed.rows, 1);
        assert_eq!(parsed.columns, 5);
        assert_eq!(parsed.cell_width, 36);
        assert_eq!(parsed.cell_height, 36);
    }

    #[test]
    fn parses_base_names_with_underscores() {
        let parsed =
            parse_animation_png_metadata("Spider_Big_Walk__r2c8__64x32")
                .unwrap();

        assert_eq!(parsed.base_name, "Spider_Big_Walk");
        assert_eq!(parsed.rows, 2);
        assert_eq!(parsed.columns, 8);
        assert_eq!(parsed.cell_width, 64);
        assert_eq!(parsed.cell_height, 32);
    }

    #[test]
    fn rejects_ambiguous_or_empty_metadata() {
        assert!(parse_animation_png_metadata("Fly_r1c5_36x36").is_none());
        assert!(parse_animation_png_metadata("Fly__r0c5__36x36").is_none());
        assert!(parse_animation_png_metadata("Fly__r1c5__0x36").is_none());
    }

    fn asset_source_section(source: &str) -> Option<String> {
        let asset_start = source.find("[asset]")?;
        let asset_source = &source[asset_start..];
        let asset_end = asset_source
            .lines()
            .enumerate()
            .skip(1)
            .find_map(|(line, text)| text.starts_with('[').then_some(line))
            .unwrap_or_else(|| asset_source.lines().count());
        Some(
            asset_source
                .lines()
                .take(asset_end)
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }

    fn assert_asset_image_metadata(path: &std::path::Path) -> usize {
        let source = std::fs::read_to_string(path).unwrap();
        let Some(asset_source) = asset_source_section(&source) else {
            return 0;
        };
        let _: SpriteAnimation = toml::from_str(&source).unwrap();
        let value: toml::Value = toml::from_str(&asset_source).unwrap();
        let asset = value.get("asset").unwrap().as_table().unwrap();
        let keys = asset_key_count(asset);
        let image = asset.get("image").and_then(|image| image.as_str()).unwrap();
        let image_path = std::path::Path::new("assets").join(image);
        assert!(image_path.exists(), "missing image {image}");
        let stem = image_path.file_stem().unwrap().to_str().unwrap();
        assert!(
            parse_animation_png_metadata(stem).is_some(),
            "image lacks atlas metadata: {image}"
        );
        keys
    }

    fn asset_key_count(asset: &toml::map::Map<String, toml::Value>) -> usize {
        if let Some(keys) = asset.get("keys").and_then(|keys| keys.as_array()) {
            return keys.len();
        }

        asset
            .get("key")
            .and_then(|key| key.as_str())
            .map_or(0, |_| 1)
    }

    fn is_animation_definition(path: &std::path::Path) -> bool {
        let Some(file_name) = path.file_name().and_then(|name| name.to_str())
        else {
            return false;
        };

        file_name.ends_with(".anim.toml")
    }

    fn visit_animation_definitions(dir: &std::path::Path, key_count: &mut usize) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            visit_animation_definition_path(&entry.path(), key_count);
        }
    }

    fn visit_animation_definition_path(
        path: &std::path::Path,
        key_count: &mut usize,
    ) {
        if path.is_dir() {
            visit_animation_definitions(path, key_count);
            return;
        }
        if !is_animation_definition(path) {
            return;
        }

        *key_count += assert_asset_image_metadata(path);
    }

    #[test]
    fn registered_animation_images_use_png_metadata() {
        let root = std::path::Path::new("assets/animations");
        if !root.exists() {
            return;
        }

        let mut key_count = 0;
        visit_animation_definitions(root, &mut key_count);
        assert!(key_count >= 150);
    }
}
