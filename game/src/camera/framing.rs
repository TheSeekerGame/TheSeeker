//! Framing: where the camera rig heads each tick. A scripted focus or the player's situation
//! can claim the camera; otherwise it follows the player through the level, holding still
//! while they stay inside a dead zone and leaning toward the open space ahead as they leave it.
use std::collections::{HashSet, VecDeque};

use bevy::ecs::query::QueryData;
use bevy::ecs::system::SystemParam;
use theseeker_engine::physics::{CoverCollisionTiles, LinearVelocity};

use super::{CameraShake, GameplayPixelScale};
use crate::game::enemy::Enemy;
use crate::game::player::foreground::ActionTag;
use crate::game::player::skills::portable_tent::{interior_center_world, TentInteriorAssets, TentInteriorOccupant};
use crate::game::player::skills::types::flicker_strike_metadata;
use crate::game::player::{states::FlickerStriking, Grounded, Player};
use crate::game::spikes::SpikeRespawnPending;
use crate::game::world_objects::{Capsule, CapsuleOccupant};
use crate::levelgen::mapgen::CHUNK_IMAGE_SIZE;
use crate::levelgen::{CompiledLevel, LevelChunkOccupancyMask};
use crate::prelude::*;

/// The share of the way to its target a claimed camera eases each tick, per axis.
const EASE_SHARE: Vec2 = Vec2::new(0.02, 0.05);
/// The dead zone's half size as a share of the view; the player moves freely inside it.
const DEAD_ZONE_SHARE: Vec2 = Vec2::new(1.0 / 8.0, 1.0 / 6.0);
/// Tracking ends once the player is back within this share of the dead zone.
const TRACKING_RELEASE_SHARE: f32 = 0.5;
/// How far inside the dead zone tracking holds the player, as a share of it.
const TRACKING_INSET_SHARE: f32 = 0.8;
/// How much of the open space's pull tracking leans by, and at most how far, per axis.
const GEOMETRY_GUIDANCE: f32 = 0.25;
const MAX_GEOMETRY_BIAS: Vec2 = Vec2::new(7.5, 4.5);
/// The spring's smoothing time per axis, in seconds.
const SMOOTH_TIME: Vec2 = Vec2::new(0.55, 0.42);
/// The spring rests on its target once within this distance and below this speed.
const SPRING_SETTLE_DISTANCE: f32 = 0.01;
const SPRING_SETTLE_SPEED: f32 = 0.01;
/// How far below a falling player the camera looks at full strength, as a share of the view.
const FALL_LOOK_DOWN_SHARE: f32 = 0.14;
/// The drop, as a share of the view, and the falling speed over which fall framing ramps in.
const FALL_DEPTH_RAMP_SHARE: f32 = 0.1;
const FALL_SPEED_RAMP: f32 = 120.0;
/// How quickly fall framing blends toward its strength, in seconds.
const FALL_BLEND_SECONDS: f32 = 0.12;
/// How far from the view's center the player may stray before dragging the camera along, as
/// shares of the view: to either side or above, and below.
const VISIBLE_SIDE_SHARE: f32 = 0.4;
const VISIBLE_BOTTOM_SHARE: f32 = 0.32;
/// The open-space search covers this multiple of the view.
const SEARCH_VIEW_SCALE: f32 = 1.1;
/// How strongly the search favors open space toward where the player is heading.
const SEARCH_DIRECTION_BIAS: f32 = 0.3;

/// Snaps the camera to the player, holding it there until the player is grounded.
#[derive(Resource, Default)]
pub struct SnapCameraNext;

/// Scripted camera center that replaces player framing while the resource exists.
#[derive(Resource, Clone, Copy, Debug)]
pub struct ScriptedCameraFocus(pub Vec2);

/// The camera's position and heading, with the state of player framing.
#[derive(Resource, Default)]
pub(crate) struct CameraRig {
    /// The camera's position before screen shake.
    pub(super) position: Vec2,
    /// The camera's position a tick earlier.
    pub(super) previous: Vec2,
    /// Where the camera heads, including the look-ahead while falling.
    target: Vec2,
    /// The center of the framing, around which lies the player's dead zone.
    anchor: Vec2,
    /// Whether the player is being framed around `anchor`.
    framing: bool,
    /// Each axis's spring and tracking, x then y.
    axes: [AxisMotion; 2],
    fall: FallFraming,
}

/// One axis of the camera's spring, and how it tracks the player.
#[derive(Clone, Copy, Default)]
struct AxisMotion {
    velocity: f32,
    /// The direction the player left the dead zone in, or zero while they are inside it.
    tracking_direction: f32,
    /// How far tracking leans toward the open space ahead.
    geometry_bias: f32,
}

/// How strongly a fall draws the camera down to show what lies below.
#[derive(Clone, Copy, Default)]
struct FallFraming {
    /// The height the player last stood at.
    takeoff_height: f32,
    strength: f32,
}

/// The player's motion, as framing sees it.
#[derive(Clone, Copy, Default)]
struct PlayerMotion {
    position: Vec2,
    /// World units per second.
    velocity: Vec2,
    grounded: bool,
}

/// What claims the camera this tick.
enum Focus {
    /// A scripted center, or the inside of the tent the player is in: the camera holds it.
    Hold(Vec2),
    /// The player after a spawn: the camera jumps there and stays until they have landed.
    Snap { position: Vec2, landed: bool },
    /// The capsule the player shelters in, or a flicker strike's targets: the camera eases there.
    Ease(Vec2),
    /// The player at large, framed by the level.
    Player(PlayerMotion),
    /// No one: the camera settles on its last framing.
    Settle,
}

/// The player components framing reads.
#[derive(QueryData)]
struct FramedPlayer {
    transform: &'static Transform,
    velocity: &'static LinearVelocity,
    grounded: Has<Grounded>,
    flicker_striking: Has<ActionTag<FlickerStriking>>,
    interior: Option<&'static TentInteriorOccupant>,
    shelter: Option<&'static CapsuleOccupant>,
}

/// What may claim the camera: a scripted focus, a spawn snap, and the player's situation.
#[derive(SystemParam)]
pub(super) struct FramingSubject<'w, 's> {
    scripted: Option<Res<'w, ScriptedCameraFocus>>,
    snap: Option<Res<'w, SnapCameraNext>>,
    players: Query<'w, 's, FramedPlayer, (With<Player>, Without<SpikeRespawnPending>)>,
    capsules: Query<'w, 's, &'static Transform, With<Capsule>>,
    enemies: Query<'w, 's, &'static Transform, With<Enemy>>,
    tent_interior: Option<Res<'w, TentInteriorAssets>>,
}

impl FramingSubject<'_, '_> {
    /// What claims the camera this tick, strongest claim first.
    fn focus(&self, tick_seconds: f32) -> Focus {
        if let Some(scripted) = &self.scripted {
            return Focus::Hold(scripted.0);
        }
        let Ok(player) = self.players.single() else {
            return Focus::Settle;
        };
        let position = player.transform.translation.truncate();
        let interior = player.interior.and_then(|occupant| interior_center_world(self.tent_interior.as_deref(), occupant.kind));
        if self.snap.is_some() {
            return Focus::Snap { position: interior.unwrap_or(position), landed: player.grounded };
        }
        if let Some(center) = interior {
            return Focus::Hold(center);
        }
        if let Some(capsule) = player.shelter.and_then(|occupant| self.capsules.get(occupant.shelter).ok()) {
            return Focus::Ease(Vec2::new(capsule.translation.x, position.y));
        }
        if player.flicker_striking {
            return Focus::Ease(self.strike_center(position).unwrap_or(position));
        }
        Focus::Player(PlayerMotion {
            position,
            velocity: player.velocity.0 / tick_seconds.max(f32::EPSILON),
            grounded: player.grounded,
        })
    }

    /// The center of the enemies within a flicker strike's reach of `position`, if any.
    fn strike_center(&self, position: Vec2) -> Option<Vec2> {
        let range = flicker_strike_metadata().range;
        let (sum, count) = self.enemies.iter()
            .map(|enemy| enemy.translation.truncate())
            .filter(|enemy| position.distance(*enemy) <= range)
            .fold((Vec2::ZERO, 0), |(sum, count), enemy| (sum + enemy, count + 1));
        (count > 0).then(|| sum / count as f32)
    }
}

/// The level geometry the camera frames the player by.
#[derive(SystemParam)]
pub(super) struct FramingGeometry<'w, 's> {
    level: Option<Res<'w, CompiledLevel>>,
    chunks: Option<Res<'w, LevelChunkOccupancyMask>>,
    covers: Query<'w, 's, &'static CoverCollisionTiles>,
    search: Local<'s, OpenSpaceSearch>,
}

impl FramingGeometry<'_, '_> {
    /// Where the open space ahead draws the camera as the player at `player` leaves the dead
    /// zone in `direction`: its center, kept within the dead zone and off the sides crossed.
    fn focus(&mut self, player: Vec2, view: Vec2, direction: Vec2) -> Vec2 {
        let Some(level) = self.level.as_deref() else {
            return player;
        };
        self.search.covers.clear();
        self.search.covers.extend(self.covers.iter().flat_map(|cover| cover.tiles.iter().copied()));
        let dead_zone = view * DEAD_ZONE_SHARE;
        let focus = self.search.centroid(level, self.chunks.as_deref(), player, view, direction)
            .unwrap_or(player)
            .clamp(player - dead_zone, player + dead_zone);
        constrain_focus_to_crossed_sides(focus, player, direction)
    }
}

/// Moves the rig toward what claims the camera this tick.
pub(super) fn follow_player(
    mut commands: Commands,
    mut rig: ResMut<CameraRig>,
    subject: FramingSubject,
    mut geometry: FramingGeometry,
    scale: Res<GameplayPixelScale>,
    shake: Option<Res<CameraShake>>,
    time: Res<GameTime>,
) {
    rig.previous = rig.position;
    let tick_seconds = time.seconds_per_tick() as f32;
    let paused = shake.is_some_and(|shake| shake.pauses_follow());
    // Easing ends within a screen pixel of the target.
    let settle = scale.projection_scale();
    let view = scale.visible_game_size();
    match subject.focus(tick_seconds) {
        Focus::Hold(center) => rig.hold(center),
        Focus::Snap { position, landed } => {
            rig.hold(position);
            rig.previous = position;
            // Spawn markers sit a few pixels above the floor; following the settle rigidly
            // keeps the first framed view from panning.
            if landed {
                commands.remove_resource::<SnapCameraNext>();
            }
        },
        Focus::Ease(target) => rig.ease_toward(target, paused, settle),
        Focus::Player(player) if geometry.level.is_none() => rig.ease_toward(player.position, paused, settle),
        Focus::Player(player) if paused => rig.keep_player_visible(player, view),
        Focus::Player(player) => rig.frame(player, view, tick_seconds, |direction| geometry.focus(player.position, view, direction)),
        Focus::Settle => rig.settle(tick_seconds, paused),
    }
}

impl CameraRig {
    /// Holds the camera at `center`, suspending player framing.
    fn hold(&mut self, center: Vec2) {
        self.suspend_framing();
        self.target = center;
        self.position = center;
    }

    /// Eases the camera a fixed share of the way to `target` each tick unless `paused`,
    /// suspending player framing, and lands on it within `settle`.
    fn ease_toward(&mut self, target: Vec2, paused: bool, settle: f32) {
        self.suspend_framing();
        self.target = target;
        if paused {
            return;
        }
        if (self.position - target).length() < settle {
            self.position = target;
        } else {
            self.position += (target - self.position) * EASE_SHARE;
        }
    }

    /// Settles the camera on its last framing while there is no player to frame.
    fn settle(&mut self, tick_seconds: f32, paused: bool) {
        if !self.framing {
            return;
        }
        self.target = self.anchor;
        if !paused {
            self.advance(tick_seconds);
        }
    }

    /// Stops framing the player, so it starts afresh when the player is framed again.
    fn suspend_framing(&mut self) {
        self.framing = false;
        self.axes = Default::default();
        self.fall = FallFraming::default();
    }

    /// Frames `player` in a `view`-sized view: holds still while the player stays inside the
    /// dead zone, then tracks them, leaning toward `open_space(direction)` when they leave it
    /// in `direction`, looking ahead during falls, and keeping them in sight.
    fn frame(&mut self, player: PlayerMotion, view: Vec2, tick_seconds: f32, open_space: impl FnOnce(Vec2) -> Vec2) {
        let dead_zone = view * DEAD_ZONE_SHARE;
        let direction = self.leaving(player.position, dead_zone);
        let lean = if direction == Vec2::ZERO { Vec2::ZERO } else { open_space(direction) - player.position };
        for (axis, motion) in self.axes.iter_mut().enumerate() {
            motion.begin_tracking(direction[axis], lean[axis], MAX_GEOMETRY_BIAS[axis]);
            motion.track(&mut self.anchor[axis], player.position[axis], dead_zone[axis]);
        }
        self.fall.update(player, view.y, tick_seconds);
        let falling = !player.grounded && player.velocity.y < 0.0;
        if falling && self.fall.strength > 0.0 {
            let fall_focus = player.position.y - view.y * FALL_LOOK_DOWN_SHARE * self.fall.strength;
            self.anchor.y = self.anchor.y.min(fall_focus);
        }
        self.target = self.anchor;
        // A critically damped camera trails a constant-speed target by speed * smooth_time.
        // Prediction cancels that lag during long falls.
        if falling {
            self.target.y += player.velocity.y * SMOOTH_TIME.y * self.fall.strength;
        }
        self.advance(tick_seconds);
        self.keep_player_visible(player, view);
    }

    /// The direction on each axis in which the player at `player` leaves the dead zone, if not
    /// already tracked that way. The first framed tick instead latches the anchor on them.
    fn leaving(&mut self, player: Vec2, dead_zone: Vec2) -> Vec2 {
        if !self.framing {
            self.anchor = player;
            self.target = player;
            self.fall.takeoff_height = player.y;
            self.framing = true;
            return Vec2::ZERO;
        }
        let offset = player - self.anchor;
        Vec2::from_array(std::array::from_fn(|axis| self.axes[axis].leaving(offset[axis], dead_zone[axis])))
    }

    /// Keeps the player within the visible shares of the view, dragging the framing along.
    fn keep_player_visible(&mut self, player: PlayerMotion, view: Vec2) {
        if view.min_element() <= 0.0 {
            return;
        }
        let min = player.position - view * VISIBLE_SIDE_SHARE;
        let max = player.position + view * Vec2::new(VISIBLE_SIDE_SHARE, VISIBLE_BOTTOM_SHARE);
        for (axis, motion) in self.axes.iter_mut().enumerate() {
            motion.constrain(&mut self.position[axis], &mut self.anchor[axis], &mut self.target[axis],
                player.velocity[axis], (min[axis], max[axis]));
        }
    }

    /// Springs the camera toward its target.
    fn advance(&mut self, tick_seconds: f32) {
        for (axis, motion) in self.axes.iter_mut().enumerate() {
            motion.advance(&mut self.position[axis], self.target[axis], tick_seconds, SMOOTH_TIME[axis]);
        }
    }
}

impl AxisMotion {
    /// The direction in which the player, `offset` from the anchor, leaves the dead zone, if
    /// not already tracked that way. Tracking ends once they are back well inside.
    fn leaving(&mut self, offset: f32, dead_zone: f32) -> f32 {
        if offset * self.tracking_direction <= dead_zone * TRACKING_RELEASE_SHARE {
            self.tracking_direction = 0.0;
        }
        if self.tracking_direction == 0.0 && offset.abs() > dead_zone {
            offset.signum()
        } else {
            0.0
        }
    }

    /// Tracks the player in `direction`, leaning `lean` toward the open space ahead.
    fn begin_tracking(&mut self, direction: f32, lean: f32, max_bias: f32) {
        if direction == 0.0 {
            return;
        }
        self.tracking_direction = direction;
        self.geometry_bias = (lean * direction * GEOMETRY_GUIDANCE).clamp(0.0, max_bias);
    }

    /// Drags the anchor after a tracked player, holding them inside the dead zone by an inset
    /// that the lean shortens; the anchor only ever moves their way.
    fn track(&self, anchor: &mut f32, player: f32, dead_zone: f32) {
        let inset = (dead_zone * TRACKING_INSET_SHARE - self.geometry_bias).max(dead_zone * 0.6);
        let candidate = player - self.tracking_direction * inset;
        if self.tracking_direction > 0.0 {
            *anchor = anchor.max(candidate);
        } else if self.tracking_direction < 0.0 {
            *anchor = anchor.min(candidate);
        }
    }

    /// Advances a critically damped spring from `position` toward `target`.
    fn advance(&mut self, position: &mut f32, target: f32, delta_seconds: f32, smooth_time: f32) {
        if delta_seconds <= 0.0 {
            return;
        }
        let angular_frequency = 2.0 / smooth_time.max(f32::EPSILON);
        let scaled_time = angular_frequency * delta_seconds;
        let decay = 1.0
            / (1.0
                + scaled_time
                + 0.48 * scaled_time * scaled_time
                + 0.235 * scaled_time * scaled_time * scaled_time);
        let displacement = *position - target;
        let integrated_velocity = (self.velocity + angular_frequency * displacement) * delta_seconds;
        self.velocity = (self.velocity - angular_frequency * integrated_velocity) * decay;
        *position = target + (displacement + integrated_velocity) * decay;
        if (*position - target).abs() <= SPRING_SETTLE_DISTANCE && self.velocity.abs() <= SPRING_SETTLE_SPEED {
            *position = target;
            self.velocity = 0.0;
        }
    }

    /// Pulls `position` back within `min..=max`, bringing the anchor and target along, and
    /// keeps the spring from pulling it out again faster than the player moves.
    fn constrain(&mut self, position: &mut f32, anchor: &mut f32, target: &mut f32, player_velocity: f32, (min, max): (f32, f32)) {
        if *position < min {
            *position = min;
            *anchor = anchor.max(min);
            *target = target.max(min);
            self.velocity = self.velocity.max(player_velocity);
        } else if *position > max {
            *position = max;
            *anchor = anchor.min(max);
            *target = target.min(max);
            self.velocity = self.velocity.min(player_velocity);
        }
    }
}

impl FallFraming {
    /// Blends the strength toward how far and how fast the player falls from their takeoff.
    fn update(&mut self, player: PlayerMotion, view_height: f32, dt: f32) {
        if player.grounded {
            self.takeoff_height = player.position.y;
        }
        let drop = (self.takeoff_height - player.position.y).max(0.0);
        let depth_weight = smooth_ramp(drop / (view_height * FALL_DEPTH_RAMP_SHARE).max(1.0));
        let speed_weight = smooth_ramp(-player.velocity.y / FALL_SPEED_RAMP);
        let desired = if player.grounded { 0.0 } else { depth_weight * speed_weight };
        let blend = 1.0 - (-dt.max(0.0) / FALL_BLEND_SECONDS).exp();
        self.strength += (desired - self.strength) * blend;
    }
}

/// Smoothstep over `0..=1`.
fn smooth_ramp(value: f32) -> f32 {
    let t = value.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Keeps the focus from lying behind the player on any axis they left the dead zone along.
fn constrain_focus_to_crossed_sides(focus: Vec2, player: Vec2, direction: Vec2) -> Vec2 {
    let ahead = Vec2::select(direction.cmpgt(Vec2::ZERO), focus.max(player), focus);
    Vec2::select(direction.cmplt(Vec2::ZERO), focus.min(player), ahead)
}

/// Reused buffers for the open-space search.
#[derive(Default)]
struct OpenSpaceSearch {
    /// Tiles a cover fills, which count as solid.
    covers: HashSet<IVec2>,
    visited: Vec<bool>,
    queue: VecDeque<IVec2>,
}

impl OpenSpaceSearch {
    /// The center of the open space the player at `player` can reach within the search window
    /// around them, each tile weighted by its clearance, its distance from the window's edges,
    /// and how far it lies toward `direction`.
    fn centroid(&mut self, level: &CompiledLevel, chunks: Option<&LevelChunkOccupancyMask>, player: Vec2, view: Vec2,
        direction: Vec2,
    ) -> Option<Vec2> {
        let Self { covers, visited, queue } = self;
        let window = FramingWindow::new(level, chunks, covers, player, view * (SEARCH_VIEW_SCALE * 0.5))?;
        let seed = window.nearest_open(window.tile_at(player))?;
        visited.clear();
        visited.resize(window.len(), false);
        visited[window.index(seed)?] = true;
        queue.clear();
        queue.push_back(seed);
        let (mut weighted, mut total) = (Vec2::ZERO, 0.0);
        while let Some(tile) = queue.pop_front() {
            let position = window.center_of(tile);
            let weight = window.weight(tile, direction);
            weighted += position * weight;
            total += weight;
            // Each tile in the window is looked at once, open or not.
            let neighbors = [tile + IVec2::X, tile - IVec2::X, tile + IVec2::Y, tile - IVec2::Y];
            queue.extend(neighbors.into_iter().filter(|&neighbor| window.index(neighbor)
                .is_some_and(|index| !std::mem::replace(&mut visited[index], true)) && window.is_open(neighbor)));
        }
        (total > f32::EPSILON).then(|| weighted / total)
    }
}

/// The window of level tiles the open-space search covers around the player. Tile rows count
/// down from the level's top, as in its image.
struct FramingWindow<'a> {
    level: &'a CompiledLevel,
    /// The chunks the level fills, when the mask matches its size.
    chunks: Option<&'a LevelChunkOccupancyMask>,
    covers: &'a HashSet<IVec2>,
    tile_size: f32,
    center: Vec2,
    half_size: Vec2,
    /// The window's first and last tiles.
    min: IVec2,
    max: IVec2,
}

impl<'a> FramingWindow<'a> {
    /// The level's tiles within `half_size` of `center`, or none if the level or window is empty.
    fn new(level: &'a CompiledLevel, chunks: Option<&'a LevelChunkOccupancyMask>, covers: &'a HashSet<IVec2>,
        center: Vec2, half_size: Vec2,
    ) -> Option<Self> {
        let tile_size = level.tile_size();
        if level.width == 0 || level.height == 0 || tile_size <= 0.0 || half_size.min_element() <= 0.0 {
            return None;
        }
        let chunks = chunks.filter(|mask| {
            mask.matches_chunks(level.width.div_ceil(CHUNK_IMAGE_SIZE), level.height.div_ceil(CHUNK_IMAGE_SIZE))
        });
        let last = IVec2::new(level.width as i32 - 1, level.height as i32 - 1);
        let whole = Self { level, chunks, covers, tile_size, center, half_size, min: IVec2::ZERO, max: last };
        let top_left = whole.tile_at(center + Vec2::new(-half_size.x, half_size.y));
        let bottom_right = whole.tile_at(center + Vec2::new(half_size.x, -half_size.y));
        let window = Self {
            min: top_left.min(bottom_right).clamp(IVec2::ZERO, last),
            max: top_left.max(bottom_right).clamp(IVec2::ZERO, last),
            ..whole
        };
        window.min.cmple(window.max).all().then_some(window)
    }

    fn width(&self) -> usize {
        (self.max.x - self.min.x + 1) as usize
    }

    fn len(&self) -> usize {
        self.width() * (self.max.y - self.min.y + 1) as usize
    }

    /// Where `tile` lies in the window's row-major tiles, if inside it.
    fn index(&self, tile: IVec2) -> Option<usize> {
        (tile.cmpge(self.min).all() && tile.cmple(self.max).all()).then(|| {
            let local = (tile - self.min).as_uvec2();
            local.y as usize * self.width() + local.x as usize
        })
    }

    /// The tile holding `world`, which may lie outside the level.
    fn tile_at(&self, world: Vec2) -> IVec2 {
        let local = world - self.level.bounds.min;
        IVec2::new(
            (local.x / self.tile_size).floor() as i32,
            self.level.height as i32 - 1 - (local.y / self.tile_size).floor() as i32,
        )
    }

    /// The world position of `tile`'s center.
    fn center_of(&self, tile: IVec2) -> Vec2 {
        Vec2::new(
            self.level.bounds.min.x + (tile.x as f32 + 0.5) * self.tile_size,
            self.level.bounds.min.y + (self.level.height as f32 - tile.y as f32 - 0.5) * self.tile_size,
        )
    }

    /// Whether `tile` lies in the level, in a chunk it fills, with no terrain or cover.
    fn is_open(&self, tile: IVec2) -> bool {
        let size = IVec2::new(self.level.width as i32, self.level.height as i32);
        if tile.cmplt(IVec2::ZERO).any() || tile.cmpge(size).any() || self.covers.contains(&tile) {
            return false;
        }
        let tile = tile.as_uvec2();
        self.chunks.is_none_or(|mask| mask.is_occupied(tile.x / CHUNK_IMAGE_SIZE, tile.y / CHUNK_IMAGE_SIZE))
            && !self.level.terrain.get(tile.y as usize * self.level.width as usize + tile.x as usize).copied().unwrap_or(true)
    }

    /// The open tile in the window nearest `tile`, at most four tiles away; the first in
    /// reading order among equals.
    fn nearest_open(&self, tile: IVec2) -> Option<IVec2> {
        (0..=4).find_map(|radius| {
            (tile.y - radius..=tile.y + radius)
                .flat_map(|y| (tile.x - radius..=tile.x + radius).map(move |x| IVec2::new(x, y)))
                .filter(|&candidate| self.index(candidate).is_some() && self.is_open(candidate))
                .min_by_key(|candidate| candidate.distance_squared(tile))
        })
    }

    /// How strongly an open tile draws the camera: less near the window's edges and in tight
    /// spaces, more toward `direction`.
    fn weight(&self, tile: IVec2, direction: Vec2) -> f32 {
        let normalized_offset = (self.center_of(tile) - self.center) / self.half_size;
        let edge_x = (1.0 - normalized_offset.x.abs().powi(4)).clamp(0.0, 1.0);
        let edge_y = (1.0 - normalized_offset.y.abs().powi(4)).clamp(0.0, 1.0);
        let edge_weight = 0.15 + 0.85 * edge_x * edge_y;
        let open_neighbors = (-1..=1)
            .flat_map(|y| (-1..=1).map(move |x| IVec2::new(x, y)))
            .filter(|offset| self.is_open(tile + *offset))
            .count();
        let clearance_weight = 0.4 + 0.6 * (open_neighbors as f32 / 9.0);
        let direction_weight = 1.0 + SEARCH_DIRECTION_BIAS * direction.dot(normalized_offset).clamp(-1.0, 1.0);
        edge_weight * clearance_weight * direction_weight
    }
}

#[cfg(test)]
#[path = "framing_tests.rs"]
mod tests;
