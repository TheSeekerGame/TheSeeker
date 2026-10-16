use crate::game::player::foreground::ActionTag;
use bevy::prelude::*;
use leafwing_input_manager::prelude::*;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::LinearVelocity;
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::{GameTickUpdate, GameTime};

use crate::game::gentstate::Facing;
use crate::game::player::player_anim::set_direction_slots;
use crate::game::player::sensors::{
    EnemyProximitySensor, GroundSensor, WallSensor,
};
use crate::game::player::states::{
    transition_locomotion, Falling, Grounded,
    HorizontalInputIntent, Idle, InAir, Jumping,
    Running,
};
use crate::game::player::{
     InputBuffer, Knockback, Player,
    PlayerAction, PlayerGfx, PlayerStatMod, PlayerStats, Ready,
};

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
                .in_set(crate::game::player::PlayerBehaviorSet::Locomotion),
        );
    }
}

// Sub-state of running: temporary roll that swaps the animation and allows enemy pass-through
#[derive(Component, Debug)]
#[component(storage = "SparseSet")]
#[derive(Default)]
pub struct AutoDodgeRolling {
    pub tick: u32,
}


// Charge tracker for auto-dodge precondition (16 ticks)
#[derive(Component, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct AutoDodgeCharge {
    pub ticks: u32,
}

fn run_enter_system(
    query: Query<
        (Entity, &Gent, &Running, &crate::game::player::foreground::ForegroundAction),
        Added<Running>,
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
    mut commands: Commands,
) {
    for (entity, gent, _running, foreground) in query.iter() {
        if !foreground.owns_motion() { commands.entity(entity).remove::<InAir>(); }
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            player.request_background("anim.player.Run")
        }
    }
}

fn run_update_system(
    mut query: Query<
        (
            Entity,
            &ActionState<PlayerAction>,
            &HorizontalInputIntent,
            &mut LinearVelocity,
            &PlayerStats,
            &PlayerStatMod,
            &GroundSensor,
            &WallSensor,
            Has<Grounded>,
            &crate::game::player::foreground::ForegroundAction,
            &mut InputBuffer,
        ),
        (
            With<Running>,
            With<Player>,
            Without<Knockback>,
            Without<crate::game::spikes::SpikeRespawnPending>,
        ),
    >,
    mut commands: Commands,
    time: Res<GameTime>,
) {
    for (
        entity,
        action_state,
        horizontal_intent,
        mut velocity,
        stats,
        stat_mod,
        ground_sensor,
        wall_sensor,
        _has_grounded_marker,
        foreground,
        mut buffer,
    ) in query.iter_mut()
    {
        let direction = horizontal_intent.movement;
        let is_grounded = ground_sensor.is_grounded;

        // Walked off a platform → Falling
        if !is_grounded {
            transition_locomotion(
                &mut commands,
                entity,
                Falling::default(),
            );
            continue;
        }

        // Release movement input while grounded → Idle
        if direction == 0.0 && is_grounded {
            transition_locomotion(&mut commands, entity, Idle);
            continue;
        }

        if foreground.owns_motion() { continue; }

        // Jump (instant or buffered) while grounded → Jumping
        let current_tick = time.tick();
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

        // Apply per-tick horizontal displacement when locomotion owns movement.
        if direction != 0.0 {
            apply_running_horizontal_velocity(
                &mut velocity,
                direction,
                wall_sensor,
                stats,
                stat_mod,
            );
        }

        // Prevent gradual sinking when grounded
        if is_grounded {
            velocity.0.y = 0.0;
        }

        // Auto-dodge handled separately
    }
}

fn apply_running_horizontal_velocity(
    velocity: &mut LinearVelocity,
    direction: f32,
    wall_sensor: &WallSensor,
    stats: &PlayerStats,
    stat_mod: &PlayerStatMod,
) {
    if running_into_wall(direction, wall_sensor) {
        velocity.0.x = 0.0;
        return;
    }

    let base_speed = stats.max_move_velocity;
    velocity.0.x = direction * base_speed * stat_mod.speed;
}

fn running_into_wall(direction: f32, wall_sensor: &WallSensor) -> bool {
    (direction < 0.0 && wall_sensor.left_contact)
        || (direction > 0.0 && wall_sensor.right_contact)
}

fn clear_auto_dodge_components(
    commands: &mut Commands,
    entity: Entity,
    has_charge: bool,
    has_roll: bool,
) {
    if has_roll {
        commands.entity(entity).remove::<AutoDodgeRolling>();
    }
    if has_charge {
        commands.entity(entity).remove::<AutoDodgeCharge>();
    }
}

fn update_auto_dodge_roll(
    commands: &mut Commands,
    entity: Entity,
    auto_roll: &mut AutoDodgeRolling,
    direction: f32,
    is_grounded: bool,
) {
    if direction == 0.0 || !is_grounded {
        commands.entity(entity).remove::<AutoDodgeRolling>();
        return;
    }

    auto_roll.tick = auto_roll.tick.saturating_add(1);
    if auto_roll.tick >= 32 {
        commands.entity(entity).remove::<AutoDodgeRolling>();
    }
}

fn update_auto_dodge_charge(
    commands: &mut Commands,
    entity: Entity,
    auto_dodge_charge: Option<&mut AutoDodgeCharge>,
    precondition: bool,
) {
    if !precondition {
        if auto_dodge_charge.is_some() {
            commands.entity(entity).remove::<AutoDodgeCharge>();
        }
        return;
    }

    let ticks = auto_dodge_charge
        .map(|charge| charge.ticks.saturating_add(1))
        .unwrap_or(1);
    commands.entity(entity).insert(AutoDodgeCharge { ticks });
    if ticks >= 16 {
        commands
            .entity(entity)
            .remove::<AutoDodgeCharge>()
            .insert(AutoDodgeRolling::default());
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
            player.request_background("anim.player.Roll");
        }
    }
}

// Revert to run animation on exit if still in Running
fn auto_dodge_roll_animation_exit(
    mut removed: RemovedComponents<AutoDodgeRolling>,
    running_gents: Query<&Gent, (With<Player>, With<Running>)>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for entity in removed.read() {
        restart_run_animation(entity, &running_gents, &mut gfx_query);
    }
}

fn restart_run_animation(
    entity: Entity,
    running_gents: &Query<&Gent, (With<Player>, With<Running>)>,
    gfx_query: &mut Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    let Ok(gent) = running_gents.get(entity) else {
        return;
    };
    let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) else {
        return;
    };

    player.request_background("anim.player.Run");
}

// Auto-dodge charge/roll lifecycle
fn run_auto_dodge_system(
    mut query: Query<
        (
            Entity,
            &HorizontalInputIntent,
            &GroundSensor,
            &crate::game::player::foreground::ForegroundAction,
            Has<ActionTag<Ready>>,
            Option<&mut AutoDodgeCharge>,
            Option<&mut AutoDodgeRolling>,
            &EnemyProximitySensor,
        ),
        (
            With<Player>,
            With<Running>,
            Without<Knockback>,
            Without<crate::game::spikes::SpikeRespawnPending>,
        ),
    >,
    mut commands: Commands,
) {
    for (
        entity,
        horizontal_intent,
        ground_sensor,
        foreground,
        has_ready,
        mut auto_dodge_charge_opt,
        mut auto_dodge_roll_opt,
        enemy_sensor,
    ) in query.iter_mut()
    {
        // Cancel if a skill overrides locomotion
        if foreground.owns_motion() {
            clear_auto_dodge_components(
                &mut commands,
                entity,
                auto_dodge_charge_opt.is_some(),
                auto_dodge_roll_opt.is_some(),
            );
            continue;
        }

        let direction = horizontal_intent.movement;
        let is_grounded = ground_sensor.is_grounded;

        // Rolling: tick and end after 32 ticks or if preconditions fail
        if let Some(auto_roll) = auto_dodge_roll_opt.as_mut() {
            update_auto_dodge_roll(
                &mut commands,
                entity,
                auto_roll,
                direction,
                is_grounded,
            );
            continue;
        }

        // Charge when: running with input, grounded, in Ready, and enemy blocks path
        let precondition = has_ready
            && direction != 0.0
            && is_grounded
            && enemy_sensor.has_blocking_enemy_ahead;

        update_auto_dodge_charge(
            &mut commands,
            entity,
            auto_dodge_charge_opt.as_deref_mut(),
            precondition,
        );
    }
}
