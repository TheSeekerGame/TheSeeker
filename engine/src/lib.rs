pub mod prelude {
    pub use anyhow::{anyhow, bail, ensure, Context, Error as AnyError, Result as AnyResult};
    pub use bevy::prelude::*;
    pub use bevy::utils::{Duration, HashMap, HashSet, Instant};
    pub use bevy_asset_loader::prelude::*;
    pub use bevy_ecs_ldtk::prelude::*;
    pub use bevy_ecs_tilemap::prelude::*;
    pub use bevy_xpbd_2d::prelude::*;
    pub use iyes_bevy_extras::prelude::*;
    pub use iyes_cli::prelude::*;
    pub use iyes_progress::prelude::*;
    pub use iyes_ui::prelude::*;
    pub use rand::prelude::*;
    pub use serde::de::DeserializeOwned;
    pub use serde::{Deserialize, Serialize};
    pub use serde_with::{serde_as, DeserializeFromStr, SerializeDisplay};
    pub use thiserror::Error;

    pub use crate::assets::{AssetKey, AssetsSet, PreloadedAssets};
    pub use crate::time::{
        at_tick_multiples, GameTickMidFlush, GameTickSet, GameTickUpdate, GameTime, TickQuant,
    };
}

pub mod assets;
pub mod time;
