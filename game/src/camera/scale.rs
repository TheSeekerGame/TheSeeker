//! The gameplay pixel scale: how many screen pixels each game pixel spans, fitted to the
//! window, and the letterboxed viewport of whole game pixels it leaves.
use bevy::camera::{CameraOutputMode, Viewport};
use bevy::window::PrimaryWindow;

use super::MainCamera;
use crate::prelude::*;

/// The count of game pixels the view aims to show.
const TARGET_VISIBLE_GAME_PIXELS: f64 = 70_000.0;
/// The divisor the UI's layout is authored at.
const DEFAULT_GAMEPLAY_PIXEL_DIVISOR: u32 = 5;

/// How the main camera fills the window outside its viewport: black letterbox bars.
pub(crate) const LETTERBOX: CameraOutputMode = CameraOutputMode::Write {
    blend_state: None,
    clear_color: ClearColorConfig::Custom(Color::BLACK),
};

/// How many screen pixels each game pixel spans, and the viewport of whole game pixels
/// centered in the window.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct GameplayPixelScale {
    physical_size: UVec2,
    viewport_position: UVec2,
    viewport_size: UVec2,
    divisor: u32,
}

impl Default for GameplayPixelScale {
    fn default() -> Self {
        Self::for_divisor(UVec2::ZERO, DEFAULT_GAMEPLAY_PIXEL_DIVISOR)
    }
}

impl GameplayPixelScale {
    /// The scale whose view of a `physical_size` window comes nearest the target count of
    /// game pixels, with larger pixels on a tie.
    pub(crate) fn fit(physical_size: UVec2) -> Self {
        if physical_size.min_element() == 0 {
            return Self::default();
        }
        let ideal = (physical_size.as_dvec2().element_product() / TARGET_VISIBLE_GAME_PIXELS).sqrt();
        let base = (ideal as u32).max(1);
        (base.saturating_sub(1).max(1)..=base + 2)
            .map(|divisor| Self::for_divisor(physical_size, divisor))
            .min_by(|a, b| a.miss().total_cmp(&b.miss()).then(b.divisor.cmp(&a.divisor)))
            .expect("the candidates include the base divisor")
    }

    /// The largest centered viewport of whole game pixels, an even count of them on each axis
    /// so the view's center, where the camera sits, falls between pixels on the screen and on
    /// the pixel layer.
    fn for_divisor(physical_size: UVec2, divisor: u32) -> Self {
        let divisor = divisor.max(1);
        let pair = 2 * divisor;
        let viewport_size = physical_size / pair * pair;
        Self { physical_size, viewport_position: (physical_size - viewport_size) / 2, viewport_size, divisor }
    }

    /// How far the view's count of game pixels misses the target.
    fn miss(&self) -> f64 {
        (self.visible_game_size().as_dvec2().element_product() - TARGET_VISIBLE_GAME_PIXELS).abs()
    }

    pub fn divisor(&self) -> u32 {
        self.divisor
    }

    /// The UI's scale, which keeps its art pixels on game pixels.
    pub fn ui_scale_factor(&self) -> f32 {
        self.divisor as f32 / DEFAULT_GAMEPLAY_PIXEL_DIVISOR as f32
    }

    /// The view's size in game pixels.
    pub fn visible_game_size(&self) -> Vec2 {
        (self.viewport_size / self.divisor).as_vec2()
    }

    /// The orthographic scale at which a world unit spans one game pixel.
    pub(super) fn projection_scale(&self) -> f32 {
        1.0 / self.divisor as f32
    }

    /// The main camera's viewport: none while it would fill the window or hold no pixels.
    pub(super) fn viewport(&self) -> Option<Viewport> {
        (self.viewport_size != self.physical_size && self.viewport_size.min_element() > 0).then(|| Viewport {
            physical_position: self.viewport_position,
            physical_size: self.viewport_size,
            ..default()
        })
    }
}

/// Fits the scale to the window and applies it to the main camera.
pub(super) fn fit_gameplay_pixel_scale(
    windows: Query<&Window, With<PrimaryWindow>>,
    mut scale: ResMut<GameplayPixelScale>,
    mut cameras: Query<(&mut Camera, &mut Projection), With<MainCamera>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let fitted = GameplayPixelScale::fit(window.physical_size());
    if *scale != fitted {
        info!("gameplay pixel scale {} for a {} window: {} game pixels in a {} viewport at {}",
            fitted.divisor, fitted.physical_size, fitted.visible_game_size(), fitted.viewport_size, fitted.viewport_position);
        *scale = fitted;
    }
    for (mut camera, mut projection) in &mut cameras {
        camera.viewport = scale.viewport();
        if let Projection::Orthographic(ortho) = &mut *projection {
            ortho.scale = scale.projection_scale();
        }
    }
}

#[cfg(test)]
#[path = "scale_tests.rs"]
mod tests;
