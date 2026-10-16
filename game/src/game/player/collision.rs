//! Player collision systems.
//!
//! Uses the custom tile-based collision world for wall contacts and the
//! custom entity collision world for enemy-only interactions.
use super::collision_view::{PlayerCollisionView, CapsuleCollisionQuery};
use theseeker_engine::physics::geometry::{Axis, ProbeDirection};
use crate::game::player::foreground::{ActionRef, ActionTag};
use bevy::ecs::change_detection::Mut;
use glam::Vec3Swizzles;
use theseeker_engine::physics::{
    Collider, CollisionGroups, LinearVelocity, PhysicsWorld, ShapeCast, TileCollisionWorld, ENEMY,
    PLAYER,
};

use crate::game::effects::stealthed::StealthEffect;
use crate::game::enemy::Enemy;
use crate::game::gentstate::Dead;
use crate::game::player::{
    skills::portable_tent::{TentInteriorAssets, TentInteriorOccupant},
    states::{
        BurningDashing, DashStrike, Dashing, Elevator,
        Falling, WallSide, Whirling,
    },
    Player, PlayerStateSet, WallSlideTime,
};
use crate::game::world_objects::{
    capsule_elevator_stop_y,
    sync_capsule_drop_platform_state, Capsule,
    CapsuleDropPlatformState,
};
use theseeker_engine::profile::TickProfiler;
use crate::prelude::*;

/// Player collision systems.
pub(crate) struct PlayerCollisionPlugin;

const CAPSULE_TRANSIT_STOP_EPS: f32 = 0.001;

impl Plugin for PlayerCollisionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            player_collisions
                .in_set(PlayerStateSet::Collisions)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

#[expect(clippy::too_many_arguments, reason = "Bevy system dependencies are expressed as explicit system parameters.")]
#[expect(
    clippy::too_many_lines,
    reason = "This collision system resolves player tiles, enemy blockers, inside checks, and profile counters in one ordered physics pass."
)]
pub fn player_collisions(
    collision_world: Option<Res<TileCollisionWorld>>,
    spatial_query: PhysicsWorld,
    mut q_gent: Query<
        (
            Entity,
            &mut Transform,
            &mut LinearVelocity,
            &Collider,
            Option<&mut WallSlideTime>,
            Option<ActionRef<Dashing>>,
            Has<ActionTag<DashStrike>>,
            Has<ActionTag<Whirling>>,
            Has<ActionTag<BurningDashing>>,
            Has<ActionTag<crate::game::player::states::FlickerStriking>>,
            Has<crate::game::player::states::locomotion::running::AutoDodgeRolling>,
            Option<&Falling>,
            Has<StealthEffect>,
            Option<&TentInteriorOccupant>,
            &mut CapsuleDropPlatformState,
        ),
        (With<Player>, Without<Dead>, Without<crate::game::spikes::SpikeRespawnPending>),
    >,
    q_enemy: Query<(), With<Enemy>>,
    q_capsule: CapsuleCollisionQuery<(With<Capsule>, Without<Player>)>,
    q_elevator: Query<Option<ActionRef<Elevator>>, With<Player>>,
    mut commands: Commands,
    time: Res<GameTime>,
    tent_interior: Option<Res<TentInteriorAssets>>,
    mut profiler: TickProfiler,
) {
    let _profile = profiler.scope("player_collisions");
    for (
        entity,
        mut pos,
        mut linear_velocity,
        collider,
        slide,
        dashing,
        is_dash_strike,
        is_whirling,
        is_burning_dashing,
        is_flicker_striking,
        is_auto_dodge_rolling,
        falling_state,
        is_stealthed,
        interior_occupant,
        mut drop_platform_state,
    ) in q_gent.iter_mut()
    {
        let Some(view) = PlayerCollisionView::new(interior_occupant,
            tent_interior.as_deref(), collision_world.as_deref(), &q_capsule) else {
            continue;
        };
        let origin = view.origin;

        let Some(half_extents) = collider.cuboid_half_extents() else {
            continue;
        };
        let original_pos = pos.translation.xy() - origin;
        let z = pos.translation.z;
        let mut velocity = linear_velocity.0;
        let wall_side = falling_state.and_then(|f| f.wall_slide);
        let is_wall_sliding = wall_side.is_some();

        if is_wall_sliding && velocity.y > 0.0 {
            velocity.y = 0.0;
        }

        let elevator = q_elevator.get(entity).ok().flatten();
        let active_elevator = elevator
            .as_deref().copied()
            .filter(|elevator| {
                interior_occupant.is_none()
                    && q_capsule.get(elevator.shelter()).is_ok()
            });
        if elevator.is_some() && active_elevator.is_none() {
            finish_optional_action(elevator, &mut commands, entity);
        }
        if let Some(elevator) = active_elevator {
            velocity = elevator.velocity();
        }
        let elevator_stop_y =
            active_elevator.and_then(|elevator| {
                capsule_elevator_stop_y(
                    pos.translation.xy(),
                    velocity.y,
                    half_extents,
                    elevator,
                    &q_capsule,
                )
            });
        if let Some(stop_y) = elevator_stop_y {
            velocity.y = stop_y - pos.translation.y;
        }

        let (resolved_y, blocked_y) = view.resolve_axis(Axis::Y, original_pos,
            velocity.y, half_extents, Some(&drop_platform_state));
        if blocked_y {
            velocity.y = 0.0;
        }
        let pos_after_y = Vec2::new(original_pos.x, resolved_y);
        let reached_elevator_stop =
            elevator_stop_y.is_some_and(|stop_y| {
                ((pos_after_y.y + origin.y) - stop_y).abs()
                    <= CAPSULE_TRANSIT_STOP_EPS
            });
        let elevator_blocked_vertically = active_elevator.is_some() && blocked_y;

        let (mut final_x, blocked_x) = view.resolve_axis(Axis::X, pos_after_y,
            velocity.x, half_extents, Some(&drop_platform_state));
        if blocked_x {
            velocity.x = 0.0;
        }

        let allow_enemy_queries = !(is_whirling
            || is_burning_dashing
            || is_dash_strike
            || is_auto_dodge_rolling
            || is_stealthed
            || interior_occupant.is_some());
        let allow_enemy_block =
            allow_enemy_queries && dashing.is_none() && !is_flicker_striking;
        let allowed_dx = final_x - pos_after_y.x;
        if let Some(enemy_blocked_x) = enemy_blocked_player_x(
            allow_enemy_block,
            &spatial_query,
            entity,
            pos_after_y,
            collider,
            allowed_dx,
            &q_enemy,
        ) {
            final_x = enemy_blocked_x;
            velocity.x = 0.0;
        }

        let final_pos = Vec2::new(final_x, pos_after_y.y);

        if allow_enemy_queries && !is_flicker_striking {
            record_player_enemy_inside(
                &spatial_query,
                final_pos,
                collider,
                entity,
                &q_enemy,
                &mut commands,
            );
        }

        if velocity.x.abs() < 0.00001
            && dashing.is_some() {
                velocity = Vec2::ZERO;
                finish_optional_action(dashing, &mut commands, entity);
            }

        pos.translation = (final_pos + origin).extend(z);
        if reached_elevator_stop || elevator_blocked_vertically {
            velocity = Vec2::ZERO;
            finish_optional_action(elevator, &mut commands, entity);
        }
        linear_velocity.0 = velocity;
        if interior_occupant.is_none() {
            sync_capsule_drop_platform_state(
                original_pos + origin,
                pos.translation.xy(),
                half_extents,
                &mut drop_platform_state,
                &q_capsule,
            );
        }

        const WALL_SLIDE_CHECK_DISTANCE: f32 = 8.0;
        const WALL_SLIDE_TOLERANCE: f32 = 0.1;
        let direction = wall_side.map(|side| match side {
            WallSide::Left => ProbeDirection::Left, WallSide::Right => ProbeDirection::Right,
        });
        let wall_slide = direction.is_some_and(|direction| {
            view.distance_local(direction, final_pos, half_extents, WALL_SLIDE_CHECK_DISTANCE)
                <= half_extents.x + WALL_SLIDE_TOLERANCE
        });

        update_wall_slide_timer(slide, wall_slide, time.hz);
    }
}




fn enemy_blocked_player_x(
    allow_enemy_block: bool,
    spatial_query: &PhysicsWorld,
    entity: Entity,
    pos_after_y: Vec2,
    collider: &Collider,
    allowed_dx: f32,
    q_enemy: &Query<(), With<Enemy>>,
) -> Option<f32> {
    if !allow_enemy_block || allowed_dx.abs() <= 0.0 {
        return None;
    }

    let dir = allowed_dx.signum();
    let max_toi = allowed_dx.abs();
    let (_enemy, hit) = spatial_query.cast_shape(
        ShapeCast {
            origin: pos_after_y,
            dir: Vec2::new(dir, 0.0),
            shape: collider,
            max_toi,
            interaction: CollisionGroups::new(PLAYER, ENEMY),
            exclude: Some(entity),
        },
        |other| q_enemy.contains(other),
    )?;
    Some(pos_after_y.x + dir * hit.time_of_impact)
}

fn record_player_enemy_inside(
    spatial_query: &PhysicsWorld,
    final_pos: Vec2,
    collider: &Collider,
    entity: Entity,
    q_enemy: &Query<(), With<Enemy>>,
    commands: &mut Commands,
) {
    for other in spatial_query.intersect(
        final_pos,
        collider,
        CollisionGroups::new(PLAYER, ENEMY),
        Some(entity),
    ) {
        if !q_enemy.contains(other) {
            continue;
        }

        theseeker_engine::physics::inside::set(commands, entity, other);
    }
}




fn update_wall_slide_timer(
    slide: Option<Mut<'_, WallSlideTime>>,
    wall_slide: bool,
    tick_hz: f64,
) {
    let Some(mut slide) = slide else {
        return;
    };
    if wall_slide {
        slide.0 = 0.0;
    } else {
        slide.0 += 1.0 / tick_hz as f32;
    }
}

fn finish_optional_action<T: crate::game::player::foreground::ActionState>(
    action: Option<crate::game::player::foreground::ActionRefItem<'_, '_, T>>,
    commands: &mut Commands,
    entity: Entity,
) {
    if let Some(action) = action { action.finish(commands, entity); }
}
