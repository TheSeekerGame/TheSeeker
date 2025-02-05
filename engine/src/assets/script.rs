use bevy::reflect::TypePath;

use super::config::DynamicConfigValue;
use crate::data::*;
use crate::prelude::*;

/// Scripted Sequence Asset type
///
/// Would typically be loaded from TOML files.
#[derive(Asset, Debug, Clone)]
#[derive(Serialize, Deserialize)]
#[derive(TypePath)]
pub struct Script {
    /// Any customization configs
    #[serde(default)]
    pub config: ScriptConfig,
    /// Settings for the script runtime
    pub settings: Option<CommonScriptSettings>,
    /// List of actions to perform during playback
    #[serde(default)]
    pub script: Vec<CommonScript>,
}

#[derive(Debug, Default, Clone)]
#[derive(Serialize, Deserialize)]
pub struct CommonScriptSettings {
    #[serde(default)]
    pub time_base: TimeBase,
    pub tick_quant: Option<ScriptTickQuant>,
}

/// From what point does a script count time (when is time/tick 0)?
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
#[derive(Component, Serialize, Deserialize)]
pub enum TimeBase {
    /// Script time counts from the moment of script init.
    /// This is the default and most common behavior for typical scripts/animations.
    #[default]
    Relative,
    /// Script time counts from when the level was loaded.
    Level,
    /// Script time counts from app startup.
    Startup,
}

/// When initting a script, quantize time (from TimeBase).
#[derive(Debug, Clone, Default)]
#[derive(Serialize, Deserialize)]
#[serde(transparent)]
pub struct ScriptConfig(pub HashMap<String, DynamicConfigValue>);

/// When initting a script, quantize time (from TimeBase).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[derive(Component, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ScriptTickQuant(pub Quant);

#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
pub struct CommonScript {
    #[serde(flatten)]
    pub params: CommonScriptParams,
    #[serde(flatten)]
    pub run_if: CommonScriptRunIf,
    #[serde(flatten)]
    pub action: CommonScriptAction,
}

#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
pub struct CommonScriptParams {
    pub rng_pct: Option<f32>,
    pub if_previous_script_key: Option<String>,
    pub delay_ticks: Option<u32>,
    #[serde(default)]
    pub require_slots_all: Vec<String>,
    #[serde(default)]
    pub require_slots_any: Vec<String>,
    #[serde(default)]
    pub forbid_slots_all: Vec<String>,
    #[serde(default)]
    pub forbid_slots_any: Vec<String>,
    pub if_runcount_is: Option<OneOrMany<u32>>,
    pub if_runcount_is_not: Option<OneOrMany<u32>>,
    pub if_runcount_lt: Option<u32>,
    pub if_runcount_le: Option<u32>,
    pub if_runcount_gt: Option<u32>,
    pub if_runcount_ge: Option<u32>,
    pub if_runcount_quant: Option<Quant>,
}

#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
pub enum CommonScriptRunIf {
    #[serde(rename = "run_at_tick")]
    Tick(OneOrMany<u64>),
    #[serde(rename = "run_every_n_ticks")]
    TickQuant(Quant),
    #[serde(rename = "run_at_time")]
    Time(OneOrMany<TimeSpec>),
    #[serde(rename = "run_at_millis")]
    Millis(OneOrMany<u64>),
    #[serde(rename = "run_on_slot_enable")]
    SlotEnable(String),
    #[serde(rename = "run_on_slot_disable")]
    SlotDisable(String),
    #[serde(rename = "run_on_playback_control")]
    PlaybackControl(PlaybackControl),
}

#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
pub enum PlaybackControl {
    Start,
    Stop,
}

/// The various actions that can be performed from scripts
#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum CommonScriptAction {
    /// Run `CliCommand`s
    RunCli {
        /// A list of cli command strings to evaluate
        cli: Vec<String>,
    },
    /// Despawn entities
    DespawnEntity {
        /// If specified, lookup entities with the given label.
        /// If unspecified, despawn ourselves.
        label: Option<String>,
    },
    /// Spawn a Bevy Scene asset
    SpawnScene {
        /// The dynamic asset key of the scene asset to spawn
        asset_key: String,
        /// If true, spawn it as a child under another entity.
        /// If false, spawn it independently (standalone).
        #[serde(default)]
        as_child: bool,
        /// If `as_child` is true, optionally specify another entity (by label)
        /// to use as the parent. If unspecified, use the current entity.
        parent_label: Option<String>,
    },
    /// Spawn a new entity to run a script
    SpawnScript { asset_key: String },
    /// Enable a Slot
    SlotEnable { slot: String },
    /// Disable a Slot
    SlotDisable { slot: String },
    /// Toggle a Slot
    SlotToggle { slot: String },
    /// Play a sound (precise timing based on action's trigger condition)
    PlayAudio {
        asset_key: String,
        label: Option<String>,
        volume: Option<f32>,
        pan: Option<f32>,
    },
    /// Stop sounds that are currently playing
    StopAudio {
        /// If true (default), only apply to sounds started by the current script.
        /// If false, apply to sounds started from anywhere.
        current_script_only: Option<bool>,
        /// Only stop sounds with the given label. If unset, stop all sounds.
        label: Option<String>,
    },
    /// Play a sound (using regular bevy audio, not our precise system)
    PlayBackgroundAudio {
        asset_key: String,
        label: Option<String>,
        volume: Option<f32>,
        r#loop: Option<bool>,
    },
    /// Stop sounds that are currently playing
    StopBackgroundAudio {
        /// If true (default), only apply to sounds started by the current script.
        /// If false, apply to sounds started from anywhere.
        current_script_only: Option<bool>,
        /// Only stop sounds with the given label. If unset, stop all sounds.
        label: Option<String>,
    },
}

#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
#[serde(from = "ExtendedScriptWorkaround<ExtParams, ExtRunIf, ExtAction>")]
pub struct ExtendedScript<ExtParams, ExtRunIf, ExtAction> {
    #[serde(flatten)]
    pub params: ExtendedScriptParams<ExtParams>,
    #[serde(flatten)]
    pub run_if: ExtendedScriptRunIf<ExtRunIf>,
    #[serde(flatten)]
    pub action: ExtendedScriptAction<ExtAction>,
}

#[derive(Debug, Clone, Default)]
#[derive(Serialize, Deserialize)]
pub struct ExtendedScriptSettings<T> {
    #[serde(flatten)]
    pub extended: T,
    #[serde(flatten)]
    pub common: CommonScriptSettings,
}

#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
pub struct ExtendedScriptParams<T> {
    #[serde(flatten)]
    pub extended: T,
    #[serde(flatten)]
    pub common: CommonScriptParams,
}

#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExtendedScriptRunIf<ExtRunIf> {
    Extended(ExtRunIf),
    Common(CommonScriptRunIf),
}

#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExtendedScriptAction<ExtAction> {
    Extended(ExtAction),
    Common(CommonScriptAction),
}

#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
pub struct Flattened<A, B> {
    #[serde(flatten)]
    a: A,
    #[serde(flatten)]
    b: B,
}

#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
pub struct ExtendedScriptWorkaround<ExtParams, ExtRunIf, ExtAction> {
    #[serde(flatten)]
    pub params: ExtendedScriptParams<ExtParams>,
    #[serde(flatten)]
    pub inner: ExtendedScriptWorkaroundInner<ExtRunIf, ExtAction>,
}

#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExtendedScriptWorkaroundInner<ExtRunIf, ExtAction> {
    EE(Flattened<ExtRunIf, ExtAction>),
    EC(Flattened<ExtRunIf, CommonScriptAction>),
    CE(Flattened<CommonScriptRunIf, ExtAction>),
    CC(Flattened<CommonScriptRunIf, CommonScriptAction>),
}

impl<ExtParams, ExtRunIf, ExtAction>
    From<ExtendedScriptWorkaround<ExtParams, ExtRunIf, ExtAction>>
    for ExtendedScript<ExtParams, ExtRunIf, ExtAction>
{
    fn from(
        wa: ExtendedScriptWorkaround<ExtParams, ExtRunIf, ExtAction>,
    ) -> ExtendedScript<ExtParams, ExtRunIf, ExtAction> {
        match wa.inner {
            ExtendedScriptWorkaroundInner::EE(x) => {
                ExtendedScript {
                    params: wa.params,
                    run_if: ExtendedScriptRunIf::Extended(x.a),
                    action: ExtendedScriptAction::Extended(x.b),
                }
            },
            ExtendedScriptWorkaroundInner::EC(x) => {
                ExtendedScript {
                    params: wa.params,
                    run_if: ExtendedScriptRunIf::Extended(x.a),
                    action: ExtendedScriptAction::Common(x.b),
                }
            },
            ExtendedScriptWorkaroundInner::CE(x) => {
                ExtendedScript {
                    params: wa.params,
                    run_if: ExtendedScriptRunIf::Common(x.a),
                    action: ExtendedScriptAction::Extended(x.b),
                }
            },
            ExtendedScriptWorkaroundInner::CC(x) => {
                ExtendedScript {
                    params: wa.params,
                    run_if: ExtendedScriptRunIf::Common(x.a),
                    action: ExtendedScriptAction::Common(x.b),
                }
            },
        }
    }
}
