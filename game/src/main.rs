/// Custom prelude, for stuff we'd like to access all over the codebase
/// Use in every file. :)
mod prelude {
    pub use theseeker_engine::prelude::*;

    pub use crate::appstate::{AppState, StateDespawnMarker};
}

use crate::prelude::*;

mod appstate;
mod assets;
mod camera;
mod cli;
mod level;
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
        watch_for_changes: bevy::asset::ChangeWatcher::with_delay(Duration::from_millis(250)),
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
    app.add_plugins(crate::appstate::AppStatesPlugin);

    // and custom fixed timestep thingy
    app.add_plugins(theseeker_engine::time::GameTimePlugin);

    // external plugins
    app.add_plugins((
        LdtkPlugin,
        bevy_tweening::TweeningPlugin,
        bevy_fluent::FluentPlugin,
        iyes_bevy_extras::d2::WorldCursorPlugin,
        ProgressPlugin::new(AppState::AssetsLoading)
            .track_assets()
            .continue_to(AppState::MainMenu),
    ));

    // our stuff
    app.add_plugins((
        crate::screens::loading::LoadscreenPlugin {
            state: AppState::AssetsLoading,
        },
        crate::assets::AssetsPlugin,
        crate::locale::LocalePlugin,
        crate::cli::CliPlugin,
        crate::ui::UiPlugin,
        crate::camera::CameraPlugin,
        crate::level::LevelManagerPlugin,
    ));

    #[cfg(feature = "dev")]
    app.add_systems(
        Last,
        debug_progress
            .run_if(resource_exists::<ProgressCounter>())
            .after(iyes_progress::TrackedProgressSet),
    );

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
#[allow(dead_code)]
fn debug_setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera2dBundle::default(),
        StateDespawnMarker,
    ));
}
