#![cfg_attr(feature = "release", windows_subsystem = "windows")]
#![allow(unused_mut)]
// FIXME: temporary, to reduce noise during the 0.15 upgrade
#![allow(warnings)]

/// Custom prelude, for stuff we'd like to access all over the codebase
/// Use in every file. :)
mod prelude {
    pub use theseeker_engine::prelude::*;

    pub use crate::appstate::{AppState, StateDespawnMarker};
    pub use crate::gamestate::GameState;
}

use bevy::core::TaskPoolThreadAssignmentPolicy;
use bevy::ecs::schedule::ExecutorKind;
use bevy::render::settings::{WgpuFeatures, WgpuSettings};
use bevy::render::RenderPlugin;
use theseeker_engine::physics::PhysicsPlugin;

use crate::prelude::*;

mod appstate;
mod assets;
mod audio;
mod camera;
mod cli;
mod game;
mod gamestate;
mod level;
mod locale;
mod stepping_egui;

mod screens {
    pub mod loading;
}

mod ui;

//#[cfg(feature = "dev")]
mod dev;
pub mod graphics;
mod parallax;
mod tilemap_staticify;

// -----------------------------------------------------------------------------
//  Screenshot suppression
// -----------------------------------------------------------------------------
// Bevy 0.15 always adds the `prepare_screenshots` render-time system.  That
// system is effectively dormant unless `Screenshot` entities exist, but some
// helper code (or the user hitting the *Print Screen* key) can spawn them at
// runtime and cause a noticeable frame hitch when the read-back happens.
//
// We completely opt-out by removing such entities as soon as they appear.
use bevy::render::view::screenshot::Screenshot;
use bevy_ecs_ldtk::prelude::Respawn;
use bevy_ecs_ldtk::systems;

struct SuppressScreenshotsPlugin;

impl Plugin for SuppressScreenshotsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, |mut commands: Commands, shots: Query<Entity, With<Screenshot>>| {
            for e in &shots {
                commands.entity(e).despawn();
            }
        });
    }
}

// -----------------------------------------------------------------------------
//  Disable LDtk live-update respawns
// -----------------------------------------------------------------------------

/// The `bevy_ecs_ldtk` fork tags the LDtk world entity with `Respawn` whenever
/// the project file on disk changes (hot-reload over a websocket / file-watch).
/// That in turn kicks off an expensive full-level despawn / respawn that causes
/// multi-millisecond stalls.  In-game we don't need live editing, so we simply
/// strip the marker as soon as it appears.

struct DisableLdtkApiPlugin;

impl Plugin for DisableLdtkApiPlugin {
    fn build(&self, app: &mut App) {
        fn strip_respawn_markers(
            mut commands: Commands,
            respawns: Query<Entity, With<Respawn>>,
        ) {
            for e in &respawns {
                commands.entity(e).remove::<Respawn>();
            }
        }

        // Remove the `Respawn` component right after `process_ldtk_assets` runs
        // (which is also in `PreUpdate`). This prevents the costly despawn /
        // respawn path from ever triggering.
        app.add_systems(
            PreUpdate,
            strip_respawn_markers.after(systems::process_ldtk_assets),
        );
    }
}

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
    });

    let cpus = num_cpus::get_physical();
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
            },
            async_compute: TaskPoolThreadAssignmentPolicy {
                min_threads: cpus_async_compute,
                max_threads: cpus_async_compute,
                percent: 1.0,
            },
            compute: TaskPoolThreadAssignmentPolicy {
                min_threads: cpus,
                max_threads: cpus,
                percent: 1.0,
            },
        },
    });

    #[cfg(feature = "dev")]
    let bevy_plugins = bevy_plugins.set(bevy::log::LogPlugin {
        filter: "info,wgpu_core=warn,wgpu_hal=warn,iyes_progress=trace,theseeker_game=trace,theseeker_engine=trace".into(),
        level: bevy::log::Level::TRACE,
        ..Default::default()
    });
    #[cfg(not(feature = "dev"))]
    let bevy_plugins = bevy_plugins.set(bevy::log::LogPlugin {
        filter: "info,wgpu_core=warn,wgpu_hal=warn,theseeker_game=info,theseeker_engine=info"
            .into(),
        level: bevy::log::Level::INFO,
        ..Default::default()
    });

    // Disable 3-D PBR (lighting etc.)
    let bevy_plugins = bevy_plugins.disable::<bevy::pbr::PbrPlugin>();
    app.add_plugins(bevy_plugins);

    // configure our app states
    app.add_plugins(crate::appstate::AppStatesPlugin);

    // and custom "engine"
    app.add_plugins(theseeker_engine::EnginePlugins);
    // app.add_plugin(Sprite3dPlugin);

    // external plugins
    app.add_plugins((
        LdtkPlugin,
        bevy_fluent::FluentPlugin,
        // iyes_bevy_extras::d2::WorldCursorPlugin,
        ProgressPlugin::<AppState>::new()
            .with_state_transition(
                AppState::AssetsLoading,
                // FIXME: fix main menu and re-enable state here
                AppState::InGame,
                // AppState::MainMenu,
            )
            .with_asset_tracking(),
        PhysicsPlugin,
    ));

    #[cfg(feature = "iyes_perf_ui")]
    app.add_plugins((iyes_perf_ui::PerfUiPlugin,));

    // our stuff
    app.add_plugins((
        crate::screens::loading::LoadscreenPlugin {
            state: AppState::AssetsLoading,
        },
        crate::assets::AssetsPlugin,
        crate::audio::AudioPlugin,
        crate::locale::LocalePlugin,
        crate::cli::CliPlugin,
        crate::ui::UiPlugin,
        crate::camera::CameraPlugin,
        crate::level::LevelManagerPlugin,
        crate::parallax::ParallaxPlugin,
        crate::tilemap_staticify::TilemapStaticifyPlugin,
        crate::game::GameplayPlugin,
        crate::gamestate::GameStatePlugin,
        crate::graphics::GraphicsFxPlugin,
        SuppressScreenshotsPlugin,
        DisableLdtkApiPlugin,
    ));

    #[cfg(feature = "dev")]
    app.add_plugins(crate::dev::DevPlugin);

    app.edit_schedule(Update, |s| {
        s.set_executor_kind(ExecutorKind::SingleThreaded);
    });

    app.run();
}
