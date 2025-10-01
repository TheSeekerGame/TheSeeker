//! Player animation helpers.
//!
//! Centralizes direction slot updates and small event-driven animation slot toggles
//! (e.g., XP orb pickup, player damage).
use bevy::ecs::event::EventReader;
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
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            *old_direction = *current_direction;

            // If wall sliding, defer flip control to wall-slide logic
            if let Some(falling) = maybe_falling {
                if falling.wall_slide.is_some() {
                    if *old_direction != *current_direction {
                        player.set_slot("DirectionChanged", true);
                    } else {
                        player.set_slot("DirectionChanged", false);
                    }
                    continue;
                }
            }

            // Use the facing direction as-is for non-wall-slide cases
            set_direction_slots(&mut player, facing);
            *current_direction = matches!(facing, Facing::Right);

            // Simple change detection for DirectionChanged slot
            if *old_direction != *current_direction {
                player.set_slot("DirectionChanged", true);
            } else {
                player.set_slot("DirectionChanged", false);
            }
        }
    }
}

// SerpentRing/FrenziedAttack slot setting moved to passive animation slot aggregator

fn xp_orb_animation_handler(
    mut xp_events: EventReader<XpOrbPickup>,
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
    mut damage_events: EventReader<DamageInfo>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
    player_query: Query<(), With<crate::game::player::Player>>,
) {
    // Only set the Damaged slot when the PLAYER entity is the damage TARGET.
    // Previously, this fired on any damage in the game and would misfire on enemy hits.
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
