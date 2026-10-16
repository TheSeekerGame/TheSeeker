//! Rectangle geometry shared by tile and game-owned collision policies.
use bevy::prelude::*;

#[derive(Clone, Copy)]
pub enum Axis {
    X,
    Y,
}

impl Axis {
    pub const fn index(self) -> usize {
        match self { Self::X => 0, Self::Y => 1 }
    }
}

#[derive(Clone, Copy)]
pub enum ProbeDirection {
    Down,
    Up,
    Left,
    Right,
}

impl ProbeDirection {
    pub const fn axis(self) -> Axis {
        match self {
            Self::Down | Self::Up => Axis::Y,
            Self::Left | Self::Right => Axis::X,
        }
    }

    pub fn positive(self) -> bool {
        matches!(self, Self::Up | Self::Right)
    }
}

pub fn intervals_overlap(a_min: f32, a_max: f32, b_min: f32, b_max: f32, epsilon: f32) -> bool {
    a_max > b_min + epsilon && a_min < b_max - epsilon
}

pub fn nearest_axis_stop(min: f32, max: f32, delta: f32, half_extent: f32, current: f32) -> Option<f32> {
    let stop = if delta < 0.0 { max + half_extent } else { min - half_extent };
    let nearer = if delta < 0.0 { stop > current } else { stop < current };
    nearer.then_some(stop)
}

/// Endpoint-overlap resolution. Candidate enumeration and swept policies belong to callers.
pub fn resolve_rects_axis(
    rects: impl Iterator<Item = Rect>,
    position: Vec2,
    delta: f32,
    half: Vec2,
    axis: Axis,
    epsilon: f32,
) -> (f32, bool) {
    let axis = axis.index();
    if delta == 0.0 { return (position[axis], false); }
    let mut target = position;
    target[axis] += delta;
    let aabb = Rect::from_corners(target - half, target + half);
    let mut resolved = target[axis];
    let mut blocked = false;
    for rect in rects {
        if !intervals_overlap(aabb.min.x, aabb.max.x, rect.min.x, rect.max.x, epsilon)
            || !intervals_overlap(aabb.min.y, aabb.max.y, rect.min.y, rect.max.y, epsilon) {
            continue;
        }
        if let Some(stop) = nearest_axis_stop(rect.min[axis], rect.max[axis], delta, half[axis], resolved) {
            resolved = stop;
            blocked = true;
        }
    }
    (resolved, blocked)
}

/// Ground probes measure from the bottom; other directions measure from the center.
pub fn distance_to_interval(
    direction: ProbeDirection,
    min: f32,
    max: f32,
    center: f32,
    half: f32,
    epsilon: f32,
) -> Option<f32> {
    if direction.positive() {
        if min < center + half - epsilon { return None; }
        return Some((min - center).max(0.0));
    }
    let edge = center - half;
    if max > edge + epsilon { return None; }
    let start = if matches!(direction, ProbeDirection::Down) { edge } else { center };
    Some((start - max).max(0.0))
}

pub fn distance_to_rects(
    rects: impl Iterator<Item = Rect>,
    position: Vec2,
    half: Vec2,
    max_distance: f32,
    direction: ProbeDirection,
    epsilon: f32,
) -> f32 {
    let axis = direction.axis().index();
    let cross = 1 - axis;
    let aabb = Rect::from_corners(position - half, position + half);
    rects.fold(max_distance, |best, rect| {
        if !intervals_overlap(aabb.min[cross], aabb.max[cross], rect.min[cross], rect.max[cross], epsilon) {
            return best;
        }
        distance_to_interval(direction, rect.min[axis], rect.max[axis], position[axis], half[axis], epsilon)
            .map_or(best, |distance| best.min(distance))
    })
}

#[cfg(test)]
#[path = "geometry_tests.rs"]
mod tests;
