use bevy::prelude::*;
use leafwing_input_manager::action_state::ActionState;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::{Collider, LinearVelocity};
use theseeker_engine::script::ScriptPlayer;

use crate::game::{
    gentstate::Facing,
    player::{
        Player, PlayerAction, PlayerGfx, PlayerStatMod,
        PlayerStats,
    },
};

use crate::game::physics::Knockback;
use crate::game::player::sensors::{CeilingSensor, WallSensor};
use crate::game::player::states::skill::attacking::Pogo;
use crate::game::player::states::WALL_PUSHBACK;
use crate::game::player::states::WALL_PUSHBACK_TICKS;
use crate::game::player::states::{
    transition_locomotion, Falling, Grounded, HorizontalInputIntent, InAir,
    Jumping,
};

pub struct JumpingStatePlugin;

impl Plugin for JumpingStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            crate::GameTickUpdate,
            (
                jumping_enter_system,
                jumping_update_system,
            )
                .chain()
                .run_if(any_with_component::<Jumping>)
                .in_set(crate::game::player::PlayerBehaviorSet::Locomotion),
        );
    }
}

// Exact per-tick upward displacements; values are units moved this tick
// Horizontal air-control multiplier during jump
const AIR_SPEED_FACTOR: f32 = 1.0; // Multiplier for air speed (can be tweaked for different feel)
const AIR_JUMP_ANIMATION_SLOT: &str = "AirJump";

fn play_jump_animation(
    player: &mut ScriptPlayer<SpriteAnimation>,
    jumping: &Jumping,
) {
    player.request_background("anim.player.Jump");
    player.set_slot(AIR_JUMP_ANIMATION_SLOT, jumping.jump_count > 1);
}

fn jumping_enter_system(
    query: Query<
        (Entity, &Gent, &Jumping),
        Added<Jumping>,
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
    mut commands: Commands,
) {
    for (entity, gent, jumping) in query.iter() {
        commands.entity(entity).insert(InAir);
        play_jump_animation_for_gent(gent, jumping, &mut gfx_query);
    }


}

fn play_jump_animation_for_gent(
    gent: &Gent,
    jumping: &Jumping,
    gfx_query: &mut Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) else {
        return;
    };

    play_jump_animation(&mut player, jumping);
}

#[derive(Clone, Copy)]
struct InitialWallKick {
    push_x: f32,
    base_jump_y: f32,
    facing: Facing,
}

fn initial_wall_jump_kick(
    jumping: &Jumping,
    is_grounded: bool,
    has_knockback: bool,
    collider: &Collider,
    wall_sensor: &WallSensor,
    move_dir: f32,
    base_jump_y: f32,
) -> Option<InitialWallKick> {
    if jumping.tick != 0 || is_grounded || has_knockback {
        return None;
    }

    let half_width = collider
        .cuboid_half_extents()
        .map(|half| half.x)
        .unwrap_or(2.0);
    const WALL_FORGIVENESS_DISTANCE: f32 = 0.2;
    let near_left_wall =
        wall_sensor.left_distance <= half_width + WALL_FORGIVENESS_DISTANCE;
    let near_right_wall =
        wall_sensor.right_distance <= half_width + WALL_FORGIVENESS_DISTANCE;
    if !(near_left_wall ^ near_right_wall) {
        return None;
    }

    initial_wall_kick_from_input(near_left_wall, near_right_wall, move_dir)
        .map(|(push_x, facing)| InitialWallKick {
            push_x,
            base_jump_y,
            facing,
        })
}

fn initial_wall_kick_from_input(
    near_left_wall: bool,
    near_right_wall: bool,
    move_dir: f32,
) -> Option<(f32, Facing)> {
    if near_left_wall && move_dir < -0.1 {
        return Some((WALL_PUSHBACK, Facing::Right));
    }

    if near_right_wall && move_dir > 0.1 {
        return Some((-WALL_PUSHBACK, Facing::Left));
    }

    None
}

fn apply_initial_wall_jump_kick(
    entity: Entity,
    commands: &mut Commands,
    velocity: &mut LinearVelocity,
    facing: &mut Facing,
    wall_kick: Option<InitialWallKick>,
) {
    let Some(wall_kick) = wall_kick else {
        return;
    };

    velocity.0.x = wall_kick.push_x;
    commands.entity(entity).insert(Knockback::new(
        Vec2::new(wall_kick.push_x, wall_kick.base_jump_y),
        WALL_PUSHBACK_TICKS,
    ));
    *facing = wall_kick.facing;
}

fn apply_jumping_air_control(
    velocity: &mut LinearVelocity,
    direction: f32,
    stats: &PlayerStats,
    stat_mod: &PlayerStatMod,
) {
    if direction != 0.0 {
        let base_speed = stats.max_move_velocity;
        velocity.0.x =
            direction * base_speed * AIR_SPEED_FACTOR * stat_mod.speed;
        return;
    }

    velocity.0.x = 0.0;
}

fn jumping_update_system(
    mut query: Query<
        (
            Entity,
            &mut LinearVelocity,
            &mut Jumping,
            &ActionState<PlayerAction>,
            &HorizontalInputIntent,
            &PlayerStats,
            &PlayerStatMod,
            &mut Facing,
            &CeilingSensor,
            &WallSensor,
            &Collider,
            Option<&Grounded>,
            Option<&Knockback>,
            (
                Has<Pogo>,
                &crate::game::player::foreground::ForegroundAction,
            ),
        ),
        (With<Player>, Without<crate::game::spikes::SpikeRespawnPending>),
    >,
    mut commands: Commands,
    stealth_q: Query<
        Has<crate::game::effects::stealthed::StealthEffect>,
        With<Player>,
    >,
) {
    for (
        entity,
        mut velocity,
        mut jumping,
        action_state,
        horizontal_intent,
        stats,
        stat_mod,
        mut facing,
        ceiling_sensor,
        wall_sensor,
        collider,
        maybe_grounded,
        maybe_knockback,
        (
            has_pogo,
            foreground,
        ),
    ) in query.iter_mut()
    {
        let _is_stealthed = stealth_q.get(entity).unwrap_or(false);
        // Suspend jump curve while a skill overrides movement
        if foreground.owns_motion() {
            continue;
        }
        // Pogo owns velocity while active
        if has_pogo {
            continue;
        }
        // Immediate transition to Falling on ceiling contact
        if ceiling_sensor.is_touching_ceiling {
            transition_locomotion(
                &mut commands,
                entity,
                Falling::from_jump(jumping.jump_count),
            );
            continue;
        }

        // Jump release → Falling
        if action_state.released(&PlayerAction::Jump) {
            transition_locomotion(
                &mut commands,
                entity,
                Falling::from_jump(jumping.jump_count),
            );
            continue;
        }

        // Advance jump curve or hand over to Falling when complete
        if jumping.tick >= crate::game::physics::JUMP_VELOCITIES.len() as u32 {
            // Reached end of jump curve, transition to falling
            transition_locomotion(
                &mut commands,
                entity,
                Falling::from_jump(jumping.jump_count),
            );
            continue;
        }

        // Apply per-tick vertical displacement
        velocity.0.y = crate::game::physics::jump_velocity(jumping.tick);

        // Wall-jump kickback at tick 0 when airborne and near a wall
        let initial_wall_kick = initial_wall_jump_kick(
            &jumping,
            maybe_grounded.is_some(),
            maybe_knockback.is_some(),
            collider,
            wall_sensor,
            horizontal_intent.movement,
            velocity.0.y,
        );
        apply_initial_wall_jump_kick(
            entity,
            &mut commands,
            &mut velocity,
            &mut facing,
            initial_wall_kick,
        );

        // Horizontal air control unless knockback is active
        if maybe_knockback.is_none() {
            apply_jumping_air_control(
                &mut velocity,
                horizontal_intent.movement,
                stats,
                stat_mod,
            );
        }

        // Increment tick counter
        jumping.tick += 1;

        // Ensure Grounded marker is removed while airborne
        commands.entity(entity).remove::<Grounded>();
    }
}
