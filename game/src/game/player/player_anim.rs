use crate::appstate::AppState;
use crate::game::gentstate::Facing;
use crate::game::player::{
    Attacking, CanAttack, Dashing, Falling, HitFreezeTime, Idle, Jumping, PlayerConfig, PlayerGfx,
    PlayerStateSet, Running, WallSlideTime,
};
use crate::prelude::{
    in_state, Added, App, Has, IntoSystemConfigs, Local, Or, Plugin, Query, Res, With, Without,
};
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::prelude::{GameTickUpdate, GameTime};
use theseeker_engine::script::ScriptPlayer;

///play animations here, run after transitions
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
                player_dashing_animation,
                sprite_flip.after(player_dashing_animation),
            )
                .in_set(PlayerStateSet::Animation)
                .after(PlayerStateSet::Transition)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

fn player_idle_animation(
    i_query: Query<
        &Gent,
        Or<(
            (Added<Idle>, Without<Attacking>),
            (With<Idle>, Added<CanAttack>),
        )>,
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for gent in i_query.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            player.play_key("anim.player.Idle")
        }
    }
}

fn player_falling_animation(
    f_query: Query<
        (&Gent, Option<&WallSlideTime>),
        Or<(
            (With<Falling>, Without<Attacking>),
            (With<Falling>, Added<CanAttack>),
        )>,
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
    config: Res<PlayerConfig>,
) {
    for (gent, sliding) in f_query.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            if let Some(sliding) = sliding {
                if sliding.sliding(&config) {
                    if player.current_key().unwrap_or("") != "anim.player.WallSlide" {
                        player.play_key("anim.player.WallSlide");
                    }
                } else {
                    if player.current_key().unwrap_or("") != "anim.player.Fall" {
                        player.play_key("anim.player.Fall");
                    }
                }
            } else {
                if player.current_key().unwrap_or("") != "anim.player.Fall" {
                    player.play_key("anim.player.Fall");
                }
            }
        }
    }
}

fn player_jumping_animation(
    f_query: Query<
        &Gent,
        Or<(
            (Added<Jumping>, Without<Attacking>),
            (With<Jumping>, Added<CanAttack>),
        )>,
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for gent in f_query.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            player.play_key("anim.player.Jump")
        }
    }
}

fn player_running_animation(
    r_query: Query<
        &Gent,
        Or<(
            (Added<Running>, Without<Attacking>),
            (With<Running>, Added<CanAttack>),
        )>,
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for gent in r_query.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            player.play_key("anim.player.Run")
        }
    }
}

fn player_attacking_animation(
    r_query: Query<
        (
            &Gent,
            Has<Falling>,
            Has<Jumping>,
            Has<Running>,
            Option<&HitFreezeTime>,
        ),
        Added<Attacking>,
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
    config: Res<PlayerConfig>,
) {
    for (gent, is_falling, is_jumping, is_running, hitfrozen) in r_query.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            let hitfrozen = hitfrozen
                .map(|f| f.0 < config.hitfreeze_ticks)
                .unwrap_or(false);
            if is_falling || is_jumping {
                player.play_key("anim.player.SwordBasicAir")
            } else if is_running && !hitfrozen {
                player.play_key("anim.player.SwordBasicRun")
            } else {
                player.play_key("anim.player.SwordBasicIdle")
            }
        }
    }
    //have to check if first or 2nd attack, play diff anim
    //also check for up attack?
}

fn player_dashing_animation(
    f_query: Query<&Gent, Or<((Added<Dashing>),)>>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
) {
    for gent in f_query.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            player.play_key("anim.player.Dash")
        }
    }
}

fn sprite_flip(
    query: Query<(&Facing, &Gent, Option<&WallSlideTime>)>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
    mut current_direction: Local<bool>,
    mut old_direction: Local<bool>,
    time: Res<GameTime>,
) {
    for (facing, gent, wall_slide_time) in query.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            *old_direction = *current_direction;
            let mut facing = facing.clone();

            // Have the player face away from the wall if they are attacking while wall sliding
            let pressed_on_wall = wall_slide_time
                // checks that player is actually against the wall, rather then it being close
                // enough time from the player having left the wall to still jump
                // (ie: not wall_jump_coyote_time)
                .map(|s| s.0 <= 1.0 / time.hz as f32)
                .unwrap_or(false);
            if pressed_on_wall && player.current_key() == Some("anim.player.SwordBasicAir") {
                facing = match facing {
                    Facing::Right => Facing::Left,
                    Facing::Left => Facing::Right,
                }
            }
            match facing {
                Facing::Right => {
                    //TODO: toggle facing script action
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
