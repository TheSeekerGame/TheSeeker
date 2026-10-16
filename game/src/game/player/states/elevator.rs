use crate::game::player::foreground::{ActionMut, ActionTag};
use bevy::prelude::*;
use bevy::camera::visibility::RenderLayers;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::LinearVelocity;
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::{GameTickSet, GameTickUpdate};

use crate::game::player::states::Elevator;
use crate::game::player::{Player, PlayerGfx};
use crate::game::world_objects::Capsule;

pub struct ElevatorStatePlugin;

const ELEVATOR_ANIMATION_KEY: &str = "anim.player.Elevator";
const PLAYER_WORLD_RENDER_LAYER: usize = 0;
const PLAYER_OVERLAY_RENDER_LAYER: usize = 2;

impl Plugin for ElevatorStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            elevator_update_system
                .in_set(crate::game::player::PlayerBehaviorSet::Execution),
        );
        app.add_systems(
            Update,
            sync_elevator_render_layers.in_set(GameTickSet::Post),
        );
    }
}

fn elevator_update_system(
    mut query: Query<
        (ActionMut<Elevator>, &mut LinearVelocity, &mut Transform, &Gent),
        With<Player>,
    >,
    capsule_q: Query<&Transform, (With<Capsule>, Without<Player>)>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for (mut elevator, mut velocity, mut transform, gent) in query.iter_mut() {
        velocity.0 = elevator.velocity();
        if let Ok(capsule_transform) = capsule_q.get(elevator.shelter()) {
            transform.translation.x = capsule_transform.translation.x;
        }

        play_elevator_animation(gent, &mut gfx_query, elevator.execution_mut());
    }
}

fn play_elevator_animation(
    gent: &Gent,
    gfx_query: &mut Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
    execution: &mut crate::game::player::foreground::ForegroundAction,
) {
    let Ok(mut animation) = gfx_query.get_mut(gent.e_gfx) else {
        return;
    };

    if execution.playback == theseeker_engine::animation::PlaybackId::default()
        || animation.current_key() != Some(ELEVATOR_ANIMATION_KEY) {
        execution.play(&mut animation, ELEVATOR_ANIMATION_KEY);
    }
}

fn sync_elevator_render_layers(
    player_query: Query<(&Gent, Has<ActionTag<Elevator>>), With<Player>>,
    mut gfx_query: Query<&mut RenderLayers, With<PlayerGfx>>,
) {
    let world_layer = RenderLayers::layer(PLAYER_WORLD_RENDER_LAYER);
    let overlay_layer = RenderLayers::layer(PLAYER_OVERLAY_RENDER_LAYER);

    for (gent, is_elevator) in player_query.iter() {
        let Ok(mut render_layers) = gfx_query.get_mut(gent.e_gfx) else {
            continue;
        };
        let target_layers = if is_elevator {
            &world_layer
        } else {
            &overlay_layer
        };

        if *render_layers != *target_layers {
            *render_layers = target_layers.clone();
        }
    }
}
