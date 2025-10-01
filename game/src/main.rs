#![cfg_attr(feature = "release", windows_subsystem = "windows")]
#![allow(unused_mut)]

/// Custom prelude, for stuff we'd like to access all over the codebase
/// Use in every file. :)
mod prelude {
    pub use theseeker_engine::prelude::*;

    pub use crate::appstate::{AppState, StateDespawnMarker};
    pub use crate::gamestate::GameState;
}

use bevy::app::{
    TaskPoolOptions, TaskPoolPlugin, TaskPoolThreadAssignmentPolicy,
};
use bevy::ecs::schedule::ExecutorKind;
use bevy::render::settings::{WgpuFeatures, WgpuSettings};
use bevy::render::RenderPlugin;
use bevy_ecs_tilemap::prelude::*;
use theseeker_engine::physics::PhysicsPlugin;

use crate::prelude::*;

mod appstate;
mod assets;
mod audio;
mod camera;
mod game;
mod gamestate;
mod level;

mod screens {
    pub mod loading;
}

mod ui;

mod graphics;
mod parallax;

fn main() {
    let mut app = App::new();
    app.insert_resource(ClearColor(Color::BLACK));

    let mut wgpu_settings = WgpuSettings::default();
    wgpu_settings.features.set(
        WgpuFeatures::VERTEX_WRITABLE_STORAGE,
        true,
    );

    let bevy_plugins = DefaultPlugins;
    let bevy_plugins = bevy_plugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "The Seeker (PRE-ALPHA)".into(),
            present_mode: bevy::window::PresentMode::AutoVsync,
            resizable: true,
            ..Default::default()
        }),
        ..Default::default()
    });
    let bevy_plugins = bevy_plugins.set(ImagePlugin::default_nearest());

    let bevy_plugins = bevy_plugins.set(RenderPlugin {
        render_creation: wgpu_settings.into(),
        synchronous_pipeline_compilation: false,
        debug_flags: Default::default(),
    });

    let cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let cpus_io = (cpus * 3 / 4).max(2);
    let cpus_async_compute = (cpus / 4).max(2);
    let bevy_plugins = bevy_plugins.set(TaskPoolPlugin {
        task_pool_options: TaskPoolOptions {
            min_total_threads: 1,
            max_total_threads: usize::MAX,
            io: TaskPoolThreadAssignmentPolicy {
                min_threads: cpus_io,
                max_threads: cpus_io,
                percent: 1.0,
                on_thread_spawn: None,
                on_thread_destroy: None,
            },
            async_compute: TaskPoolThreadAssignmentPolicy {
                min_threads: cpus_async_compute,
                max_threads: cpus_async_compute,
                percent: 1.0,
                on_thread_spawn: None,
                on_thread_destroy: None,
            },
            compute: TaskPoolThreadAssignmentPolicy {
                min_threads: cpus,
                max_threads: cpus,
                percent: 1.0,
                on_thread_spawn: None,
                on_thread_destroy: None,
            },
        },
    });

    let bevy_plugins = bevy_plugins.set(bevy::log::LogPlugin {
        filter: "info,wgpu_core=warn,wgpu_hal=warn,theseeker_game=info,theseeker_engine=info"
            .into(),
        level: bevy::log::Level::INFO,
        ..Default::default()
    });
    app.add_plugins(bevy_plugins);

    // configure our app states
    app.add_plugins(crate::appstate::AppStatesPlugin);

    // and custom "engine"
    app.add_plugins(theseeker_engine::EnginePlugins);

    // Remove LDtk's default background
    // (for some reason it was covering our parallax backgrounds)
    app.insert_resource(LdtkSettings {
        level_background: LevelBackground::Nonexistent,
        ..Default::default()
    });

    // external plugins
    app.add_plugins((TilemapPlugin, LdtkPlugin, PhysicsPlugin));

    // our stuff
    app.add_plugins((
        crate::screens::loading::LoadscreenPlugin {
            state: AppState::AssetsLoading,
        },
        crate::assets::AssetsPlugin,
        crate::audio::AudioPlugin,
        crate::ui::UiPlugin,
        crate::camera::CameraPlugin,
        crate::level::LevelManagerPlugin,
        crate::parallax::ParallaxPlugin,
        crate::game::GameplayPlugin,
        crate::gamestate::GameStatePlugin,
        crate::graphics::GraphicsFxPlugin,
    ));

    app.edit_schedule(Update, |s| {
        s.set_executor_kind(ExecutorKind::SingleThreaded);
    });

    app.run();
}
