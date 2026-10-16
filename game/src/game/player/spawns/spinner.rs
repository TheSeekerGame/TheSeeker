use bevy::camera::visibility::RenderLayers;

use theseeker_engine::animation::PlaybackState;
use theseeker_engine::animation::SpriteAnimationBundle;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::physics::{groups, Collider};
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::GameTickUpdate;

use crate::game::combat::{DamageSource, Stealthed as DmgStealthed};
use crate::game::effects::stealthed::StealthEffect;
use crate::game::player::{Player, PlayerStatMod, PlayerStateSet};
use crate::prelude::*;

/// Marker for spinner logic entity
#[derive(Component)]
pub struct Spinner {
    pub owner: Entity,
    pub gfx_entity: Entity,
    pub damage_entity: Entity,
    pub vel: Vec2,
    pub initial_ticks_left: u32,
    pub outro_ticks_left: Option<u32>,
    pub despawn_block_ticks: u32,
    pub last_slot: Option<&'static str>,
    pub vel_mult: f32,
    pub hover_phase: f32,
    pub hover_prev: f32,
}

/// Marker for spinner gfx sprite
#[derive(Component)]
pub struct SpinnerGfx;

const SPINNER_INTRO_TICKS: u32 = 8 * 3; // 3 frames @ 8 ticks (intro visuals), also initial outward duration
const SPINNER_OUTRO_TICKS: u32 = 8 * 5; // frames 16-20 @ 8 ticks
const SPINNER_MIN_LIFETIME_TICKS: u32 = 96;
const INITIAL_SPEED: f32 = 1.7; // px / tick away from player (20% faster)
const PULL_SPEED: f32 = 0.62; // px /
const PULL_SPEED_V_MULT: f32 = 1.6;
const INITIAL_DAMPING: f32 = 0.90; // decay while in intro phase
const PICKUP_RADIUS: f32 = 6.0; // px
const HOVER_AMPLITUDE: f32 = 8.0; // px
const HOVER_PERIOD_TICKS: f32 = 128.0; // ticks per full oscillation

pub struct SpinnerPlugin;

impl Plugin for SpinnerPlugin {
    fn build(&self, app: &mut App) {
        // Movement + animation slots – keep late in player animation stage
        app.add_systems(
            GameTickUpdate,
            (
                update_spinner_movement,
                update_spinner_animation_slots,
                spawn_spinner_damage_on_frames,
                pickup_and_outro_cut_cooldown,
                cleanup_spinner,
            )
                .chain()
                .in_set(PlayerStateSet::Animation),
        );
    }
}

/// Public spawn helper
pub fn spawn_spinner(
    commands: &mut Commands,
    owner: Entity,
    pos: Vec2,
    facing_dir: f32,
) {
    // Logic entity root (drives motion)
    // Render above player/enemies/kinetic orb. Player is ~15e-6; use higher.
    let base_z = 17.0 * 0.000_001;
    let vel_mult = rand::rng().random_range(0.85..=1.15);
    let gfx_entity = commands
        .spawn((
            SpinnerGfx,
            SpriteAnimationBundle::new_play_key("anim.player.Spinner"),
            Sprite {
                texture_atlas: Some(TextureAtlas::default()),
                ..Default::default()
            },
            Transform::from_translation(Vec3::ZERO),
            GlobalTransform::default(),
            Visibility::Visible,
            InheritedVisibility::default(),
            ViewVisibility::default(),
            // Render on the same layer as the player so Z ordering applies
            RenderLayers::layer(2),
            StateDespawnMarker,
            Name::new("SpinnerGfx"),
        ))
        .id();
    let dmg_entity = commands
        .spawn((
            DamageSource::new(u32::MAX, owner, 12.0).with_max_targets(u32::MAX),
            // Use a simple area collider around spinner; keeps behavior consistent with pulses
            Collider::cuboid(30.0, 30.0),
            groups::player_attack(),
            Transform::from_translation(Vec3::ZERO),
            GlobalTransform::default(),
            StateDespawnMarker,
            Name::new("SpinnerDamage"),
        ))
        .id();

    let spinner_entity = commands
        .spawn((
            Spinner {
                owner,
                gfx_entity,
                damage_entity: dmg_entity,
                vel_mult,
                vel: Vec2::new(
                    INITIAL_SPEED * facing_dir * vel_mult,
                    0.0,
                ),
                initial_ticks_left: SPINNER_INTRO_TICKS,
                outro_ticks_left: None,
                despawn_block_ticks: SPINNER_MIN_LIFETIME_TICKS,
                last_slot: None,
                hover_phase: 0.0,
                hover_prev: 0.0,
            },
            Transform::from_translation(pos.extend(base_z)),
            GlobalTransform::default(),
            StateDespawnMarker,
            Name::new("Spinner"),
        ))
        .id();

    // Parent gfx and damage to logic so they move together
    commands.entity(spinner_entity).add_child(gfx_entity);
    commands.entity(spinner_entity).add_child(dmg_entity);
}

fn update_spinner_movement(
    mut q: Query<(&mut Spinner, &mut Transform)>,
    player_tf_q: Query<&GlobalTransform, With<Player>>,
    stealthed_q: Query<(), With<StealthEffect>>, // check owner stealth
) {
    for (mut sp, mut tf) in q.iter_mut() {
        sp.despawn_block_ticks = sp.despawn_block_ticks.saturating_sub(1);
        let is_stealthed = stealthed_q.get(sp.owner).is_ok();
        let despawn_blocked = is_stealthed || sp.despawn_block_ticks > 0;

        // Outro: count down and freeze
        if let Some(ref mut ticks) = sp.outro_ticks_left {
            tick_spinner_outro(ticks, despawn_blocked);
            continue;
        }

        let owner_pos = if let Ok(tfp) = player_tf_q.get(sp.owner) {
            tfp.translation().truncate()
        } else {
            // Owner missing – mark outro to cleanup soon
            sp.outro_ticks_left = Some(1);
            continue;
        };

        // Initial phase: outward velocity decays (uses facing dir set at spawn time)
        if sp.initial_ticks_left > 0 {
            sp.initial_ticks_left = sp.initial_ticks_left.saturating_sub(1);
            sp.vel *= INITIAL_DAMPING;
            tf.translation.x += sp.vel.x;
            tf.translation.y += sp.vel.y;
            // Apply hover oscillation even during intro
            apply_spinner_hover(&mut sp, &mut tf);
            continue;
        }

        // Pull towards player at anisotropic constant speed (randomized per spinner)
        let pos = tf.translation.truncate();
        let to_owner = owner_pos - pos;
        sp.vel = spinner_pull_velocity(to_owner, sp.vel_mult, is_stealthed);
        tf.translation.x += sp.vel.x;
        tf.translation.y += sp.vel.y;

        // Apply hover oscillation every tick
        apply_spinner_hover(&mut sp, &mut tf);
    }
}

fn tick_spinner_outro(ticks: &mut u32, despawn_blocked: bool) {
    if despawn_blocked {
        return;
    }

    *ticks = ticks.saturating_sub(1);
}

fn spinner_pull_velocity(
    to_owner: Vec2,
    vel_mult: f32,
    is_stealthed: bool,
) -> Vec2 {
    if is_stealthed {
        return Vec2::ZERO;
    }

    let dist = to_owner.length();
    if dist <= f32::EPSILON {
        return Vec2::ZERO;
    }

    let dir = to_owner / dist;
    let vx = dir.x * PULL_SPEED * vel_mult;
    let vy = dir.y * (PULL_SPEED * PULL_SPEED_V_MULT) * vel_mult;
    Vec2::new(vx, vy)
}

fn apply_spinner_hover(sp: &mut Spinner, tf: &mut Transform) {
    let omega = core::f32::consts::TAU / HOVER_PERIOD_TICKS;
    sp.hover_phase = (sp.hover_phase + omega) % core::f32::consts::TAU;
    let hover_now = HOVER_AMPLITUDE * sp.hover_phase.sin();
    let dy = hover_now - sp.hover_prev;
    sp.hover_prev = hover_now;
    tf.translation.y += dy;
}

fn update_spinner_animation_slots(
    mut sp_q: Query<&mut Spinner>,
    mut anim_q: Query<&mut ScriptPlayer<SpriteAnimation>, With<SpinnerGfx>>,
) {
    for mut sp in sp_q.iter_mut() {
        // Determine desired slot, but do not override intro frames
        let desired_slot = desired_spinner_slot(&sp);

        // Only toggle if changed
        if desired_slot != sp.last_slot {
            set_spinner_animation_slot(
                sp.gfx_entity,
                sp.last_slot,
                desired_slot,
                &mut anim_q,
            );
            sp.last_slot = desired_slot;
        }
    }
}

fn desired_spinner_slot(sp: &Spinner) -> Option<&'static str> {
    if sp.outro_ticks_left.is_some() {
        return Some("Outro");
    }
    if sp.initial_ticks_left > 0 {
        return None;
    }

    spinner_motion_slot(sp.vel.x)
}

fn spinner_motion_slot(velocity_x: f32) -> Option<&'static str> {
    if velocity_x.abs() <= 0.01 {
        return Some("Idle");
    }
    if velocity_x < 0.0 {
        return Some("Left");
    }

    Some("Right")
}

fn set_spinner_animation_slot(
    gfx_entity: Entity,
    previous_slot: Option<&'static str>,
    desired_slot: Option<&'static str>,
    anim_q: &mut Query<&mut ScriptPlayer<SpriteAnimation>, With<SpinnerGfx>>,
) {
    let Ok(mut player) = anim_q.get_mut(gfx_entity) else {
        return;
    };

    if let Some(prev) = previous_slot {
        player.set_slot(prev, false);
    }
    if let Some(newslot) = desired_slot {
        player.set_slot(newslot, true);
    }
}

/// Clear damaged_set on each animation damage pulse so spinner can hit again
fn spawn_spinner_damage_on_frames(
    mut commands: Commands,
    sp_q: Query<&Spinner>,
    signal_q: Query<&PlaybackState, With<SpinnerGfx>>,
    mut dmg_q: Query<&mut DamageSource>,
    player_stats_q: Query<
        (
            Option<&PlayerStatMod>,
            Has<StealthEffect>,
        ),
        With<Player>,
    >,
) {
    for sp in sp_q.iter() {
        let Ok(signals) = signal_q.get(sp.gfx_entity) else {
            continue;
        };
        if !signals.has_event("damage_pulse") {
            continue;
        }

        let (statmod, is_stealthed) =
            player_stats_q.get(sp.owner).ok().unwrap_or((None, false));
        let base_mult = statmod.map(|s| s.damage).unwrap_or(1.0);
        if let Ok(mut ds) = dmg_q.get_mut(sp.damage_entity) {
            ds.base_damage = 12.0 * base_mult;
            ds.damage = ds.base_damage;
            ds.damaged_set.clear();
        }
        if is_stealthed {
            commands.entity(sp.damage_entity).insert(DmgStealthed);
        } else {
            commands.entity(sp.damage_entity).remove::<DmgStealthed>();
        }
    }
}

fn pickup_and_outro_cut_cooldown(
    mut sp_q: Query<(&mut Spinner, &GlobalTransform)>,
    player_tf_q: Query<&GlobalTransform, With<Player>>,
    mut anim_q: Query<&mut ScriptPlayer<SpriteAnimation>, With<SpinnerGfx>>,
    mut cooldowns: ResMut<crate::game::player::skills::cooldowns::Cooldowns>,
    stealthed_q: Query<(), With<StealthEffect>>,
) {
    for (mut sp, tf) in sp_q.iter_mut() {
        if sp.outro_ticks_left.is_some() {
            continue;
        }
        let is_stealthed = stealthed_q.get(sp.owner).is_ok();
        if is_stealthed || sp.despawn_block_ticks > 0 {
            continue;
        }
        if spinner_is_in_pickup_range(&sp, tf, &player_tf_q) {
            trigger_spinner_pickup_outro(&mut sp, &mut anim_q, &mut cooldowns);
        }
    }
}

fn spinner_is_in_pickup_range(
    sp: &Spinner,
    tf: &GlobalTransform,
    player_tf_q: &Query<&GlobalTransform, With<Player>>,
) -> bool {
    let Ok(p_tf) = player_tf_q.get(sp.owner) else {
        return false;
    };

    let dist = p_tf
        .translation()
        .truncate()
        .distance(tf.translation().truncate());
    dist <= PICKUP_RADIUS
}

fn trigger_spinner_pickup_outro(
    sp: &mut Spinner,
    anim_q: &mut Query<&mut ScriptPlayer<SpriteAnimation>, With<SpinnerGfx>>,
    cooldowns: &mut ResMut<crate::game::player::skills::cooldowns::Cooldowns>,
) {
    sp.outro_ticks_left = Some(SPINNER_OUTRO_TICKS);

    if sp.last_slot != Some("Outro") {
        set_spinner_animation_slot(
            sp.gfx_entity,
            sp.last_slot,
            Some("Outro"),
            anim_q,
        );
        sp.last_slot = Some("Outro");
    }

    reduce_spinner_cooldown_by_half(sp.owner, cooldowns);
}

fn reduce_spinner_cooldown_by_half(
    owner: Entity,
    cooldowns: &mut ResMut<crate::game::player::skills::cooldowns::Cooldowns>,
) {
    let Some(entry) = cooldowns.get(
        owner,
        crate::game::player::skills::types::SkillId::Spinner,
    ) else {
        return;
    };

    let delta = entry.remaining * 0.5;
    cooldowns.reduce_specific(
        owner,
        crate::game::player::skills::types::SkillId::Spinner,
        delta,
    );
}

fn cleanup_spinner(
    mut commands: Commands,
    q: Query<(Entity, &Spinner)>,
    stealthed_q: Query<(), With<StealthEffect>>,
) {
    for (entity, spinner) in q.iter() {
        if spinner_is_ready_for_cleanup(spinner, &stealthed_q) {
            commands.entity(spinner.gfx_entity).despawn();
            commands.entity(spinner.damage_entity).despawn();
            commands.entity(entity).despawn();
        }
    }
}

fn spinner_is_ready_for_cleanup(
    spinner: &Spinner,
    stealthed_q: &Query<(), With<StealthEffect>>,
) -> bool {
    spinner.outro_ticks_left == Some(0)
        && spinner.despawn_block_ticks == 0
        && stealthed_q.get(spinner.owner).is_err()
}

// (no sync system needed; pulses snapshot statmod and stealth)
