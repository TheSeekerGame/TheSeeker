use crate::game::player::foreground::{ActionMut, ActionTag};
use bevy::prelude::*;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::LinearVelocity;
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::GameTickUpdate;

use crate::game::gentstate::Facing;
use crate::game::player::player_anim::set_direction_slots;
use crate::game::player::skills::types::DASH_METADATA;
use crate::game::player::states::{Dashing, Falling};
use crate::game::player::HitFreezeTime;
use crate::game::player::{Player, PlayerStateSet};

pub struct DashingStatePlugin;

impl Plugin for DashingStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                dashing_enter_system,
                dashing_update_system,
            )
                .chain()
                .in_set(PlayerStateSet::Behavior)
                .in_set(crate::game::player::PlayerBehaviorSet::Execution),
        );
    }
}

/// Horizontal dash displacement in pixels per tick. Duration comes from `DASH_METADATA`.
pub(crate) const DASH_VELOCITY: f32 = 4.167;

// System that runs when entering Dashing state
fn dashing_enter_system(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            ActionMut<Dashing>,
            &Facing,
            &mut LinearVelocity,
            &Gent,
            Option<&HitFreezeTime>,
            Option<&mut Falling>,
        ),
        (With<Player>, Added<ActionTag<Dashing>>),
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>>,
) {
    for (
        entity,
        mut dashing,
        facing,
        mut velocity,
        gent,
        hit_freeze,
        maybe_falling,
    ) in query.iter_mut()
    {
        apply_dash_velocity(&mut velocity, &dashing, facing);
        reinsert_hit_freeze_if_active(&mut commands, entity, hit_freeze);
        reset_falling_curve(maybe_falling);
        play_dash_animation(&mut gfx_query, gent, facing, dashing.execution_mut());
    }
}

fn dash_direction(dashing: &Dashing, facing: &Facing) -> f32 {
    if dashing.dir.abs() > 0.0 {
        return dashing.dir.signum();
    }

    facing.direction()
}

fn apply_dash_velocity(
    velocity: &mut LinearVelocity,
    dashing: &Dashing,
    facing: &Facing,
) {
    let dir = dash_direction(dashing, facing);
    velocity.0.x = dir * DASH_VELOCITY * dashing.speed_mod;
    velocity.0.y = 0.0;
}

fn reinsert_hit_freeze_if_active(
    commands: &mut Commands,
    entity: Entity,
    hit_freeze: Option<&HitFreezeTime>,
) {
    let Some(hit_freeze) = hit_freeze else {
        return;
    };
    if hit_freeze.0 == 0 {
        return;
    }

    commands.entity(entity).insert(HitFreezeTime(
        hit_freeze.0,
        hit_freeze.1,
    ));
}

fn reset_falling_curve(maybe_falling: Option<Mut<'_, Falling>>) {
    let Some(mut falling) = maybe_falling else {
        return;
    };

    falling.fall_ticks = 0;
    falling.wall_slide = None;
}

fn play_dash_animation(
    gfx_query: &mut Query<&mut ScriptPlayer<SpriteAnimation>>,
    gent: &Gent,
    facing: &Facing,
    execution: &mut crate::game::player::foreground::ForegroundAction,
) {
    let Ok(mut script_player) = gfx_query.get_mut(gent.e_gfx) else {
        return;
    };

    set_direction_slots(&mut script_player, facing);
    execution.play(&mut script_player, DASH_METADATA.animation_key);
}

// System that updates dashing state each tick
fn dashing_update_system(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            ActionMut<Dashing>,
            &mut LinearVelocity,
            &Facing,
        ),
        (
            With<Player>,
            With<ActionTag<crate::game::player::Dashing>>,
        ),
    >,
) {
    for (entity, mut dashing, mut velocity, facing) in query.iter_mut() {
        dashing.tick = dashing.tick.saturating_add(1);

        apply_dash_velocity(&mut velocity, &dashing, facing);

        if dashing.tick >= dashing.max_ticks {
            finish_dash(&mut commands, entity, &mut velocity, dashing.execution().id);
        }

        if velocity.0.x.abs() < 0.01 {
            finish_dash(&mut commands, entity, &mut velocity, dashing.execution().id);
        }
    }
}

fn finish_dash(
    commands: &mut Commands,
    entity: Entity,
    velocity: &mut LinearVelocity,
    execution: u64,
) {
    velocity.0 = Vec2::ZERO;
    crate::game::player::foreground::finish_action(commands, entity, execution);
}
