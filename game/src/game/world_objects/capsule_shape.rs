//! Capsule pod geometry in the pixel grid of its sprite.
//!
//! Positions count sprite pixels rightward and downward from the top-left corner of the
//! pod artwork, so every value can be read directly against the image. The pod is
//! symmetric about its vertical axis: span tables list left-half spans `[x0, x1)`, and
//! each span is mirrored into the right half. Rects below the sprite reach down the rail.

use crate::prelude::*;

use super::{CapsuleCollisionMask, CapsuleLightMask, CapsulePlatform};

/// Size of the outside and inside pod artwork.
pub(super) const POD_SPRITE_SIZE: UVec2 = UVec2::new(54, 154);

/// Rows `first..=last` filled over the left-half span `[x0, x1)` and its mirror.
struct MirroredRows {
    first: i32,
    last: i32,
    x0: i32,
    x1: i32,
}

const fn rows(first: i32, last: i32, x0: i32, x1: i32) -> MirroredRows {
    MirroredRows { first, last, x0, x1 }
}

/// `w`×`h` pixels starting at (`x`, `y`).
struct PodRect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

const fn rect(x: i32, y: i32, w: i32, h: i32) -> PodRect {
    PodRect { x, y, w, h }
}

/// Solid pressure shell. Rows 41–48 include the cabin floor on both sides of the hatch.
const SHELL: [MirroredRows; 25] = [
    rows(1, 2, 19, 27),
    rows(3, 3, 18, 27),
    rows(4, 5, 14, 19),
    rows(6, 6, 12, 17),
    rows(7, 7, 11, 17),
    rows(8, 8, 10, 17),
    rows(9, 9, 9, 17),
    rows(10, 10, 8, 17),
    rows(11, 11, 7, 12),
    rows(12, 12, 6, 12),
    rows(13, 14, 5, 12),
    rows(15, 16, 4, 12),
    rows(17, 18, 3, 12),
    rows(19, 22, 2, 12),
    rows(23, 30, 1, 12),
    rows(31, 34, 2, 12),
    rows(35, 36, 3, 12),
    rows(37, 38, 4, 12),
    rows(39, 39, 5, 12),
    rows(40, 40, 6, 12),
    rows(41, 42, 6, 22),
    rows(43, 44, 7, 22),
    rows(45, 46, 8, 22),
    rows(47, 47, 9, 22),
    rows(48, 48, 10, 22),
];

/// Silhouette kept lit and undimmed while the player is inside: the sphere and its collar.
const LIGHT_AREA: [MirroredRows; 37] = [
    rows(0, 0, 23, 26),
    rows(1, 1, 20, 27),
    rows(2, 3, 19, 27),
    rows(4, 4, 18, 27),
    rows(5, 5, 13, 27),
    rows(6, 6, 14, 27),
    rows(7, 7, 12, 27),
    rows(8, 8, 11, 27),
    rows(9, 9, 10, 27),
    rows(10, 10, 9, 27),
    rows(11, 11, 8, 27),
    rows(12, 12, 7, 27),
    rows(13, 13, 6, 27),
    rows(14, 15, 5, 27),
    rows(16, 17, 4, 27),
    rows(18, 19, 3, 27),
    rows(20, 23, 2, 27),
    rows(24, 31, 1, 27),
    rows(32, 35, 2, 27),
    rows(36, 37, 3, 27),
    rows(38, 39, 4, 27),
    rows(40, 40, 5, 27),
    rows(41, 43, 6, 27),
    rows(44, 45, 7, 27),
    rows(46, 47, 8, 27),
    rows(48, 48, 9, 27),
    rows(49, 49, 10, 27),
    rows(50, 50, 9, 11),
    rows(50, 50, 12, 13),
    rows(50, 50, 14, 27),
    rows(51, 51, 14, 27),
    rows(52, 52, 16, 27),
    rows(53, 53, 17, 27),
    rows(54, 55, 16, 27),
    rows(56, 56, 17, 27),
    rows(57, 57, 21, 27),
    rows(58, 58, 22, 27),
];

/// One-way deck of the upper cabin.
const UPPER_DECK: PodRect = rect(12, 21, 30, 1);
/// One-way cover over the hatch; the player drops through it to leave.
const HATCH: PodRect = rect(22, 41, 10, 1);
/// Crossing it upward makes the player the occupant.
const INSIDE_TRIGGER: PodRect = rect(12, 36, 30, 1);
/// Crossing it downward ends occupancy.
const OUTSIDE_TRIGGER: PodRect = rect(22, 54, 10, 1);
/// Falling through the hatch into it starts the lift down the rail.
const LIFT_DOWN_TRIGGER: PodRect = rect(22, 44, 10, 1);
/// The lift stops at the bottom of the rail.
const LIFT_DOWN_STOP: PodRect = rect(21, 159, 13, 1);
/// Pressing up at the bottom of the rail starts the lift up.
const LIFT_UP_TRIGGER: PodRect = rect(24, 163, 7, 1);
/// The lift stops above the upper deck.
const LIFT_UP_STOP: PodRect = rect(20, 27, 15, 1);

pub(super) fn collision_mask() -> CapsuleCollisionMask {
    CapsuleCollisionMask {
        wall_rects: merge_pixels_into_rects(&mirrored_pixels(&SHELL)),
        cabin_rects: merge_pixels_into_rects(&mirrored_pixels(&LIGHT_AREA).into_iter()
            .filter(|pixel| pixel.y < OUTSIDE_TRIGGER.y).collect::<Vec<_>>()),
        outside_trigger_rects: vec![OUTSIDE_TRIGGER.local_rect()],
        inside_trigger_rects: vec![INSIDE_TRIGGER.local_rect()],
        vertical_transit_up_trigger_rects: vec![LIFT_UP_TRIGGER.local_rect()],
        vertical_transit_down_trigger_rects: vec![
            LIFT_DOWN_TRIGGER.local_rect(),
        ],
        vertical_transit_up_stop_rects: vec![LIFT_UP_STOP.local_rect()],
        vertical_transit_down_stop_rects: vec![LIFT_DOWN_STOP.local_rect()],
        // Platform contacts are keyed by index; keep this order stable.
        platforms: vec![
            CapsulePlatform { rect: UPPER_DECK.local_rect(), requires_lift_jump: false },
            CapsulePlatform { rect: HATCH.local_rect(), requires_lift_jump: true },
        ],
    }
}

pub(super) fn light_mask() -> CapsuleLightMask {
    CapsuleLightMask {
        carve_rects: merge_pixels_into_rects(&mirrored_pixels(&LIGHT_AREA)),
    }
}

impl PodRect {
    fn local_rect(&self) -> Rect {
        pixel_span_to_local_rect(self.x, self.y, self.w, self.h)
    }
}

fn mirrored_pixels(spans: &[MirroredRows]) -> Vec<IVec2> {
    spans.iter().flat_map(MirroredRows::pixels).collect()
}

impl MirroredRows {
    fn pixels(&self) -> impl Iterator<Item = IVec2> + '_ {
        let right_edge = POD_SPRITE_SIZE.x as i32 - 1;
        let columns = self.x0..self.x1;
        (self.first..=self.last).flat_map(move |y| {
            columns
                .clone()
                .flat_map(move |x| [IVec2::new(x, y), IVec2::new(right_edge - x, y)])
        })
    }
}

/// Greedily covers the pixels with rects, widest run first, in sprite-local space.
fn merge_pixels_into_rects(pixels: &[IVec2]) -> Vec<Rect> {
    let width = POD_SPRITE_SIZE.x as usize;
    let height = POD_SPRITE_SIZE.y as usize;
    let mut filled = vec![false; width * height];
    for pixel in pixels {
        let idx = pixel.y as usize * width + pixel.x as usize;
        if let Some(cell) = filled.get_mut(idx) {
            *cell = true;
        }
    }

    let grid = PixelGrid { width, height, filled: &filled };
    let mut visited = vec![false; filled.len()];
    let mut rects = Vec::new();
    for y in 0..height {
        for x in 0..width {
            rects.extend(grid.merge_rect_at(x, y, &mut visited));
        }
    }

    rects
}

struct PixelGrid<'a> {
    width: usize,
    height: usize,
    filled: &'a [bool],
}

impl PixelGrid<'_> {
    fn merge_rect_at(
        &self,
        x: usize,
        y: usize,
        visited: &mut [bool],
    ) -> Option<Rect> {
        let idx = y * self.width + x;
        if !self.filled[idx] || visited[idx] {
            return None;
        }

        let rect_width = self.run_width(x, y, visited);
        let rect_height = self.run_height(x, y, rect_width, visited);
        for row in y..y + rect_height {
            let start = row * self.width + x;
            visited[start..start + rect_width].fill(true);
        }

        Some(pixel_span_to_local_rect(
            x as i32,
            y as i32,
            rect_width as i32,
            rect_height as i32,
        ))
    }

    fn available(&self, x: usize, y: usize, visited: &[bool]) -> bool {
        let idx = y * self.width + x;
        self.filled[idx] && !visited[idx]
    }

    fn run_width(&self, x: usize, y: usize, visited: &[bool]) -> usize {
        let mut rect_width = 1usize;
        while x + rect_width < self.width
            && self.available(x + rect_width, y, visited)
        {
            rect_width += 1;
        }
        rect_width
    }

    fn run_height(
        &self,
        x: usize,
        y: usize,
        rect_width: usize,
        visited: &[bool],
    ) -> usize {
        let mut rect_height = 1usize;
        while y + rect_height < self.height
            && (0..rect_width)
                .all(|dx| self.available(x + dx, y + rect_height, visited))
        {
            rect_height += 1;
        }
        rect_height
    }
}

/// Converts a sprite pixel span to a rect with a bottom-left origin and upward y.
fn pixel_span_to_local_rect(x: i32, y: i32, width: i32, height: i32) -> Rect {
    let size_y = POD_SPRITE_SIZE.y as i32;
    let min = Vec2::new(x as f32, (size_y - (y + height)) as f32);
    let max = Vec2::new((x + width) as f32, (size_y - y) as f32);
    Rect::from_corners(min, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_area_fits_the_dim_shader_rect_budget() {
        assert!(
            light_mask().carve_rects.len()
                <= crate::graphics::post_processing::capsule_dim::CAPSULE_DIM_RECT_LIMIT
        );
    }

    #[test]
    fn cabin_is_enclosed_between_the_shell_walls() {
        let mask = collision_mask();
        let cabin_center = INSIDE_TRIGGER.local_rect().center();
        let solid_at = |point: Vec2| {
            mask.wall_rects.iter().any(|rect| rect.contains(point))
        };
        assert!(!solid_at(cabin_center));
        let walls = [Vec2::new(11.5, cabin_center.y), Vec2::new(42.5, cabin_center.y)];
        assert!(walls.into_iter().all(solid_at));
    }
}
