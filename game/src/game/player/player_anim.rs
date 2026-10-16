//! Player animation helpers.
//!
//! Centralizes direction slot updates and small event-driven animation slot toggles
//! (e.g., XP orb pickup, player damage).
use bevy::ecs::message::MessageReader;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::prelude::GameTickUpdate;
use theseeker_engine::script::ScriptPlayer;

use crate::appstate::AppState;
use crate::game::gentstate::Facing;
use crate::game::player::{Falling, PlayerGfx, PlayerStateSet};
use crate::prelude::{in_state, App, Local, Plugin, Query, With};
use bevy::ecs::schedule::IntoScheduleConfigs;

use crate::game::combat::DamageInfo;
use crate::game::player::spawns::xp_orbs::XpOrbPickup;

/// Player animation helper systems for direction slots and event-based animation triggers
pub struct PlayerAnimationPlugin;

impl Plugin for PlayerAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                // Direction slots must reflect `Facing` before picking animation keys/frames
                sprite_flip,
                // Event-based animation slot handlers
                xp_orb_animation_handler,
                player_damage_animation_handler,
            )
                .in_set(PlayerStateSet::Animation)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

/// Sets DirectionRight/DirectionLeft slots based on `Facing`.
/// Centralized to avoid duplicated branching across states.
pub fn set_direction_slots(
    anim: &mut ScriptPlayer<SpriteAnimation>,
    facing: &Facing,
) {
    match facing {
        Facing::Right => {
            anim.set_slot("DirectionRight", true);
            anim.set_slot("DirectionLeft", false);
        },
        Facing::Left => {
            anim.set_slot("DirectionRight", false);
            anim.set_slot("DirectionLeft", true);
        },
    }
}

fn sprite_flip(
    query: Query<(&Facing, &Gent, Option<&Falling>)>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
    mut current_direction: Local<bool>,
    mut old_direction: Local<bool>,
) {
    for (facing, gent, maybe_falling) in query.iter() {
        let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) else {
            continue;
        };

        update_player_direction_slots(
            &mut player,
            facing,
            maybe_falling,
            &mut current_direction,
            &mut old_direction,
        );
    }
}

fn update_player_direction_slots(
    player: &mut ScriptPlayer<SpriteAnimation>,
    facing: &Facing,
    maybe_falling: Option<&Falling>,
    current_direction: &mut Local<bool>,
    old_direction: &mut Local<bool>,
) {
    **old_direction = **current_direction;

    // If wall sliding, defer flip control to wall-slide logic.
    if is_wall_sliding(maybe_falling) {
        set_direction_changed_slot(player, **old_direction, **current_direction);
        return;
    }

    // Use the facing direction as-is for non-wall-slide cases.
    set_direction_slots(player, facing);
    **current_direction = matches!(facing, Facing::Right);

    // Simple change detection for DirectionChanged slot.
    set_direction_changed_slot(player, **old_direction, **current_direction);
}

fn is_wall_sliding(maybe_falling: Option<&Falling>) -> bool {
    maybe_falling
        .map(|falling| falling.wall_slide.is_some())
        .unwrap_or(false)
}

fn set_direction_changed_slot(
    player: &mut ScriptPlayer<SpriteAnimation>,
    old_direction: bool,
    current_direction: bool,
) {
    player.set_slot("DirectionChanged", old_direction != current_direction);
}

fn xp_orb_animation_handler(
    xp_events: MessageReader<XpOrbPickup>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    let xp_event_occurred = !xp_events.is_empty();
    for mut anim in gfx_query.iter_mut() {
        if xp_event_occurred {
            anim.set_slot("XpOrb", true);
        } else {
            anim.set_slot("XpOrb", false);
        }
    }
}

fn player_damage_animation_handler(
    mut damage_events: MessageReader<DamageInfo>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
    player_query: Query<(), With<crate::game::player::Player>>,
) {
    // Only set the Damaged slot when the PLAYER entity is the damage TARGET.
    let mut player_was_damaged = false;
    for dmg in damage_events.read() {
        if player_query.get(dmg.target).is_ok() {
            player_was_damaged = true;
            break;
        }
    }

    for mut anim in gfx_query.iter_mut() {
        if player_was_damaged {
            anim.set_slot("Damaged", true);
        } else {
            anim.set_slot("Damaged", false);
        }
    }
}
