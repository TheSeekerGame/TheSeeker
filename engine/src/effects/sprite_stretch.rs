use super::SpriteScale;
use crate::prelude::*;
use crate::time::GameTickUpdate;

/// Anchor point for the stretch/squeeze effect
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StretchAnchor {
    /// Stretch from center (default sprite behavior)
    Center,
    /// Keep bottom edge fixed, stretch upward only
    Bottom,
}

/// Component that drives sprite stretching/squeezing animation
#[derive(Component, Debug)]
pub struct SpriteStretch {
    /// Current tick in the animation
    pub current_tick: u32,
    /// Ticks to reach maximum deformation
    pub squeeze_ticks: u32,
    /// Ticks to hold at maximum deformation
    pub hold_ticks: u32,
    /// Ticks to return to normal from maximum deformation
    pub release_ticks: u32,
    /// Scale factor at maximum deformation (horizontal)
    pub max_squeeze_x: f32,
    /// Scale factor at maximum deformation (vertical)
    pub max_stretch_y: f32,
    /// How the stretch should be anchored
    pub anchor: StretchAnchor,
    /// Estimated sprite height in pixels (used for bottom-anchor offset calculation)
    pub sprite_height: f32,
}

impl SpriteStretch {
    /// Total duration of the animation in ticks
    pub fn total_ticks(&self) -> u32 {
        self.squeeze_ticks + self.hold_ticks + self.release_ticks
    }
}

impl Default for SpriteStretch {
    fn default() -> Self {
        Self {
            current_tick: 0,
            squeeze_ticks: 0,
            hold_ticks: 0,
            release_ticks: 8,
            max_squeeze_x: 0.7,
            max_stretch_y: 1.3,
            anchor: StretchAnchor::Bottom,
            sprite_height: 24.0, // Reasonable default
        }
    }
}

pub struct SpriteStretchPlugin;

impl Plugin for SpriteStretchPlugin {
    fn build(&self, app: &mut App) {
        // Update stretch animation each game tick
        app.add_systems(
            GameTickUpdate,
            update_sprite_stretch
                .after(crate::animation::AnimationSet::LoopClear),
        );

        // Apply scale to `GlobalTransform` during rendering.
        // Must run AFTER `transform_gfx_from_gent` to avoid being overwritten.
        app.add_systems(
            PostUpdate,
            apply_sprite_scale_to_global
                .after(crate::gent::transform_gfx_from_gent),
        );
    }
}

/// Updates the stretch animation state each tick
fn update_sprite_stretch(
    mut query: Query<(
        Entity,
        &mut SpriteStretch,
        &mut SpriteScale,
    )>,
    mut commands: Commands,
) {
    for (entity, mut stretch, mut scale) in query.iter_mut() {
        let squeeze_end = stretch.squeeze_ticks;
        let hold_end = squeeze_end + stretch.hold_ticks;
        let total_ticks = stretch.total_ticks();

        // Interpolation progress based on animation phase
        let progress = if stretch.current_tick < squeeze_end {
            // Phase 1: Squeeze phase (0 to squeeze_ticks)
            // Interpolate from 0.0 to 1.0 (no deformation to max deformation)
            stretch.current_tick as f32 / stretch.squeeze_ticks as f32
        } else if stretch.current_tick < hold_end {
            // Phase 2: Hold phase (squeeze_ticks to squeeze_ticks + hold_ticks)
            // Stay at maximum deformation (progress = 1.0)
            1.0
        } else if stretch.current_tick < total_ticks {
            // Phase 3: Release phase (hold_end to total_ticks)
            // Interpolate from 1.0 to 0.0 (max deformation back to normal)
            let release_progress = (stretch.current_tick - hold_end) as f32
                / stretch.release_ticks as f32;
            1.0 - release_progress
        } else {
            // Animation complete
            0.0
        };

        // Easing curve (smooth in/out) for squeeze and release phases
        let eased_progress = if stretch.current_tick < squeeze_end
            || stretch.current_tick >= hold_end
        {
            ease_in_out_cubic(progress)
        } else {
            progress // No easing during hold phase
        };

        // Apply scale values
        scale.x = 1.0 - (1.0 - stretch.max_squeeze_x) * eased_progress;
        scale.y = 1.0 + (stretch.max_stretch_y - 1.0) * eased_progress;

        // Advance animation
        stretch.current_tick += 1;

        // Remove component when animation completes
        if stretch.current_tick >= total_ticks {
            commands.entity(entity).remove::<SpriteStretch>();
            scale.reset();
        }
    }
}

/// Applies sprite scale to the entity's global transform
fn apply_sprite_scale_to_global(
    mut query: Query<
        (
            &SpriteScale,
            Option<&SpriteStretch>,
            &mut GlobalTransform,
        ),
        With<Sprite>,
    >,
) {
    for (scale, stretch_opt, mut global_transform) in query.iter_mut() {
        // Only apply if scale is not default (1.0, 1.0)
        if (scale.x - 1.0).abs() > 0.001 || (scale.y - 1.0).abs() > 0.001 {
            // Get the current transform
            let mut transform = global_transform.compute_transform();

            // Store original scale signs to preserve flip states
            let x_sign = transform.scale.x.signum();
            let y_sign = transform.scale.y.signum();

            // Apply scale while preserving flip
            transform.scale.x = x_sign * scale.x.abs();
            transform.scale.y = y_sign * scale.y.abs();
            transform.scale.z = 1.0; // Keep Z scale at 1.0

            // Apply vertical offset for bottom-anchored stretching
            if let Some(stretch) = stretch_opt {
                if stretch.anchor == StretchAnchor::Bottom {
                    // When scaling from center, the bottom edge moves down by (scale_y - 1) * height / 2
                    // To keep it fixed, we offset upward by that amount
                    let vertical_offset =
                        (scale.y - 1.0) * stretch.sprite_height * 0.5;
                    transform.translation.y += vertical_offset;
                }
            }

            // Update the global transform
            *global_transform = transform.into();
        }
    }
}

/// Cubic easing function for smooth animation
fn ease_in_out_cubic(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}
