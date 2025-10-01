use bevy::prelude::*;
use rand::Rng;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::effects::GhostingSource;
// Pickup uses pure distance checks (no physics queries)
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::{GameTickUpdate, GameTimeAppExt};

use crate::game::enemy::Enemy;
use crate::game::gentstate::Dead;
use crate::game::player::{Passive, Passives, Player};
use crate::prelude::StateDespawnMarker;

pub struct XpOrbsPlugin;

impl Plugin for XpOrbsPlugin {
    fn build(&self, app: &mut App) {
        app.add_gametick_event::<XpOrbPickup>();
        app.add_systems(
            GameTickUpdate,
            (
                spawn_orbs_on_death,
                update_orbs_pos,
                update_orbs_vel_and_pickup,
            )
                .chain(),
        );
    }
}

#[derive(Component)]
pub struct XpOrb {
    explosion_ticks_left: u32,
    pickup_active: bool,
}

#[derive(Event)]
pub struct XpOrbPickup;

fn spawn_orbs_on_death(
    enemy_q: Query<&GlobalTransform, (With<Enemy>, Added<Dead>)>,
    player_q: Query<&Passives, With<Player>>,
    mut commands: Commands,
) {
    for tr in enemy_q.iter() {
        let enemy_pos = tr.translation().truncate();

        let mut rng = rand::rng();

        // Initial scatter radius and tick-based explosion speed (units per tick)
        const POS_RADIUS: f32 = 3.0;
        // Previously 120..220 units/sec at 96 Hz -> ≈1.25..2.30 units/tick
        const EXPLOSION_SPEED_PER_TICK_MIN: f32 = 1.25;
        const EXPLOSION_SPEED_PER_TICK_MAX: f32 = 2.30;

        let use_red = player_q
            .single()
            .map(|passives| passives.contains(&Passive::Bloodstone))
            .unwrap_or(false);

        // Spawn a fixed number of orbs per enemy
        for _ in 0..7 {
            let pos = Vec2::new(
                rng.random_range(-POS_RADIUS..POS_RADIUS),
                rng.random_range(-POS_RADIUS..POS_RADIUS),
            )
            .clamp_length_max(POS_RADIUS);
            let orb_position = enemy_pos + pos;

            let dir = if pos.length_squared() > 1e-3 {
                pos.normalize()
            } else {
                let a = rng.random_range(0.0..std::f32::consts::TAU);
                Vec2::new(a.cos(), a.sin())
            };
            let speed = rng.random_range(
                EXPLOSION_SPEED_PER_TICK_MIN..EXPLOSION_SPEED_PER_TICK_MAX,
            );
            let vel = dir * speed;

            // Prepare animation; set Bloodstone slot when passive is equipped
            let mut player = ScriptPlayer::<SpriteAnimation>::default();
            player.play_key("anim.particles.Soul");
            if use_red {
                player.set_slot("Bloodstone", true);
            }

            commands.spawn((
                // Animation + sprite
                player,
                Sprite {
                    texture_atlas: Some(TextureAtlas::default()),
                    ..Default::default()
                },
                // Kinematics
                theseeker_engine::physics::LinearVelocity(vel),
                XpOrb {
                    explosion_ticks_left: EXPLOSION_TICKS,
                    pickup_active: false,
                },
                // Visuals & transforms
                Transform::from_translation(
                    orb_position.extend((15.0 * 0.000001) - 0.0000001),
                ),
                GlobalTransform::default(),
                Visibility::Visible,
                InheritedVisibility::VISIBLE,
                ViewVisibility::default(),
                // Simple ghosting trail
                GhostingSource {
                    spawn_interval_ticks: 2,
                    ghost_lifetime_ticks: 40,
                    initial_alpha: 0.5,
                    ..Default::default()
                },
                StateDespawnMarker,
                Name::new("xp_orb"),
            ));
        }
    }
}

// Tick-based tuning
const EXPLOSION_TICKS: u32 = 22; // ~0.23s at 96 Hz
const EXPLOSION_STRENGTH_START: f32 = 0.98; // per-tick multiplier at start of explosion
const EXPLOSION_STRENGTH_END: f32 = 0.92; // per-tick multiplier at end of explosion
const DIST_THRESHOLD: f32 = 2.0;

fn update_orbs_pos(
    mut query: Query<(
        &mut Transform,
        &mut theseeker_engine::physics::LinearVelocity,
        &XpOrb,
    )>,
) {
    for (mut tr, vel, _orb) in query.iter_mut() {
        tr.translation += vel.0.extend(0.0);
    }
}

fn update_orbs_vel_and_pickup(
    mut commands: Commands,
    mut query: Query<(
        Entity,
        &GlobalTransform,
        &mut theseeker_engine::physics::LinearVelocity,
        &mut XpOrb,
    )>,
    p_query: Query<&GlobalTransform, With<Player>>,
    mut xp_event: EventWriter<XpOrbPickup>,
) {
    let Ok(p) = p_query.single() else {
        return;
    };
    let p_pos = p.translation().truncate();

    for (entity, tr, mut vel, mut xp_orb) in query.iter_mut() {
        let pos = tr.translation().truncate();
        let dist = p_pos.distance(pos);
        let dir = (p_pos - pos).normalize_or_zero();

        // Explosion phase: gradual slowdown each tick between start and end strengths
        if xp_orb.explosion_ticks_left > 0 {
            let t = 1.0
                - (xp_orb.explosion_ticks_left as f32 / EXPLOSION_TICKS as f32);
            let strength = EXPLOSION_STRENGTH_START
                + (EXPLOSION_STRENGTH_END - EXPLOSION_STRENGTH_START) * t;
            vel.0 *= strength;
            xp_orb.explosion_ticks_left -= 1;
            if xp_orb.explosion_ticks_left == 0 {
                xp_orb.pickup_active = true;
            }
        }

        // Pickup detection during chasing phase via transform distance only
        if xp_orb.pickup_active && dist < DIST_THRESHOLD {
            commands.entity(entity).despawn();
            xp_event.write(XpOrbPickup);
            continue;
        }

        // Homing velocity control (tick-based, constant-speed target)
        if dist >= DIST_THRESHOLD {
            const HOMING_MAX_SPEED_PER_TICK: f32 = 2.0;
            let mut target_vel = dir * HOMING_MAX_SPEED_PER_TICK;

            // Prevent overshoot
            let max_speed = dist * 0.9;
            let current_speed = target_vel.length();
            if current_speed > max_speed {
                target_vel = target_vel.normalize_or_zero() * max_speed;
            }

            // Smooth steering scaled by how far through homing we are
            let t = 1.0
                - (xp_orb.explosion_ticks_left as f32 / EXPLOSION_TICKS as f32);
            let homing_scale = t * t * (3.0 - 2.0 * t); // smoothstep
            const HOMING_ALPHA_PER_TICK: f32 = 0.10;
            let alpha = HOMING_ALPHA_PER_TICK * homing_scale;
            vel.0 = vel.0.lerp(target_vel, alpha);
        }
    }
}
