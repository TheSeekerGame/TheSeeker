use crate::game::player::foreground::{ActionMut, ActionTag};
use bevy::prelude::*;
use theseeker_engine::animation::PlaybackState;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::LinearVelocity;
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::{GameTickUpdate, GameTime};

use super::animation_hitbox;
use crate::game::combat::{DamageSource, Stealthed};
use crate::game::effects::stealthed::StealthEffect;
use crate::game::gentstate::Facing;
use crate::game::player::passives::pulse_echo::PulseEcho;
use crate::game::player::player_anim::set_direction_slots;
use crate::game::player::states::Whirling;
use crate::game::player::{Passives, Player, PlayerStatMod, PlayerStateSet};

pub struct WhirlingStatePlugin;

impl Plugin for WhirlingStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                whirling_enter_system,
                whirling_update_system,
            )
                .chain()
                .in_set(PlayerStateSet::Behavior)
                .in_set(crate::game::player::PlayerBehaviorSet::Execution),
        );
    }
}

/// Plays the whirl and spawns its sustained hitbox.
fn whirling_enter_system(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            ActionMut<Whirling>,
            &Gent,
            &Facing,
            Option<&PlayerStatMod>,
        ),
        (With<Player>, Added<ActionTag<Whirling>>),
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>>,
) {
    for (entity, mut whirling, gent, facing, stat_mod) in &mut query {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            set_direction_slots(&mut player, facing);
            let animation = format!("anim.player.{}Whirling", whirling.weapon.name());
            whirling.execution_mut().play(&mut player, &animation);
        }

        let attack = whirling.weapon.attack();
        let damage = attack.damage * stat_mod.map(|s| s.damage).unwrap_or(1.0);
        let hitbox = commands
            .spawn(animation_hitbox(
                entity,
                gent,
                DamageSource::new(u32::MAX, entity, damage),
                attack.hit_slot,
            ))
            .id();
        whirling.damage_entity = Some(hitbox);
    }
}

/// Each whirl beat lets the hitbox strike again and releases Pulse Echoes; the hitbox
/// follows the player's stealth.
pub fn whirling_update_system(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            ActionMut<Whirling>,
            &Gent,
            &Transform,
            &LinearVelocity,
            &Facing,
            Option<&Passives>,
            Option<&PlayerStatMod>,
            Has<StealthEffect>,
        ),
        With<Player>,
    >,
    signal_query: Query<&PlaybackState>,
    mut damage_query: Query<&mut DamageSource>,
    time: Res<GameTime>,
) {
    for (
        entity,
        mut whirling,
        gent,
        transform,
        velocity,
        facing,
        passives,
        stat_mod,
        is_stealthed,
    ) in &mut query
    {
        whirling.tick = whirling.tick.saturating_add(1);
        let Some(hitbox) = whirling.damage_entity else {
            continue;
        };
        if is_stealthed {
            commands.entity(hitbox).insert(Stealthed);
        } else {
            commands.entity(hitbox).remove::<Stealthed>();
        }

        let beat = whirling
            .execution()
            .playback(signal_query.get(gent.e_gfx).ok())
            .is_some_and(|signals| signals.has_event("whirl_damage"));
        let Some(mut damage) = beat.then(|| damage_query.get_mut(hitbox).ok()).flatten() else {
            continue;
        };
        damage.damaged_set.clear();

        let damage_mult = stat_mod.map(|s| s.damage).unwrap_or(1.0);
        let echo = passives.and_then(|passives| PulseEcho::of(passives, damage_mult, is_stealthed));
        if let Some(echo) = echo {
            echo.release(
                &mut commands,
                entity,
                transform.translation,
                velocity.0,
                time.hz,
                &[Vec2::new(facing.direction(), 0.0)],
            );
        }
    }
}
