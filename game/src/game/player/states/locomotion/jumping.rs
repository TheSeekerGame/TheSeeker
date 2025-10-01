use bevy::prelude::*;
use leafwing_input_manager::action_state::ActionState;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::LinearVelocity;
use theseeker_engine::script::ScriptPlayer;

use crate::game::{
    gentstate::Facing,
    player::{
        Attacking, DashStrike, Player, PlayerAction, PlayerGfx, PlayerStatMod,
        PlayerStats, Ready, StatType, Whirling,
    },
};

use crate::game::physics::Knockback;
use crate::game::player::sensors::{CeilingSensor, WallSensor};
use crate::game::player::states::skill::attacking::Pogo;
use crate::game::player::states::WALL_PUSHBACK;
use crate::game::player::states::WALL_PUSHBACK_TICKS;
use crate::game::player::states::{
    transition_locomotion, Falling, Grounded, InAir, Jumping,
    OverridesLocomotion,
};
use crate::game::player::weapon::CurrentWeapon;
use crate::game::player::BowAutoAimState;
use theseeker_engine::physics::Collider;

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
                .in_set(crate::game::player::PlayerStateSet::Behavior),
        );
    }
}

// Exact per-tick upward displacements; values are units moved this tick
const JUMP_VELOCITIES: &[f32] = &[
    0.0, 0.0, 1.615, 1.579, 1.542, 1.506, 1.469, 1.433, 1.396, 1.360, 1.323,
    1.287, 1.250, 1.214, 1.177, 1.141, 1.104, 1.068, 1.031, 0.995, 0.958,
    0.922, 0.885, 0.849, 0.812, 0.776, 0.739, 0.703, 0.666, 0.630, 0.593,
    0.557, 0.520, 0.484, 0.447, 0.411, 0.374, 0.338, 0.301, 0.265, 0.228,
    0.192, 0.155, 0.119, 0.082, 0.046, 0.010, 0.0, 0.0, 0.0, 0.0,
];

// Horizontal air-control multiplier during jump
const AIR_SPEED_FACTOR: f32 = 1.0; // Multiplier for air speed (can be tweaked for different feel)

fn jumping_enter_system(
    query: Query<
        (Entity, &Gent, &Jumping),
        Or<(
            (
                Added<Jumping>,
                Without<Attacking>,
                Without<Whirling>,
                Without<crate::game::player::Dashing>,
                Without<DashStrike>,
            ),
            (
                With<Jumping>,
                Added<Ready>,
                Without<Whirling>,
                Without<crate::game::player::Dashing>,
                Without<DashStrike>,
            ),
        )>,
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
    mut removed_dashing: RemovedComponents<crate::game::player::Dashing>,
    mut commands: Commands,
) {
    for (entity, gent, _jumping) in query.iter() {
        commands.entity(entity).insert(InAir);

        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            player.play_key("anim.player.Jump")
        }
    }

    // If dash ended while in Jumping, restart jump animation
    for entity in removed_dashing.read() {
        if let Ok((_entity, gent, _jumping)) = query.get(entity) {
            if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
                player.play_key("anim.player.Jump");
            }
        }
    }
}

fn jumping_update_system(
    mut query: Query<
        (
            Entity,
            &mut LinearVelocity,
            &mut Jumping,
            &ActionState<PlayerAction>,
            &PlayerStats,
            &PlayerStatMod,
            &mut Facing,
            &CeilingSensor,
            &WallSensor,
            &Collider,
            Option<&Grounded>,
            Option<&Knockback>,
            Has<Pogo>,
            Has<crate::game::player::Dashing>,
            Has<OverridesLocomotion>,
        ),
        With<Player>,
    >,
    mut commands: Commands,
    weapon: CurrentWeapon,
    autoaim: Res<BowAutoAimState>,
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
        stats,
        stat_mod,
        mut facing,
        ceiling_sensor,
        wall_sensor,
        collider,
        maybe_grounded,
        maybe_knockback,
        has_pogo,
        _has_dashing,
        has_locomotion_override,
    ) in query.iter_mut()
    {
        let _is_stealthed = stealth_q.get(entity).unwrap_or(false);
        // Suspend jump curve while a skill overrides movement
        if has_locomotion_override {
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
        if jumping.tick >= JUMP_VELOCITIES.len() as u32 {
            // Reached end of jump curve, transition to falling
            transition_locomotion(
                &mut commands,
                entity,
                Falling::from_jump(jumping.jump_count),
            );
            continue;
        }

        // Apply per-tick vertical displacement
        velocity.0.y = JUMP_VELOCITIES[jumping.tick as usize];

        // Wall-jump kickback at tick 0 when airborne and near a wall
        if jumping.tick == 0 {
            let is_grounded = maybe_grounded.is_some();
            if !is_grounded {
                // Determine wall proximity using collider half-width with small tolerance
                let half_width = if let Some(cuboid) = collider.as_cuboid() {
                    cuboid.half_extents().x
                } else {
                    2.0
                };
                const WALL_FORGIVENESS_DISTANCE: f32 = 0.2;
                let near_left_wall = wall_sensor.left_distance
                    <= half_width + WALL_FORGIVENESS_DISTANCE;
                let near_right_wall = wall_sensor.right_distance
                    <= half_width + WALL_FORGIVENESS_DISTANCE;

                // Require input toward the wall
                let move_dir = action_state.clamped_value(&PlayerAction::Move);
                let pressing_left = move_dir < -0.1;
                let pressing_right = move_dir > 0.1;

                // Apply when near exactly one wall (avoid ambiguous corridors)
                if (near_left_wall ^ near_right_wall)
                    && maybe_knockback.is_none()
                    && ((near_left_wall && pressing_left)
                        || (near_right_wall && pressing_right))
                {
                    // Kick away from the wall; preserve current vertical component
                    let push_x = if near_left_wall {
                        WALL_PUSHBACK
                    } else {
                        -WALL_PUSHBACK
                    };
                    let base_jump_y = velocity.0.y;

                    commands.entity(entity).insert(Knockback::new(
                        Vec2::new(push_x, base_jump_y),
                        WALL_PUSHBACK_TICKS,
                    ));

                    // Face away from the wall (bow auto-aim can override later)
                    *facing = if near_left_wall {
                        Facing::Right
                    } else {
                        Facing::Left
                    };
                }
            }
        }

        // Horizontal air control unless knockback is active
        if maybe_knockback.is_none() {
            let direction = action_state.clamped_value(&PlayerAction::Move);
            if direction != 0.0 {
                // Per-tick displacement (stats already per-tick)
                let base_speed = stats.get(StatType::MoveVelMax);
                velocity.0.x =
                    direction * base_speed * AIR_SPEED_FACTOR * stat_mod.speed;

                // Bow auto-aim may own facing; otherwise reflect input
                if !weapon.has_bow_equipped()
                    || !autoaim.blocks_manual(*facing)
                {
                    if direction > 0.0 {
                        *facing = Facing::Right;
                    } else if direction < 0.0 {
                        *facing = Facing::Left;
                    }
                }
            } else {
                // No input → zero horizontal movement
                velocity.0.x = 0.0;
            }
        }

        // Increment tick counter
        jumping.tick += 1;

        // Ensure Grounded marker is removed while airborne
        commands.entity(entity).remove::<Grounded>();
    }
}
