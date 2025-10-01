use bevy::prelude::*;
use leafwing_input_manager::action_state::ActionState;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::{
    LinearVelocity, PhysicsWorld, ShapeCaster, GROUNDED_THRESHOLD,
};
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::GameTime;

use crate::game::{
    gentstate::Facing,
    player::{
        Attacking, DashStrike, InputBuffer, Player, PlayerAction, PlayerGfx,
        PlayerStatMod, PlayerStats, Ready, StatType, Whirling,
    },
};

use crate::game::player::sensors::WallSensor;
use crate::game::player::states::skill::attacking::Pogo;
use crate::game::player::states::WALL_PUSHBACK;
use crate::game::player::states::WALL_PUSHBACK_TICKS;
use crate::game::player::states::{
    transition_locomotion, Falling, Idle, InAir, Jumping, OverridesLocomotion,
    Running, WallSide,
};
use crate::game::player::weapon::CurrentWeapon;
use crate::game::player::BowAutoAimState;

pub struct FallingStatePlugin;

impl Plugin for FallingStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            crate::GameTickUpdate,
            (
                falling_enter_system,
                falling_update_system,
            )
                .chain()
                .run_if(any_with_component::<Falling>)
                .in_set(crate::game::player::PlayerStateSet::Behavior),
        );
    }
}

// Coyote time window (ticks @ 96 Hz)
const COYOTE_TIME_TICKS: u32 = 10;

// Horizontal air-control multiplier (applied to `MoveVelMax` each tick)
const AIR_SPEED_FACTOR: f32 = 1.0; // Multiplier for air speed (can be tweaked for different feel)

// Wall slide uses the early portion of the fall curve only
// (limit to first 14 entries; 0-based index 13)
const WALL_SLIDE_MAX_INDEX: u32 = 13;

// Exact per-tick vertical displacements; negative values move down
const FALL_VELOCITIES: &[f32] = &[
    0.0, 0.0, 0.0, 0.0, -0.047, -0.090, -0.132, -0.175, -0.217, -0.260, -0.302,
    -0.345, -0.387, -0.430, -0.472, -0.515, -0.557, -0.600, -0.642, -0.685,
    -0.727, -0.770, -0.812, -0.855, -0.897, -0.940, -0.982, -1.025, -1.067,
    -1.110, -1.152, -1.195, -1.237, -1.280, -1.322, -1.365, -1.407, -1.450,
    -1.492, -1.535, -1.577, -1.620, -1.662, -1.705, -1.705, -1.705, -1.705,
    -1.705,
];

fn falling_enter_system(
    query: Query<
        (Entity, &Gent, &Falling),
        Or<(
            (
                Added<Falling>,
                Without<Attacking>,
                Without<Whirling>,
                Without<crate::game::player::Dashing>,
                Without<DashStrike>,
            ),
            (
                With<Falling>,
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
    for (entity, gent, falling) in query.iter() {
        commands.entity(entity).insert(InAir);
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            if let Some(wall_side) = falling.wall_slide {
                player.play_key("anim.player.WallSlide");
                // Face the contacted wall to match WallSlide visuals
                match wall_side {
                    WallSide::Left => {
                        crate::game::player::player_anim::set_direction_slots(
                            &mut player,
                            &crate::game::gentstate::Facing::Left,
                        );
                    },
                    WallSide::Right => {
                        crate::game::player::player_anim::set_direction_slots(
                            &mut player,
                            &crate::game::gentstate::Facing::Right,
                        );
                    },
                }
            } else {
                // Entering normal fall (not wall sliding)
                player.play_key("anim.player.Fall");
            }
        }
    }

    // If dash ended while in Falling, restart the appropriate animation
    for entity in removed_dashing.read() {
        if let Ok((_entity, gent, falling)) = query.get(entity) {
            if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
                if falling.wall_slide.is_some() {
                    player.play_key("anim.player.WallSlide");
                } else {
                    player.play_key("anim.player.Fall");
                }
            }
        }
    }
}

fn falling_update_system(
    mut query: Query<
        (
            Entity,
            &mut LinearVelocity,
            &mut Falling,
            &ActionState<PlayerAction>,
            &mut Transform,
            &ShapeCaster,
            &PlayerStats,
            &PlayerStatMod,
            &mut Facing,
            &Gent,
            &WallSensor,
            &mut InputBuffer,
            Has<crate::game::player::Dashing>,
            Has<OverridesLocomotion>,
            Has<Pogo>,
        ),
        With<Player>,
    >,
    mut commands: Commands,
    spatial_query: PhysicsWorld,
    time: Res<GameTime>,
    weapon: CurrentWeapon,
    autoaim: Res<BowAutoAimState>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
    skill_query: Query<
        (
            Has<Attacking>,
            Has<Whirling>,
            Has<DashStrike>,
        ),
        With<Player>,
    >,
    stealth_q: Query<
        Has<crate::game::effects::stealthed::StealthEffect>,
        With<Player>,
    >,
) {
    for (
        entity,
        mut velocity,
        mut falling,
        action_state,
        mut transform,
        shape_caster,
        stats,
        stat_mod,
        mut facing,
        gent,
        wall_sensor,
        mut buffer,
        has_dashing,
        has_locomotion_override,
        has_pogo,
    ) in query.iter_mut()
    {
        let _is_stealthed = stealth_q.get(entity).unwrap_or(false);
        // Get skill states from separate query
        let (has_attacking, has_whirling, has_dash_strike) =
            skill_query.get(entity).unwrap_or((false, false, false));
        // If pogo is active, defer to pogo system
        if has_pogo {
            continue;
        }
        // Suspend locomotion while a skill overrides movement
        if has_locomotion_override {
            continue;
        }
        let move_input = action_state.value(&PlayerAction::Move);

        // Check for wall sliding conditions
        let should_wall_slide = velocity.0.y < 0.0 // Falling
            && move_input.abs() > 0.1 // Holding a direction
            && ((move_input < 0.0 && wall_sensor.left_contact) // Pushing left into left wall
                || (move_input > 0.0 && wall_sensor.right_contact)); // Pushing right into right wall

        // Wall slide state transitions and visuals
        if should_wall_slide && falling.wall_slide.is_none() {
            let wall_side = if move_input < 0.0 && wall_sensor.left_contact {
                WallSide::Left
            } else {
                WallSide::Right
            };
            falling.wall_slide = Some(wall_side);
            commands
                .entity(entity)
                .insert(crate::game::player::WallSlideTime(0.0));
            // Clamp fall index to wall-slide subset if we entered with high speed
            if falling.fall_ticks > WALL_SLIDE_MAX_INDEX {
                falling.fall_ticks = WALL_SLIDE_MAX_INDEX;
            }

            // Play wall-slide animation if no skill is driving visuals
            if !has_attacking
                && !has_whirling
                && !has_dashing
                && !has_dash_strike
            {
                if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
                    player.play_key("anim.player.WallSlide");
                    // Face the contacted wall to match the animation
                    match wall_side {
                        WallSide::Left => {
                            crate::game::player::player_anim::set_direction_slots(
                                &mut player,
                                &crate::game::gentstate::Facing::Left,
                            );
                        },
                        WallSide::Right => {
                            crate::game::player::player_anim::set_direction_slots(
                                &mut player,
                                &crate::game::gentstate::Facing::Right,
                            );
                        },
                    }
                }
            }
        } else if !should_wall_slide && falling.wall_slide.is_some() {
            // Exit wall slide; resume normal fall curve
            falling.wall_slide = None;

            // Switch back to fall animation if not in a skill state
            if !has_attacking
                && !has_whirling
                && !has_dashing
                && !has_dash_strike
            {
                if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
                    if velocity.0.y < 0.0 {
                        player.play_key("anim.player.Fall");
                    }
                }
            }
        }

        // Ground detection via ShapeCaster
        let ray_cast = shape_caster.cast(
            &spatial_query.context(),
            &transform,
            Some(entity),
        );
        let ground_hit = ray_cast.iter().next();

        // Predict landing based on time-of-impact and current per-tick displacement
        let is_grounded = match ground_hit {
            Some((_, toi)) => {
                let predicted_distance = toi.time_of_impact + velocity.0.y;
                predicted_distance < GROUNDED_THRESHOLD
            },
            None => false,
        };

        if is_grounded {
            // Determine if we should transition to Idle or Running based on input
            let input_x = action_state.value(&PlayerAction::Move);
            if input_x.abs() > 0.1 {
                transition_locomotion(
                    &mut commands,
                    entity,
                    Running::default(),
                );
            } else {
                transition_locomotion(&mut commands, entity, Idle::default());
            }
            continue;
        }

        // Jump handling (instant, buffered, wall/coyote/double)
        let current_tick = time.tick() as u64;
        if action_state.just_pressed(&PlayerAction::Jump)
            || buffer
                .check_buffered(PlayerAction::Jump, current_tick)
                .is_some()
        {
            // Wall jump
            if let Some(wall_side) = falling.wall_slide {
                let direction = action_state.clamped_value(&PlayerAction::Move);
                let pressing_into_wall = match wall_side {
                    WallSide::Left => direction < -0.1,
                    WallSide::Right => direction > 0.1,
                };

                if pressing_into_wall {
                    *facing = match wall_side {
                        WallSide::Left => Facing::Right,
                        WallSide::Right => Facing::Left,
                    };

                    // Apply horizontal kick-off and temporary lock
                    use crate::game::physics::Knockback;
                    velocity.0.x = match wall_side {
                        WallSide::Left => WALL_PUSHBACK,
                        WallSide::Right => -WALL_PUSHBACK,
                    };

                    commands.entity(entity).insert(Knockback::new(
                        Vec2::new(velocity.0.x, 0.0),
                        WALL_PUSHBACK_TICKS,
                    ));

                    // Set WallSlideTime to MAX to prevent immediate re-grab
                    commands.entity(entity).insert(
                        crate::game::player::WallSlideTime(f32::MAX),
                    );
                }
                // Transition to Jumping regardless of press direction
                buffer.clear_action(PlayerAction::Jump); // Consume the buffered input
                transition_locomotion(&mut commands, entity, Jumping::new());
                continue;
            }
            // Coyote-time jump (only when falling from ground)
            else if falling.from_ground
                && falling.coyote_ticks < COYOTE_TIME_TICKS
            {
                buffer.clear_action(PlayerAction::Jump); // Consume the buffered input
                transition_locomotion(&mut commands, entity, Jumping::new());
                continue;
            }
            // Mid-air additional jumps
            else if !falling.from_ground {
                // Base allowance: 1 initial + 1 mid-air, plus bonuses
                let allowed_jumps = 1 /* initial jump */ + 1 /* one mid-air jump */ + stat_mod.extra_jumps as u8;
                if falling.jump_count < allowed_jumps {
                    buffer.clear_action(PlayerAction::Jump); // Consume the buffered input
                    transition_locomotion(
                        &mut commands,
                        entity,
                        Jumping::with_count(falling.jump_count + 1),
                    );
                    continue;
                }
            }
        }

        // Set fall velocity based on wall slide vs. normal fall
        if falling.wall_slide.is_some() {
            // Wall slide: constrained curve and locked horizontal
            let max_idx =
                WALL_SLIDE_MAX_INDEX.min(FALL_VELOCITIES.len() as u32 - 1);
            let velocity_index = falling.fall_ticks.min(max_idx) as usize;
            velocity.0.y = FALL_VELOCITIES[velocity_index];

            // Lock horizontal during wall slide
            velocity.0.x = 0.0;

            // Face away from the wall for consistent control
            if let Some(wall_side) = falling.wall_slide {
                *facing = match wall_side {
                    WallSide::Left => Facing::Right,
                    WallSide::Right => Facing::Left,
                };
            }
        } else {
            // Normal fall curve
            let velocity_index =
                falling.fall_ticks.min(FALL_VELOCITIES.len() as u32 - 1)
                    as usize;
            velocity.0.y = FALL_VELOCITIES[velocity_index];

            // Horizontal air control
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
            } else if !has_dashing {
                // Stop horizontal motion unless dashing
                velocity.0.x = 0.0;
            }
        }

        // Advance counters
        falling.coyote_ticks += 1;
        // While wall sliding, clamp fall tick to the wall-slide subset
        if falling.wall_slide.is_some() {
            if falling.fall_ticks < WALL_SLIDE_MAX_INDEX {
                falling.fall_ticks += 1;
            }
        } else {
            let max_fall_idx = (FALL_VELOCITIES.len() as u32).saturating_sub(1);
            if falling.fall_ticks < max_fall_idx {
                falling.fall_ticks += 1;
            }
        }
    }
}
