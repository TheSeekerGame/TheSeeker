use bevy::prelude::*;
use leafwing_input_manager::prelude::*;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::LinearVelocity;
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::{GameTickUpdate, GameTime};

use crate::game::gentstate::Facing;
use crate::game::player::player_anim::set_direction_slots;
use crate::game::player::sensors::GroundSensor;
use crate::game::player::states::{
    transition_locomotion, Falling, Grounded,
    HorizontalInputIntent, Idle, InAir, Jumping,
    Running,
};
use crate::game::player::{
    InputBuffer, Player, PlayerAction, PlayerGfx,
    PlayerStatMod, PlayerStats,
};

// Idle: horizontal velocity is zeroed; when grounded, vertical is forced to 0 as well

pub struct IdleStatePlugin;

impl Plugin for IdleStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (idle_enter_system, idle_update_system)
                .chain()
                .run_if(any_with_component::<Idle>)
                .in_set(crate::game::player::PlayerBehaviorSet::Locomotion),
        );
    }
}

fn idle_enter_system(
    query: Query<
        (Entity, &Gent, &Facing, &Idle, &crate::game::player::foreground::ForegroundAction),
        Added<Idle>,
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
    mut commands: Commands,
) {
    for (entity, gent, facing, _idle, foreground) in query.iter() {
        if !foreground.owns_motion() { commands.entity(entity).remove::<InAir>(); }
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            // Clear transient slots that must not persist into Idle
            player.set_slot("AttackTransition", false);
            player.set_slot("DownwardAttack", false);
            // Ensure direction slots match `Facing` before playing Idle
            set_direction_slots(&mut player, facing);
            player.request_background("anim.player.Idle")
        }
    }
}

fn idle_update_system(
    mut query: Query<
        (
            Entity,
            &ActionState<PlayerAction>,
            &HorizontalInputIntent,
            &mut LinearVelocity,
            &PlayerStats,
            &PlayerStatMod,
            &GroundSensor,
            Has<Grounded>,
            &mut InputBuffer,
            &crate::game::player::foreground::ForegroundAction,
        ),
        (With<Idle>, With<Player>, Without<crate::game::spikes::SpikeRespawnPending>),
    >,
    mut commands: Commands,
    time: Res<GameTime>,
) {
    for (
        entity,
        action_state,
        horizontal_intent,
        mut velocity,
        _stats,
        _stat_mod,
        ground_sensor,
        _has_grounded_marker,
        mut buffer,
        foreground,
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

        // Movement input while grounded → Running
        if direction != 0.0 && is_grounded {
            transition_locomotion(&mut commands, entity, Running);
            continue;
        }

        if foreground.owns_motion() { continue; }

        // Jump (instant or buffered) when grounded → Jumping
        let current_tick = time.tick();
        if (action_state.just_pressed(&PlayerAction::Jump)
            || buffer
                .check_buffered(PlayerAction::Jump, current_tick)
                .is_some())
            && is_grounded
        {
            buffer.clear_action(PlayerAction::Jump);
            transition_locomotion(&mut commands, entity, Jumping::new());
            continue;
        }

        settle_idle_velocity(&mut velocity, is_grounded);
    }
}

fn settle_idle_velocity(velocity: &mut LinearVelocity, is_grounded: bool) {
    velocity.0.x = 0.0;
    if is_grounded {
        velocity.0.y = 0.0;
    }
}
