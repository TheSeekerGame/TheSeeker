use crate::graphics::quad_mesh::{QuadMesh, add_mesh, publish_mesh};
use bevy::asset::RenderAssetUsages;
use bevy::math::Mat2;
use bevy::mesh::{Mesh, Mesh2d};
use bevy::render::render_resource::PrimitiveTopology;
use bevy::sprite_render::{ColorMaterial, MeshMaterial2d};

use crate::graphics::pixel_perfect::pixel_perfect_layers;

use theseeker_engine::animation::SpriteAnimationBundle;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::physics::{
    Collider, CollisionGroups, Group, PhysicsWorld,
};
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::GameTickUpdate;

use crate::game::combat::damage_pipeline::NoOnHitShake;
use crate::game::combat::damage_source::DamageSourceSkill;
use crate::game::combat::{DamageSource, Stealthed};
use crate::game::enemy::Enemy;
use crate::game::player::skills::quark_pylon::QUARK_PYLON_METADATA;
use crate::game::player::skills::types::SkillId;
use crate::game::player::spawns::amplified_bell::Bell;
use crate::game::player::{Player, PlayerStatMod};
use crate::prelude::*;
use std::collections::{HashMap, HashSet};

const PYLON_INTRO_TICKS: u32 = 8 * 4;
const PYLON_OUTRO_TICKS: u32 = 8 * 7;
const PYLON_BASE_Z: f32 = 17.0 * 0.000_001;
const ELECTRIC_WIDTH: f32 = 1.2;
const PIXEL_STEP: f32 = 0.5;
const ELECTRIC_DAMAGE_SCALAR: f32 = 0.18;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PylonPhase {
    Intro(u32),
    Idle,
    Outro(u32),
}

/// Logic marker for a deployed pylon
#[derive(Component)]
pub struct QuarkPylon {
    pub owner: Entity,
    pub ticks_to_expire: u32,
    pub phase: PylonPhase,
    pub last_slot: Option<&'static str>,
    pub counts_for_cooldown: bool,
}

/// Marker for the pylon animation entity
#[derive(Component)]
pub struct QuarkPylonGfx;

/// Connection between two idle pylons
#[derive(Component)]
pub struct QuarkPylonLink {
    pub a: Entity,
    pub b: Entity,
    pub owner: Entity,
    pub pulse_timer: u32,
    pub seed: u64,
}

pub struct QuarkPylonPlugin;

impl Plugin for QuarkPylonPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                tick_pylons,
                manage_links,
                cleanup_pylons,
            )
                .chain()
                .in_set(crate::game::player::PlayerStateSet::Behavior),
        );
        app.add_systems(
            GameTickUpdate,
            update_links_visuals_and_damage
                .after(manage_links)
                .after(crate::game::combat::damage_source::determine_damage_targets)
                .before(
                    crate::game::combat::damage_source::apply_damage_modifications,
                ),
        );
    }
}

/// Public helper to spawn a Quark Pylon at `center`.
pub fn spawn_quark_pylon(
    commands: &mut Commands,
    owner: Entity,
    center: Vec2,
    counts_for_cooldown: bool,
) -> Entity {
    let gfx = commands
        .spawn((
            QuarkPylonGfx,
            SpriteAnimationBundle::new_play_key("anim.player.QuarkPylon"),
            Sprite {
                texture_atlas: Some(TextureAtlas::default()),
                ..Default::default()
            },
            Transform::from_translation(Vec3::ZERO),
            GlobalTransform::default(),
            Visibility::Visible,
            InheritedVisibility::default(),
            ViewVisibility::default(),
            pixel_perfect_layers(),
            StateDespawnMarker,
            Name::new("QuarkPylonGfx"),
        ))
        .id();

    let pylon_entity = commands
        .spawn((
            QuarkPylon {
                owner,
                ticks_to_expire: QUARK_PYLON_METADATA.lifetime_ticks,
                phase: PylonPhase::Intro(PYLON_INTRO_TICKS),
                last_slot: None,
                counts_for_cooldown,
            },
            Transform::from_translation(center.extend(PYLON_BASE_Z)),
            GlobalTransform::default(),
            StateDespawnMarker,
            Name::new("QuarkPylon"),
        ))
        .id();

    commands.entity(pylon_entity).add_child(gfx);

    pylon_entity
}

fn tick_pylons(
    mut pylons: Query<(&mut QuarkPylon, &Children)>,
    mut anim_q: Query<&mut ScriptPlayer<SpriteAnimation>, With<QuarkPylonGfx>>,
) {
    for (mut pylon, children) in pylons.iter_mut() {
        // Handle intro/idle/outro phase timers
        tick_pylon_phase(&mut pylon);

        // Lifetime countdown toward outro
        tick_pylon_lifetime(&mut pylon);

        // Toggle Outro slot when entering outro
        if matches!(pylon.phase, PylonPhase::Outro(_))
            && pylon.last_slot != Some("Outro")
        {
            set_pylon_animation_slot(children, &mut anim_q, pylon.last_slot);
            pylon.last_slot = Some("Outro");
        }
    }
}

fn tick_pylon_phase(pylon: &mut QuarkPylon) {
    match pylon.phase {
        PylonPhase::Intro(ref mut intro) => {
            *intro = intro.saturating_sub(1);
            if *intro == 0 {
                pylon.phase = PylonPhase::Idle;
            }
        },
        PylonPhase::Idle => {},
        PylonPhase::Outro(ref mut outro) => {
            *outro = outro.saturating_sub(1);
        },
    }
}

fn tick_pylon_lifetime(pylon: &mut QuarkPylon) {
    if matches!(pylon.phase, PylonPhase::Outro(_)) {
        return;
    }

    pylon.ticks_to_expire = pylon.ticks_to_expire.saturating_sub(1);
    if pylon.ticks_to_expire == 0 {
        pylon.phase = PylonPhase::Outro(PYLON_OUTRO_TICKS);
    }
}

fn set_pylon_animation_slot(
    children: &Children,
    anim_q: &mut Query<&mut ScriptPlayer<SpriteAnimation>, With<QuarkPylonGfx>>,
    previous_slot: Option<&'static str>,
) {
    for child in children.iter() {
        set_pylon_child_animation_slot(child, anim_q, previous_slot);
    }
}

fn set_pylon_child_animation_slot(
    child: Entity,
    anim_q: &mut Query<&mut ScriptPlayer<SpriteAnimation>, With<QuarkPylonGfx>>,
    previous_slot: Option<&'static str>,
) {
    let Ok(mut player) = anim_q.get_mut(child) else {
        return;
    };

    if let Some(prev) = previous_slot {
        player.set_slot(prev, false);
    }
    player.set_slot("Outro", true);
}

fn manage_links(
    mut commands: Commands,
    pylons: Query<(Entity, &QuarkPylon, &GlobalTransform)>,
    mut links: Query<(Entity, &QuarkPylonLink)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    // Collect idle pylons by owner
    let mut idle: Vec<(Entity, Entity, Vec2)> = Vec::new();
    let mut idle_positions: HashMap<Entity, Vec2> = HashMap::new();
    for (entity, pylon, tf) in pylons.iter() {
        if matches!(pylon.phase, PylonPhase::Idle) {
            idle.push((
                entity,
                pylon.owner,
                tf.translation().truncate(),
            ));
            idle_positions.insert(entity, tf.translation().truncate());
        }
    }

    // Map existing link pairs for quick lookup
    let mut existing: HashMap<(Entity, Entity), Entity> = HashMap::new();
    for (entity, link) in links.iter_mut() {
        let pair = ordered_pair(link.a, link.b);
        existing.insert(pair, entity);
    }

    // Desired pairs (complete graph per owner)
    let mut desired: HashSet<(Entity, Entity, Entity)> = HashSet::new();
    for i in 0..idle.len() {
        for j in (i + 1)..idle.len() {
            record_desired_pylon_link(idle[i], idle[j], &mut desired);
        }
    }

    // Remove links that are no longer needed
    for (pair, entity) in existing.iter() {
        if !desired.iter().any(|(a, b, _)| (*a, *b) == *pair) {
            despawn_if_exists(&mut commands, *entity);
        }
    }

    // Spawn missing links
    for (a, b, owner) in desired.into_iter() {
        if existing.contains_key(&(a, b)) {
            continue;
        }
        let seed = rand::rng().random_range(0..=u64::MAX / 2);
        let pos_a = idle_positions.get(&a).copied().unwrap_or(Vec2::ZERO);
        let pos_b = idle_positions.get(&b).copied().unwrap_or(Vec2::ZERO);
        let midpoint = (pos_a + pos_b) * 0.5;
        let snap_midpoint = Vec2::new(
            quantize(midpoint.x),
            quantize(midpoint.y),
        );
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        rebuild_lightning_mesh(
            &mut mesh,
            pos_a,
            pos_b,
            snap_midpoint,
            ELECTRIC_WIDTH,
            seed,
            0,
        );
        let mesh_handle = add_mesh(&mut meshes, mesh);
        let material_handle = materials.add(ColorMaterial::from(Color::srgba(
            0.5, 0.85, 1.0, 0.9,
        )));
        let link_entity = commands
            .spawn((
                QuarkPylonLink {
                    a,
                    b,
                    owner,
                    pulse_timer: QUARK_PYLON_METADATA.pulse_interval_ticks,
                    seed,
                },
                Mesh2d(mesh_handle),
                MeshMaterial2d(material_handle),
                Transform::from_translation(Vec3::new(
                    0.0,
                    0.0,
                    PYLON_BASE_Z + 0.000_001,
                )),
                GlobalTransform::default(),
                Visibility::Visible,
                InheritedVisibility::default(),
                ViewVisibility::default(),
                Collider::ball(ELECTRIC_WIDTH),
                DamageSource::new(u32::MAX, owner, 1.0)
                    .with_max_targets(u32::MAX),
                DamageSourceSkill(SkillId::QuarkPylon),
                NoOnHitShake,
                pixel_perfect_layers(),
                StateDespawnMarker,
                Name::new("QuarkPylonLink"),
            ))
            .id();
        commands.entity(link_entity).insert((
            CollisionGroups::new(
                theseeker_engine::physics::PLAYER_ATTACK,
                Group::from_bits_truncate(
                    theseeker_engine::physics::ENEMY.bits()
                        | theseeker_engine::physics::ENEMY_INSIDE.bits()
                        | theseeker_engine::physics::PLAYER.bits(),
                ),
            ),
            crate::game::combat::damage_source::AllowOwnerHit,
        ));

        existing.insert((a, b), link_entity);
    }
}

fn record_desired_pylon_link(
    a: (Entity, Entity, Vec2),
    b: (Entity, Entity, Vec2),
    desired: &mut HashSet<(Entity, Entity, Entity)>,
) {
    let (entity_a, owner_a, _pos_a) = a;
    let (entity_b, owner_b, _pos_b) = b;
    if owner_a != owner_b {
        return;
    }

    let pair = ordered_pair(entity_a, entity_b);
    desired.insert((pair.0, pair.1, owner_a));
}

fn despawn_if_exists(commands: &mut Commands, entity: Entity) {
    commands.entity(entity).try_despawn();
}

#[expect(clippy::too_many_arguments, reason = "Bevy system dependencies are expressed as explicit system parameters.")]
#[expect(
    clippy::too_many_lines,
    reason = "This pylon link system synchronizes visuals and damage volumes for one active-link tick."
)]
fn update_links_visuals_and_damage(
    mut commands: Commands,
    mut links: Query<(
        Entity,
        &mut QuarkPylonLink,
        &mut Transform,
        &MeshMaterial2d<ColorMaterial>,
        &mut Mesh2d,
        &mut DamageSource,
    )>,
    pylons: Query<(Entity, &QuarkPylon, &GlobalTransform)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    enemy_query: Query<
        Entity,
        (
            Or<(With<Enemy>, With<Bell>, With<Player>)>,
            Without<crate::game::gentstate::Dead>,
        ),
    >,
    stat_query: Query<&PlayerStatMod, With<Player>>,
    stealth_query: Query<
        (),
        With<crate::game::effects::stealthed::StealthEffect>,
    >,
    spatial_query: PhysicsWorld,
    time: Res<theseeker_engine::time::GameTime>,
) {
    for (
        entity,
        mut link,
        mut tf,
        material_handle,
        mut mesh_handle,
        mut damage_source,
    ) in links.iter_mut()
    {
        let Ok((_, pylon_a, tf_a)) = pylons.get(link.a) else {
            despawn_if_exists(&mut commands, entity);
            continue;
        };
        let Ok((_, pylon_b, tf_b)) = pylons.get(link.b) else {
            despawn_if_exists(&mut commands, entity);
            continue;
        };
        if !matches!(pylon_a.phase, PylonPhase::Idle)
            || !matches!(pylon_b.phase, PylonPhase::Idle)
        {
            despawn_if_exists(&mut commands, entity);
            continue;
        }

        let pos_a = tf_a.translation().truncate();
        let pos_b = tf_b.translation().truncate();
        let midpoint = (pos_a + pos_b) * 0.5;
        let snap_midpoint = Vec2::new(
            quantize(midpoint.x),
            quantize(midpoint.y),
        );
        tf.translation.x = snap_midpoint.x;
        tf.translation.y = snap_midpoint.y;

        let thickness = ELECTRIC_WIDTH;
        let tick = time.tick();
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        rebuild_lightning_mesh(
            &mut mesh,
            pos_a,
            pos_b,
            snap_midpoint,
            thickness,
            link.seed,
            tick,
        );
        publish_mesh(&mut meshes, &mut mesh_handle.0, mesh);

        if let Some(mut material) = materials.get_mut(&material_handle.0) {
            let phase =
                ((tick as f32) * 0.12 + link.seed as f32 * 0.0001).sin();
            let intensity = 0.8 + 0.2 * phase.abs();
            material.color = Color::srgba(
                1.0 * intensity,
                1.0 * intensity,
                1.0 * intensity,
                0.92,
            );
        }

        if link.pulse_timer > 0 {
            link.pulse_timer -= 1;
            continue;
        }
        link.pulse_timer = QUARK_PYLON_METADATA.pulse_interval_ticks;

        let dir = pos_b - pos_a;
        let distance = dir.length();
        if distance <= f32::EPSILON {
            continue;
        }
        let rot = dir.y.atan2(dir.x);
        let half_len = distance * 0.5;
        let rot_mat = Mat2::from_angle(rot);
        let local_a = rot_mat.mul_vec2(Vec2::new(-half_len, 0.0));
        let local_b = rot_mat.mul_vec2(Vec2::new(half_len, 0.0));
        let shape = Collider::capsule(local_a, local_b, thickness * 0.5);
        let hits = spatial_query.intersect(
            midpoint,
            &shape,
            CollisionGroups::new(Group::all(), Group::all()),
            Some(entity),
        );

        let valid_hits = hits
            .into_iter()
            .filter(|e| enemy_query.get(*e).is_ok())
            .collect::<Vec<_>>();

        if valid_hits.is_empty() {
            continue;
        }

        let stat_mult =
            stat_query.get(link.owner).map(|s| s.damage).unwrap_or(1.0);
        let base_damage = distance * ELECTRIC_DAMAGE_SCALAR * stat_mult;
        damage_source.base_damage = base_damage;
        damage_source.damage = base_damage;
        damage_source.damaged_set.clear();
        damage_source.target_set.clear();
        for hit in valid_hits {
            damage_source.target_set.insert(hit);
        }

        if stealth_query.get(link.owner).is_ok() {
            commands.entity(entity).insert(Stealthed);
        } else {
            commands.entity(entity).remove::<Stealthed>();
        }
    }
}

fn rebuild_lightning_mesh(
    mesh: &mut Mesh,
    start: Vec2,
    end: Vec2,
    midpoint: Vec2,
    thickness: f32,
    seed: u64,
    tick: u64,
) {
    // Coarse jittered points (rounded to world pixels)
    let segments = 12;
    let dir = end - start;
    let len = dir.length().max(f32::EPSILON);
    let base = Vec2::new(quantize(start.x), quantize(start.y));
    let end_q = Vec2::new(quantize(end.x), quantize(end.y));
    let perp = Vec2::new(-dir.y, dir.x).normalize_or_zero();
    let mut coarse: Vec<Vec2> = Vec::with_capacity(segments + 1);
    let tick_f = tick as f32;

    let hash_noise = |seg: u32, salt: u64| -> f32 {
        let mut x = seed
            ^ (seg as u64).wrapping_mul(0x9E37_79B1_85EB_CA87)
            ^ (tick.wrapping_mul(0xC2B2_AE3D_27D4_EB4F))
            ^ salt;
        x ^= x >> 33;
        x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
        x ^= x >> 33;
        x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
        x ^= x >> 33;
        let v = (x as f64) / (u64::MAX as f64);
        (v as f32) * 2.0 - 1.0
    };

    for i in 0..=segments {
        let t = i as f32 / segments as f32;
        let anchor = base + dir * t;
        if i == 0 {
            coarse.push(base);
            continue;
        }
        if i == segments {
            coarse.push(end_q);
            continue;
        }
        let attenuation = (1.0 - (t - 0.5).abs() * 1.8).clamp(0.0, 1.0);
        let jitter_base = (len * 0.08).clamp(3.0, 22.0);
        let noise_fast = hash_noise(i as u32, 0xA24B_AED4_963E_E407);
        let noise_slow = hash_noise(i as u32, 0x9E37_79B9_7F4A_7C15);
        let wobble = (tick_f * 0.45 + i as f32 * 0.8).sin();
        let offset_mag = jitter_base
            * attenuation
            * (0.55 * noise_fast + 0.35 * noise_slow + 0.25 * wobble);
        let along = (len * 0.01 * noise_slow).clamp(-6.0, 6.0);
        let mut p =
            anchor + perp * offset_mag + dir.normalize_or_zero() * along;
        p.x = quantize(p.x);
        p.y = quantize(p.y);
        coarse.push(p);
    }

    // Convert coarse points into pixel-stair path (axis-aligned steps)
    let mut stair: Vec<Vec2> = Vec::with_capacity(coarse.len() * 2);
    stair.push(coarse[0]);
    for seg in 0..coarse.len().saturating_sub(1) {
        let a = stair.last().copied().unwrap_or(coarse[seg]);
        let b = coarse[seg + 1];
        if (a.x - b.x).abs() < f32::EPSILON || (a.y - b.y).abs() < f32::EPSILON
        {
            stair.push(b);
            continue;
        }
        let noise = hash_noise(seg as u32, 0xD1B5_4A32_D192_ED03);
        if noise >= 0.0 {
            stair.push(Vec2::new(b.x, a.y));
            stair.push(b);
        } else {
            stair.push(Vec2::new(a.x, b.y));
            stair.push(b);
        }
    }

    // Build axis-aligned quads along the stair path
    let mut quads = QuadMesh::default();
    let half = thickness * 0.5;

    for seg in 0..stair.len().saturating_sub(1) {
        let a = stair[seg];
        let b = stair[seg + 1];
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        if dx.abs() < f32::EPSILON && dy.abs() < f32::EPSILON {
            continue;
        }

        let (w, h, offset_x, offset_y) = if dx.abs() >= dy.abs() {
            let len = dx.signum() * dx.abs();
            (len, thickness, 0.0, 0.0)
        } else {
            let len = dy.signum() * dy.abs();
            (thickness, len, 0.0, 0.0)
        };

        let rect_origin = Vec2::new(
            quantize(a.x + offset_x),
            quantize(a.y + offset_y),
        ) - midpoint;
        quads.quad([
            (rect_origin + Vec2::new(0.0, -half)).extend(0.0).to_array(),
            (rect_origin + Vec2::new(w, -half)).extend(0.0).to_array(),
            (rect_origin + Vec2::new(w, h + half)).extend(0.0).to_array(),
            (rect_origin + Vec2::new(0.0, h + half)).extend(0.0).to_array(),
        ], [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
    }

    quads.write(mesh);
}

fn cleanup_pylons(
    mut commands: Commands,
    pylons: Query<(Entity, &QuarkPylon, &Children)>,
) {
    for (entity, pylon, children) in pylons.iter() {
        cleanup_pylon_if_finished(&mut commands, entity, pylon, children);
    }
}

fn cleanup_pylon_if_finished(
    commands: &mut Commands,
    entity: Entity,
    pylon: &QuarkPylon,
    children: &Children,
) {
    if !matches!(pylon.phase, PylonPhase::Outro(0)) {
        return;
    }

    for child in children.iter() {
        despawn_if_present(commands, child);
    }
    despawn_if_present(commands, entity);
}

fn despawn_if_present(commands: &mut Commands, entity: Entity) {
    commands.entity(entity).try_despawn();
}

fn ordered_pair(a: Entity, b: Entity) -> (Entity, Entity) {
    if a.index() <= b.index() {
        (a, b)
    } else {
        (b, a)
    }
}

fn quantize(v: f32) -> f32 {
    (v / PIXEL_STEP).round() * PIXEL_STEP
}
