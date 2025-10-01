use bevy::prelude::*;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::LinearVelocity;
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::{GameTickUpdate, GameTime};

use crate::game::gentstate::Facing;
use crate::game::player::states::{transition_action, Dashing, Falling, Ready};
use crate::game::player::skills::types::DASH_METADATA;
use crate::game::player::HitFreezeTime;
use crate::game::player::{Player, PlayerStateSet};
use crate::game::player::player_anim::set_direction_slots;

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
                .run_if(any_with_component::<Dashing>)
                .in_set(PlayerStateSet::Behavior),
        );
    }
}

// Dash tuning – per-tick displacements (units per tick), not px/s
// Durations are owned by the dash skill; constants kept for reference
pub(crate) const DASH_VELOCITY: f32 = 4.167; // ~400 px/s at 96 Hz
// Cooldown tuning lives in `skills::types::DASH_METADATA`.

// System that runs when entering Dashing state
fn dashing_enter_system(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &Dashing,
            &Facing,
            &mut LinearVelocity,
            &Gent,
            Option<&HitFreezeTime>,
            Option<&mut Falling>,
        ),
        (With<Player>, Added<Dashing>),
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>>,
) {
    for (
        entity,
        dashing,
        facing,
        mut velocity,
        gent,
        hit_freeze,
        mut maybe_falling,
    ) in query.iter_mut()
    {
        // Seed velocity; use stored dash direction if provided, otherwise fallback to facing
        let dir = if dashing.dir.abs() > 0.0 {
            dashing.dir.signum()
        } else {
            facing.direction()
        };
        velocity.0.x = dir * DASH_VELOCITY * dashing.speed_mod;
        velocity.0.y = 0.0;

        // Re-apply hit freeze if present
        if let Some(hit_freeze) = hit_freeze {
            if hit_freeze.0 > 0 {
                commands.entity(entity).insert(HitFreezeTime(
                    hit_freeze.0,
                    hit_freeze.1,
                ));
            }
        }

        // Reset falling curve so that post-dash falling starts from the beginning
        if let Some(ref mut falling) = maybe_falling {
            falling.fall_ticks = 0;
            falling.wall_slide = None;
        }

        // Play dash animation
        if let Ok(mut script_player) = gfx_query.get_mut(gent.e_gfx) {
            // Ensure direction slots reflect current Facing before playing a key
            set_direction_slots(&mut script_player, facing);
            script_player.play_key(DASH_METADATA.animation_key);
        }
    }
}

// System that updates dashing state each tick
fn dashing_update_system(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &mut Dashing,
            &mut LinearVelocity,
            &Facing,
        ),
        (
            With<Player>,
            With<crate::game::player::Dashing>,
        ),
    >,
    time: Res<GameTime>,
) {
    for (entity, mut dashing, mut velocity, facing) in query.iter_mut() {
        // Advance lifetime counter (ticks at 96 Hz)
        dashing.tick = dashing.tick.saturating_add(1);
        // Maintain dash velocity every frame to override locomotion states (applying speed modifier)
        if !dashing.hit {
            let dir = if dashing.dir.abs() > 0.0 {
                dashing.dir.signum()
            } else {
                facing.direction()
            };
            velocity.0.x = dir * DASH_VELOCITY * dashing.speed_mod;
            velocity.0.y = 0.0;
        }

        // Increment duration by one tick (duration is seconds-like only for FX)
        dashing.duration += 1.0 / time.hz as f32;

        // Ground impact dash strike is handled by collision system to avoid duplication

        // End when tick >= max_ticks
        if dashing.tick >= dashing.max_ticks && !dashing.hit {
            // Zero velocity on exit before handing control back
            velocity.0 = Vec2::ZERO;
            end_dash(&mut commands, entity);
            // No forced locomotion transitions; locomotion resumes next tick
        }

        // Cancel dash if horizontal movement is stopped by collision
        if velocity.0.x.abs() < 0.01 {
            // Exit dash early and zero velocity to avoid carry-over
            velocity.0 = Vec2::ZERO;
            end_dash(&mut commands, entity);
        }
    }
}

fn end_dash(commands: &mut Commands, entity: Entity) {
    transition_action(commands, entity, Ready);
}

// Dash strike trigger centralized in player_collision
