use theseeker_engine::physics::LinearVelocity;
use crate::game::player::scatter_homing::{ScatterHoming, ScatterHomingProfile};
use bevy::prelude::*;
use bevy::camera::visibility::RenderLayers;
use rand::Rng;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::effects::{FadeCurve, GhostingSource, ScaleCurve};
// Pickup uses pure distance checks (no physics queries)
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::{GameTickUpdate, GameTimeAppExt};

use crate::game::enemy::Enemy;
use crate::game::gentstate::Dead;
use crate::game::player::{
    progression::AdaptationProgress, Passive, Passives, Player,
    PlayerStateSet,
};
use crate::graphics::pixel_perfect::pixel_perfect_layers;
use crate::prelude::StateDespawnMarker;

pub struct XpOrbsPlugin;

#[derive(SystemSet, Clone, PartialEq, Eq, Debug, Hash)]
pub struct XpOrbSet;

impl Plugin for XpOrbsPlugin {
    fn build(&self, app: &mut App) {
        app.add_gametick_event::<XpOrbPickup>();
        app.configure_sets(
            GameTickUpdate,
            XpOrbSet
                .after(PlayerStateSet::Collisions)
                .before(PlayerStateSet::Animation),
        );
        app.add_systems(
            GameTickUpdate,
            (
                spawn_orbs_on_death,
                update_orbs_pos,
                update_orbs_vel_and_pickup,
            )
                .chain()
                .in_set(XpOrbSet),
        );
    }
}

#[derive(Component)]
pub struct XpOrb {
    motion: ScatterHoming,
    pickup_active: bool,
}

#[derive(Message)]
pub struct XpOrbPickup {
    pub amount: u32,
}

fn spawn_orbs_on_death(
    enemy_q: Query<
        (
            &GlobalTransform,
            Option<&crate::game::enemy::NoXpDrops>,
        ),
        (With<Enemy>, Added<Dead>),
    >,
    player_q: Query<&Passives, With<Player>>,
    mut commands: Commands,
) {
    for (tr, skip_xp) in enemy_q.iter() {
        if skip_xp.is_some() {
            continue;
        }
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

            let dir = scatter_direction(pos, &mut rng);
            let speed = rng.random_range(
                EXPLOSION_SPEED_PER_TICK_MIN..EXPLOSION_SPEED_PER_TICK_MAX,
            );
            let vel = dir * speed;

            let player = xp_orb_animation_player(use_red);

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
                    motion: ScatterHoming::new(MOTION),
                    pickup_active: false,

                },
                // Visuals & transforms
                Transform::from_translation(
                    orb_position.extend(
                        15.0 * 0.000_001 - 0.000_000_1,
                    ),
                ),
                GlobalTransform::default(),
                Visibility::Visible,
                InheritedVisibility::VISIBLE,
                ViewVisibility::default(),
                // Render orb on pixel-perfect layer
                pixel_perfect_layers(),
                // Simple ghosting trail
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
                Name::new("xp_orb"),
            ));
        }
    }
}

fn scatter_direction<R: Rng + ?Sized>(pos: Vec2, rng: &mut R) -> Vec2 {
    if pos.length_squared() > 1e-3 {
        return pos.normalize();
    }

    let angle = rng.random_range(0.0..std::f32::consts::TAU);
    Vec2::new(angle.cos(), angle.sin())
}

fn xp_orb_animation_player(
    use_bloodstone: bool,
) -> ScriptPlayer<SpriteAnimation> {
    let mut player = ScriptPlayer::<SpriteAnimation>::default();
    player.play_key("anim.particles.Soul");
    if use_bloodstone {
        player.set_slot("Bloodstone", true);
    }
    player
}

// Tick-based tuning

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
    mut xp_event: MessageWriter<XpOrbPickup>,
) {
    let Ok(p) = p_query.single() else {
        return;
    };
    let p_pos = p.translation().truncate();

    for (entity, tr, mut vel, mut xp_orb) in query.iter_mut() {
        let pos = tr.translation().truncate();
        let dist = p_pos.distance(pos);
        let dir = (p_pos - pos).normalize_or_zero();

        tick_explosion_phase(&mut xp_orb, &mut vel);

        // Pickup detection during chasing phase via transform distance only
        if xp_orb.pickup_active && dist < DIST_THRESHOLD {
            commands.entity(entity).despawn();
            xp_event.write(XpOrbPickup {
                amount: AdaptationProgress::XP_PER_ORB,
            });
            continue;
        }

        if dist >= DIST_THRESHOLD {
            xp_orb.motion.steer(&mut vel.0, dir, dist, MOTION);
        }
    }
}

fn tick_explosion_phase(xp_orb: &mut XpOrb, vel: &mut LinearVelocity) {
    if let Some(finished) = xp_orb.motion.scatter(&mut vel.0, MOTION) {
        xp_orb.pickup_active = finished;
    }
}

const MOTION: ScatterHomingProfile = ScatterHomingProfile {
    ticks: 22, damping_start: 0.98, damping_end: 0.92, max_speed: 2.0, steering: 0.10,
};
