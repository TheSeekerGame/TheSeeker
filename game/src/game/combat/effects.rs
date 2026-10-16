use super::DamageInfo;
use crate::game::enemy::EnemyGfx;
use crate::game::player::PlayerGfx;
use crate::prelude::*;
use theseeker_engine::effects::{SpriteScale, SpriteStretch};
use theseeker_engine::gent::Gent;
use theseeker_engine::time::GameTickUpdate;

#[derive(Component)]
pub struct DamageFlash {
    pub current_ticks: u32,
    pub max_ticks: u32,
}

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                apply_damage_flash,
                apply_damage_stretch_reset,
                damage_flash,
            )
                .chain()
                .after(super::damage_source::apply_damage),
        );
    }
}

fn apply_damage_flash(
    sprite_query: Query<
        Entity,
        (
            With<Sprite>,
            Or<(With<EnemyGfx>, With<PlayerGfx>)>,
            Without<Gent>,
        ),
    >,
    enemy_check_query: Query<Entity, With<EnemyGfx>>,
    gent_query: Query<&Gent>,
    mut damage_events: MessageReader<DamageInfo>,
    mut commands: Commands,
) {
    for damage_info in damage_events.read() {
        if let Ok(gent) = gent_query.get(damage_info.target) {
            apply_damage_flash_to_gent(
                gent,
                &sprite_query,
                &enemy_check_query,
                &mut commands,
            );
        }
    }
}

fn apply_damage_flash_to_gent(
    gent: &Gent,
    sprite_query: &Query<
        Entity,
        (
            With<Sprite>,
            Or<(With<EnemyGfx>, With<PlayerGfx>)>,
            Without<Gent>,
        ),
    >,
    enemy_check_query: &Query<Entity, With<EnemyGfx>>,
    commands: &mut Commands,
) {
    for gfx_entity in [gent.e_gfx, gent.e_effects_gfx] {
        let Ok(entity) = sprite_query.get(gfx_entity) else {
            continue;
        };

        commands.entity(entity).insert(DamageFlash {
            current_ticks: 0,
            max_ticks: 8,
        });

        if enemy_check_query.get(entity).is_ok() {
            commands
                .entity(entity)
                .insert(SpriteScale::new())
                .insert(SpriteStretch::default());
        }
    }
}

fn apply_damage_stretch_reset(
    mut stretch_query: Query<(&mut SpriteStretch, &mut SpriteScale)>,
    sprite_query: Query<
        Entity,
        (
            With<Sprite>,
            With<EnemyGfx>,
            Without<Gent>,
        ),
    >,
    gent_query: Query<&Gent>,
    mut damage_events: MessageReader<DamageInfo>,
) {
    for damage_info in damage_events.read() {
        if let Ok(gent) = gent_query.get(damage_info.target) {
            reset_damage_stretch_for_gent(
                gent,
                &sprite_query,
                &mut stretch_query,
            );
        }
    }
}

fn reset_damage_stretch_for_gent(
    gent: &Gent,
    sprite_query: &Query<
        Entity,
        (
            With<Sprite>,
            With<EnemyGfx>,
            Without<Gent>,
        ),
    >,
    stretch_query: &mut Query<(&mut SpriteStretch, &mut SpriteScale)>,
) {
    for gfx_entity in [gent.e_gfx, gent.e_effects_gfx] {
        let Ok(entity) = sprite_query.get(gfx_entity) else {
            continue;
        };

        if let Ok((mut stretch, mut scale)) = stretch_query.get_mut(entity) {
            stretch.current_tick = 0;
            scale.reset();
        }
    }
}

fn damage_flash(
    mut query: Query<(Entity, &mut Sprite, &mut DamageFlash)>,
    mut commands: Commands,
) {
    for (entity, mut sprite, mut damage_flash) in query.iter_mut() {
        sprite.color = Color::linear_rgb(50.0, 50.0, 50.0);

        if damage_flash.current_ticks == damage_flash.max_ticks {
            commands.entity(entity).remove::<DamageFlash>();
            sprite.color = Color::linear_rgb(1.0, 1.0, 1.0);
        }
        damage_flash.current_ticks += 1;
    }
}
