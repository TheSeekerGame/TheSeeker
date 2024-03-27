use bevy::reflect::{TypePath, TypeUuid};

use super::script::*;
use crate::data::*;
use crate::prelude::*;

/// Sprite Animation Asset type
///
/// Would typically be loaded from TOML files.
#[derive(Asset, Debug, Clone)]
#[derive(Serialize, Deserialize)]
#[derive(TypeUuid, TypePath)]
#[uuid = "6D201246-BDB4-4803-A52A-76D95E3B6C77"]
pub struct SpriteAnimation {
    /// General animation parameters
    pub settings: ExtendedScriptSettings<SpriteAnimationSettings>,
    /// Optional "script": list of actions to perform during playback
    pub script: Vec<
        ExtendedScript<
            SpriteAnimationScriptParams,
            SpriteAnimationScriptRunIf,
            SpriteAnimationScriptAction,
        >,
    >,
}

#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
pub struct SpriteAnimationSettings {
    pub atlas_asset_key: Option<String>,
    pub image_asset_key: Option<String>,
    pub ticks_per_frame: u32,
    pub frame_start: u32,
    pub frame_min: u32,
    pub frame_max: u32,
}

#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
pub struct SpriteAnimationScriptParams {}

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
