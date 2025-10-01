use bevy::ecs::hierarchy::ChildOf;
use bevy::prelude::*;
use bevy::render::view::RenderLayers;

use theseeker_engine::animation::SpriteAnimationBundle;
use theseeker_engine::effects::{GhostMovement, GhostingSource};
use theseeker_engine::time::GameTickUpdate;

use crate::game::combat::{DamageSource, Health, Stealthed};
// Note: Kinetic orbs target any enemy bodies; explicit imports are unnecessary here
use crate::game::player::{
    Passive, Passives, Player, PlayerStatMod, PlayerStateSet,
};
use crate::game::effects::stealthed::{self, StealthEffect};
use crate::prelude::*;
use theseeker_engine::physics::{
    CollisionGroups as InteractionGroups, PhysicsWorld, ENEMY, PLAYER_ATTACK,
};

const NUM_ORBS: usize = 3;
const ORB_RADIUS: f32 = 18.0; // Orb distance from player
const _HIT_RADIUS: f32 = 15.0; // Unused: kept for potential future widening of damage area
const TWO_PI: f32 = core::f32::consts::PI * 2.0;
// Re-arm threshold per hit: one third of a full rotation
const ROTATION_SEGMENT: f32 = TWO_PI / 3.0;

#[derive(Component)]
pub struct KineticOrbController {
    pub orbs: Vec<Entity>,
    pub radius: f32,
    pub vel_hist: VelocityAverager,
    pub avg_speed_px_per_tick: f32,
}

#[derive(Component)]
pub struct KineticOrb {
    pub owner: Entity,
    pub angle: f32,
    pub prev_angle: f32,
    pub accum_since_hit: f32,
    pub can_hit: bool,
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
    #[allow(dead_code)]
    pub fn capacity(&self) -> usize {
        self.buf.len()
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
                .after(crate::game::player::passives::runtime::process_passives_new_system),
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
        match (should_have, maybe_ctrl) {
            (true, None) => {
                // Create controller
                let mut ctrl = KineticOrbController {
                    orbs: Vec::with_capacity(NUM_ORBS),
                    radius: ORB_RADIUS,
                    vel_hist: VelocityAverager::new(1152),
                    avg_speed_px_per_tick: 0.0,
                };
                ctrl.vel_hist
                    .set_initial_pos(player_tf.translation().truncate());

                // Spawn orbs evenly spaced
                let base_angle = 0.0_f32;
                // Local z and player render layer to ensure on-top rendering (relative to player)
                let z = 30.0 * 0.000001;
                for i in 0..NUM_ORBS {
                    let angle =
                        base_angle + (i as f32) * (TWO_PI / NUM_ORBS as f32);
                    let offset =
                        Vec2::new(angle.cos(), angle.sin()) * ctrl.radius;
                    // Local position relative to player
                    let local = Vec3::new(offset.x, offset.y, z);
                    let orb_e = commands
                        .spawn((
                            KineticOrb {
                                owner: player_e,
                                angle,
                                prev_angle: angle,
                                accum_since_hit: 0.0,
                                can_hit: true,
                            },
                            Transform::from_translation(local),
                            GlobalTransform::default(),
                            Visibility::Visible,
                            InheritedVisibility::default(),
                            ViewVisibility::default(),
                            RenderLayers::layer(2),
                            SpriteAnimationBundle::new_play_key(
                                "anim.player.KineticOrb",
                            ),
                            Sprite {
                                texture_atlas: Some(TextureAtlas::default()),
                                ..Default::default()
                            },
                            DamageSource::new(u32::MAX, player_e, 12.0)
                                .with_max_targets(3),
                            // Ghosting: copy player velocity so trails stay circular while moving
                            GhostingSource {
                                spawn_interval_ticks: 2,
                                ghost_lifetime_ticks: 40,
                                initial_alpha: 0.5,
                                movement: GhostMovement::CopyDisplacementFrom(
                                    player_e,
                                ),
                                ..Default::default()
                            },
                            StateDespawnMarker,
                        ))
                        .insert(ChildOf(player_e))
                        .id();
                    ctrl.orbs.push(orb_e);
                }

                commands.entity(player_e).insert(ctrl);
            },
            (true, Some(mut ctrl)) => {
                // Keep controller radius in sync with tuning constant
                if ctrl.radius != ORB_RADIUS {
                    ctrl.radius = ORB_RADIUS;
                }
            },
            (false, Some(ctrl)) => {
                // Despawn orbs and remove controller
                for orb in ctrl.orbs.iter() {
                    commands.entity(*orb).despawn();
                }
                commands.entity(player_e).remove::<KineticOrbController>();
            },
            _ => {},
        }
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
        let radius = ctrl.radius.max(1.0);
        let omega = ctrl.avg_speed_px_per_tick / radius; // radians per tick
        orb.angle -= omega; // clockwise

        // Accumulate rotation since last hit while disarmed
        if !orb.can_hit {
            let delta = normalize_angle(orb.prev_angle - orb.angle).abs();
            orb.accum_since_hit += delta;
            if orb.accum_since_hit >= ROTATION_SEGMENT {
                // Re-arm and allow new hits; clear damaged set so previous targets are eligible again
                orb.can_hit = true;
                orb.accum_since_hit -= ROTATION_SEGMENT;
                dmg.damaged_set.clear();
            }
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

        // Only interact with enemy bodies (includes bells) — ignore ground
        let interaction = InteractionGroups::new(PLAYER_ATTACK, ENEMY);
        let mut origin = player_pos;
        let mut remaining = cast_len;
        let max_add = dmg.max_targets as usize;
        let mut added = 0usize;

        // Iterate along the segment, advancing origin past each hit
        while added < max_add && remaining > 0.01 {
            if let Some((hit_entity, hit)) = physics.ray_cast(
                origin,
                dir,
                remaining,
                true,
                interaction,
                Some(orb.owner),
            ) {
                // Validate alive
                if let Ok(hp) = health_q.get(hit_entity) {
                    if hp.current > 0 && !dmg.damaged_set.contains(&hit_entity)
                    {
                        dmg.target_set.insert(hit_entity);
                        added += 1;
                    }
                }
                // Advance origin slightly beyond hit point to search for next
                let eps = 0.25_f32; // quarter-pixel step to avoid re-hitting
                let step = (hit.time_of_impact + eps).min(remaining);
                origin += dir * step;
                remaining -= step;
            } else {
                break;
            }
        }

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
            Option<&PlayerStatMod>,
            Has<StealthEffect>,
        ),
        With<Player>,
    >,
    mut orb_dmg_q: Query<(&KineticOrb, &mut DamageSource, Entity)>,
    mut commands: Commands,
) {
    // Build a small snapshot for the (single) player
    for (player_e, statmod, is_stealthed) in player_q.iter() {
        let base_mult = statmod.map(|s| s.damage).unwrap_or(1.0);
        let base_damage = 12.0 * base_mult;
        for (orb, mut dmg, orb_entity) in orb_dmg_q.iter_mut() {
            if orb.owner != player_e {
                continue;
            }
            dmg.base_damage = base_damage;
            if is_stealthed {
                commands.entity(orb_entity).insert(Stealthed);
            } else {
                commands.entity(orb_entity).remove::<Stealthed>();
            }
        }
    }
}
