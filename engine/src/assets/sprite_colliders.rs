//! Sprite colliders. The magenta pixels of an animation frame mark its collider: the convex
//! hull of those pixels, covered by a few rectangles. The magenta pixels are cleared so they
//! never show.
use std::cmp::Reverse;

use bevy::math::I64Vec2;
use bevy::render::render_resource::TextureFormat;

use super::animation::SpriteAnimation;
use super::PreloadedAssets;
use crate::physics::{Collider, SpriteShapeMap};
use crate::prelude::*;

const MASK_COLOR: [u8; 4] = [255, 0, 255, 255];
/// The most rectangles a frame's collider is made of.
const MAX_RECTS: usize = 5;

/// Builds the collider of every animation frame, each image once. Frames without a mask
/// share the empty collider at index 0.
pub(super) fn populate_collider_map(
    preloaded: Res<PreloadedAssets>,
    animations: Res<Assets<SpriteAnimation>>,
    mut images: ResMut<Assets<Image>>,
    layouts: Res<Assets<TextureAtlasLayout>>,
    mut collider_map: ResMut<SpriteShapeMap>,
) {
    collider_map.shapes.push(std::array::from_fn(|_| Collider::Empty));
    let sheets = animations
        .iter()
        .filter_map(|(id, animation)| animation.resolve_image_atlas(&preloaded, preloaded.get_key_for_asset(id)));
    for (image, layout) in sheets {
        if !collider_map.map.contains_key(&image.id()) {
            add_sheet_colliders(&image, &layout, &mut images, &layouts, &mut collider_map);
        }
    }
}

/// Adds the colliders of each frame of a sprite sheet, and clears its masks from the image.
fn add_sheet_colliders(
    image: &Handle<Image>,
    layout: &Handle<TextureAtlasLayout>,
    images: &mut Assets<Image>,
    layouts: &Assets<TextureAtlasLayout>,
    collider_map: &mut SpriteShapeMap,
) {
    let (Some(mut original), Some(layout)) = (images.get_mut(image), layouts.get(layout)) else {
        warn!("image {image:?} or its atlas layout is not loaded; its frames have no colliders");
        return;
    };
    let Some(mut converted) = original.convert(TextureFormat::Rgba8UnormSrgb) else {
        warn!("image {image:?} cannot be read as RGBA8; its frames have no colliders");
        collider_map.map.insert(image.id(), vec![0; layout.textures.len()]);
        return;
    };
    let width = converted.width();
    let Some(data) = converted.data.as_mut() else {
        warn!("image {image:?} has no pixel data; its frames have no colliders");
        return;
    };
    let ids = layout
        .textures
        .iter()
        .map(|&frame| {
            frame_colliders(data, width, frame).map_or(0, |shapes| {
                collider_map.shapes.push(shapes);
                collider_map.shapes.len() - 1
            })
        })
        .collect();
    *original = converted;
    collider_map.map.insert(image.id(), ids);
}

/// The collider of the `frame` of an RGBA8 image `image_width` pixels wide, in each flip:
/// none, x, y, and both. Clears the frame's mask pixels.
fn frame_colliders(data: &mut [u8], image_width: u32, frame: URect) -> Option<[Collider; 4]> {
    let mut mask = take_mask(data, image_width, frame)?;
    fill_convex_hull(&mut mask);
    let size = mask.size.as_vec2();
    // Centered on the frame, with y up.
    let local = |pixel: UVec2| Vec2::new(pixel.x as f32 - size.x * 0.5, size.y * 0.5 - pixel.y as f32);
    let rects: Vec<Rect> = cover(mask).iter().map(|rect| Rect::from_corners(local(rect.min), local(rect.max))).collect();
    Some([Vec2::ONE, Vec2::new(-1.0, 1.0), Vec2::new(1.0, -1.0), Vec2::NEG_ONE].map(|flip| {
        Collider::multi_rect(rects.iter().map(|rect| Rect::from_corners(rect.min * flip, rect.max * flip)).collect())
    }))
}

/// A frame's collider mask: one cell per pixel, row-major.
struct Mask {
    size: UVec2,
    cells: Vec<bool>,
}

impl Mask {
    fn index(&self, cell: UVec2) -> usize {
        (cell.y * self.size.x + cell.x) as usize
    }

    fn set_cells(&self) -> impl Iterator<Item = UVec2> + '_ {
        cells_in(URect::from_corners(UVec2::ZERO, self.size)).filter(|&cell| self.cells[self.index(cell)])
    }

    /// The smallest rectangle holding every set cell.
    fn bounds(&self) -> Option<URect> {
        self.set_cells().map(|cell| URect::from_corners(cell, cell + 1)).reduce(|bounds, cell| bounds.union(cell))
    }

    fn fill(&mut self, cells: impl IntoIterator<Item = UVec2>, set: bool) {
        for cell in cells {
            let index = self.index(cell);
            self.cells[index] = set;
        }
    }
}

/// The cells of `rect`, row by row.
fn cells_in(rect: URect) -> impl Iterator<Item = UVec2> {
    (rect.min.y..rect.max.y).flat_map(move |y| (rect.min.x..rect.max.x).map(move |x| UVec2::new(x, y)))
}

/// The magenta mask of the `frame` of an RGBA8 image `image_width` pixels wide, whose pixels
/// it clears; none if the frame has no mask.
fn take_mask(data: &mut [u8], image_width: u32, frame: URect) -> Option<Mask> {
    let mut mask = Mask { size: frame.size(), cells: vec![false; frame.size().element_product() as usize] };
    for (index, pixel) in cells_in(frame).enumerate() {
        let start = (pixel.y * image_width + pixel.x) as usize * 4;
        let rgba = &mut data[start..start + 4];
        if rgba == MASK_COLOR {
            rgba.fill(0);
            mask.cells[index] = true;
        }
    }
    mask.cells.contains(&true).then_some(mask)
}

/// Also sets each cell whose center lies within the convex hull of the set cells' centers.
fn fill_convex_hull(mask: &mut Mask) {
    // Cell centers, doubled onto whole numbers.
    let center = |cell: UVec2| (cell * 2 + 1).as_i64vec2();
    let hull = convex_hull(mask.set_cells().map(center).collect());
    let Some(bounds) = mask.bounds().filter(|_| hull.len() >= 3) else {
        return;
    };
    let inside: Vec<UVec2> = cells_in(bounds).filter(|&cell| encloses(&hull, center(cell))).collect();
    mask.fill(inside, true);
}

/// The convex hull of `points` without collinear points, by Andrew's monotone chain.
fn convex_hull(mut points: Vec<I64Vec2>) -> Vec<I64Vec2> {
    points.sort_unstable_by_key(|point| (point.x, point.y));
    points.dedup();
    if points.len() <= 2 {
        return points;
    }
    let mut lower = half_hull(points.iter().copied());
    let mut upper = half_hull(points.iter().rev().copied());
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// The chain of `points` that turns left at every point it keeps.
fn half_hull(points: impl Iterator<Item = I64Vec2>) -> Vec<I64Vec2> {
    let mut hull: Vec<I64Vec2> = Vec::new();
    for point in points {
        while matches!(*hull.as_slice(), [.., a, b] if turn(a, b, point) <= 0) {
            hull.pop();
        }
        hull.push(point);
    }
    hull
}

/// Positive where `a` to `b` turns left toward `c`, negative where right, zero where straight.
fn turn(a: I64Vec2, b: I64Vec2, c: I64Vec2) -> i64 {
    (b - a).perp_dot(c - a)
}

/// Whether `point` lies within or on the convex `hull`: on one side of every edge it is not on.
fn encloses(hull: &[I64Vec2], point: I64Vec2) -> bool {
    let edges = hull.iter().zip(hull.iter().cycle().skip(1));
    let mut sides = edges.map(|(&a, &b)| turn(a, b, point).signum()).filter(|&side| side != 0);
    let first = sides.next();
    sides.all(|side| Some(side) == first)
}

/// The largest rectangles of set cells in the mask, at most `MAX_RECTS`, the last grown over
/// whatever set cells the others leave.
fn cover(mut mask: Mask) -> Vec<URect> {
    let mut rects: Vec<URect> = Vec::new();
    while rects.len() < MAX_RECTS {
        let Some(rect) = largest_rect(&mask) else {
            break;
        };
        mask.fill(cells_in(rect), false);
        rects.push(rect);
    }
    if let (Some(last), Some(rest)) = (rects.last_mut(), mask.bounds()) {
        *last = last.union(rest);
    }
    rects
}

/// The largest rectangle of set cells. Of equals, it is the one whose top-left cell comes
/// first row by row, and then the shortest.
fn largest_rect(mask: &Mask) -> Option<URect> {
    let runs = right_runs(mask);
    let UVec2 { x: width, y: height } = mask.size;
    let run = |x: u32, y: u32| runs[(y * width + x) as usize];
    // Each rectangle of set cells from each top-left cell, shortest first.
    let rects = cells_in(URect::from_corners(UVec2::ZERO, mask.size)).flat_map(|corner| {
        (corner.y..height).scan(u32::MAX, move |span, bottom| {
            *span = (*span).min(run(corner.x, bottom));
            (*span > 0).then(|| URect::from_corners(corner, UVec2::new(corner.x + *span, bottom + 1)))
        })
    });
    rects.min_by_key(|rect| Reverse(rect.size().element_product()))
}

/// How many set cells run rightward from each cell, itself included.
fn right_runs(mask: &Mask) -> Vec<u32> {
    let width = mask.size.x as usize;
    let mut runs = vec![0; mask.cells.len()];
    for (cells, runs) in mask.cells.chunks(width).zip(runs.chunks_mut(width)) {
        let mut run = 0;
        for (&set, out) in cells.iter().zip(runs).rev() {
            // A clear cell breaks the run.
            run = u32::from(set) * (run + 1);
            *out = run;
        }
    }
    runs
}
