use bevy::prelude::*;
use bevy::render::view::RenderLayers;

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
    pub vel: Vec2,
    pub initial_ticks_left: u32,
    pub outro_ticks_left: Option<u32>,
    pub last_slot: Option<&'static str>,
    pub last_gfx_frame: Option<u32>,
    pub vel_mult: f32,
    pub hover_phase: f32,
    pub hover_prev: f32,
}

/// Marker for spinner gfx sprite
#[derive(Component)]
pub struct SpinnerGfx;

// No persistent damage collider entity

const SPINNER_INTRO_TICKS: u32 = 8 * 3; // 3 frames @ 8 ticks (intro visuals), also initial outward duration
const SPINNER_OUTRO_TICKS: u32 = 8 * 5; // frames 16-20 @ 8 ticks
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
    let base_z = 17.0 * 0.000001;
    let vel_mult = rand::rng().random_range(0.85..=1.15);
    let spinner_entity = commands
        .spawn((
            Spinner {
                owner,
                // Randomize speed multiplier within ±15%
                vel_mult,
                vel: Vec2::new(
                    INITIAL_SPEED * facing_dir * vel_mult,
                    0.0,
                ),
                initial_ticks_left: SPINNER_INTRO_TICKS,
                outro_ticks_left: None,
                last_slot: None,
                last_gfx_frame: None,
                hover_phase: 0.0,
                hover_prev: 0.0,
            },
            Transform::from_translation(pos.extend(base_z)),
            GlobalTransform::default(),
            StateDespawnMarker,
            Name::new("Spinner"),
        ))
        .id();

    // Gfx entity with animation and sprite
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

    // Parent gfx to logic so they move together
    commands.entity(spinner_entity).add_child(gfx_entity);

    // Persistent damage entity that follows the spinner (damage applied on specified frames)
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
    commands.entity(spinner_entity).add_child(dmg_entity);
}

fn update_spinner_movement(
    mut q: Query<(&mut Spinner, &mut Transform)>,
    player_tf_q: Query<&GlobalTransform, With<Player>>,
    stealthed_q: Query<(), With<StealthEffect>>, // check owner stealth
) {
    for (mut sp, mut tf) in q.iter_mut() {
        // Outro: count down and freeze
        if let Some(ref mut ticks) = sp.outro_ticks_left {
            if *ticks > 0 {
                *ticks -= 1;
            }
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
            let omega = core::f32::consts::TAU / HOVER_PERIOD_TICKS;
            sp.hover_phase = (sp.hover_phase + omega) % core::f32::consts::TAU;
            let hover_now = HOVER_AMPLITUDE * sp.hover_phase.sin();
            let dy = hover_now - sp.hover_prev;
            sp.hover_prev = hover_now;
            tf.translation.y += dy;
            continue;
        }

        // Pull towards player at anisotropic constant speed (randomized per spinner)
        let pos = tf.translation.truncate();
        let to_owner = owner_pos - pos;
        let dist = to_owner.length();
        let is_stealthed = stealthed_q.get(sp.owner).is_ok();
        if !is_stealthed {
            if dist > f32::EPSILON {
                let dir = to_owner / dist;
                let vx = dir.x * PULL_SPEED * sp.vel_mult;
                let vy = dir.y * (PULL_SPEED * PULL_SPEED_V_MULT) * sp.vel_mult;
                sp.vel = Vec2::new(vx, vy);
            } else {
                sp.vel = Vec2::ZERO;
            }
        } else {
            sp.vel = Vec2::ZERO; // freeze pull, but still apply hover below
        }
        tf.translation.x += sp.vel.x;
        tf.translation.y += sp.vel.y;

        // Apply hover oscillation every tick
        let omega = core::f32::consts::TAU / HOVER_PERIOD_TICKS;
        sp.hover_phase = (sp.hover_phase + omega) % core::f32::consts::TAU;
        let hover_now = HOVER_AMPLITUDE * sp.hover_phase.sin();
        let dy = hover_now - sp.hover_prev;
        sp.hover_prev = hover_now;
        tf.translation.y += dy;
    }
}

fn update_spinner_animation_slots(
    mut sp_q: Query<(
        &mut Spinner,
        &GlobalTransform,
        &Children,
    )>,
    mut anim_q: Query<&mut ScriptPlayer<SpriteAnimation>, With<SpinnerGfx>>,
) {
    for (mut sp, _tf, children) in sp_q.iter_mut() {
        // Determine desired slot, but do not override intro frames
        let desired_slot: Option<&'static str> =
            if sp.outro_ticks_left.is_some() {
                Some("Outro")
            } else if sp.initial_ticks_left > 0 {
                None // stay on intro; no slot change
            } else {
                // Normal phase – choose based on horizontal motion or idle
                if sp.vel.x.abs() <= 0.01 {
                    Some("Idle")
                } else if sp.vel.x < 0.0 {
                    Some("Left")
                } else {
                    Some("Right")
                }
            };

        // Only toggle if changed
        if desired_slot != sp.last_slot {
            for child in children.iter() {
                if let Ok(mut player) = anim_q.get_mut(child) {
                    if let Some(prev) = sp.last_slot {
                        player.set_slot(prev, false);
                    }
                    if let Some(newslot) = desired_slot {
                        player.set_slot(newslot, true);
                    }
                }
            }
            sp.last_slot = desired_slot;
        }
    }
}

/// Clear damaged_set on each animation frame change so spinner can hit again
fn spawn_spinner_damage_on_frames(
    mut commands: Commands,
    mut sp_q: Query<(
        Entity,
        &mut Spinner,
        &GlobalTransform,
        &Children,
    )>,
    gfx_q: Query<&Sprite, With<SpinnerGfx>>,
    mut dmg_q: Query<&mut DamageSource>,
    player_stats_q: Query<
        (
            Option<&PlayerStatMod>,
            Has<StealthEffect>,
        ),
        With<Player>,
    >,
) {
    // 1-indexed frames where damage pulses occur
    const DAMAGE_FRAMES_1IDX: [u32; 6] = [4, 6, 8, 10, 12, 14];
    for (spinner_e, mut sp, _sp_tf, children) in sp_q.iter_mut() {
        // Find or attach a persistent damage child entity
        let mut found_dmg: Option<Entity> = None;
        for child in children.iter() {
            if dmg_q.get_mut(child).is_ok() {
                found_dmg = Some(child.clone());
                break;
            }
        }
        let dmg_entity = if let Some(e) = found_dmg {
            e
        } else {
            let e = commands
                .spawn((
                    DamageSource::new(u32::MAX, sp.owner, 12.0)
                        .with_max_targets(u32::MAX),
                    Collider::cuboid(30.0, 30.0),
                    groups::player_attack(),
                    Transform::from_translation(Vec3::ZERO),
                    GlobalTransform::default(),
                    StateDespawnMarker,
                    Name::new("SpinnerDamage"),
                ))
                .id();
            commands.entity(spinner_e).add_child(e);
            e
        };

        // Read current frame from gfx child (0-indexed → 1-indexed)
        let mut cf: Option<u32> = None;
        for child in children.iter() {
            if let Ok(sprite) = gfx_q.get(child) {
                if let Some(atlas) = &sprite.texture_atlas {
                    cf = Some(atlas.index as u32 + 1);
                }
                break;
            }
        }
        let Some(cf) = cf else {
            continue;
        };
        if sp.last_gfx_frame == Some(cf) {
            continue;
        }
        sp.last_gfx_frame = Some(cf);

        // On damage frames: refresh damage parameters and allow re-hits
        if DAMAGE_FRAMES_1IDX.contains(&cf) {
            // Update base damage from current player stat mod and stealth tag
            let (statmod, is_stealthed) =
                player_stats_q.get(sp.owner).ok().unwrap_or((None, false));
            let base_mult = statmod.map(|s| s.damage).unwrap_or(1.0);
            if let Ok(mut ds) = dmg_q.get_mut(dmg_entity) {
                ds.base_damage = 12.0 * base_mult;
                ds.damage = ds.base_damage; // reset immediately; will be re-modified by systems
                ds.damaged_set.clear(); // allow hitting again on this pulse
            }
            if is_stealthed {
                commands.entity(dmg_entity).insert(DmgStealthed);
            } else {
                commands.entity(dmg_entity).remove::<DmgStealthed>();
            }
        }
    }
}

fn pickup_and_outro_cut_cooldown(
    mut sp_q: Query<(
        Entity,
        &mut Spinner,
        &GlobalTransform,
        &Children,
    )>,
    player_tf_q: Query<&GlobalTransform, With<Player>>,
    mut anim_q: Query<&mut ScriptPlayer<SpriteAnimation>, With<SpinnerGfx>>,
    mut cooldowns: ResMut<crate::game::player::skills::cooldowns::Cooldowns>,
    stealthed_q: Query<(), With<StealthEffect>>,
) {
    for (_e, mut sp, tf, children) in sp_q.iter_mut() {
        if sp.outro_ticks_left.is_some() {
            continue;
        }
        if let Ok(p_tf) = player_tf_q.get(sp.owner) {
            // Do not allow pickup while the owner is stealthed
            if stealthed_q.get(sp.owner).is_ok() {
                continue;
            }
            let dist = p_tf
                .translation()
                .truncate()
                .distance(tf.translation().truncate());
            if dist <= PICKUP_RADIUS {
                // Trigger outro and halve remaining cooldown
                sp.outro_ticks_left = Some(SPINNER_OUTRO_TICKS);

                // Switch to Outro slot on anim now (only if changed)
                if sp.last_slot != Some("Outro") {
                    for child in children.iter() {
                        if let Ok(mut player) = anim_q.get_mut(child) {
                            if let Some(prev) = sp.last_slot {
                                player.set_slot(prev, false);
                            }
                            player.set_slot("Outro", true);
                        }
                    }
                    sp.last_slot = Some("Outro");
                }

                // Halve remaining cooldown for Spinner on owner
                if let Some(entry) = cooldowns.get(
                    sp.owner,
                    crate::game::player::skills::types::SkillId::Spinner,
                ) {
                    let delta = entry.remaining * 0.5;
                    cooldowns.reduce_specific(
                        sp.owner,
                        crate::game::player::skills::types::SkillId::Spinner,
                        delta,
                    );
                }
            }
        }
    }
}

fn cleanup_spinner(
    mut commands: Commands,
    q: Query<(Entity, &Spinner, &Children)>,
) {
    for (e, sp, children) in q.iter() {
        if let Some(t) = sp.outro_ticks_left {
            if t == 0 {
                // Despawn children first
                for c in children.iter() {
                    commands.entity(c).despawn();
                }
                commands.entity(e).despawn();
            }
        }
    }
}

// (no sync system needed; pulses snapshot statmod and stealth)
