use bevy::reflect::{TypePath, TypeUuid};

use super::script::*;
use crate::prelude::*;
use crate::data::*;

/// Sprite Animation Asset type
///
/// Would typically be loaded from TOML files.
#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
#[derive(TypeUuid, TypePath)]
#[uuid = "6D201246-BDB4-4803-A52A-76D95E3B6C77"]
pub struct SpriteAnimation {
    /// The Dynamic Asset key of the texture atlas asset to use
    pub atlas_asset_key: String,
    /// General animation parameters
    pub settings: ExtendedScriptSettings<SpriteAnimationSettings>,
    /// Optional "script": list of actions to perform during playback
    pub script: Vec<ExtendedScript<SpriteAnimationScriptRunIf, SpriteAnimationScriptAction>>,
}

#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
pub struct SpriteAnimationSettings {
    pub ticks_per_frame: u32,
    pub frame_start: u32,
    pub frame_min: u32,
    pub frame_max: u32,
}

#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
pub enum SpriteAnimationScriptRunIf {
    #[serde(rename = "run_at_frame")]
    Frame(u32),
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
    },
    /// Immediately change to the given frame, without waiting for `ticks_per_frame`
    SetFrameNow {
        /// The frame index
        frame_index: u32,
    },
    /// Change the next frame to be displayed, after `ticks_per_frame` elapses.
    SetFrameNext {
        /// The frame index
        frame_index: u32,
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
}
