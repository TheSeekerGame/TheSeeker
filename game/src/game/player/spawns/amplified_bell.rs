use bevy::prelude::*;
use std::collections::BTreeMap;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::{Gent, TransformGfxFromGent};
use theseeker_engine::physics::{groups, Collider, Sensor};
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::{GameTickUpdate, GameTime};

use crate::game::combat::damage_source::DamageSource;
use crate::game::combat::Stealthed;
use crate::game::effects::stealthed::StealthEffect;
use crate::game::gentstate::Facing;
use crate::graphics::ripple::{RippleDescriptor, RippleEvent};
use crate::levelgen::foliage::FoliageImpulseEvent;
use crate::StateDespawnMarker;
use bevy::camera::visibility::RenderLayers;

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
                // before determine_damage_targets (which is after Propagate).
                tick_bell_alive
                    .before(bevy::transform::TransformSystems::Propagate)
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
    let base_z = 13.0 * 0.000_001;
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
            play_bell_end_animation(gent, &mut gfx_query);
            continue;
        }

        // Handle visibility during spark
        update_bell_spark_visibility(&mut alive, gent, &mut gfx_query);

        // Fire due rings (spawn AoE damage)
        // Drain triggers <= now and aggregate by scheduled tick so multiple hits in
        // the same originating tick produce a single ring with multiplied damage.
        let due_counts = drain_due_bell_ring_counts(&mut schedule, now);
        spawn_due_bell_rings(
            &mut commands,
            &stealthed_owner,
            bell_tf,
            &alive,
            due_counts,
        );
    }
}

fn play_bell_end_animation(
    gent: &Gent,
    gfx_query: &mut Query<
        (
            &mut Visibility,
            &mut ScriptPlayer<SpriteAnimation>,
        ),
        With<BellGfx>,
    >,
) {
    let Ok((_vis, mut anim)) = gfx_query.get_mut(gent.e_gfx) else {
        return;
    };

    anim.play_key("anim.player.AmplifiedBellEnd");
}

fn update_bell_spark_visibility(
    alive: &mut BellAlive,
    gent: &Gent,
    gfx_query: &mut Query<
        (
            &mut Visibility,
            &mut ScriptPlayer<SpriteAnimation>,
        ),
        With<BellGfx>,
    >,
) {
    let target_visibility = if alive.hide_ticks_remaining > 0 {
        alive.hide_ticks_remaining -= 1;
        Visibility::Hidden
    } else {
        Visibility::Visible
    };

    let Ok((mut vis, _anim)) = gfx_query.get_mut(gent.e_gfx) else {
        return;
    };

    *vis = target_visibility;
}

fn drain_due_bell_ring_counts(
    schedule: &mut BellRingSchedule,
    now: u64,
) -> BTreeMap<u64, u32> {
    let mut remaining = Vec::with_capacity(schedule.triggers.len());
    let mut due_counts = BTreeMap::new();
    for &trigger_tick in schedule.triggers.iter() {
        record_bell_ring_trigger(
            trigger_tick,
            now,
            &mut due_counts,
            &mut remaining,
        );
    }
    schedule.triggers = remaining;

    due_counts
}

fn record_bell_ring_trigger(
    trigger_tick: u64,
    now: u64,
    due_counts: &mut BTreeMap<u64, u32>,
    remaining: &mut Vec<u64>,
) {
    if trigger_tick > now {
        remaining.push(trigger_tick);
        return;
    }

    *due_counts.entry(trigger_tick).or_insert(0) += 1;
}

fn spawn_due_bell_rings(
    commands: &mut Commands,
    stealthed_owner: &Query<(), With<StealthEffect>>,
    bell_tf: &GlobalTransform,
    alive: &BellAlive,
    due_counts: BTreeMap<u64, u32>,
) {
    if due_counts.is_empty() {
        return;
    }

    let world_pos = bell_tf.translation();
    let owner_is_stealthed = stealthed_owner.get(alive.owner).is_ok();
    for (_scheduled_tick, count) in due_counts {
        let scaled_damage = RING_DAMAGE * (count as f32);
        spawn_ring_damage(
            commands,
            world_pos,
            alive.owner,
            owner_is_stealthed,
            scaled_damage,
        );
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
    mut damage_events: MessageReader<crate::game::combat::DamageInfo>,
    mut query: Query<(
        Entity,
        &mut BellAlive,
        &mut BellRingSchedule,
    )>,
    time: Res<GameTime>,
    mut ripple_events: MessageWriter<RippleEvent>,
    mut foliage_impulses: MessageWriter<FoliageImpulseEvent>,
    bell_tf_query: Query<&GlobalTransform>,
) {
    let now = time.tick();
    for dmg in damage_events.read() {
        if let Ok((bell_entity, mut alive, mut schedule)) =
            query.get_mut(dmg.target)
        {
            // Schedule ring with delay
            schedule.triggers.push(now + RING_DELAY_TICKS as u64);
            // Spawn ring visual spark immediately (as independent entity followed to gent)
            spawn_ring_visual(
                &mut commands,
                bell_entity,
                alive.ring_z_counter,
            );
            // Keep Z deltas tiny to remain behind player (player ~15e-6)
            alive.ring_z_counter += 0.000_000_01; // 1e-8 per ring
            alive.hide_ticks_remaining = RING_SPARK_TICKS;
            write_bell_ring_ripple(
                bell_entity,
                &bell_tf_query,
                &mut ripple_events,
                &mut foliage_impulses,
            );
        }
    }
}

fn write_bell_ring_ripple(
    bell_entity: Entity,
    bell_tf_query: &Query<&GlobalTransform>,
    ripple_events: &mut MessageWriter<RippleEvent>,
    foliage_impulses: &mut MessageWriter<FoliageImpulseEvent>,
) {
    let Ok(tf) = bell_tf_query.get(bell_entity) else {
        return;
    };

    let descriptor = RippleDescriptor::shockwave();
    ripple_events.write(RippleEvent {
        origin: tf.translation(),
        descriptor,
    });
    foliage_impulses.write(FoliageImpulseEvent::ring(
        tf.translation().truncate(),
        descriptor.start_radius,
        descriptor.max_radius,
        descriptor.speed_per_tick,
        descriptor.width,
        4.0,
        descriptor.fade_power,
    ));
}

// Fallback/robust trigger: detect direct overlaps between the bell collider and any player attack colliders.
// This allows the bell to ring even if the damage event route misses for any reason.
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
        new_set.retain(|target| {
            !should_prune_bell_aoe_target(
                *target, src_pos, &target_tf, &is_bell,
            )
        });
        src.target_set = new_set;
    }
}

fn should_prune_bell_aoe_target(
    target: Entity,
    src_pos: Vec3,
    target_tf: &Query<&GlobalTransform>,
    is_bell: &Query<(), With<Bell>>,
) -> bool {
    if is_bell.get(target).is_ok() {
        return true;
    }

    let Ok(tf) = target_tf.get(target) else {
        return false;
    };

    let bell_bottom = src_pos.y - BELL_HALF_HEIGHT - 1.0;
    tf.translation().y < bell_bottom
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
