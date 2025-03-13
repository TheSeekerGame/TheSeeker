use bevy::prelude::*;
use rand::Rng;
use theseeker_engine::{
    physics::LinearVelocity,
    time::{GameTickUpdate, GameTime, GameTimeAppExt},
};

use crate::{
    game::player::{Passive, Passives},
    prelude::StateDespawnMarker,
};

use super::{enemy::Enemy, gentstate::Dead, player::Player};

pub struct XpPlugin;
impl Plugin for XpPlugin {
    fn build(&self, app: &mut App) {
        app.add_gametick_event::<XpOrbPickup>();
        app.add_systems(
            GameTickUpdate,
            (
                spawn_orbs_on_death,
                update_orbs_pos,
                update_orbs_vel,
            ),
        );
    }
}

#[derive(Component)]
pub struct XpOrb {
    init_timer: f32,
}

#[derive(Event)]
pub struct XpOrbPickup;

fn spawn_orbs_on_death(
    enemy_q: Query<&GlobalTransform, (With<Enemy>, Added<Dead>)>,
    player_q: Query<&Passives, With<Player>>,
    mut commands: Commands,
    asset_server: Res<AssetServer>
) {
    let size = Vec2::splat(2.0);

    for tr in enemy_q.iter() {
        let enemy_pos = tr.translation().truncate();

        let mut rng = rand::thread_rng();

        let init_vel = Vec2::new(0.0, 2.0);
        const POS_RADIUS: f32 = 3.0;
        for _ in 0..12 {
            let pos = Vec2::new(
                rng.gen_range(-POS_RADIUS..POS_RADIUS),
                rng.gen_range(-POS_RADIUS..POS_RADIUS),
            )
            .clamp_length_max(POS_RADIUS);
            let orb_position = enemy_pos + pos;
            let vel = pos * 0.25;
            let color: Color = if let Ok(passives) = player_q.get_single() {
                if passives.contains(&Passive::Bloodstone) {
                    Color::RED
                } else {
                    Color::WHITE
                }
            } else {
                Color::WHITE
            };

            commands.spawn((
                LinearVelocity(vel + init_vel),
                XpOrb { init_timer: 1.0 },
                SpriteBundle {
                    texture: asset_server.load("fx/xporb.png"),
                    sprite: Sprite {
                        color,
                        custom_size: Some(size),
                        ..default()
                    },
                    transform: Transform::from_translation(
                        orb_position.extend((15.0 * 0.000001) - 0.0000001),
                    ),
                    ..default()
                },
                StateDespawnMarker,
            ));
        }
    }
}

const DIST_THRESHOLD: f32 = 0.75;

fn update_orbs_vel(
    mut commands: Commands,
    mut query: Query<(
        Entity,
        &GlobalTransform,
        &mut LinearVelocity,
        &XpOrb,
        &mut Sprite,
    )>,
    mut p_query: Query<(&GlobalTransform, &Passives), With<Player>>,
    mut xp_event: EventWriter<XpOrbPickup>,
) {
    let Ok((p, passives)) = p_query.get_single() else {
        return;
    };

    let p_pos = p.translation().truncate();

    for (entity, mut tr, mut vel, xp_orb, mut sprite) in query.iter_mut() {
        if passives.contains(&Passive::Bloodstone) {
            sprite.color = Color::RED;
        }

        if xp_orb.init_timer > 0.0 {
            continue;
        }

        let pos = tr.translation().truncate();
        let dist = p_pos.distance(pos);

        let dir = (p_pos - pos).normalize();

        if dist < DIST_THRESHOLD {
            commands.entity(entity).despawn();
            xp_event.send(XpOrbPickup);
        } else {
            const SPEEDUP_DIST: f32 = 150.0;
            //let scaled_dist = ((100.0 - dist).powi(2) / 100.).clamp(0.0, 2.);
            let scaled_dist = (2. * (SPEEDUP_DIST - dist.min(SPEEDUP_DIST))
                / SPEEDUP_DIST)
                .powi(2);
            vel.0 = dir * (1.0 + scaled_dist * 2.0) * 25.0;
        }
    }
}

fn update_orbs_pos(
    mut query: Query<(
        &mut Transform,
        &LinearVelocity,
        &mut XpOrb,
    )>,
    time: Res<GameTime>,
) {
    let delta = 1.0 / time.hz as f32;

    for (mut tr, vel, mut xp_orb) in query.iter_mut() {
        tr.translation += vel.0.extend(0.) * delta;

        if xp_orb.init_timer > 0.0 {
            xp_orb.init_timer -= delta;
        }
    }
}
