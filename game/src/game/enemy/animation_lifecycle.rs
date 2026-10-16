use super::death::{
    advance_death_animation, despawn_decay_when_far,
    despawn_opi_tombstones_when_far, enemy_decay_animation,
    enemy_decay_visibility, enemy_death_animation,
    update_corpse_despawn_anchor, CorpseDespawnAnchor,
};
use super::flying_motor::sync_bat_death_gfx_offset;
use super::*;

pub(super) struct EnemyAnimationPlugin;

#[derive(Component)]
pub(super) struct FacingSlots {
    right: String,
    left: String,
}

impl FacingSlots {
    pub(super) fn new(right: String, left: String) -> Self {
        Self { right, left }
    }
}

impl Plugin for EnemyAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CorpseDespawnAnchor>();
        app.add_systems(
            GameTickUpdate,
            (
                enemy_death_animation,
                advance_death_animation,
                enemy_decay_animation,
                enemy_decay_visibility,
                sync_bat_death_gfx_offset,
                update_corpse_despawn_anchor,
                despawn_decay_when_far.after(update_corpse_despawn_anchor),
                despawn_opi_tombstones_when_far
                    .after(update_corpse_despawn_anchor),
                sprite_flip,
            )
                .in_set(EnemyStateSet::Animation)
                .after(EnemyStateSet::Transition)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

pub(super) fn animation_duration_ticks(
    key: &str,
    preloaded: &PreloadedAssets,
    anim_assets: &Assets<SpriteAnimation>,
    override_tpf: Option<u32>,
    fallback: u32,
) -> u32 {
    preloaded
        .get_single_asset::<SpriteAnimation>(key)
        .and_then(|handle| anim_assets.get(&handle))
        .map(|anim| {
            let frames = anim
                .settings
                .extended
                .frame_max
                .0
                .saturating_sub(anim.settings.extended.frame_min.0)
                .saturating_add(1)
                .max(1);
            let tpf = override_tpf
                .unwrap_or(anim.settings.extended.ticks_per_frame)
                .max(1);
            frames * tpf
        })
        .unwrap_or(fallback)
}

fn sprite_flip(
    query: Query<(&Facing, &FacingSlots, &Gent), With<Enemy>>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    for (facing, slots, gent) in query.iter() {
        for gfx_entity in [gent.e_gfx, gent.e_effects_gfx] {
            set_sprite_flip_for_entity(
                gfx_entity,
                facing,
                slots,
                &mut gfx_query,
            );
        }
    }
}

fn set_sprite_flip_for_entity(
    gfx_entity: Entity,
    facing: &Facing,
    slots: &FacingSlots,
    gfx_query: &mut Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    let Ok(mut player) = gfx_query.get_mut(gfx_entity) else {
        return;
    };

    set_sprite_flip_slots(&mut player, facing, slots);
}

fn set_sprite_flip_slots(
    player: &mut ScriptPlayer<SpriteAnimation>,
    facing: &Facing,
    slots: &FacingSlots,
) {
    match facing {
        Facing::Right => {
            player.set_slot(&slots.right, true);
            player.set_slot(&slots.left, false);
        },
        Facing::Left => {
            player.set_slot(&slots.right, false);
            player.set_slot(&slots.left, true);
        },
    }
}
