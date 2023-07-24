/// Custom prelude, for stuff we'd like to access all over the codebase
/// Use in every file. :)
mod prelude {
    pub use anyhow::{anyhow, bail, ensure, Context, Error as AnyError, Result as AnyResult};
    pub use bevy::prelude::*;
    pub use bevy::utils::{Duration, HashMap, HashSet, Instant};
    pub use bevy_ecs_ldtk::prelude::*;
    pub use bevy_ecs_tilemap::prelude::*;
    pub use iyes_bevy_extras::prelude::*;
    pub use iyes_cli::prelude::*;
    pub use iyes_progress::prelude::*;
    pub use iyes_ui::prelude::*;
    pub use rand::prelude::*;
    pub use serde::de::DeserializeOwned;
    pub use serde::{Deserialize, Serialize};
    pub use thiserror::Error;

    pub use crate::appstate::{AppState, StateDespawnMarker};
}

use crate::prelude::*;

mod appstate;
mod assets;
mod cli;
mod locale;
mod screens {
    pub mod loading;
}
mod ui;

fn main() {
    let mut app = App::new();
    app.insert_resource(ClearColor(Color::BLACK));

    let bevy_plugins = DefaultPlugins;
    let bevy_plugins = bevy_plugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "The Seeker (PRE-ALPHA)".into(),
            present_mode: bevy::window::PresentMode::Fifo,
            resizable: true,
            ..Default::default()
        }),
        ..Default::default()
    });
    let bevy_plugins = bevy_plugins.set(ImagePlugin::default_nearest());
    #[cfg(feature = "dev")]
    let bevy_plugins = bevy_plugins.set(bevy::asset::AssetPlugin {
        watch_for_changes: true,
        ..default()
    });
    #[cfg(feature = "dev")]
    let bevy_plugins = bevy_plugins.set(bevy::log::LogPlugin {
        filter: "info,wgpu_core=warn,wgpu_hal=warn,iyes_progress=trace,theseeker=trace".into(),
        level: bevy::log::Level::TRACE,
    });
    #[cfg(not(feature = "dev"))]
    let bevy_plugins = bevy_plugins.set(bevy::log::LogPlugin {
        filter: "info,wgpu_core=warn,wgpu_hal=warn,theseeker=info".into(),
        level: bevy::log::Level::INFO,
    });
    app.add_plugins(bevy_plugins);

    // configure our app states
    app.add_plugin(crate::appstate::AppStatesPlugin);

    // external plugins
    app.add_plugin(LdtkPlugin);
    app.add_plugin(bevy_tweening::TweeningPlugin);
    app.add_plugin(bevy_fluent::FluentPlugin);
    app.add_plugin(iyes_bevy_extras::d2::WorldCursorPlugin);
    app.add_plugin(
        ProgressPlugin::new(AppState::AssetsLoading)
            .track_assets()
            .continue_to(AppState::MainMenu),
    );
    #[cfg(feature = "dev")]
    app.add_system(
        debug_progress
            .run_if(resource_exists::<ProgressCounter>())
            .in_base_set(iyes_progress::ProgressSystemSet::CheckProgress),
    );

    // our stuff
    app.add_plugin(
        crate::screens::loading::LoadscreenPlugin {
            state: AppState::AssetsLoading,
        },
    );
    app.add_plugin(crate::assets::AssetsPlugin);
    app.add_plugin(crate::locale::LocalePlugin);
    app.add_plugin(crate::cli::CliPlugin);
    app.add_plugin(crate::ui::UiPlugin);

    // FIXME: temporary
    app.add_system(debug_setup_camera.in_schedule(OnEnter(AppState::MainMenu)));

    app.run();
}

#[allow(dead_code)]
fn debug_progress(counter: Res<ProgressCounter>) {
    let progress = counter.progress();
    let progress_full = counter.progress_complete();
    trace!(
        "Progress: {}/{}; Full Progress: {}/{}",
        progress.done,
        progress.total,
        progress_full.done,
        progress_full.total,
    );
}

/// Temporary function to use during development
///
/// If there is no proper code to set up a camera in a given app state (or whatever)
/// yet, use this to spawn a default 2d camera.
fn debug_setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera2dBundle::default(),
        StateDespawnMarker,
    ));
}
