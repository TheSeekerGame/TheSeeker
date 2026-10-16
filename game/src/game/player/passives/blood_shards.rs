use crate::game::player::scatter_homing::{ScatterHoming, ScatterHomingProfile};
use bevy::prelude::*;
use bevy::camera::visibility::RenderLayers;
use rand::prelude::SliceRandom;
use rand::Rng;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::effects::{FadeCurve, GhostingSource, ScaleCurve};
use theseeker_engine::physics::{groups, Collider, LinearVelocity};
use theseeker_engine::script::ScriptPlayer;

use crate::game::combat::damage_source::DamageSource;
use crate::game::combat::{Health, Hit};
use crate::game::enemy::Enemy;
use crate::game::gentstate::Dead;
use crate::game::player::spawns::amplified_bell::Bell;
use crate::game::player::{Passive, Passives, Player};
use crate::graphics::pixel_perfect::pixel_perfect_layers;
use crate::StateDespawnMarker;


/// A heal was applied; Blood Shards turn its overheal into projectiles.
#[derive(Message, Clone, Copy)]
pub struct HealApplied {
    pub target: Entity,
    /// Healing that would have been applied beyond max health
    pub overheal: u32,
    /// Max health at the time of the heal
    pub max_health: u32,
}

impl HealApplied {
    /// Heals `health` by `amount`, capped at max health.
    pub fn heal(target: Entity, health: &mut Health, amount: u32) -> Self {
        let before = health.current;
        health.current = health.current.saturating_add(amount).min(health.max);
        let healed = health.current.saturating_sub(before);
        Self { target, overheal: amount.saturating_sub(healed), max_health: health.max }
    }
}

#[derive(Component)]
pub struct BloodShard {
    pub target: Option<Entity>,
    motion: ScatterHoming,
    pub pickup_active: bool,
}

// Shard size tiers for visual representation
#[derive(Clone, Copy, PartialEq)]
enum ShardSizeTier {
    Tiny,
    Small,
    Medium,
    Large,
}

impl ShardSizeTier {
    /// Determine size tier based on heal percentage of max health
    fn from_heal_percent(heal_pct: f32) -> Self {
        if heal_pct <= 0.03 {
            // Up to 3% (e.g., bloodstone 2 HP on 120 max HP = 1.67%)
            ShardSizeTier::Small
        } else if heal_pct <= 0.10 {
            ShardSizeTier::Medium
        } else {
            ShardSizeTier::Large
        }
    }

    /// Demote tier by one (for Prism effect)
    fn demoted(self) -> Self {
        match self {
            ShardSizeTier::Large => ShardSizeTier::Medium,
            ShardSizeTier::Medium => ShardSizeTier::Small,
            ShardSizeTier::Small | ShardSizeTier::Tiny => ShardSizeTier::Tiny,
        }
    }

    /// Animation slot name for this tier
    fn slot_name(self) -> &'static str {
        match self {
            ShardSizeTier::Tiny => "ShardTiny",
            ShardSizeTier::Small => "ShardSmall",
            ShardSizeTier::Medium => "ShardMedium",
            ShardSizeTier::Large => "ShardLarge",
        }
    }
}

// Tick-based tuning (matching xp_orbs pattern)
const SHARD_LIFETIME_TICKS: u32 = 240;

const EXPLOSION_SPEED_PER_TICK_MIN: f32 = 1.25;
const EXPLOSION_SPEED_PER_TICK_MAX: f32 = 2.30;
const POS_RADIUS: f32 = 3.0;

const DIST_THRESHOLD: f32 = 2.0;
const SHARD_COLLIDER_RADIUS: f32 = 3.0;
const SHARD_MAX_TARGET_DISTANCE: f32 = 180.0;

// Damage scaling constants
const BASE_DAMAGE_PER_OVERHEAL_DEFAULT: f32 = 1.5;

type BloodShardTargetFilter = (
    Or<(With<Enemy>, With<Bell>)>,
    Without<Dead>,
);
type BloodShardTargetQuery<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static GlobalTransform),
    BloodShardTargetFilter,
>;

pub fn spawn_blood_shards_on_heal(
    mut commands: Commands,
    mut heal_events: MessageReader<HealApplied>,
    player_q: Query<(Entity, &GlobalTransform, &Passives), With<Player>>,
    targets_q: BloodShardTargetQuery<'_, '_>,
) {
    let Ok((player, player_tr, passives)) = player_q.single() else {
        return;
    };
    if !passives.contains(&Passive::BloodShards) {
        return;
    }

    let has_prism = passives.contains(&Passive::Prism);
    let damage_per_overheal = passives
        .passive_effective_value(Passive::BloodShards)
        .unwrap_or(BASE_DAMAGE_PER_OVERHEAL_DEFAULT);
    let player_pos = player_tr.translation().truncate();
    let mut rng = rand::rng();

    for evt in heal_events.read() {
        // Only trigger on overheal (healing while at full health)
        if evt.target != player || evt.overheal == 0 {
            continue;
        }

        // Calculate heal percentage and size tier
        let heal_pct = evt.overheal as f32 / evt.max_health.max(1) as f32;
        let base_tier = ShardSizeTier::from_heal_percent(heal_pct);

        // Prism: triple shards but demote tier
        let (shard_count, size_tier) = if has_prism {
            (3, base_tier.demoted())
        } else {
            (1, base_tier)
        };

        // Damage scales smoothly with overheal (not tied to visual tier)
        // Prism keeps full damage per shard, effectively tripling total damage
        let per_shard_damage =
            (evt.overheal as f32 * damage_per_overheal).max(1.0);

        // Gather candidate targets and shuffle for uniqueness
        let mut candidates: Vec<(Entity, Vec2)> = targets_q
            .iter()
            .filter_map(|(e, tr)| blood_shard_candidate(e, tr, player_pos))
            .collect();
        candidates.shuffle(&mut rng);

        for _ in 0..shard_count {
            // Random offset from player position
            let offset = Vec2::new(
                rng.random_range(-POS_RADIUS..POS_RADIUS),
                rng.random_range(-POS_RADIUS..POS_RADIUS),
            )
            .clamp_length_max(POS_RADIUS);
            let spawn_pos = player_pos + offset;

            // Random explosion direction
            let dir = blood_shard_explosion_direction(offset, &mut rng);
            let speed = rng.random_range(
                EXPLOSION_SPEED_PER_TICK_MIN..EXPLOSION_SPEED_PER_TICK_MAX,
            );
            let vel = dir * speed;

            // Assign target from candidates (each shard gets a unique target if available)
            let target = candidates.pop().map(|(e, _)| e);

            // Set up animation with correct size tier
            let mut anim = ScriptPlayer::<SpriteAnimation>::default();
            anim.play_key("anim.particles.BloodShards");
            anim.set_slot(size_tier.slot_name(), true);

            commands.spawn((
                // Core gameplay components
                (
                    DamageSource::new(
                        SHARD_LIFETIME_TICKS,
                        player,
                        per_shard_damage,
                    )
                    .with_max_targets(1),
                    BloodShard {
                        target,
                        motion: ScatterHoming::new(MOTION),
                        pickup_active: false,

                    },
                    LinearVelocity(vel),
                    Collider::ball(SHARD_COLLIDER_RADIUS),
                    groups::player_attack(),
                ),
                // Animation + sprite
                (
                    anim,
                    Sprite {
                        texture_atlas: Some(TextureAtlas::default()),
                        ..Default::default()
                    },
                ),
                // Transforms and visibility
                (
                    Transform::from_translation(
                        spawn_pos.extend(15.0 * 0.000_001 - 0.000_000_1),
                    ),
                    GlobalTransform::default(),
                    Visibility::Visible,
                    InheritedVisibility::VISIBLE,
                    ViewVisibility::default(),
                ),
                // Rendering and effects
                (
                    pixel_perfect_layers(),
                    GhostingSource {
                        spawn_interval_ticks: 1,
                        ghost_lifetime_ticks: 20,
                        initial_alpha: 1.0,
                        fade_curve: FadeCurve::Constant,
                        scale_over_time: ScaleCurve::Axes { x: 1.0, y: 0.5 },
                        render_layers: Some(RenderLayers::layer(0)),
                        ..Default::default()
                    },
                    StateDespawnMarker,
                    Name::new("BloodShard"),
                ),
            ));
        }
    }
}

fn blood_shard_candidate(
    entity: Entity,
    transform: &GlobalTransform,
    player_pos: Vec2,
) -> Option<(Entity, Vec2)> {
    let pos = transform.translation().truncate();
    let max_dist2 = SHARD_MAX_TARGET_DISTANCE * SHARD_MAX_TARGET_DISTANCE;
    (pos.distance_squared(player_pos) <= max_dist2).then_some((entity, pos))
}

fn blood_shard_explosion_direction(offset: Vec2, rng: &mut impl Rng) -> Vec2 {
    if offset.length_squared() > 1e-3 {
        return offset.normalize();
    }

    let angle = rng.random_range(0.0..std::f32::consts::TAU);
    Vec2::new(angle.cos(), angle.sin())
}

pub fn update_blood_shards_pos(
    mut query: Query<(
        &mut Transform,
        &LinearVelocity,
        &BloodShard,
    )>,
) {
    for (mut tr, vel, _shard) in query.iter_mut() {
        tr.translation += vel.0.extend(0.0);
    }
}

pub fn update_blood_shards_vel_and_hit(
    mut commands: Commands,
    mut query: Query<(
        Entity,
        &GlobalTransform,
        &mut LinearVelocity,
        &mut BloodShard,
    )>,
    target_q: BloodShardTargetQuery<'_, '_>,
) {
    for (entity, tr, mut vel, mut shard) in query.iter_mut() {
        let pos = tr.translation().truncate();

        tick_blood_shard_explosion(&mut shard, &mut vel);

        clear_invalid_blood_shard_target(&mut shard, &target_q);
        retarget_blood_shard(&mut shard, pos, &target_q);

        if update_blood_shard_homing(pos, &mut vel, &mut shard, &target_q) {
            // Shard reached target - damage is applied via DamageSource collision.
            commands.entity(entity).despawn();
            continue;
        }

        // Despawn if velocity collapsed and no target to chase
        if shard.pickup_active
            && shard.target.is_none()
            && vel.0.length_squared() < 0.01
        {
            commands.entity(entity).despawn();
        }
    }
}

fn tick_blood_shard_explosion(shard: &mut BloodShard, vel: &mut LinearVelocity) {
    if let Some(finished) = shard.motion.scatter(&mut vel.0, MOTION) {
        shard.pickup_active |= finished;
    }
}

fn clear_invalid_blood_shard_target(
    shard: &mut BloodShard,
    target_q: &BloodShardTargetQuery<'_, '_>,
) {
    let Some(target_entity) = shard.target else {
        return;
    };

    if target_q.get(target_entity).is_err() {
        shard.target = None;
    }
}

fn retarget_blood_shard(
    shard: &mut BloodShard,
    pos: Vec2,
    target_q: &BloodShardTargetQuery<'_, '_>,
) {
    if !shard.pickup_active || shard.target.is_some() {
        return;
    }

    shard.target = closest_blood_shard_target(pos, target_q);
}

fn closest_blood_shard_target(
    pos: Vec2,
    target_q: &BloodShardTargetQuery<'_, '_>,
) -> Option<Entity> {
    target_q
        .iter()
        .min_by(|a, b| compare_blood_shard_target_distance(pos, a.1, b.1))
        .map(|(entity, _)| entity)
}

fn compare_blood_shard_target_distance(
    pos: Vec2,
    a: &GlobalTransform,
    b: &GlobalTransform,
) -> std::cmp::Ordering {
    let dist_a = a.translation().truncate().distance_squared(pos);
    let dist_b = b.translation().truncate().distance_squared(pos);
    dist_a
        .partial_cmp(&dist_b)
        .unwrap_or(std::cmp::Ordering::Equal)
}

fn update_blood_shard_homing(
    pos: Vec2,
    vel: &mut LinearVelocity,
    shard: &mut BloodShard,
    target_q: &BloodShardTargetQuery<'_, '_>,
) -> bool {
    let Some(target_entity) = shard.target else {
        return false;
    };
    let Ok((_, target_tr)) = target_q.get(target_entity) else {
        return false;
    };

    let target_pos = target_tr.translation().truncate();
    let dist = pos.distance(target_pos);
    if shard.pickup_active && dist < DIST_THRESHOLD {
        return true;
    }
    if dist < DIST_THRESHOLD {
        return false;
    }

    steer_blood_shard_toward_target(pos, target_pos, dist, vel, shard);
    false
}

fn steer_blood_shard_toward_target(
    pos: Vec2, target_pos: Vec2, dist: f32, vel: &mut LinearVelocity, shard: &mut BloodShard,
) {
    shard.motion.steer(&mut vel.0, (target_pos - pos).normalize_or_zero(), dist, MOTION);
}

/// Cleanup on hit - removes shards immediately after registering a hit.
pub fn despawn_blood_shard_on_hit(
    query: Query<(Entity, &DamageSource), (With<BloodShard>, Added<Hit>)>,
    mut commands: Commands,
) {
    for (entity, _) in query.iter() {
        commands.entity(entity).despawn();
    }
}

const MOTION: ScatterHomingProfile = ScatterHomingProfile {
    ticks: 22, damping_start: 0.98, damping_end: 0.92, max_speed: 2.0, steering: 0.10,
};
