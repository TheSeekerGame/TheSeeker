//! Attack auto-aim: basic ranged attacks turn toward the nearest enemy, and airborne
//! melee swings turn downward onto an enemy below.
use bevy::prelude::*;
use theseeker_engine::physics::Collider;
use theseeker_engine::time::{GameTickUpdate, GameTime};

use crate::appstate::AppState;
use crate::camera::GameplayPixelScale;
use crate::game::enemy::Enemy;
use crate::game::gentstate::{Dead, Facing};
use crate::game::player::{Player, PlayerStateSet};

const RANGED_BASIC_ATTACK_FACING_LOCK_TICKS: u64 = 48;
/// Reach used when the visible area is unknown.
const FALLBACK_VISIBLE_REACH: f32 = 160.0;
/// The area below an airborne player that turns a forward swing downward.
const MELEE_DOWNWARD_REACH: f32 = 48.0;
const MELEE_DOWNWARD_HALF_WIDTH: f32 = 10.0;
const MELEE_DOWNWARD_HALF_HEIGHT: f32 = 2.0;

pub(super) type LiveEnemies<'w, 's> = Query<
    'w,
    's,
    (&'static GlobalTransform, &'static Collider),
    (With<Enemy>, Without<Dead>),
>;

pub(super) struct RangedAutoAimPlugin;

impl Plugin for RangedAutoAimPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            enforce_ranged_attack_facing_lock
                .in_set(PlayerStateSet::Collisions)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub(super) struct RangedAttackFacingLock {
    facing: Facing,
    expires_at_tick: u64,
}

impl RangedAttackFacingLock {
    pub(super) fn new(facing: Facing, current_tick: u64) -> Self {
        Self {
            facing,
            expires_at_tick: current_tick
                + RANGED_BASIC_ATTACK_FACING_LOCK_TICKS,
        }
    }

    pub(super) fn active_facing(self, current_tick: u64) -> Option<Facing> {
        (current_tick < self.expires_at_tick).then_some(self.facing)
    }
}

pub(super) fn effective_facing(
    facing: &Facing,
    lock: Option<&RangedAttackFacingLock>,
    current_tick: u64,
) -> Facing {
    lock.and_then(|lock| lock.active_facing(current_tick))
        .unwrap_or(*facing)
}

pub(super) fn ranged_basic_attack_auto_aim_facing(
    player_pos: Vec2,
    fallback_facing: Facing,
    gameplay_pixel_scale: Option<&GameplayPixelScale>,
    enemy_query: &LiveEnemies,
) -> Facing {
    let horizontal_range = visible_reach(gameplay_pixel_scale);
    let enemy_aabbs = enemy_query.iter().filter_map(|(transform, collider)| {
        collider.world_aabb(transform.translation().xy())
    });

    ranged_basic_attack_auto_aim_facing_from_aabbs(
        player_pos,
        fallback_facing,
        horizontal_range,
        enemy_aabbs,
    )
}

/// Half the visible width: how far the player can see ahead.
pub(super) fn visible_reach(
    gameplay_pixel_scale: Option<&GameplayPixelScale>,
) -> f32 {
    gameplay_pixel_scale
        .map(|scale| scale.visible_game_size().x * 0.5)
        .filter(|range| *range > 0.0)
        .unwrap_or(FALLBACK_VISIBLE_REACH)
}

/// Whether an enemy is close below the player, which turns an airborne forward swing
/// into a downward one.
pub(super) fn melee_target_below(player_pos: Vec2, enemy_query: &LiveEnemies) -> bool {
    let min = Vec2::new(
        player_pos.x - MELEE_DOWNWARD_HALF_WIDTH,
        player_pos.y - MELEE_DOWNWARD_REACH - MELEE_DOWNWARD_HALF_HEIGHT,
    );
    let max = Vec2::new(
        player_pos.x + MELEE_DOWNWARD_HALF_WIDTH,
        player_pos.y + MELEE_DOWNWARD_HALF_HEIGHT,
    );
    enemy_query
        .iter()
        .filter_map(|(transform, collider)| collider.world_aabb(transform.translation().xy()))
        .any(|aabb| {
            max.x >= aabb.min.x && min.x <= aabb.max.x && max.y >= aabb.min.y && min.y <= aabb.max.y
        })
}

fn ranged_basic_attack_auto_aim_facing_from_aabbs(
    player_pos: Vec2,
    fallback_facing: Facing,
    horizontal_range: f32,
    enemy_aabbs: impl IntoIterator<Item = Rect>,
) -> Facing {
    enemy_aabbs
        .into_iter()
        .filter_map(|aabb| {
            ranged_basic_attack_auto_aim_candidate(
                player_pos,
                fallback_facing,
                horizontal_range,
                aabb,
            )
        })
        .min_by(|a, b| {
            a.vertical_distance
                .total_cmp(&b.vertical_distance)
                .then_with(|| {
                    a.horizontal_distance.total_cmp(&b.horizontal_distance)
                })
        })
        .map(|candidate| candidate.facing)
        .unwrap_or(fallback_facing)
}

#[derive(Clone, Copy)]
struct RangedAutoAimCandidate {
    facing: Facing,
    vertical_distance: f32,
    horizontal_distance: f32,
}

fn ranged_basic_attack_auto_aim_candidate(
    player_pos: Vec2,
    fallback_facing: Facing,
    horizontal_range: f32,
    enemy_aabb: Rect,
) -> Option<RangedAutoAimCandidate> {
    let horizontal_distance = horizontal_gap(player_pos.x, enemy_aabb);
    if horizontal_distance > horizontal_range {
        return None;
    }

    Some(RangedAutoAimCandidate {
        facing: horizontal_facing_to_aabb(player_pos.x, enemy_aabb)
            .unwrap_or(fallback_facing),
        vertical_distance: vertical_gap(player_pos.y, enemy_aabb),
        horizontal_distance,
    })
}

fn horizontal_gap(player_x: f32, aabb: Rect) -> f32 {
    if player_x < aabb.min.x {
        aabb.min.x - player_x
    } else if player_x > aabb.max.x {
        player_x - aabb.max.x
    } else {
        0.0
    }
}

fn vertical_gap(player_y: f32, aabb: Rect) -> f32 {
    if player_y < aabb.min.y {
        aabb.min.y - player_y
    } else if player_y > aabb.max.y {
        player_y - aabb.max.y
    } else {
        0.0
    }
}

fn horizontal_facing_to_aabb(player_x: f32, aabb: Rect) -> Option<Facing> {
    let center_x = (aabb.min.x + aabb.max.x) * 0.5;
    match center_x.total_cmp(&player_x) {
        std::cmp::Ordering::Greater => Some(Facing::Right),
        std::cmp::Ordering::Less => Some(Facing::Left),
        std::cmp::Ordering::Equal => None,
    }
}

fn enforce_ranged_attack_facing_lock(
    mut commands: Commands,
    time: Res<GameTime>,
    mut query: Query<
        (Entity, &mut Facing, &RangedAttackFacingLock),
        With<Player>,
    >,
) {
    for (entity, mut facing, lock) in query.iter_mut() {
        let Some(locked_facing) = lock.active_facing(time.tick()) else {
            commands.entity(entity).remove::<RangedAttackFacingLock>();
            continue;
        };

        *facing = locked_facing;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aabb(center: Vec2, half_extents: Vec2) -> Rect {
        theseeker_engine::physics::rect_around(center, half_extents)
    }

    #[test]
    fn chooses_vertically_nearest_enemy_within_horizontal_range() {
        let player_pos = Vec2::ZERO;
        let facing = ranged_basic_attack_auto_aim_facing_from_aabbs(
            player_pos,
            Facing::Right,
            100.0,
            [
                aabb(Vec2::new(20.0, 60.0), Vec2::splat(4.0)),
                aabb(Vec2::new(-40.0, 8.0), Vec2::splat(4.0)),
            ],
        );

        assert_eq!(facing, Facing::Left);
    }

    #[test]
    fn ignores_enemies_outside_horizontal_range() {
        let player_pos = Vec2::ZERO;
        let facing = ranged_basic_attack_auto_aim_facing_from_aabbs(
            player_pos,
            Facing::Right,
            20.0,
            [aabb(Vec2::new(-40.0, 0.0), Vec2::splat(4.0))],
        );

        assert_eq!(facing, Facing::Right);
    }

    #[test]
    fn keeps_fallback_direction_when_enemy_is_centered_on_player() {
        let player_pos = Vec2::ZERO;
        let facing = ranged_basic_attack_auto_aim_facing_from_aabbs(
            player_pos,
            Facing::Left,
            20.0,
            [aabb(Vec2::new(0.0, 4.0), Vec2::splat(4.0))],
        );

        assert_eq!(facing, Facing::Left);
    }
}
