use bevy::prelude::*;
use leafwing_input_manager::prelude::*;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::LinearVelocity;
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::{GameTickUpdate, GameTime};

use crate::game::gentstate::Facing;
use crate::game::player::sensors::GroundSensor;
use crate::game::player::states::{
    transition_locomotion, Dashing, Falling, Grounded, Idle, InAir, Jumping,
    OverridesLocomotion, Running,
};
use crate::game::player::weapon::CurrentWeapon;
use crate::game::player::BowAutoAimState;
use crate::game::player::{
    Attacking, DashStrike, InputBuffer, Player, PlayerAction, PlayerGfx,
    PlayerStatMod, PlayerStateSet, PlayerStats, Ready, Whirling,
};
use crate::game::player::player_anim::set_direction_slots;

// Idle: horizontal velocity is zeroed; when grounded, vertical is forced to 0 as well

pub struct IdleStatePlugin;

impl Plugin for IdleStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (idle_enter_system, idle_update_system)
                .chain()
                .run_if(any_with_component::<Idle>)
                .in_set(PlayerStateSet::Behavior),
        );
    }
}

fn idle_enter_system(
    query: Query<
        (Entity, &Gent, &Facing, &Idle),
        Or<(
            (
                Added<Idle>,
                Without<Attacking>,
                Without<Whirling>,
                Without<Dashing>,
                Without<DashStrike>,
            ),
            (
                With<Idle>,
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
    for (entity, gent, facing, _idle) in query.iter() {
        commands.entity(entity).remove::<InAir>();
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            // Clear transient slots that must not persist into Idle
            player.set_slot("AttackTransition", false);
            player.set_slot("DownwardAttack", false);
            // Ensure direction slots match `Facing` before playing Idle
            set_direction_slots(&mut player, facing);
            player.play_key("anim.player.Idle")
        }
    }
}

fn idle_update_system(
    mut query: Query<
        (
            Entity,
            &ActionState<PlayerAction>,
            &mut LinearVelocity,
            &PlayerStats,
            &PlayerStatMod,
            &mut Facing,
            &GroundSensor,
            Has<Grounded>,
            &mut InputBuffer,
            Has<Dashing>,
            Has<OverridesLocomotion>,
        ),
        (With<Idle>, With<Player>),
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
        _stats,
        _stat_mod,
        mut facing,
        ground_sensor,
        _has_grounded_marker,
        mut buffer,
        is_dashing,
        has_locomotion_override,
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

        // Movement input while grounded → Running
        if direction != 0.0 && is_grounded {
            transition_locomotion(&mut commands, entity, Running);
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
            continue;
        }

        // Jump (instant or buffered) when grounded → Jumping
        let current_tick = time.tick() as u64;
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

        // Zero velocity unless dashing (dash owns motion)
        if !is_dashing {
            velocity.0.x = 0.0;
            // Prevent gradual sinking when grounded
            if is_grounded {
                velocity.0.y = 0.0;
            }
        }
    }
}
