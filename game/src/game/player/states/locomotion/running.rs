use bevy::prelude::*;
use leafwing_input_manager::prelude::*;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::LinearVelocity;
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::{GameTickUpdate, GameTime};

use crate::game::gentstate::Facing;
use crate::game::player::sensors::{
    EnemyProximitySensor, GroundSensor, WallSensor,
};
use crate::game::player::states::{
    transition_locomotion, Falling, Grounded, Idle, InAir, Jumping,
    OverridesLocomotion, Running,
};
use crate::game::player::weapon::CurrentWeapon;
use crate::game::player::BowAutoAimState;
use crate::game::player::{
    Attacking, DashStrike, Dashing, InputBuffer, Knockback, Player,
    PlayerAction, PlayerGfx, PlayerStatMod, PlayerStateSet, PlayerStats, Ready,
    StatType, Whirling,
};
use crate::game::player::player_anim::set_direction_slots;

pub struct RunStatePlugin;

impl Plugin for RunStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                run_enter_system,
                run_update_system,
                run_auto_dodge_system,
                auto_dodge_roll_animation_enter,
                auto_dodge_roll_animation_exit,
            )
                .chain()
                .run_if(any_with_component::<Running>)
                .in_set(PlayerStateSet::Behavior),
        );
    }
}

// Sub-state of running: temporary roll that swaps the animation and allows enemy pass-through
#[derive(Component, Debug)]
#[component(storage = "SparseSet")]
pub struct AutoDodgeRolling {
    pub tick: u32,
}

impl Default for AutoDodgeRolling {
    fn default() -> Self {
        Self { tick: 0 }
    }
}

// Charge tracker for auto-dodge precondition (16 ticks)
#[derive(Component, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct AutoDodgeCharge {
    pub ticks: u32,
}

fn run_enter_system(
    query: Query<
        (Entity, &Gent, &Running),
        Or<(
            (
                Added<Running>,
                Without<Attacking>,
                Without<Whirling>,
                Without<Dashing>,
                Without<DashStrike>,
            ),
            (
                With<Running>,
                Added<Ready>,
                Without<Whirling>,
                Without<Dashing>,
                Without<DashStrike>,
            ),
        )>,
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
    mut commands: Commands,
) {
    for (entity, gent, _running) in query.iter() {
        commands.entity(entity).remove::<InAir>();
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            player.play_key("anim.player.Run")
        }
    }
}

fn run_update_system(
    mut query: Query<
        (
            Entity,
            &ActionState<PlayerAction>,
            &mut LinearVelocity,
            &PlayerStats,
            &PlayerStatMod,
            &mut Facing,
            &GroundSensor,
            &WallSensor,
            Has<Grounded>,
            Has<Dashing>,
            Has<OverridesLocomotion>,
            Has<DashStrike>,
            &mut InputBuffer,
        ),
        (
            With<Running>,
            With<Player>,
            Without<Knockback>,
        ),
    >,
    mut commands: Commands,
    time: Res<GameTime>,
    weapon: CurrentWeapon,
    autoaim: Res<BowAutoAimState>,
) {
    for (
        entity,
        action_state,
        mut velocity,
        stats,
        stat_mod,
        mut facing,
        ground_sensor,
        wall_sensor,
        _has_grounded_marker,
        is_dashing,
        has_locomotion_override,
        is_dash_strike,
        mut buffer,
    ) in query.iter_mut()
    {
        let direction = action_state.clamped_value(&PlayerAction::Move);
        let is_grounded = ground_sensor.is_grounded;

        // Suspend locomotion while a skill overrides movement
        if has_locomotion_override {
            continue;
        }

        // Walked off a platform → Falling
        if !is_grounded {
            transition_locomotion(
                &mut commands,
                entity,
                Falling::default(),
            );
            continue;
        }

        // Dash Strike owns horizontal control
        let controllable = !is_dash_strike;

        // Release movement input while grounded → Idle
        if direction == 0.0 && is_grounded && controllable {
            transition_locomotion(&mut commands, entity, Idle);
            continue;
        }

        // Jump (instant or buffered) while grounded → Jumping
        let current_tick = time.tick() as u64;
        if (action_state.just_pressed(&PlayerAction::Jump)
            || buffer
                .check_buffered(PlayerAction::Jump, current_tick)
                .is_some())
            && is_grounded
        {
            buffer.clear_action(PlayerAction::Jump); // Consume the buffered input
            transition_locomotion(&mut commands, entity, Jumping::new());
            continue;
        }

        // Apply per-tick horizontal displacement when controllable (auto-dodge roll allowed)
        if direction != 0.0 && controllable && !is_dashing {
            // Prevent running into walls
            let hitting_wall = (direction < 0.0 && wall_sensor.left_contact)
                || (direction > 0.0 && wall_sensor.right_contact);

            if hitting_wall {
                // Stop horizontal movement when hitting a wall
                velocity.0.x = 0.0;
            } else {
                // Per-tick displacement (stats already per-tick)
                let base_speed = stats.get(StatType::MoveVelMax);

                // Frame-perfect control including modifiers
                velocity.0.x = direction * base_speed * stat_mod.speed;
            }

            // Bow auto-aim may own facing; otherwise reflect input
            if !weapon.has_bow_equipped()
                || !autoaim.blocks_manual(*facing)
            {
                // Update facing direction regardless of wall contact
                if direction > 0.0 {
                    *facing = Facing::Right;
                } else if direction < 0.0 {
                    *facing = Facing::Left;
                }
            }
        }
        // If dashing, dash owns velocity

        // Prevent gradual sinking when grounded
        if is_grounded && !is_dashing {
            velocity.0.y = 0.0;
        }

        // Auto-dodge handled separately
    }
}

// Play roll animation on enter (also handled in update when component is inserted)
fn auto_dodge_roll_animation_enter(
    added: Query<
        (&Gent, &Facing),
        (
            With<Player>,
            With<Running>,
            Added<AutoDodgeRolling>,
        ),
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for (gent, facing) in added.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            set_direction_slots(&mut player, facing);
            player.play_key("anim.player.Roll");
        }
    }
}

// Revert to run animation on exit if still in Running
fn auto_dodge_roll_animation_exit(
    mut removed: RemovedComponents<AutoDodgeRolling>,
    running_gents: Query<&Gent, (With<Player>, With<Running>)>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for e in removed.read() {
        if let Ok(gent) = running_gents.get(e) {
            if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
                player.play_key("anim.player.Run");
            }
        }
    }
}

// Auto-dodge charge/roll lifecycle
fn run_auto_dodge_system(
    mut query: Query<
        (
            Entity,
            &ActionState<PlayerAction>,
            &GroundSensor,
            Has<OverridesLocomotion>,
            Has<Ready>,
            Option<&mut AutoDodgeCharge>,
            Option<&mut AutoDodgeRolling>,
            &EnemyProximitySensor,
        ),
        (
            With<Player>,
            With<Running>,
            Without<Knockback>,
        ),
    >,
    mut commands: Commands,
) {
    for (
        entity,
        action_state,
        ground_sensor,
        has_locomotion_override,
        has_ready,
        auto_dodge_charge_opt,
        mut auto_dodge_roll_opt,
        enemy_sensor,
    ) in query.iter_mut()
    {
        // Cancel if a skill overrides locomotion
        if has_locomotion_override {
            if auto_dodge_roll_opt.is_some() {
                commands.entity(entity).remove::<AutoDodgeRolling>();
            }
            if auto_dodge_charge_opt.is_some() {
                commands.entity(entity).remove::<AutoDodgeCharge>();
            }
            continue;
        }

        let direction = action_state.clamped_value(&PlayerAction::Move);
        let is_grounded = ground_sensor.is_grounded;

        // Rolling: tick and end after 32 ticks or if preconditions fail
        if let Some(mut auto_roll) = auto_dodge_roll_opt.as_mut() {
            if direction == 0.0 || !is_grounded {
                commands.entity(entity).remove::<AutoDodgeRolling>();
            } else {
                auto_roll.tick = auto_roll.tick.saturating_add(1);
                if auto_roll.tick >= 32 {
                    commands.entity(entity).remove::<AutoDodgeRolling>();
                }
            }
            continue;
        }

        // Charge when: running with input, grounded, in Ready, and enemy blocks path
        let precondition = has_ready
            && direction != 0.0
            && is_grounded
            && enemy_sensor.has_blocking_enemy_ahead;

        if precondition {
            let ticks = if let Some(charge) = auto_dodge_charge_opt {
                charge.ticks.saturating_add(1)
            } else {
                1
            };
            commands.entity(entity).insert(AutoDodgeCharge { ticks });
            if ticks >= 16 {
                // Begin roll and clear charge
                commands
                    .entity(entity)
                    .remove::<AutoDodgeCharge>()
                    .insert(AutoDodgeRolling::default());
            }
        } else if auto_dodge_charge_opt.is_some() {
            // Preconditions not met: reset charge if present
            commands.entity(entity).remove::<AutoDodgeCharge>();
        }
    }
}
