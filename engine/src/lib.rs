#![allow(unused_mut)]

pub mod prelude {
    pub use std::sync::Arc;
    pub use std::time::{Duration, Instant};
    pub use std::collections::{HashMap, HashSet};

    pub use anyhow::{
        anyhow, bail, ensure, Context, Error as AnyError, Result as AnyResult,
    };
    pub use bevy::prelude::*;
    pub use bevy_asset_loader::prelude::*;
    pub use bevy_ecs_ldtk::prelude::*;
    pub use rand::prelude::*;
    pub use serde::de::DeserializeOwned;
    pub use serde::{Deserialize, Serialize};
    pub use serde_with::{serde_as, DeserializeFromStr, SerializeDisplay};
    pub use thiserror::Error;

    pub use crate::assets::PreloadedAssets;
    pub use crate::condition::*;
    pub use crate::data::Quant;
    pub use crate::time::{
        at_tick_multiples, GameTickEventClearSet, GameTickSet, GameTickUpdate,
        GameTime, GameTimeAppExt,
    };
}

use bevy::app::PluginGroupBuilder;

use crate::prelude::*;

pub mod ai;
pub mod animation;
pub mod assets;
pub mod audio;
pub mod ballistics_math;
pub mod condition;
pub mod data;
pub mod gent;
pub mod input;
pub mod physics;
pub mod script;
pub mod time;

pub struct EnginePlugins;

impl PluginGroup for EnginePlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(crate::time::GameTimePlugin)
            .add(crate::script::ScriptPlugin)
            .add(crate::animation::SpriteAnimationPlugin)
            .add(crate::audio::AudioPlugin)
            .add(crate::gent::GentPlugin)
            .add(crate::ai::AiPlugin)
    }
}
