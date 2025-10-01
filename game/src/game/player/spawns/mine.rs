use bevy::prelude::*;
use bevy::render::view::RenderLayers;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::{Gent, TransformGfxFromGent};
use theseeker_engine::physics::{groups, Collider};
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::GameTickUpdate;

use crate::game::combat::damage_source::DamageSource;
use crate::game::combat::Stealthed;
use crate::StateDespawnMarker;

/// Marker for the mine logic entity
#[derive(Component)]
pub struct Mine;

/// Marker for mine graphics entity
#[derive(Component)]
pub struct MineGfx;

/// Armed/waiting state: waits for proximity to an enemy to explode
#[derive(Component, Debug)]
pub struct MineArmed {
    pub owner: Entity,
}

/// Exploding state: drives lifetime of the explosion animation
#[derive(Component, Debug)]
pub struct MineExploding {
    pub ticks_left: u32,
}

pub struct ExplosiveMinePlugin;

impl Plugin for ExplosiveMinePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                mine_armed_tick
                    .before(bevy::transform::TransformSystem::TransformPropagate)
                    .before(crate::game::combat::damage_source::determine_damage_targets),
                mine_exploding_tick,
            ).chain(),
        );
    }
}

/// Public spawn helper called by the skill driver
pub fn spawn_mine(
    commands: &mut Commands,
    owner: Entity,
    center: Vec2,
    _facing_dir: f32,
) {
    // Render above the player: player gent z is ~15e-6; use a higher z
    let base_z = 17.0 * 0.000001;
    let gfx = commands
        .spawn((
            MineGfx,
            SpriteAnimationBundle::new_play_key("anim.player.MineDeploy"),
            Sprite {
                texture_atlas: Some(TextureAtlas::default()),
                ..Default::default()
            },
            Transform::from_translation(center.extend(base_z)),
            GlobalTransform::default(),
            Visibility::Visible,
            InheritedVisibility::default(),
            ViewVisibility::default(),
            // Render on the same layer as the player so Z ordering applies; higher Z draws in front
            RenderLayers::layer(2),
            StateDespawnMarker,
        ))
        .id();

    // Spawn logic entity and attach gfx follower
    let mine_entity = commands
        .spawn((
            Mine,
            MineArmed { owner },
            Gent {
                e_gfx: gfx,
                e_effects_gfx: gfx,
            },
            Transform::from_translation(center.extend(base_z)),
            GlobalTransform::default(),
            StateDespawnMarker,
        ))
        .id();

    // Ensure gfx follows gent with exact z offset maintained
    commands.entity(gfx).insert(TransformGfxFromGent {
        pixel_aligned: false,
        gent: mine_entity,
        offset: None,
    });
}

// Proximity trigger constants
const TRIGGER_RADIUS: f32 = 12.0;
const EXPLOSION_RADIUS: f32 = 24.0;
const EXPLOSION_DAMAGE: f32 = 50.0;
const EXPLOSION_TICKS: u32 = 32; // 4 frames * 8 ticks

fn mine_armed_tick(
    mut commands: Commands,
    mut query: Query<(
        Entity,
        &mut Transform,
        &GlobalTransform,
        &MineArmed,
        &Gent,
    )>,
    // Enemies and bell are both valid targets to trigger and receive damage
    enemy_query: Query<
        Entity,
        Or<(
            With<crate::game::enemy::Enemy>,
            With<crate::game::player::spawns::amplified_bell::Bell>,
        )>,
    >,
    enemy_tf: Query<&GlobalTransform>,
    owner_stealth: Query<
        (),
        With<crate::game::effects::stealthed::StealthEffect>,
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<MineGfx>>,
) {
    for (entity, mut xf, tf, armed, gent) in query.iter_mut() {
        let pos = tf.translation();
        // Check if any enemy within trigger radius
        let mut should_explode = false;
        // Transform-based proximity scan (cheap and robust)
        for e in enemy_query.iter() {
            if let Ok(target_tf) = enemy_tf.get(e) {
                let d2 = target_tf
                    .translation()
                    .truncate()
                    .distance_squared(pos.truncate());
                if d2 <= TRIGGER_RADIUS * TRIGGER_RADIUS {
                    should_explode = true;
                    break;
                }
            }
        }

        if should_explode {
            // Switch to exploding state and play explosion animation
            commands.entity(entity).remove::<MineArmed>();
            commands.entity(entity).insert(MineExploding {
                ticks_left: EXPLOSION_TICKS,
            });
            // Place explosion behind player but in front of enemies
            xf.translation.z = 14.0 * 0.000001;
            if let Ok(mut anim) = gfx_query.get_mut(gent.e_gfx) {
                anim.play_key("anim.player.MineExplosion");
            }
            // Spawn AoE damage only for 1 tick, marking stealthed if owner is currently stealthed
            let ds = DamageSource::new(1, armed.owner, EXPLOSION_DAMAGE)
                .with_max_targets(64);
            let mut builder = commands.spawn((
                ds,
                Collider::ball(EXPLOSION_RADIUS),
                groups::player_attack(),
                Transform::from_translation(pos),
                GlobalTransform::from_translation(pos),
                StateDespawnMarker,
            ));
            if owner_stealth.get(armed.owner).is_ok() {
                builder.insert(Stealthed);
            }
        }
    }
}

fn mine_exploding_tick(
    mut commands: Commands,
    mut query: Query<(Entity, &mut MineExploding, &Gent)>,
) {
    for (entity, mut exploding, gent) in query.iter_mut() {
        if exploding.ticks_left > 0 {
            exploding.ticks_left -= 1;
        }
        if exploding.ticks_left == 0 {
            // Despawn gfx then logic entity
            commands.entity(gent.e_gfx).despawn();
            commands.entity(entity).despawn();
        }
    }
}

// Helper bundle to use ScriptPlayer in this module (copied from bell)
#[derive(Bundle)]
struct SpriteAnimationBundle {
    pub player: ScriptPlayer<SpriteAnimation>,
}

impl SpriteAnimationBundle {
    fn new_play_key(key: &str) -> Self {
        let mut player = ScriptPlayer::<SpriteAnimation>::default();
        player.play_key(key);
        SpriteAnimationBundle { player }
    }
}
