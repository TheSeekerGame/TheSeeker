/// Custom prelude, for stuff we'd like to access all over the codebase
/// Use in every file. :)
mod prelude {
    pub use theseeker_engine::prelude::*;

    pub use crate::appstate::{AppState, StateDespawnMarker};
    pub use crate::gamestate::GameState;
}

use crate::prelude::*;
use theseeker_engine::physics::PhysicsPlugin;

mod appstate;
mod assets;
mod camera;
mod cli;
mod game;
mod gamestate;
mod level;
mod locale;

mod screens {
    pub mod loading;
}

mod ui;

#[cfg(feature = "dev")]
mod dev;
pub mod graphics;
mod parallax;

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
    let bevy_plugins = bevy_plugins.set(bevy::log::LogPlugin {
        filter: "info,wgpu_core=warn,wgpu_hal=warn,iyes_progress=trace,theseeker_game=trace,theseeker_engine=trace".into(),
        level: bevy::log::Level::TRACE,
        update_subscriber: None,
    });
    #[cfg(not(feature = "dev"))]
    let bevy_plugins = bevy_plugins.set(bevy::log::LogPlugin {
        filter: "info,wgpu_core=warn,wgpu_hal=warn,theseeker_game=info,theseeker_engine=info"
            .into(),
        level: bevy::log::Level::INFO,
        update_subscriber: None,
    });
    app.add_plugins(bevy_plugins);

    // configure our app states
    app.add_plugins(crate::appstate::AppStatesPlugin);

    // and custom "engine"
    app.add_plugins(theseeker_engine::EnginePlugins);

    // external plugins
    app.add_plugins((
        LdtkPlugin,
        bevy_tweening::TweeningPlugin,
        bevy_fluent::FluentPlugin,
        iyes_bevy_extras::d2::WorldCursorPlugin,
        ProgressPlugin::new(AppState::AssetsLoading)
            .track_assets()
            .continue_to(AppState::MainMenu),
        PhysicsPlugin,
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
        crate::parallax::ParallaxPlugin,
        crate::game::GameplayPlugin,
        crate::gamestate::GameStatePlugin,
        crate::graphics::GraphicsFxPlugin,
    ));

    #[cfg(feature = "dev")]
    app.add_plugins(crate::dev::DevPlugin);

    app.run();
}
