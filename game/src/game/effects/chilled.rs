use bevy::prelude::*;

use crate::game::player::{Player, PlayerGfx, PlayerStatMod};
use theseeker_engine::gent::Gent;

pub const CHILLED_COLOR: Color = Color::srgba(0.48, 0.65, 1.0, 1.0);

#[derive(Component, Debug, Clone)]
pub struct ChilledEffect {
    pub remaining_ticks: u32,
    pub speed_modifier: f32,
}

impl ChilledEffect {
    pub fn new(duration_ticks: u32, speed_modifier: f32) -> Self {
        Self {
            remaining_ticks: duration_ticks,
            speed_modifier,
        }
    }
}

pub fn chilled_effect_system(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &mut ChilledEffect,
            &Gent,
            &mut PlayerStatMod,
        ),
        With<Player>,
    >,
    mut gfx_query: Query<&mut Sprite, With<PlayerGfx>>,
) {
    for (entity, mut chilled, gent, mut stats) in query.iter_mut() {
        stats.speed *= chilled.speed_modifier;

        if let Ok(mut sprite) = gfx_query.get_mut(gent.e_gfx) {
            sprite.color = CHILLED_COLOR;
        }

        chilled.remaining_ticks = chilled.remaining_ticks.saturating_sub(1);
        if chilled.remaining_ticks == 0 {
            commands.entity(entity).remove::<ChilledEffect>();
        }
    }
}

pub fn chilled_exit_restoration_system(
    mut removed: RemovedComponents<ChilledEffect>,
    query: Query<&Gent, With<Player>>,
    mut gfx_query: Query<&mut Sprite, With<PlayerGfx>>,
) {
    for entity in removed.read() {
        restore_chilled_sprite(entity, &query, &mut gfx_query);
    }
}

fn restore_chilled_sprite(
    entity: Entity,
    query: &Query<&Gent, With<Player>>,
    gfx_query: &mut Query<&mut Sprite, With<PlayerGfx>>,
) {
    let Ok(gent) = query.get(entity) else {
        return;
    };
    let Ok(mut sprite) = gfx_query.get_mut(gent.e_gfx) else {
        return;
    };

    sprite.color = Color::WHITE;
}
