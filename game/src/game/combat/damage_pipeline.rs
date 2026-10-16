use super::{DamageInfo, Health};
use crate::camera::CameraShake;
use crate::game::combat::damage_source::{self, DamageSource, Hit, Stealthed};
use crate::game::gentstate::Dead;
use crate::game::player::weapon::PlayerMeleeWeapon;
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
                kill_on_damage.after(
                    crate::game::player::passives::runtime::process_passive_events_system,
                ),
                on_hit_cam_shake,
                on_hit_lifesteal,
                damage_source::on_hit_self_pushback,
                on_enemy_killed_cam_shake,
                damage_source::damage_source_tick,
                damage_source::cleanup_owner_bound_damage_sources,
                damage_source::damage_source_cleanup,
            )
                .chain()
                .after(theseeker_engine::physics::PhysicsSet),
        );
    }
}

/// Suppresses default on-hit camera shake for a given damage source.
#[derive(Component)]
pub struct NoOnHitShake;

fn kill_on_damage(
    query: Query<(Entity, &Health), With<Gent>>,
    mut damage_events: MessageReader<DamageInfo>,
    mut commands: Commands,
    mut passive_events: MessageWriter<
        crate::game::player::passives::PassiveEvent,
    >,
    players: Query<(), With<Player>>,
    camera: Option<Res<CameraShake>>,
) {
    let mut camera_available = camera.is_none();

    for damage_info in damage_events.read() {
        handle_kill_on_damage_event(
            damage_info,
            &query,
            &mut commands,
            &mut passive_events,
            &players,
            &mut camera_available,
        );
    }
}

fn handle_kill_on_damage_event(
    damage_info: &DamageInfo,
    query: &Query<(Entity, &Health), With<Gent>>,
    commands: &mut Commands,
    passive_events: &mut MessageWriter<
        crate::game::player::passives::PassiveEvent,
    >,
    players: &Query<(), With<Player>>,
    camera_available: &mut bool,
) {
    let Ok((entity, health)) = query.get(damage_info.target) else {
        return;
    };
    if health.current != 0 {
        return;
    }

    passive_events.write(
        crate::game::player::passives::PassiveEvent::EnemyKilled {
            owner: damage_info.owner,
        },
    );
    commands.entity(entity).insert(Dead::default());
    if !*camera_available || players.get(damage_info.owner).is_err() {
        return;
    }

    commands.insert_resource(kill_camera_shake());
    *camera_available = false;
}

fn on_hit_cam_shake(
    query: Query<(&DamageSource, Option<&NoOnHitShake>), Added<Hit>>,
    players: Query<Entity, With<Player>>,
    weapon: Res<PlayerMeleeWeapon>,
    mut commands: Commands,
) {
    for (damage_source, no_shake) in query.iter() {
        on_hit_camera_shake_for_source(
            damage_source,
            no_shake,
            &players,
            *weapon,
            &mut commands,
        );
    }
}

fn on_hit_camera_shake_for_source(
    damage_source: &DamageSource,
    no_shake: Option<&NoOnHitShake>,
    players: &Query<Entity, With<Player>>,
    weapon: PlayerMeleeWeapon,
    commands: &mut Commands,
) {
    if no_shake.is_some() {
        return;
    }
    if players.get(damage_source.owner).is_err() {
        return;
    }

    commands.insert_resource(on_hit_camera_shake(weapon));
}

fn on_hit_camera_shake(weapon: PlayerMeleeWeapon) -> CameraShake {
    if weapon == PlayerMeleeWeapon::Hammer {
        return CameraShake::new(
            HAMMER_ON_HIT_SCREENSHAKE_STRENGTH,
            HAMMER_ON_HIT_SCREENSHAKE_DURATION_SECS,
            HAMMER_ON_HIT_SCREENSHAKE_FREQUENCY,
        );
    }

    CameraShake::new(
        DEFAULT_ON_HIT_SCREENSHAKE_STRENGTH,
        DEFAULT_ON_HIT_SCREENSHAKE_DURATION_SECS,
        DEFAULT_ON_HIT_SCREENSHAKE_FREQUENCY,
    )
}

fn on_hit_lifesteal(
    query: Query<&DamageSource, (Added<Hit>, With<Stealthed>)>,
    mut health_query: Query<&mut Health, Without<DamageSource>>,
    passives_q: Query<&Passives>,
    mut heal_events: MessageWriter<
        crate::game::player::passives::blood_shards::HealApplied,
    >,
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
            let heal_amount = (stealth_lifesteal * health.max as f32) as u32;
            // Always emit the event: Blood Shards trigger on overheal.
            heal_events.write(
                crate::game::player::passives::blood_shards::HealApplied::heal(
                    damage_source.owner,
                    &mut health,
                    heal_amount,
                ),
            );
        }
    }
}

fn on_enemy_killed_cam_shake(
    mut events: MessageReader<crate::game::player::passives::PassiveEvent>,
    players: Query<(), With<Player>>,
    camera: Option<Res<CameraShake>>,
    mut commands: Commands,
) {
    for evt in events.read() {
        on_enemy_killed_camera_shake_event(
            evt,
            &players,
            camera.as_deref(),
            &mut commands,
        );
    }
}

fn on_enemy_killed_camera_shake_event(
    evt: &crate::game::player::passives::PassiveEvent,
    players: &Query<(), With<Player>>,
    camera: Option<&CameraShake>,
    commands: &mut Commands,
) {
    let crate::game::player::passives::PassiveEvent::EnemyKilled { owner } = evt
    else {
        return;
    };
    if camera.is_some() || players.get(*owner).is_err() {
        return;
    }

    commands.insert_resource(kill_camera_shake());
}

fn kill_camera_shake() -> CameraShake {
    CameraShake::new(
        KILL_SCREENSHAKE_STRENGTH,
        KILL_SCREENSHAKE_DURATION_SECS,
        KILL_SCREENSHAKE_FREQUENCY,
    )
}
