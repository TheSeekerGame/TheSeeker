use bevy::ecs::hierarchy::ChildOf;
use bevy::camera::visibility::RenderLayers;
use theseeker_engine::animation::SpriteAnimationBundle;
use theseeker_engine::effects::{GhostMovement, GhostingSource};
use theseeker_engine::time::GameTickUpdate;

use crate::game::combat::{DamageSource, Health, Stealthed};
// Note: Kinetic orbs target any enemy bodies; explicit imports are unnecessary here
use crate::game::effects::stealthed::{self, StealthEffect};
use crate::game::player::{
    Passive, Passives, Player, PlayerStatMod, PlayerStateSet,
};
use crate::prelude::*;
use theseeker_engine::physics::{
    CollisionGroups, PhysicsWorld, ENEMY, PLAYER_ATTACK,
};

const NUM_ORBS: usize = 3;
const NUM_ORBS_WITH_PRISM: usize = 9;
const PRISM_ANGLE_OFFSET: f32 = 30.0 * core::f32::consts::PI / 180.0; // 30 degrees in radians
const PRISM_RADIUS_STEP: f32 = 4.0; // Additional radius per Prism copy (base, +4, +8)
const ORB_RADIUS: f32 = 18.0; // Base orb distance from player
const TWO_PI: f32 = core::f32::consts::PI * 2.0;
// Re-arm threshold per hit: one third of a full rotation
const ROTATION_SEGMENT: f32 = TWO_PI / 3.0;
const BASE_ORB_DAMAGE_DEFAULT: f32 = 12.0;

#[derive(Component)]
pub struct KineticOrbController {
    pub orbs: Vec<Entity>,
    pub radius: f32,
    pub vel_hist: VelocityAverager,
    pub avg_speed_px_per_tick: f32,
    pub prism_active: bool,
}

#[derive(Component)]
pub struct KineticOrb {
    pub owner: Entity,
    pub angle: f32,
    pub prev_angle: f32,
    pub accum_since_hit: f32,
    pub can_hit: bool,
    pub radius: f32,
}

#[derive(Default, Clone)]
pub struct VelocityAverager {
    buf: Vec<f32>,
    head: usize,
    count: usize,
    sum: f32,
    prev_pos: Vec2,
    initialized: bool,
}

impl VelocityAverager {
    pub fn new(capacity: usize) -> Self {
        Self {
            buf: vec![0.0; capacity],
            head: 0,
            count: 0,
            sum: 0.0,
            prev_pos: Vec2::ZERO,
            initialized: false,
        }
    }
    pub fn set_initial_pos(&mut self, pos: Vec2) {
        self.prev_pos = pos;
        self.initialized = true;
    }
    pub fn push_distance(&mut self, dist: f32) {
        let cap = self.buf.len();
        if cap == 0 {
            return;
        }
        let old = self.buf[self.head];
        self.buf[self.head] = dist;
        self.head = (self.head + 1) % cap;
        if self.count < cap {
            self.count += 1;
        }
        self.sum += dist - old;
    }
    pub fn average(&self) -> f32 {
        if self.count == 0 {
            0.0
        } else {
            self.sum / (self.count as f32)
        }
    }
}

pub struct KineticOrbPlugin;

impl Plugin for KineticOrbPlugin {
    fn build(&self, app: &mut App) {
        // Presence management: after passives establish inventory
        app.add_systems(
            GameTickUpdate,
            ensure_kinetic_orb_presence
                .in_set(PlayerStateSet::Behavior)
                .after(crate::game::player::passives::runtime::process_passive_stats),
        );

        // Motion and damage sync: run late, after player movement resolves, before ghost spawn
        app.add_systems(
            GameTickUpdate,
            (
                sample_player_speed_end_of_tick,
                update_kinetic_orbs,
            )
                .chain()
                .in_set(PlayerStateSet::Animation)
                .before(theseeker_engine::effects::GhostingSet::Spawn),
        );

        // Target collection + damage property sync in their existing locations
        app.add_systems(
            GameTickUpdate,
            (
                populate_kinetic_orb_targets
                    .after(crate::game::combat::damage_source::determine_damage_targets)
                    .before(crate::game::combat::damage_source::apply_damage_modifications),
                sync_kinetic_orb_damage_properties
                    .after(stealthed::stealth_effect_system)
                    .before(crate::game::combat::damage_source::apply_damage_modifications),
            )
                .chain(),
        );
    }
}

fn ensure_kinetic_orb_presence(
    mut commands: Commands,
    mut q: Query<
        (
            Entity,
            Option<&mut KineticOrbController>,
            &Passives,
            &GlobalTransform,
        ),
        With<Player>,
    >,
) {
    for (player_e, maybe_ctrl, passives, player_tf) in q.iter_mut() {
        let should_have = passives.contains(&Passive::KineticOrb);
        let has_prism = passives.contains(&Passive::Prism);
        let base_orb_damage = passives
            .passive_effective_value(Passive::KineticOrb)
            .unwrap_or(BASE_ORB_DAMAGE_DEFAULT);

        match (should_have, maybe_ctrl) {
            (true, None) => {
                // Create controller with orbs
                spawn_orb_controller(
                    &mut commands,
                    player_e,
                    player_tf,
                    has_prism,
                    base_orb_damage,
                );
            },
            (true, Some(mut ctrl)) => {
                update_existing_orb_controller(
                    &mut commands,
                    player_e,
                    player_tf,
                    &mut ctrl,
                    has_prism,
                    base_orb_damage,
                );
            },
            (false, Some(ctrl)) => {
                // Despawn orbs and remove controller
                despawn_kinetic_orbs(&mut commands, &ctrl.orbs);
                commands.entity(player_e).remove::<KineticOrbController>();
            },
            _ => {},
        }
    }
}

fn update_existing_orb_controller(
    commands: &mut Commands,
    player_e: Entity,
    player_tf: &GlobalTransform,
    ctrl: &mut KineticOrbController,
    has_prism: bool,
    base_orb_damage: f32,
) {
    if ctrl.prism_active != has_prism {
        despawn_kinetic_orbs(commands, &ctrl.orbs);
        commands.entity(player_e).remove::<KineticOrbController>();
        spawn_orb_controller(
            commands,
            player_e,
            player_tf,
            has_prism,
            base_orb_damage,
        );
        return;
    }

    ctrl.radius = ORB_RADIUS;
}

fn despawn_kinetic_orbs(commands: &mut Commands, orbs: &[Entity]) {
    for orb in orbs {
        commands.entity(*orb).despawn();
    }
}

/// Spawns the KineticOrbController and all orbs for a player.
/// With Prism: 9 orbs in 3 clusters (each cluster has orbs at base_angle ± 5°).
/// Without Prism: 3 orbs evenly spaced at 120° apart.
fn spawn_orb_controller(
    commands: &mut Commands,
    player_e: Entity,
    player_tf: &GlobalTransform,
    has_prism: bool,
    base_orb_damage: f32,
) {
    let orb_count = if has_prism {
        NUM_ORBS_WITH_PRISM
    } else {
        NUM_ORBS
    };
    let damage_per_orb = base_orb_damage * NUM_ORBS as f32 / orb_count as f32;

    let mut ctrl = KineticOrbController {
        orbs: Vec::with_capacity(orb_count),
        radius: ORB_RADIUS,
        vel_hist: VelocityAverager::new(1152),
        avg_speed_px_per_tick: 0.0,
        prism_active: has_prism,
    };
    ctrl.vel_hist
        .set_initial_pos(player_tf.translation().truncate());

    // Local z for on-top rendering relative to player
    let z = 30.0 * 0.000_001;
    for i in 0..orb_count {
        let angle = calculate_orb_angle(i, orb_count, has_prism);
        let radius = calculate_orb_radius(i, has_prism);
        let offset = Vec2::new(angle.cos(), angle.sin()) * radius;
        let local = Vec3::new(offset.x, offset.y, z);

        let orb_e = commands
            .spawn((
                KineticOrb {
                    owner: player_e,
                    angle,
                    prev_angle: angle,
                    accum_since_hit: 0.0,
                    can_hit: true,
                    radius,
                },
                Transform::from_translation(local),
                GlobalTransform::default(),
                Visibility::Visible,
                InheritedVisibility::default(),
                ViewVisibility::default(),
                SpriteAnimationBundle::new_play_key("anim.player.KineticOrb"),
                Sprite {
                    texture_atlas: Some(TextureAtlas::default()),
                    ..Default::default()
                },
                DamageSource::new(u32::MAX, player_e, damage_per_orb)
                    .with_max_targets(3),
                GhostingSource {
                    spawn_interval_ticks: 2,
                    ghost_lifetime_ticks: 40,
                    initial_alpha: 0.5,
                    movement: GhostMovement::CopyDisplacementFrom(player_e),
                    render_layers: Some(RenderLayers::default()),
                    ..Default::default()
                },
                StateDespawnMarker,
            ))
            .insert(ChildOf(player_e))
            .id();
        ctrl.orbs.push(orb_e);
    }

    commands.entity(player_e).insert(ctrl);
}

/// Calculates the angular position for an orb based on its index.
/// With Prism: orbs are grouped into 3 clusters, each cluster has 3 orbs at ±30° offsets.
/// Without Prism: orbs are evenly spaced.
fn calculate_orb_angle(
    index: usize,
    _orb_count: usize,
    has_prism: bool,
) -> f32 {
    if has_prism {
        // 9 orbs in 3 clusters of 3
        let cluster = index / 3; // 0, 0, 0, 1, 1, 1, 2, 2, 2
        let offset_idx = index % 3; // 0, 1, 2 within each cluster
        let base_angle = cluster as f32 * (TWO_PI / 3.0);
        let offset = match offset_idx {
            0 => 0.0,
            1 => PRISM_ANGLE_OFFSET,
            _ => -PRISM_ANGLE_OFFSET,
        };
        base_angle + offset
    } else {
        // 3 orbs evenly spaced
        index as f32 * (TWO_PI / NUM_ORBS as f32)
    }
}

/// Calculates the orbital radius for an orb based on its index.
/// With Prism: orbs in a cluster have progressively larger radii.
/// Without Prism: all orbs use the base radius.
fn calculate_orb_radius(index: usize, has_prism: bool) -> f32 {
    if has_prism {
        let offset_idx = index % 3; // 0, 1, 2 within each cluster
        ORB_RADIUS + (offset_idx as f32) * PRISM_RADIUS_STEP
    } else {
        ORB_RADIUS
    }
}

fn sample_player_speed_end_of_tick(
    mut q: Query<
        (
            &GlobalTransform,
            &mut KineticOrbController,
        ),
        With<Player>,
    >,
) {
    for (tf, mut ctrl) in q.iter_mut() {
        let pos = tf.translation().truncate();
        if !ctrl.vel_hist.initialized {
            ctrl.vel_hist.set_initial_pos(pos);
        }
        let dist = pos.distance(ctrl.vel_hist.prev_pos);
        ctrl.vel_hist.prev_pos = pos;
        ctrl.vel_hist.push_distance(dist);
        ctrl.avg_speed_px_per_tick = ctrl.vel_hist.average();
    }
}

fn normalize_angle(mut a: f32) -> f32 {
    while a > core::f32::consts::PI {
        a -= TWO_PI;
    }
    while a < -core::f32::consts::PI {
        a += TWO_PI;
    }
    a
}

fn update_kinetic_orbs(
    mut orb_q: Query<(
        &mut KineticOrb,
        &mut Transform,
        &mut DamageSource,
    )>,
    ctrl_q: Query<&KineticOrbController, With<Player>>,
) {
    for (mut orb, mut tf, mut dmg) in orb_q.iter_mut() {
        let Ok(ctrl) = ctrl_q.get(orb.owner) else {
            continue;
        };
        let radius = orb.radius.max(1.0);
        let omega = ctrl.avg_speed_px_per_tick / radius; // radians per tick
        orb.angle -= omega; // clockwise

        // Accumulate rotation since last hit while disarmed
        if !orb.can_hit {
            update_orb_rearm_progress(&mut orb, &mut dmg);
        }
        orb.prev_angle = orb.angle;

        // Position orb relative to player (local transform since orb is a child)
        let offset = Vec2::new(orb.angle.cos(), orb.angle.sin()) * radius;
        tf.translation.x = offset.x;
        tf.translation.y = offset.y;
    }
}

fn populate_kinetic_orb_targets(
    mut orb_q: Query<(
        &mut KineticOrb,
        &Transform,
        &mut DamageSource,
    )>,
    player_tf_q: Query<&GlobalTransform, With<Player>>,
    health_q: Query<&Health>,
    physics: PhysicsWorld,
) {
    // Cast a line from the player to each orb and collect up to 3 targets along it
    for (mut orb, orb_tf, mut dmg) in orb_q.iter_mut() {
        if !orb.can_hit {
            continue;
        }

        // Get player world position
        let Ok(player_tf) = player_tf_q.get(orb.owner) else {
            continue;
        };
        let player_pos = player_tf.translation().truncate();
        // Compute orb world position from player's world + orb's local
        let orb_pos = player_pos + orb_tf.translation.truncate();
        let cast_vec = orb_pos - player_pos;
        let cast_len = cast_vec.length();
        if cast_len <= f32::EPSILON {
            continue;
        }
        let dir = cast_vec / cast_len;

        let max_add = dmg.max_targets as usize;
        let added = collect_kinetic_orb_ray_targets(
            &physics,
            &health_q,
            &mut dmg,
            orb.owner,
            player_pos,
            dir,
            cast_len,
            max_add,
        );

        if added > 0 {
            // Disarm until a full rotation passes
            orb.can_hit = false;
            orb.accum_since_hit = 0.0;
        }
    }
}

fn sync_kinetic_orb_damage_properties(
    player_q: Query<
        (
            Entity,
            Option<&Passives>,
            Option<&PlayerStatMod>,
            Has<StealthEffect>,
        ),
        With<Player>,
    >,
    mut orb_dmg_q: Query<(&KineticOrb, &mut DamageSource, Entity)>,
    mut commands: Commands,
) {
    // Build a small snapshot for the (single) player
    for (player_e, maybe_passives, statmod, is_stealthed) in player_q.iter() {
        let base_mult = statmod.map(|s| s.damage).unwrap_or(1.0);
        let base_orb_damage = maybe_passives
            .and_then(|p| p.passive_effective_value(Passive::KineticOrb))
            .unwrap_or(BASE_ORB_DAMAGE_DEFAULT);
        let base_damage = base_orb_damage * base_mult;
        sync_kinetic_orbs_for_player(
            player_e,
            base_damage,
            is_stealthed,
            &mut orb_dmg_q,
            &mut commands,
        );
    }
}

fn sync_kinetic_orbs_for_player(
    player_e: Entity,
    base_damage: f32,
    is_stealthed: bool,
    orb_dmg_q: &mut Query<(&KineticOrb, &mut DamageSource, Entity)>,
    commands: &mut Commands,
) {
    for (orb, mut dmg, orb_entity) in orb_dmg_q.iter_mut() {
        if orb.owner != player_e {
            continue;
        }

        dmg.base_damage = base_damage;
        sync_kinetic_orb_stealth(commands, orb_entity, is_stealthed);
    }
}

fn update_orb_rearm_progress(
    orb: &mut KineticOrb,
    dmg: &mut DamageSource,
) {
    let delta = normalize_angle(orb.prev_angle - orb.angle).abs();
    orb.accum_since_hit += delta;
    if orb.accum_since_hit < ROTATION_SEGMENT {
        return;
    }

    orb.can_hit = true;
    orb.accum_since_hit -= ROTATION_SEGMENT;
    dmg.damaged_set.clear();
}

#[expect(
    clippy::too_many_arguments,
    reason = "Ray target collection needs the physics query, health query, owner, segment state, and target limit in one deterministic walk."
)]
fn collect_kinetic_orb_ray_targets(
    physics: &PhysicsWorld,
    health_q: &Query<&Health>,
    dmg: &mut DamageSource,
    owner: Entity,
    mut origin: Vec2,
    dir: Vec2,
    mut remaining: f32,
    max_add: usize,
) -> usize {
    let interaction = CollisionGroups::new(PLAYER_ATTACK, ENEMY);
    let mut added = 0usize;
    while added < max_add && remaining > 0.01 {
        let Some((hit_entity, hit)) = physics.ray_cast(
            origin,
            dir,
            remaining,
            interaction,
            Some(owner),
        ) else {
            break;
        };

        if can_kinetic_orb_hit_entity(hit_entity, health_q, dmg) {
            dmg.target_set.insert(hit_entity);
            added += 1;
        }

        let eps = 0.25_f32;
        let step = (hit.time_of_impact + eps).min(remaining);
        origin += dir * step;
        remaining -= step;
    }

    added
}

fn can_kinetic_orb_hit_entity(
    hit_entity: Entity,
    health_q: &Query<&Health>,
    dmg: &DamageSource,
) -> bool {
    let Ok(hp) = health_q.get(hit_entity) else {
        return false;
    };

    hp.current > 0 && !dmg.damaged_set.contains(&hit_entity)
}

fn sync_kinetic_orb_stealth(
    commands: &mut Commands,
    orb_entity: Entity,
    is_stealthed: bool,
) {
    if is_stealthed {
        commands.entity(orb_entity).insert(Stealthed);
        return;
    }

    commands.entity(orb_entity).remove::<Stealthed>();
}
