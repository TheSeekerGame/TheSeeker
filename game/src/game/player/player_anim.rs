use bevy::prelude::{DetectChanges, Ref};
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::LinearVelocity;
use theseeker_engine::prelude::{GameTickUpdate, GameTime};
use theseeker_engine::script::ScriptPlayer;
use bevy::ecs::event::EventReader;

use super::DashStrike;
use crate::appstate::AppState;
use crate::game::gentstate::Facing;
use crate::game::player::{
    Attacking, CanAttack, Dashing, Falling, HitFreezeTime, Idle, Jumping,
    PlayerConfig, PlayerGfx, PlayerStateSet, Running, WallSlideTime, Whirling,
    Passive, Passives, Player,
};
use crate::prelude::{
    in_state, Added, App, Has, IntoSystemConfigs, Local, Or, Plugin, Query,
    Res, With, Without,
};

use super::player_weapon::CurrentWeapon;
use crate::game::xp_orbs::XpOrbPickup;
use crate::game::attack::DamageInfo;

/// play animations here, run after transitions
pub struct PlayerAnimationPlugin;

impl Plugin for PlayerAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                player_idle_animation,
                player_falling_animation,
                player_jumping_animation,
                player_running_animation,
                player_attacking_animation,
                player_whirling_animation,
                player_dashing_animation,
                player_dashing_strike_animation,
                sprite_flip.after(player_dashing_animation),
                update_serpent_ring_slot.after(sprite_flip),
                update_frenzied_attack_slot.after(update_serpent_ring_slot),
                xp_orb_animation_handler,
                player_damage_animation_handler
            )
                .in_set(PlayerStateSet::Animation)
                .after(PlayerStateSet::Transition)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

fn player_idle_animation(
    query: Query<
        &Gent,
        Or<(
            (
                Added<Idle>,
                Without<Attacking>,
                Without<Whirling>,
            ),
            (With<Idle>, Added<CanAttack>),
        )>,
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for gent in query.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            player.play_key("anim.player.Idle")
        }
    }
}

fn player_falling_animation(
    f_query: Query<
        (
            &Gent,
            &LinearVelocity,
            Option<&WallSlideTime>,
        ),
        Or<(
            (With<Falling>, Without<Attacking>),
            (With<Falling>, Added<CanAttack>),
        )>,
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
    config: Res<PlayerConfig>,
) {
    for (gent, velocity, sliding) in f_query.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            if let Some(sliding) = sliding {
                if sliding.sliding(&config) {
                    if player.current_key().unwrap_or("")
                        != "anim.player.WallSlide"
                    {
                        player.play_key("anim.player.WallSlide");
                    }
                } else if velocity.y < 0.
                    && player.current_key().unwrap_or("") != "anim.player.Fall"
                {
                    player.play_key("anim.player.Fall");
                }
            } else if velocity.y < 0.
                && player.current_key().unwrap_or("") != "anim.player.Fall"
            {
                player.play_key("anim.player.Fall");
            }
        }
    }
}

fn player_jumping_animation(
    query: Query<
        &Gent,
        Or<(
            (Added<Jumping>, Without<Attacking>),
            (With<Jumping>, Added<CanAttack>),
        )>,
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for gent in query.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            player.play_key("anim.player.Jump")
        }
    }
}

fn player_running_animation(
    query: Query<
        &Gent,
        Or<(
            (
                Added<Running>,
                Without<Attacking>,
                Without<Whirling>,
            ),
            (With<Running>, Added<CanAttack>),
        )>,
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for gent in query.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            player.play_key("anim.player.Run")
        }
    }
}

fn player_attacking_animation(
    query: Query<
        (
            &Gent,
            Has<Falling>,
            Has<Jumping>,
            Has<Running>,
            Option<&HitFreezeTime>,
            Ref<Attacking>,
        ),
        Without<Whirling>,
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
    config: Res<PlayerConfig>,
    weapon: CurrentWeapon,
) {
    for (gent, is_falling, is_jumping, is_running, hitfrozen, attacking) in
        query.iter()
    {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            let hitfrozen = hitfrozen
                .map(|f| f.0 < config.hitfreeze_ticks)
                .unwrap_or(false);
            player.set_slot("AttackTransition", false);
            let basic_air_anim_key_str = &weapon.get_anim_key("BasicAir");
            let basic_run_anim_key_str = &weapon.get_anim_key("BasicRun");
            let basic_idle_anim_key_str = &weapon.get_anim_key("BasicIdle");

            if is_falling || is_jumping {
                // TODO: These need a way to resume the new animation from the current frame index
                // or specified offset
                if player.current_key() != Some(basic_air_anim_key_str) {
                    player.play_key(basic_air_anim_key_str);
                    if !attacking.is_added() {
                        player.set_slot("AttackTransition", true);
                    }
                }
            } else if is_running && !hitfrozen {
                if player.current_key() != Some(basic_run_anim_key_str) {
                    player.play_key(basic_run_anim_key_str);
                    if !attacking.is_added() {
                        player.set_slot("AttackTransition", true);
                    }
                }
            } else if player.current_key() != Some(basic_idle_anim_key_str) {
                player.play_key(basic_idle_anim_key_str);
                if !attacking.is_added() {
                    player.set_slot("AttackTransition", true);
                }
            }
        }
    }
}

fn player_whirling_animation(
    query: Query<&Gent, Added<Whirling>>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
    weapon: CurrentWeapon,
) {
    for gent in query.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            player.play_key(&weapon.whirling_anim_key());
        }
    }
}

fn player_dashing_animation(
    query: Query<(&Gent, &Dashing), Added<Dashing>>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for (gent, dashing) in query.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            if dashing.is_down_dash() {
                player.play_key("anim.player.SwordDashDown")
            } else {
                player.play_key("anim.player.Dash")
            }
        }
    }
}

fn player_dashing_strike_animation(
    query: Query<(&Gent, &DashStrike), Added<DashStrike>>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for (gent, _dash_strike) in query.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            player.play_key("anim.player.SwordDashDownStrike")
        }
    }
}

fn sprite_flip(
    query: Query<(&Facing, &Gent, Option<&WallSlideTime>)>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
    mut current_direction: Local<bool>,
    mut old_direction: Local<bool>,
    time: Res<GameTime>,
    weapon: CurrentWeapon,
) {
    for (facing, gent, wall_slide_time) in query.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            *old_direction = *current_direction;
            let mut facing = facing.clone();

            // Have the player face away from the wall if they are attacking while wall sliding
            let pressed_on_wall = wall_slide_time
                .is_some_and(|s| s.is_pressed_against_wall(&time));
            let is_attacking_while_falling =
                player.current_key() == Some(&weapon.get_anim_key("BasicAir"));
            if pressed_on_wall && is_attacking_while_falling {
                facing = match facing {
                    Facing::Right => Facing::Left,
                    Facing::Left => Facing::Right,
                }
            }
            match facing {
                Facing::Right => {
                    // TODO: toggle facing script action
                    player.set_slot("DirectionRight", true);
                    player.set_slot("DirectionLeft", false);
                    *current_direction = true;
                },
                Facing::Left => {
                    player.set_slot("DirectionRight", false);
                    player.set_slot("DirectionLeft", true);
                    *current_direction = false;
                },
            }

            // lazy change detection cause I can't be asked to learn proper bevy way lel ~c12
            if *old_direction != *current_direction {
                player.set_slot("DirectionChanged", true);
            } else {
                player.set_slot("DirectionChanged", false);
            }
        }
    }
}

fn update_serpent_ring_slot(
    player_query: Query<(&Gent, &Passives), With<Player>>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for (gent, passives) in player_query.iter() {
        let has_serpent_ring = passives.contains(&Passive::SerpentRing);
        if let Ok(mut anim_player) = gfx_query.get_mut(gent.e_gfx) {
            if has_serpent_ring {
                anim_player.set_slot("SerpentRing", true);
            } else {
                anim_player.set_slot("SerpentRing", false);
            }
        }
    }
}

fn update_frenzied_attack_slot(
    player_query: Query<(&Gent, &Passives), With<Player>>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for (gent, passives) in player_query.iter() {
        let has_frenzied_attack = passives.contains(&Passive::FrenziedAttack);
        if let Ok(mut anim_player) = gfx_query.get_mut(gent.e_gfx) {
            if has_frenzied_attack {
                anim_player.set_slot("FrenziedAttack", true);
            } else {
                anim_player.set_slot("FrenziedAttack", false);
            }
        }
    }
}

fn xp_orb_animation_handler(
    mut xp_events: EventReader<XpOrbPickup>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>
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
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>
) {
    let damaged_event_occurred = !damage_events.is_empty();
    for mut anim in gfx_query.iter_mut() {
        if damaged_event_occurred {
            anim.set_slot("Damaged", true);
        } else {
            anim.set_slot("Damaged", false);
        }
    }
}
