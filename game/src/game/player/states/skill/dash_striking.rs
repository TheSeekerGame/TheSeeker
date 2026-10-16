use crate::game::player::foreground::{ActionMut, ActionTag};
use crate::game::actors::action::{ActiveAction, ActionRequest, ActionPresentation};
use crate::game::actors::catalog::ActorCatalog;
use crate::game::actors::lunge::{LungeRuntime, MotionContact, LungeHits, LungeHitRequest, emit_lunge_hits, swept_hit_shape};
use theseeker_engine::animation::{PlaybackState, PlaybackId};
use crate::camera::CameraShake;
use bevy::prelude::*;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::{
    CollisionGroups, LinearVelocity, PhysicsWorld, ENEMY,
    PLAYER_ATTACK,
};
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::GameTickUpdate;

use crate::game::gentstate::Facing;
use crate::game::player::skills::types::SkillId;
use crate::game::player::passives::pulse_echo::PulseEcho;
use crate::game::player::{
    states::DashStrike, Passives, Player, PlayerStatMod, PlayerStateSet,
};

// For sensors and enemy proximity/damage events
use crate::game::enemy::Enemy;
use crate::game::player::player_anim::set_direction_slots;
use crate::game::player::sensors::GroundSensor;

/// Player downward dash binding for the shared lunge action.
pub struct DashStrikingStatePlugin;

impl Plugin for DashStrikingStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                dash_strike_enter_system,
                dash_strike_update_system,
            )
                .chain()
                .in_set(PlayerStateSet::Behavior)
                .in_set(crate::game::player::PlayerBehaviorSet::Execution),
        );
        app.add_systems(
            GameTickUpdate,
            dash_strike_flight_damage_system
                .in_set(PlayerStateSet::Collisions)
                .after(crate::game::player::collision::player_collisions),
        );
    }
}

const STRIKE_IMPACT_SHAKE_STRENGTH: f32 = 5.0;
const STRIKE_IMPACT_SHAKE_DURATION_SECS: f32 = 0.30;
const STRIKE_IMPACT_SHAKE_FREQUENCY: f32 = 3.0;

type DashStrikeTargetFilter = (
    Or<(
        With<Enemy>,
        With<crate::game::player::spawns::amplified_bell::Bell>,
    )>,
    Without<crate::game::gentstate::Dead>,
);

struct DashStrikeImpactParams<'a> {
    entity: Entity,
    player_pos: Vec3,
    gent: &'a Gent,
    hit_ground: bool,
    impact_velocity: Vec2,
    is_stealthed: bool,
    passives: Option<&'a Passives>,
    stat_mod: Option<&'a PlayerStatMod>,
}

fn dash_strike_enter_system(
    mut q: Query<
        (
            Entity,
            ActionMut<DashStrike>,
            &Facing,
            &Gent,
            &Transform,
            &mut LinearVelocity,
        ),
        (With<Player>, Added<ActionTag<DashStrike>>, Without<crate::game::gentstate::Dead>),
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>>,
    catalog: Res<ActorCatalog>,
) {
    for (entity, mut ds, facing, gent, transform, mut vel) in q.iter_mut() {
        let definition = catalog.actions.get("player_dive").expect("validated player dive").clone();
        let mut action = ActionRequest { forward: facing.direction(), actor: entity, damage: definition.damage.expect("dive damage"), target: None, definition: definition.clone() }.activate(PlaybackId::default());
        action.id = ds.execution().id;
        let dir = dash_strike_direction(&ds, facing);
        action.lunge = Some(LungeRuntime::new(definition.motion.expect("dive motion"), Vec2::new(dir, 0.0), transform.translation.xy()));
        action.motion_intent(action.lunge.as_ref().expect("dive motion").displacement()).apply(action.id, &mut vel);
        play_dash_strike_start_animation(&mut gfx_query, gent, facing, &mut action);
        ds.action = Some(action);
    }
}

fn dash_strike_direction(ds: &DashStrike, facing: &Facing) -> f32 {
    if ds.dir.abs() > 0.0 {
        return ds.dir.signum();
    }

    facing.direction()
}

fn play_dash_strike_start_animation(
    gfx_query: &mut Query<&mut ScriptPlayer<SpriteAnimation>>,
    gent: &Gent,
    facing: &Facing,
    action: &mut ActiveAction,
) {
    let Ok(mut script) = gfx_query.get_mut(gent.e_gfx) else {
        return;
    };

    set_direction_slots(&mut script, facing);
    action.present(&mut script, ActionPresentation::Execute);
}

fn dash_strike_update_system(
    mut commands: Commands,
    mut q: Query<
        (
            Entity,
            ActionMut<DashStrike>,
            &Facing,
            &mut LinearVelocity,
            &Transform,
            &Gent,
            Option<&crate::game::effects::stealthed::StealthEffect>,
            Option<&Passives>,
            Option<&PlayerStatMod>,
            &GroundSensor,
        ),
        (With<Player>, Without<crate::game::gentstate::Dead>),
    >,
    targets: Query<(Entity, &Transform), DashStrikeTargetFilter>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>>,
    time: Res<theseeker_engine::time::GameTime>,
    signals: Query<&PlaybackState>,
) {
    for (
        entity,
        mut ds,
        _facing,
        mut vel,
        transform,
        gent,
        is_stealthed,
        passives,
        stat_mod,
        ground,
    ) in q.iter_mut()
    {
        let Some(action) = ds.action.as_mut() else { continue; };
        let Some(lunge) = action.lunge.as_ref() else { continue; };
        action.motion_intent(lunge.displacement()).apply(action.id, &mut vel);
        let impact_velocity = vel.0;
        try_start_dash_strike_impact(
            &mut commands,
            action,
            &mut vel,
            DashStrikeImpactParams {
                entity,
                player_pos: transform.translation,
                gent,
                hit_ground: ground.is_grounded,
                impact_velocity,
                is_stealthed: is_stealthed.is_some(),
                passives,
                stat_mod,
            },
            &targets,
            &mut gfx_query,
            &time,
        );

        let effects = action.advance(time.tick(), signals.get(gent.e_gfx).ok(), false);
        if effects.outcome.is_some() {
            crate::game::player::foreground::finish_action(&mut commands, entity, action.id);
        }
    }
}

fn dash_strike_flight_damage_system(
    mut commands: Commands,
    mut players: Query<
        (
            Entity,
            ActionMut<DashStrike>,
            &Transform,
            Has<crate::game::effects::stealthed::StealthEffect>,
        ),
        (With<Player>, Without<crate::game::gentstate::Dead>),
    >,
    targets: Query<(), DashStrikeTargetFilter>,
    spatial_query: PhysicsWorld,
) {
    for (player, mut strike, transform, is_stealthed) in players.iter_mut() {
        let Some(action) = strike.action.as_mut() else { continue; };
        let Some(lunge) = action.lunge.as_mut() else { continue; };
        let current_position = transform.translation.truncate();
        let (sweep_origin, travel) = lunge.sweep(current_position);

        if lunge.contact.is_some() {
            continue;
        }

        let Some(LungeHits::Sweep { radius, .. }) = action.definition.hits else { continue; };
        let sweep = swept_hit_shape(travel, radius);
        let hits = spatial_query.intersect(
            sweep_origin,
            &sweep,
            CollisionGroups::new(PLAYER_ATTACK, ENEMY),
            Some(player),
        );
        let new_targets = hits.into_iter().filter(|target| targets.contains(*target));
        emit_lunge_hits(&mut commands, action, LungeHitRequest {
            owner: player, position: transform.translation, groups: theseeker_engine::physics::groups::player_attack(),
            skill: Some(SkillId::DashStrike), stealthed: is_stealthed, max_targets: u32::MAX,
        }, new_targets);
    }
}

fn try_start_dash_strike_impact(
    commands: &mut Commands,
    action: &mut ActiveAction,
    vel: &mut LinearVelocity,
    impact: DashStrikeImpactParams<'_>,
    targets: &Query<(Entity, &Transform), DashStrikeTargetFilter>,
    gfx_query: &mut Query<&mut ScriptPlayer<SpriteAnimation>>,
    time: &theseeker_engine::time::GameTime,
) {
    if !impact.hit_ground || !action.lunge.as_mut().is_some_and(|lunge| lunge.impact(MotionContact::Ground)) {
        return;
    }

    vel.0 = Vec2::ZERO;

    spawn_dash_strike_impact_damage(commands, action, &impact, targets);
    spawn_dash_strike_pulse_echo(commands, &impact, time);
    insert_dash_strike_impact_shake(commands);
    play_dash_strike_impact(gfx_query, impact.gent, action);
}

fn spawn_dash_strike_impact_damage(commands: &mut Commands, action: &mut ActiveAction, impact: &DashStrikeImpactParams<'_>, targets: &Query<(Entity, &Transform), DashStrikeTargetFilter>) {
    let Some(LungeHits::Sweep { impact_radius, impact_max_targets, .. }) = action.definition.hits else { return; };
    let targets = targets.iter().filter(|(_, transform)| transform.translation.xy().distance(impact.player_pos.xy()) <= impact_radius)
        .map(|(entity, _)| entity);
    emit_lunge_hits(commands, action, LungeHitRequest {
        owner: impact.entity, position: impact.player_pos, groups: theseeker_engine::physics::groups::player_attack(),
        skill: Some(SkillId::DashStrike), stealthed: impact.is_stealthed, max_targets: impact_max_targets,
    }, targets);
}

/// The impact echoes in all four directions.
fn spawn_dash_strike_pulse_echo(
    commands: &mut Commands,
    impact: &DashStrikeImpactParams<'_>,
    time: &theseeker_engine::time::GameTime,
) {
    let damage_mult = impact.stat_mod.map(|stat| stat.damage).unwrap_or(1.0);
    let Some(echo) = impact
        .passives
        .and_then(|passives| PulseEcho::of(passives, damage_mult, impact.is_stealthed))
    else {
        return;
    };
    echo.release(
        commands,
        impact.entity,
        impact.player_pos,
        impact.impact_velocity,
        time.hz,
        &[Vec2::Y, Vec2::NEG_Y, Vec2::X, Vec2::NEG_X],
    );
}

fn insert_dash_strike_impact_shake(commands: &mut Commands) {
    commands.insert_resource(CameraShake::new(
        STRIKE_IMPACT_SHAKE_STRENGTH,
        STRIKE_IMPACT_SHAKE_DURATION_SECS,
        STRIKE_IMPACT_SHAKE_FREQUENCY,
    ));
}

fn play_dash_strike_impact(gfx_query: &mut Query<&mut ScriptPlayer<SpriteAnimation>>, gent: &Gent, action: &mut ActiveAction) {
    let Ok(mut script) = gfx_query.get_mut(gent.e_gfx) else { return; };
    action.present(&mut script, ActionPresentation::Impact);
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flight_sweep_covers_the_full_movement_segment() {
        let travel = Vec2::new(5.0, -5.0);
        let aabb = swept_hit_shape(travel, 8.0).local_aabb().unwrap_or_default();

        assert!(aabb.min.x <= -8.0);
        assert!(aabb.max.x >= travel.x + 8.0);
        assert!(aabb.min.y <= travel.y - 8.0);
        assert!(aabb.max.y >= 8.0);
    }

    #[test]
    fn collision_pass_ignores_a_pending_dash_action() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        world.init_resource::<theseeker_engine::physics::EntityCollisionWorld>();
        let entity = world.spawn((Player, Transform::default())).id();
        crate::game::player::foreground::start_action(&mut world, entity, DashStrike::new());
        world.run_system_once(dash_strike_flight_damage_system).unwrap();
        assert!(world.get::<crate::game::player::foreground::ForegroundAction>(entity).unwrap().state::<DashStrike>().unwrap().action.is_none());
    }

    #[test]
    fn flight_to_impact_updates_the_foreground_playback_lease() {
        use bevy::ecs::system::RunSystemOnce;
        use theseeker_engine::animation::{PlaybackEvent, PlaybackEventKind};
        let mut world = crate::game::actors::fixtures::fixture_world();
        let catalog = crate::game::actors::fixtures::fixture_catalog(&mut world).unwrap();
        world.insert_resource(catalog);
        world.init_resource::<theseeker_engine::time::GameTime>();
        let gfx = world.spawn(ScriptPlayer::<SpriteAnimation>::default()).id();
        let entity = world.spawn((Player, Facing::Right, Gent { e_gfx: gfx, e_effects_gfx: gfx },
            Transform::default(), LinearVelocity::default(), GroundSensor { is_grounded: true, distance: 0.0 })).id();
        crate::game::player::foreground::start_action(&mut world, entity, DashStrike::new());
        world.run_system_once(dash_strike_enter_system).unwrap();
        let flight = world.get::<crate::game::player::foreground::ForegroundAction>(entity).unwrap().playback_id();
        world.run_system_once(dash_strike_update_system).unwrap();
        let foreground = world.get::<crate::game::player::foreground::ForegroundAction>(entity).unwrap();
        let impact = foreground.state::<DashStrike>().unwrap().action.as_ref().unwrap().playback;
        assert_ne!(flight, impact);
        assert_eq!(foreground.playback_id(), impact);
        world.entity_mut(gfx).insert(PlaybackState { playback: impact, ordered: vec![PlaybackEvent {
            playback: flight, kind: PlaybackEventKind::Interrupted,
        }], ..Default::default() });
        world.run_system_once(crate::game::player::foreground::interrupt_stopped_playback).unwrap();
        assert!(world.get::<ActionTag<DashStrike>>(entity).is_some());
    }

    #[test]
    fn dash_activation_commits_motion_before_its_first_collision_pass() {
        let mut world = crate::game::actors::fixtures::fixture_world();
        let catalog = crate::game::actors::fixtures::fixture_catalog(&mut world).unwrap();
        world.insert_resource(catalog);
        world.init_resource::<theseeker_engine::physics::EntityCollisionWorld>();
        let gfx = world.spawn(ScriptPlayer::<SpriteAnimation>::default()).id();
        let entity = world.spawn((Player,
            Facing::Right, Gent { e_gfx: gfx, e_effects_gfx: gfx },
            Transform::default(), LinearVelocity::default())).id();
        crate::game::player::foreground::start_action(&mut world, entity, DashStrike { dir: -1.0, action: None });
        let mut schedule = Schedule::default();
        schedule.add_systems((dash_strike_enter_system, dash_strike_flight_damage_system).chain());
        schedule.run(&mut world);
        assert_eq!(world.get::<LinearVelocity>(entity).unwrap().0, Vec2::new(-4.167, -4.167));
        assert!(world.get::<crate::game::player::foreground::ForegroundAction>(entity).unwrap().state::<DashStrike>().unwrap().action.is_some());
    }
}
