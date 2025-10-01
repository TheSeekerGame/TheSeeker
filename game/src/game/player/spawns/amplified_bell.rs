use bevy::prelude::*;
use bevy_rapier2d::prelude::Sensor;
use std::collections::BTreeMap;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::{Gent, TransformGfxFromGent};
use theseeker_engine::physics::{groups, Collider};
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::{GameTickUpdate, GameTime};

use crate::game::combat::damage_source::DamageSource;
use crate::game::combat::Stealthed;
use crate::game::gentstate::Facing;
use crate::game::effects::stealthed::StealthEffect;
use crate::StateDespawnMarker;
use bevy::render::view::RenderLayers;

/// Marker identifying the bell gent (logic entity)
#[derive(Component)]
pub struct Bell;

/// Graphics marker for the bell sprite entity
#[derive(Component)]
pub struct BellGfx;

/// Marker for our bell AoE damage sources (used to prune targets)
#[derive(Component)]
pub struct BellDamageSource;

/// Simple marker used to suppress damage numbers on entities
/// (Reuses graphics system filter)
use crate::graphics::NoDamageNumbers;

/// Alive state for the bell (single-state mini-SM)
#[derive(Component, Debug)]
pub struct BellAlive {
    pub owner: Entity,
    pub tick: u32,
    pub max_ticks: u32,
    pub hide_ticks_remaining: u32,
    pub ring_z_counter: f32,
}

#[derive(Component, Debug)]
pub struct BellEnding {
    pub ticks_left: u32,
}

/// Internal ring schedule
#[derive(Component, Default)]
pub struct BellRingSchedule {
    pub triggers: Vec<u64>,
}

pub struct AmplifiedBellPlugin;

impl Plugin for AmplifiedBellPlugin {
    fn build(&self, app: &mut App) {
        // Core bell tick and state changes: ensure ring spawns happen before damage target computation
        app.add_systems(
            GameTickUpdate,
            (
                on_bell_alive_enter,
                // Spawn rings before transform propagation so GlobalTransform is valid
                // for the damage query on the same tick. This also guarantees we run
                // before determine_damage_targets (which is after TransformPropagate).
                tick_bell_alive
                    .before(bevy::transform::TransformSystem::TransformPropagate)
                    .before(crate::game::combat::damage_source::determine_damage_targets),
                tick_bell_ending,
            )
                .chain(),
        );
        // React to damage after it has been applied (schedule new rings and play visuals)
        app.add_systems(
            GameTickUpdate,
            bell_damage_event_listener
                .after(crate::game::combat::damage_source::apply_damage),
        );
        // Prune AoE targets between determine_targets and damage modifications
        app.add_systems(
            GameTickUpdate,
            prune_bell_aoe_targets
                .after(crate::game::combat::damage_source::determine_damage_targets)
                .before(crate::game::combat::damage_source::apply_damage_modifications),
        );
        // Spark lifetime management
        app.add_systems(GameTickUpdate, tick_ring_sparks);

        // Ensure arrows are not consumed by hitting the bell: run after damage is applied
        // (so the bell can schedule its ring) but before projectile despawn logic.
        app.add_systems(
            GameTickUpdate,
            prune_arrow_bell_hits
                .after(crate::game::combat::damage_source::apply_damage)
                .before(crate::game::physics::projectile::despawn_projectile),
        );
    }
}

// Tuning constants
const BELL_HALF_WIDTH: f32 = 6.0;
const BELL_HALF_HEIGHT: f32 = 8.0;
const BELL_LIFETIME_TICKS: u32 = 1152; // 12s at 96Hz
const RING_DELAY_TICKS: u32 = 12;
const RING_DAMAGE: f32 = 12.0;
const RING_RADIUS: f32 = 48.0;
const RING_SPARK_TICKS: u32 = 24; // visual duration to hide main bell
const END_ANIM_TICKS: u32 = 24; // time to wait while playing end anim
const RING_SPARK_LIFETIME_TICKS: u32 = 24; // 4 frames * 8 ticks per frame

/// Public spawn helper called by the skill driver
pub fn spawn_bell(
    commands: &mut Commands,
    owner: Entity,
    center: Vec2,
    _facing_dir: f32,
) {
    // Spawn logic entity (gent)
    // Use a Z below enemies (0.000014) and player (0.000015)
    let base_z = 13.0 * 0.000001;
    let gfx = commands
        .spawn((
            BellGfx,
            SpriteAnimationBundle::new_play_key(
                "anim.player.AmplifiedBellDeploy",
            ),
            Sprite {
                texture_atlas: Some(TextureAtlas::default()),
                ..Default::default()
            },
            Transform::from_translation(center.extend(base_z)),
            GlobalTransform::default(),
            Visibility::Visible,
            InheritedVisibility::default(),
            ViewVisibility::default(),
            // Render on world layer (0) so player (layer 2) renders on top
            RenderLayers::layer(0),
            StateDespawnMarker,
        ))
        .id();

    let bell_entity = commands
        .spawn((
            Bell,
            Gent {
                e_gfx: gfx,
                e_effects_gfx: gfx,
            },
            Transform::from_translation(center.extend(base_z)),
            GlobalTransform::default(),
            Collider::cuboid(BELL_HALF_WIDTH, BELL_HALF_HEIGHT),
            Sensor,               // Do not block player movement
            groups::enemy_body(), // So player attacks (PLAYER_ATTACK) can intersect this
            NoDamageNumbers,      // Don't spawn damage numbers on bell hits
            // Provide Facing so damage system queries match and emit DamageInfo
            Facing::Right,
            crate::game::combat::Health {
                current: u32::MAX,
                max: u32::MAX,
            },
            BellAlive {
                owner,
                tick: 0,
                max_ticks: BELL_LIFETIME_TICKS,
                hide_ticks_remaining: 0,
                ring_z_counter: 0.0,
            },
            BellRingSchedule::default(),
            StateDespawnMarker,
        ))
        .id();

    // Attach gfx to gent transform
    commands.entity(gfx).insert((
        TransformGfxFromGent {
            pixel_aligned: false,
            gent: bell_entity,
            offset: None,
        },
        Name::new("BellGfx"),
    ));

    // Optionally set direction slots if needed by your animation runtime here (omitted for simplicity)
}

fn on_bell_alive_enter(
    query: Query<(Entity, &Gent), Added<BellAlive>>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>>,
) {
    for (_entity, gent) in query.iter() {
        if let Ok(mut anim) = gfx_query.get_mut(gent.e_gfx) {
            anim.play_key("anim.player.AmplifiedBellDeploy");
        }
    }
}

fn tick_bell_alive(
    mut commands: Commands,
    mut query: Query<(
        Entity,
        &mut BellAlive,
        &Gent,
        &mut BellRingSchedule,
        &GlobalTransform,
    )>,
    mut gfx_query: Query<
        (
            &mut Visibility,
            &mut ScriptPlayer<SpriteAnimation>,
        ),
        With<BellGfx>,
    >,
    time: Res<GameTime>,
    stealthed_owner: Query<(), With<StealthEffect>>,
) {
    let now = time.tick();
    for (entity, mut alive, gent, mut schedule, bell_tf) in query.iter_mut() {
        // Lifetime
        alive.tick = alive.tick.saturating_add(1);
        if alive.tick >= alive.max_ticks {
            // Transition to ending
            commands.entity(entity).remove::<BellAlive>();
            commands.entity(entity).insert(BellEnding {
                ticks_left: END_ANIM_TICKS,
            });
            if let Ok((_vis, mut anim)) = gfx_query.get_mut(gent.e_gfx) {
                anim.play_key("anim.player.AmplifiedBellEnd");
            }
            continue;
        }

        // Handle visibility during spark
        if alive.hide_ticks_remaining > 0 {
            if let Ok((mut vis, _anim)) = gfx_query.get_mut(gent.e_gfx) {
                *vis = Visibility::Hidden;
            }
            alive.hide_ticks_remaining -= 1;
        } else {
            if let Ok((mut vis, _anim)) = gfx_query.get_mut(gent.e_gfx) {
                *vis = Visibility::Visible;
            }
        }

        // Fire due rings (spawn AoE damage)
        // Drain triggers <= now and aggregate by scheduled tick so multiple hits in
        // the same originating tick produce a single ring with multiplied damage.
        let mut remaining = Vec::with_capacity(schedule.triggers.len());
        let mut due_counts: BTreeMap<u64, u32> = BTreeMap::new();
        for &t in schedule.triggers.iter() {
            if t <= now {
                *due_counts.entry(t).or_insert(0) += 1;
            } else {
                remaining.push(t);
            }
        }
        schedule.triggers = remaining;

        if !due_counts.is_empty() {
            let world_pos = bell_tf.translation();
            let owner_is_stealthed = stealthed_owner.get(alive.owner).is_ok();
            for (_scheduled_tick, count) in due_counts.into_iter() {
                let scaled_damage = RING_DAMAGE * (count as f32);
                spawn_ring_damage(
                    &mut commands,
                    world_pos,
                    alive.owner,
                    owner_is_stealthed,
                    scaled_damage,
                );
            }
        }
    }
}

fn tick_bell_ending(
    mut commands: Commands,
    mut query: Query<(Entity, &mut BellEnding, &Gent)>,
) {
    for (entity, mut ending, gent) in query.iter_mut() {
        if ending.ticks_left > 0 {
            ending.ticks_left -= 1;
        }
        if ending.ticks_left == 0 {
            commands.entity(gent.e_gfx).despawn();
            commands.entity(entity).despawn();
        }
    }
}

fn bell_damage_event_listener(
    mut commands: Commands,
    mut damage_events: EventReader<crate::game::combat::DamageInfo>,
    mut query: Query<(
        Entity,
        &mut BellAlive,
        &Gent,
        &mut BellRingSchedule,
    )>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<BellGfx>>,
    time: Res<GameTime>,
) {
    let now = time.tick();
    for dmg in damage_events.read() {
        if let Ok((_entity, mut alive, gent, mut schedule)) =
            query.get_mut(dmg.target)
        {
            // Schedule ring with delay
            schedule.triggers.push(now + RING_DELAY_TICKS as u64);
            // Spawn ring visual spark immediately (as independent entity followed to gent)
            spawn_ring_visual(
                &mut commands,
                _entity,
                alive.ring_z_counter,
            );
            // Keep Z deltas tiny to remain behind player (player ~15e-6)
            alive.ring_z_counter += 0.00000001; // 1e-8 per ring
            alive.hide_ticks_remaining = RING_SPARK_TICKS;
            // Optionally restart deploy anim to ensure idle frame locks; not necessary if deploy contains idle loop
            if let Ok(mut anim) = gfx_query.get_mut(gent.e_gfx) {
                // No-op here; bell stays hidden while spark plays
                let _ = &mut anim;
            }
        }
    }
}

/// Fallback/robust trigger: detect direct overlaps between the bell collider and any player attack colliders.
/// This allows the bell to ring even if the damage event route misses for any reason.
// Removed fallback contact-based ring trigger to avoid false positives

fn spawn_ring_visual(
    commands: &mut Commands,
    bell_entity: Entity,
    z_counter: f32,
) {
    // Spawn an independent entity that follows the bell gent (not hidden by bell gfx visibility)
    commands.spawn((
        SpriteAnimationBundle::new_play_key("anim.player.AmplifiedBellRing"),
        Sprite {
            texture_atlas: Some(TextureAtlas::default()),
            ..Default::default()
        },
        Transform::from_translation(Vec3::new(0.0, 0.0, z_counter)),
        GlobalTransform::default(),
        Visibility::Visible,
        InheritedVisibility::default(),
        ViewVisibility::default(),
        // Render on world layer (0) so the player (layer 2) appears above
        RenderLayers::layer(0),
        TransformGfxFromGent {
            pixel_aligned: false,
            gent: bell_entity,
            offset: Some(Vec3::new(0.0, 0.0, z_counter)),
        },
        RingSpark {
            lifetime_ticks: RING_SPARK_LIFETIME_TICKS,
        },
        StateDespawnMarker,
    ));
}

fn spawn_ring_damage(
    commands: &mut Commands,
    world_pos: Vec3,
    owner: Entity,
    stealthed: bool,
    damage: f32,
) {
    let mut e = commands.spawn((
        BellDamageSource,
        DamageSource::new(1, owner, damage).with_max_targets(16),
        Collider::ball(RING_RADIUS),
        // Use canonical group helper for player attacks
        groups::player_attack(),
        Transform::from_translation(world_pos),
        GlobalTransform::from_translation(world_pos),
        StateDespawnMarker,
    ));
    if stealthed {
        e.insert(Stealthed);
    }
}

/// Allow player arrows to trigger the bell without being consumed.
/// After damage has been applied (and DamageInfo emitted for the bell),
/// remove bell targets from the arrow's damaged_set so the arrow can still
/// hit an enemy behind the bell before despawning.
fn prune_arrow_bell_hits(
    mut query: Query<
        &mut DamageSource,
        With<crate::game::physics::projectile::Arrow>,
    >,
    is_bell: Query<(), With<Bell>>,
) {
    for mut src in query.iter_mut() {
        if src.damaged_set.is_empty() {
            continue;
        }
        let to_remove: Vec<_> = src
            .damaged_set
            .iter()
            .filter(|e| is_bell.get(**e).is_ok())
            .copied()
            .collect();
        for e in to_remove {
            src.damaged_set.remove(&e);
        }
    }
}

/// Tracks lifetime of a bell ring spark animation entity
#[derive(Component)]
struct RingSpark {
    lifetime_ticks: u32,
}

/// Ticks ring spark lifetimes and despawns them when done
fn tick_ring_sparks(
    mut commands: Commands,
    mut query: Query<(Entity, &mut RingSpark)>,
) {
    for (entity, mut spark) in query.iter_mut() {
        if spark.lifetime_ticks > 0 {
            spark.lifetime_ticks -= 1;
        }
        if spark.lifetime_ticks == 0 {
            commands.entity(entity).despawn();
        }
    }
}

/// After targets are computed, prune self and downward hemisphere
fn prune_bell_aoe_targets(
    mut query: Query<
        (&GlobalTransform, &mut DamageSource),
        With<BellDamageSource>,
    >,
    target_tf: Query<&GlobalTransform>,
    is_bell: Query<(), With<Bell>>,
) {
    for (src_tf, mut src) in query.iter_mut() {
        let src_pos = src_tf.translation();
        let mut new_set = src.target_set.clone();
        for t in src.target_set.iter() {
            // Remove if target is a bell
            if is_bell.get(*t).is_ok() {
                new_set.remove(t);
                continue;
            }
            // Remove if below bell bottom (exclude downward only)
            if let Ok(tf) = target_tf.get(*t) {
                let bell_bottom = src_pos.y - (BELL_HALF_HEIGHT as f32) - 1.0;
                if tf.translation().y < bell_bottom {
                    new_set.remove(t);
                }
            }
        }
        src.target_set = new_set;
    }
}

// Helper bundle to use ScriptPlayer in this module
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
