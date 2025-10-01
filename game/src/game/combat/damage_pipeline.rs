use super::{DamageInfo, Health};
use crate::camera::CameraShake;
use crate::game::combat::damage_source::{self, DamageSource, Hit, Stealthed};
use crate::game::gentstate::Dead;
use crate::game::player::weapon::CurrentWeapon;
use crate::game::player::{Passive, Passives, Player};
use crate::prelude::*;
use theseeker_engine::gent::Gent;
use theseeker_engine::time::GameTickUpdate;

// Camera shake tuning
const DEFAULT_ON_HIT_SCREENSHAKE_STRENGTH: f32 = 0.9;
const DEFAULT_ON_HIT_SCREENSHAKE_DURATION_SECS: f32 = 0.1;
const DEFAULT_ON_HIT_SCREENSHAKE_FREQUENCY: f32 = 2.0;

const HAMMER_ON_HIT_SCREENSHAKE_STRENGTH: f32 = 1.35;
const HAMMER_ON_HIT_SCREENSHAKE_DURATION_SECS: f32 = 0.2;
const HAMMER_ON_HIT_SCREENSHAKE_FREQUENCY: f32 = 2.5;

const KILL_SCREENSHAKE_STRENGTH: f32 = 2.1;
const KILL_SCREENSHAKE_DURATION_SECS: f32 = 0.24;
const KILL_SCREENSHAKE_FREQUENCY: f32 = 3.0;

pub struct DamagePipelinePlugin;

impl Plugin for DamagePipelinePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                damage_source::tag_damage_source_sensors,
                damage_source::determine_damage_targets,
                damage_source::apply_damage_modifications,
                damage_source::emit_passive_events_from_damage_hit,
                damage_source::apply_damage,
                damage_source::emit_passive_events_from_damage,
                kill_on_damage,
                on_hit_cam_shake,
                on_hit_lifesteal,
                damage_source::on_hit_self_pushback,
                on_enemy_killed_cam_shake,
                damage_source::damage_source_tick,
                damage_source::damage_source_cleanup,
            )
                .chain()
                .after(theseeker_engine::physics::update_sprite_colliders)
                .after(bevy::transform::TransformSystem::TransformPropagate),
        );
    }
}

/// Suppresses default on-hit camera shake for a given damage source.
#[derive(Component)]
pub struct NoOnHitShake;

fn kill_on_damage(
    query: Query<(Entity, &Health), With<Gent>>,
    mut damage_events: EventReader<DamageInfo>,
    mut commands: Commands,
    mut passive_events: EventWriter<crate::game::player::passives::PassiveEvent>,
    players: Query<(), With<Player>>,
    camera: Option<Res<CameraShake>>,
) {
    let mut camera_available = camera.is_none();

    for damage_info in damage_events.read() {
        if let Ok((entity, health)) = query.get(damage_info.target) {
            if health.current == 0 {
                passive_events.write(
                    crate::game::player::passives::PassiveEvent::EnemyKilled {
                        owner: damage_info.owner,
                    },
                );
                commands.entity(entity).insert(Dead::default());

                if camera_available && players.get(damage_info.owner).is_ok() {
                    let camera_shake = CameraShake::new(
                        KILL_SCREENSHAKE_STRENGTH,
                        KILL_SCREENSHAKE_DURATION_SECS,
                        KILL_SCREENSHAKE_FREQUENCY,
                    );
                    commands.insert_resource(camera_shake);
                    camera_available = false;
                }
            }
        }
    }
}

fn on_hit_cam_shake(
    query: Query<(&DamageSource, Option<&NoOnHitShake>), Added<Hit>>,
    players: Query<Entity, With<Player>>,
    weapon: CurrentWeapon,
    mut commands: Commands,
) {
    for (damage_source, no_shake) in query.iter() {
        if no_shake.is_some() {
            continue;
        }
        if players.get(damage_source.owner).is_ok() {
            let camera_shake = if weapon.is_wielding_hammer() {
                CameraShake::new(
                    HAMMER_ON_HIT_SCREENSHAKE_STRENGTH,
                    HAMMER_ON_HIT_SCREENSHAKE_DURATION_SECS,
                    HAMMER_ON_HIT_SCREENSHAKE_FREQUENCY,
                )
            } else {
                CameraShake::new(
                    DEFAULT_ON_HIT_SCREENSHAKE_STRENGTH,
                    DEFAULT_ON_HIT_SCREENSHAKE_DURATION_SECS,
                    DEFAULT_ON_HIT_SCREENSHAKE_FREQUENCY,
                )
            };
            commands.insert_resource(camera_shake);
        }
    }
}

fn on_hit_lifesteal(
    query: Query<&DamageSource, (Added<Hit>, With<Stealthed>)>,
    mut health_query: Query<&mut Health, Without<DamageSource>>,
    passives_q: Query<&Passives>,
) {
    for damage_source in query.iter() {
        let has_shadow_cloak = passives_q
            .get(damage_source.owner)
            .ok()
            .map(|p| p.contains(&Passive::ShadowCloak))
            .unwrap_or(false);
        if has_shadow_cloak {
            continue;
        }

        if let Ok(mut health) = health_query.get_mut(damage_source.owner) {
            let stealth_lifesteal = 0.2;
            health.current = u32::min(
                health.current.saturating_add(
                    (stealth_lifesteal * health.max as f32) as u32,
                ),
                health.max,
            );
        }
    }
}

fn on_enemy_killed_cam_shake(
    mut events: EventReader<crate::game::player::passives::PassiveEvent>,
    players: Query<(), With<Player>>,
    camera: Option<Res<CameraShake>>,
    mut commands: Commands,
) {
    for evt in events.read() {
        if let crate::game::player::passives::PassiveEvent::EnemyKilled { owner } = evt {
            if players.get(*owner).is_ok() {
                if camera.is_some() {
                    continue;
                }
                let camera_shake = CameraShake::new(
                    KILL_SCREENSHAKE_STRENGTH,
                    KILL_SCREENSHAKE_DURATION_SECS,
                    KILL_SCREENSHAKE_FREQUENCY,
                );
                commands.insert_resource(camera_shake);
            }
        }
    }
}
