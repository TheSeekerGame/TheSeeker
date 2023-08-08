use bevy::reflect::TypeUuid;

use crate::prelude::*;

/// How to count ticks/time during animation playback?
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[derive(Serialize, Deserialize)]
pub enum TickMode {
    /// Count starting from the tick when the animation began playing.
    ///
    /// This is the default and most common behavior for typical animations.
    #[default]
    Relative,
    /// Like relative, but quantize the starting tick.
    ///
    /// That is, ensure that the animation can only start on a tick that is
    /// a multiple of some value.
    RelativeQuantized(TickQuant),
    /// Global time, from when the player entered the level.
    /// This can be used for background animations that should play in lock-step.
    /// Imagine the "car turn signals" effect, or a polyrhythm. ;)
    Absolute,
}

/// State of animation playback: full, scripts only, or nothing
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[derive(Serialize, Deserialize)]
pub enum PauseMode {
    /// The animation plays normally: visuals update and scripts run
    #[default]
    Playing,
    /// Freeze the animation (visuals do not update), but continue counting ticks and running scripts
    Frozen,
    /// Pause playback completely: visuals do not update, time is not counted, and scripts do not run.
    NoScripts,
}

#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
pub struct SpriteAnimationSettings {
    pub ticks_per_frame: u32,
    pub tick_mode: TickMode,
    pub frame_index_start: u32,
    pub frame_index_end: u32,
}

/// An entry in the animation's script. Allows you to perform actions at specific times.
#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
pub enum SpriteAnimationAction {
    /// Perform an action on the given tick number.
    ///
    /// This respects the `TickMode` of the animation.
    AtTick {
        tick: u64,
        action: SpriteAnimationActionKind,
    },
    /// Perform an action when a given animation frame is displayed.
    AtFrame {
        frame_index: u32,
        action: SpriteAnimationActionKind,
    },
    /// Perform an action periodically.
    EveryNTicks {
        quant: TickQuant,
        action: SpriteAnimationActionKind,
    },
}

/// The various actions that can be performed from an animation script.
#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
pub enum SpriteAnimationActionKind {
    /// Stop/cancel the animation
    Stop,
    /// Change the playback speed
    SetTicksPerFrame { ticks_per_frame: u32 },
    /// Pause/unpause playback
    ///
    /// Use `duration_ticks` to auto-resume after a certain time.
    ///
    /// If you set `mode = Frozen`, the script keeps running, so you can also
    /// use a subsequent `SetPaused` action (with `mode = Playing`) to resume.
    /// This obviously wouldn't work if you set `mode = NoScripts`.
    SetPaused {
        mode: PauseMode,
        duration_ticks: Option<u32>,
    },
    /// Immediately change to a different animation frame, without waiting `ticks_per_frame`.
    SetFrameNow { frame_index: u32 },
    /// Change the next frame to be displayed, after the current `ticks_per_frame` elapses.
    SetFrameNext { frame_index: u32 },
    /// Change the colorization of the sprite
    SetSpriteColor { color: Color },
}

/// Sprite Animation Asset type
///
/// Would typically be loaded from RON files.
#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
#[derive(TypeUuid)]
#[uuid = "6D201246-BDB4-4803-A52A-76D95E3B6C77"]
pub struct SpriteAnimation {
    /// The Dynamic Asset key of the texture atlas asset to use
    pub atlas_asset_key: String,
    /// General animation parameters
    pub settings: SpriteAnimationSettings,
    /// Optional "script": list of actions to perform during playback
    pub script: Vec<SpriteAnimationAction>,
}
