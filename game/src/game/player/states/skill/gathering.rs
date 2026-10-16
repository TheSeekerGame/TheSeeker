use crate::game::player::foreground::{ActionMut, ActionTag};
use bevy::prelude::*;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::LinearVelocity;
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::GameTickUpdate;

use crate::game::player::player_anim::set_direction_slots;
use crate::game::player::sensors::GroundSensor;
use crate::game::player::states::Gathering;
use crate::game::player::{Player, PlayerGfx, PlayerStateSet};

pub const GATHERING_TICKS: u32 = 36;
const GATHERING_ANIMATION_KEY: &str = "anim.player.Gather";

pub struct GatheringStatePlugin;

impl Plugin for GatheringStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (gathering_enter_system, gathering_update_system)
                .chain()
                .in_set(PlayerStateSet::Behavior)
                .in_set(crate::game::player::PlayerBehaviorSet::Execution),
        );
    }
}

fn gathering_enter_system(
    mut query: Query<
        (
            ActionMut<Gathering>,
            &mut LinearVelocity,
            &crate::game::gentstate::Facing,
            &Gent,
        ),
        (With<Player>, Added<ActionTag<Gathering>>),
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for (mut gathering, mut velocity, facing, gent) in query.iter_mut() {
        velocity.0 = Vec2::ZERO;

        if let Ok(mut script_player) = gfx_query.get_mut(gent.e_gfx) {
            set_direction_slots(&mut script_player, facing);
            gathering.execution_mut().play(&mut script_player, GATHERING_ANIMATION_KEY);
        }
    }
}

fn gathering_update_system(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            ActionMut<Gathering>,
            &mut LinearVelocity,
            &GroundSensor,
        ),
        With<Player>,
    >,
) {
    for (entity, mut gathering, mut velocity, ground_sensor) in query.iter_mut()
    {
        if !ground_sensor.is_grounded {
            gathering.finish(&mut commands, entity);
            continue;
        }

        velocity.0 = Vec2::ZERO;
        gathering.tick = gathering.tick.saturating_add(1);
        if gathering.tick >= GATHERING_TICKS {
            gathering.finish(&mut commands, entity);
        }
    }
}
