//! The in-game cameras. The main camera shows the world through the gameplay viewport, and
//! the player overlay camera shares its view to draw the player and the UI above the
//! post-processing. Each frame places them between the framing rig's last two ticks, shaken.
mod framing;
mod scale;

use std::f32::consts::TAU;

use bevy::camera::{OrthographicProjection, Projection};
use bevy::ui::IsDefaultUiCamera;
use rand::{rng, Rng};
use theseeker_engine::presentation::{PresentationSet, TickHistory};

use crate::game::player::PlayerStateSet;
use crate::graphics::pixel_view::RenderBand;
use crate::levelgen::CompiledLevel;
use crate::prelude::*;
pub(crate) use framing::CameraRig;
pub use framing::{ScriptedCameraFocus, SnapCameraNext};
pub use scale::GameplayPixelScale;
pub(crate) use scale::LETTERBOX;

/// Temporary debug toggle: disable world-edge camera clamping to inspect generated content
/// beyond chunk boundaries.
const CLAMP_CAMERA_TO_LEVEL_BOUNDS: bool = false;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameplayPixelScale>()
            .init_resource::<CameraRig>()
            .init_resource::<CameraHistory>()
            .add_systems(OnEnter(AppState::InGame), (scale::fit_gameplay_pixel_scale, spawn_cameras).chain())
            .add_systems(GameTickUpdate, (
                framing::follow_player.after(PlayerStateSet::Collisions),
                update_screen_shake.run_if(resource_exists::<CameraShake>),
            ))
            .add_systems(GameTickPost, capture_camera_history.in_set(PresentationSet::Capture))
            .add_systems(Update, (scale::fit_gameplay_pixel_scale, update_camera, sync_player_camera)
                .chain()
                .in_set(GameTickSet::Post));
    }
}

/// Marker component for the main gameplay camera. Each fullscreen effect's plugin makes it
/// require that effect's settings.
#[derive(Component)]
pub struct MainCamera;

/// Marker component for the player camera (renders player on top of post-processing effects)
#[derive(Component)]
pub struct PlayerCamera;

/// Spawns the main camera and the player overlay camera, both showing the gameplay viewport.
fn spawn_cameras(mut commands: Commands, scale: Res<GameplayPixelScale>) {
    let projection = Projection::Orthographic(OrthographicProjection {
        scale: scale.projection_scale(),
        ..OrthographicProjection::default_2d()
    });
    commands.spawn((
        Camera2d,
        Camera { viewport: scale.viewport(), output_mode: LETTERBOX, ..default() },
        projection.clone(),
        MainCamera,
        // The world (0) and light sources (1); the player (2) is drawn by the overlay camera,
        // above the pixel-perfect composite.
        RenderBand::World.layers(),
        StateDespawnMarker,
        Name::new("MainCamera"),
    ));
    commands.spawn((
        Camera2d,
        Camera { order: 1, clear_color: ClearColorConfig::None, viewport: scale.viewport(), ..default() },
        projection,
        PlayerCamera,
        IsDefaultUiCamera,
        RenderBand::ActorOverlay.layers(),
        StateDespawnMarker,
        Name::new("PlayerCamera"),
    ));
}

/// Places the main camera between the rig's last two ticks, then shakes it.
pub fn update_camera(
    mut cameras: Query<(&mut Transform, &Projection), With<MainCamera>>,
    rig: Res<CameraRig>,
    level: Option<Res<CompiledLevel>>,
    shake: Option<Res<CameraShake>>,
    interpolation: Res<RenderInterpolation>,
    history: Res<CameraHistory>,
) {
    let Ok((mut transform, projection)) = cameras.single_mut() else {
        return;
    };
    let (position, shake) = match history.0.pair(interpolation.position()) {
        Some((previous, next, alpha)) => (
            previous.position.lerp(next.position, if next.snap { 0.0 } else { alpha }),
            previous.shake.lerp(next.shake, alpha),
        ),
        None => (
            rig.previous.lerp(rig.position, interpolation.alpha().clamp(0.0, 1.0)),
            shake.map_or(Vec2::ZERO, |shake| shake.offset),
        ),
    };
    transform.translation.x = position.x;
    transform.translation.y = position.y;
    let (Some(level), Projection::Orthographic(ortho)) = (level, projection) else {
        return;
    };
    // Shaking after clamping keeps the camera shaking at the level's edges, and clamping again
    // keeps the shake from showing past them.
    clamp_to_level(&mut transform, level.bounds, ortho.area);
    transform.translation += shake.extend(0.0);
    clamp_to_level(&mut transform, level.bounds, ortho.area);
}

/// Keeps a `view`-sized view inside the level while `CLAMP_CAMERA_TO_LEVEL_BOUNDS` is on.
fn clamp_to_level(transform: &mut Transform, level: Rect, view: Rect) {
    if CLAMP_CAMERA_TO_LEVEL_BOUNDS {
        let xy = transform.translation.xy().clamp(level.min + view.half_size(), level.max - view.half_size());
        transform.translation = xy.extend(transform.translation.z);
    }
}

fn sync_player_camera(
    main: Query<(&Transform, &Projection, &Camera), (With<MainCamera>, Without<PlayerCamera>)>,
    mut player: Query<(&mut Transform, &mut Projection, &mut Camera), (With<PlayerCamera>, Without<MainCamera>)>,
) {
    let (Ok((main_transform, main_projection, main_camera)), Ok((mut transform, mut projection, mut camera))) =
        (main.single(), player.single_mut())
    else {
        return;
    };
    *transform = *main_transform;
    *projection = main_projection.clone();
    camera.is_active = main_camera.is_active;
    camera.viewport.clone_from(&main_camera.viewport);
}

/// A decaying shake of the camera along a direction that changes at random, with a wobble
/// across it. The camera stops following for the shake's first two ticks.
#[derive(Resource, Clone)]
pub struct CameraShake {
    strength: f32,
    freq: f32,
    dir: Vec2,
    /// The camera's current displacement.
    offset: Vec2,
    timer: Timer,
    /// Each period, the shake picks a new direction.
    sub_timer: Timer,
    /// Game ticks left before the camera follows again.
    pause_lerp_ticks: u32,
}

impl CameraShake {
    pub fn new(strength: f32, t: f32, freq: f32) -> Self {
        Self {
            strength,
            freq,
            dir: random_direction(),
            offset: Vec2::ZERO,
            timer: Timer::from_seconds(t, TimerMode::Once),
            sub_timer: Timer::from_seconds(t / freq, TimerMode::Repeating),
            pause_lerp_ticks: 2,
        }
    }

    /// Whether the shake holds the camera where it is this tick.
    fn pauses_follow(&self) -> bool {
        self.pause_lerp_ticks > 0
    }
}

fn random_direction() -> Vec2 {
    Vec2::from_angle(rng().random_range(0.0..TAU))
}

/// Advances the shake, and ends it when its time runs out.
fn update_screen_shake(mut commands: Commands, time: Res<ClockTime>, mut shake: ResMut<CameraShake>) {
    // The wobble across the shake runs at twice its frequency and half its amplitude.
    const WOBBLE_FREQ_SCALE: f32 = 2.0;
    const WOBBLE_AMP_SCALE: f32 = 0.5;
    let ratio = shake.timer.fraction();
    let decay = 1.0 - ratio.powi(2);
    let phase = shake.freq * ratio * TAU;
    if shake.sub_timer.is_finished() {
        shake.dir = random_direction();
    }
    let along = shake.dir * phase.sin();
    let across = shake.dir.perp() * (WOBBLE_FREQ_SCALE * phase).sin() * WOBBLE_AMP_SCALE;
    shake.offset = (along + across) * decay * shake.strength;
    shake.timer.tick(time.delta());
    shake.sub_timer.tick(time.delta());
    shake.pause_lerp_ticks = shake.pause_lerp_ticks.saturating_sub(1);
    if shake.timer.is_finished() {
        commands.remove_resource::<CameraShake>();
    }
}

/// The camera's pose at each recent tick, which `update_camera` interpolates between.
#[derive(Resource, Default)]
pub(crate) struct CameraHistory(TickHistory<CameraPose>);

struct CameraPose {
    position: Vec2,
    shake: Vec2,
    /// Whether the camera jumped to this pose rather than moving there.
    snap: bool,
}

fn capture_camera_history(rig: Res<CameraRig>, shake: Option<Res<CameraShake>>, time: Res<GameTime>, mut history: ResMut<CameraHistory>) {
    history.0.push(time.tick(), CameraPose {
        position: rig.position,
        shake: shake.map_or(Vec2::ZERO, |shake| shake.offset),
        snap: rig.previous == rig.position,
    });
}
