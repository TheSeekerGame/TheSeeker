#![cfg_attr(feature = "release", windows_subsystem = "windows")]

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
use bevy::ecs::schedule::SingleThreadedExecutor;
use bevy::render::settings::{
    WgpuFeatures, WgpuSettings,
};
use bevy::render::{RenderDebugFlags, RenderPlugin};
use bevy::window::{MonitorSelection, WindowMode, WindowResolution};
use theseeker_engine::physics::PhysicsPlugin;

use crate::prelude::*;

mod appstate;
mod assets;
mod camera;
mod game;
mod gamestate;
mod music;
mod timing_probe;
mod frame_metrics;
mod levelgen;

mod screens {
    pub mod loading;
}

mod ui;

mod graphics;
mod parallax;

#[expect(
    clippy::too_many_lines,
    reason = "This entrypoint declares the Bevy app resources, plugins, and startup configuration in one auditable builder chain."
)]
fn main() {
    let mut app = App::new();
    app.insert_resource(ClearColor(Color::srgb(
        245.0 / 255.0,
        250.0 / 255.0,
        253.0 / 255.0,
    )));

    let mut wgpu_settings = WgpuSettings::default();
    wgpu_settings.features.set(
        WgpuFeatures::VERTEX_WRITABLE_STORAGE,
        true,
    );

    let bevy_plugins = DefaultPlugins.build().disable::<bevy::audio::AudioPlugin>();
    let bevy_plugins = bevy_plugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "The Seeker (PRE-ALPHA)".into(),
            resolution: WindowResolution::default()
                .with_scale_factor_override(1.0),
            present_mode: bevy::window::PresentMode::AutoVsync,
            desired_maximum_frame_latency: std::num::NonZeroU32::new(crate::frame_metrics::DEFAULT_FRAME_LATENCY),
            resizable: true,
            mode: WindowMode::BorderlessFullscreen(MonitorSelection::Primary),
            ..Default::default()
        }),
        ..Default::default()
    });
    let bevy_plugins = bevy_plugins.set(ImagePlugin::default_nearest());

    let bevy_plugins = bevy_plugins.set(RenderPlugin {
        render_creation: wgpu_settings.into(),
        synchronous_pipeline_compilation: false,
        debug_flags: RenderDebugFlags::default(),
    });

    let cpus = std::thread::available_parallelism()
        .map(std::num::NonZero::get)
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
        filter: "info,wgpu_core=warn,wgpu_hal=warn,theseeker_game=info,theseeker_engine=info,theseeker_game::tick_profile=debug"
            .into(),
        level: bevy::log::Level::INFO,
        ..Default::default()
    });
    app.add_plugins(bevy_plugins);

    // configure our app states
    app.add_plugins(crate::appstate::AppStatesPlugin);

    // and custom "engine"
    app.add_plugins(theseeker_engine::EnginePlugins);
    app.add_plugins(crate::music::MusicPlugin);
    app.add_plugins(crate::timing_probe::TimingProbePlugin);

    // external plugins
    app.add_plugins(PhysicsPlugin);
    // Disable MSAA globally to avoid any on-screen blending
    app.add_systems(
        Startup,
        (
            |mut q: bevy::prelude::Query<
                &mut bevy::render::view::Msaa,
                bevy::prelude::With<bevy::prelude::Camera>,
            >| {
                if let Ok(mut msaa) = q.single_mut() {
                    *msaa = bevy::render::view::Msaa::Off;
                }
            },
        ),
    );

    // our stuff
    app.add_plugins((
        crate::screens::loading::LoadscreenPlugin {
            state: AppState::AssetsLoading,
        },
        crate::assets::AssetsPlugin,
        crate::ui::UiPlugin,
        crate::camera::CameraPlugin,
        crate::levelgen::PngLevelPlugin,
        crate::parallax::ParallaxPlugin,
        crate::game::GameplayPlugin,
        crate::gamestate::GameStatePlugin,
        crate::graphics::GraphicsFxPlugin,
    ));

    app.edit_schedule(Update, |s| {
        s.set_executor(SingleThreadedExecutor::new());
    });

    app.run();
}
