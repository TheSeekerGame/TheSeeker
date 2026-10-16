
use theseeker_engine::physics::TileCollisionWorld;
use theseeker_engine::profile::TickProfiler;
use bevy::prelude::*;

use super::{
    AiPaused, ActorController, GroundSensor, HealthSensor, PerceivedGroundSensor,
    PerceivedRangeSensor, PerceivedTargetSensor, RangeSensor, SensorHistory,
    TargetSensor,
};

#[derive(Component)]
pub struct CachedArchetypeStats {
    pub vision_range: f32,
    pub melee_range: f32,
    pub walk_speed: f32,
    pub chase_speed: f32,
    pub needs_line_of_sight: bool,
    pub reaction_time: u8,
}

#[derive(Component)]
pub struct AiTarget;

#[derive(Component)]
pub struct AiTargetInvisible;

pub fn sensor_target(
    mut enemy_query: Query<
        (
            &mut TargetSensor,
            &Transform,
            &CachedArchetypeStats,
        ),
        (With<ActorController>, Without<AiPaused>),
    >,
    target_query: Query<
        (Entity, &Transform),
        (
            With<AiTarget>,
            Without<AiTargetInvisible>,
        ),
    >,
    collision_world: Option<Res<TileCollisionWorld>>,
    mut profiler: TickProfiler,
) {
    let _profile = profiler.scope("ai_sensor_target");
    let target_info = target_query.iter().next();

    for (mut target_sensor, enemy_tf, cached_stats) in enemy_query.iter_mut() {
        update_target_sensor(
            &mut target_sensor,
            enemy_tf,
            cached_stats,
            target_info,
            collision_world.as_deref(),
        );
    }
}

fn update_target_sensor(
    target_sensor: &mut TargetSensor,
    enemy_tf: &Transform,
    cached_stats: &CachedArchetypeStats,
    target_info: Option<(Entity, &Transform)>,
    collision_world: Option<&TileCollisionWorld>,
) {
    let Some((target_e, target_tf)) = target_info else {
        clear_target_sensor(target_sensor);
        return;
    };

    let delta = target_tf.translation - enemy_tf.translation;
    let dist2 = delta.x * delta.x + delta.y * delta.y;

    if target_los_blocked(
        enemy_tf,
        target_tf,
        cached_stats,
        dist2,
        collision_world,
    ) {
        clear_target_sensor(target_sensor);
        return;
    }

    target_sensor.entity = Some(target_e);
    target_sensor.dist2 = dist2;
}

fn clear_target_sensor(target_sensor: &mut TargetSensor) {
    target_sensor.entity = None;
    target_sensor.dist2 = f32::MAX;
}

fn target_los_blocked(
    enemy_tf: &Transform,
    target_tf: &Transform,
    cached_stats: &CachedArchetypeStats,
    dist2: f32,
    collision_world: Option<&TileCollisionWorld>,
) -> bool {
    if !cached_stats.needs_line_of_sight {
        return false;
    }

    let vision_range_sq = cached_stats.vision_range * cached_stats.vision_range;
    if dist2 > vision_range_sq {
        return false;
    }

    let origin = enemy_tf.translation.xy();
    let dest = target_tf.translation.xy();
    let dist = (dest - origin).length();
    if dist <= 0.1 {
        return false;
    }

    let Some(world) = collision_world.filter(|world| world.is_ready()) else {
        return false;
    };
    let Some(hit) = world.segment_cast_solid(origin, dest) else {
        return false;
    };

    hit.time_of_impact < dist
}

pub fn sensor_range(
    mut query: Query<
        (&TargetSensor, &mut RangeSensor, &CachedArchetypeStats),
        Without<AiPaused>,
    >,
    mut profiler: TickProfiler,
) {
    let _profile = profiler.scope("ai_sensor_range");
    for (target_sensor, mut range_sensor, cached_stats) in query.iter_mut() {
        let vision_range = cached_stats.vision_range;
        let melee_range = cached_stats.melee_range;

        if target_sensor.entity.is_some() {
            let vision_sq = vision_range * vision_range;
            let melee_sq = melee_range * melee_range;
            range_sensor.in_melee = target_sensor.dist2 <= melee_sq;
            range_sensor.in_aggro = target_sensor.dist2 <= vision_sq;
        } else {
            range_sensor.in_melee = false;
            range_sensor.in_aggro = false;
        }
    }
}

pub fn sensor_ground<T>(
    mut query: Query<
        (&mut GroundSensor, &T),
        (With<ActorController>, Without<AiPaused>),
    >,
    mut profiler: TickProfiler,
) where
    T: Component + GroundedCheck,
{
    let _profile = profiler.scope("ai_sensor_ground");
    for (mut ground_sensor, nav) in query.iter_mut() {
        ground_sensor.on = nav.is_grounded();
    }
}

pub trait GroundedCheck {
    fn is_grounded(&self) -> bool;
}

pub fn sensor_health<T>(
    mut query: Query<(&T, &mut HealthSensor), Without<AiPaused>>,
    mut profiler: TickProfiler,
) where
    T: Component + HealthCheck,
{
    let _profile = profiler.scope("ai_sensor_health");
    for (health, mut sensor) in query.iter_mut() {
        sensor.zero = health.is_zero();
    }
}

pub trait HealthCheck {
    fn is_zero(&self) -> bool;
}

pub fn update_sensor_history(
    mut query: Query<
        (&TargetSensor, &GroundSensor, &RangeSensor, &mut SensorHistory),
        Without<AiPaused>,
    >,
    mut profiler: TickProfiler,
) {
    let _profile = profiler.scope("ai_sensor_history");
    for (target, ground, range, mut history) in query.iter_mut() {
        let write_idx = history.write_index as usize;

        history.target_history[write_idx] = (target.entity, target.dist2);
        history.ground_history[write_idx] = ground.on;
        history.range_history[write_idx] = (range.in_melee, range.in_aggro);

        history.write_index = (history.write_index + 1) % 32;
    }
}

pub fn update_perceived_sensors(
    mut query: Query<
        (
            &SensorHistory,
            &mut PerceivedTargetSensor,
            &mut PerceivedGroundSensor,
            &mut PerceivedRangeSensor,
        ),
        Without<AiPaused>,
    >,
    mut profiler: TickProfiler,
) {
    let _profile = profiler.scope("ai_sensor_perceived");
    for (history, mut p_target, mut p_ground, mut p_range) in query.iter_mut() {
        let read_idx = history.get_read_index();

        let (target_entity, target_dist2) = history.target_history[read_idx];
        p_target.entity = target_entity;
        p_target.dist2 = target_dist2;

        p_ground.on = history.ground_history[read_idx];

        let (in_melee, in_aggro) = history.range_history[read_idx];
        p_range.in_melee = in_melee;
        p_range.in_aggro = in_aggro;
    }
}

pub fn copy_actual_to_perceived(
    mut query: Query<
        (
            &TargetSensor,
            &GroundSensor,
            &RangeSensor,
            &mut PerceivedTargetSensor,
            &mut PerceivedGroundSensor,
            &mut PerceivedRangeSensor,
        ),
        (Without<SensorHistory>, Without<AiPaused>),
    >,
    mut profiler: TickProfiler,
) {
    let _profile = profiler.scope("ai_sensor_copy");
    for (target, ground, range, mut p_target, mut p_ground, mut p_range) in
        query.iter_mut()
    {
        p_target.entity = target.entity;
        p_target.dist2 = target.dist2;
        p_ground.on = ground.on;
        p_range.in_melee = range.in_melee;
        p_range.in_aggro = range.in_aggro;
    }
}

pub fn initialize_sensor_history(
    mut commands: Commands,
    query: Query<
        (
            Entity,
            &super::ActorDefinition,
            &TargetSensor,
            &GroundSensor,
            &RangeSensor,
            Option<&CachedArchetypeStats>,
        ),
        Added<super::ActorDefinition>,
    >,
    mut profiler: TickProfiler,
) {
    let _profile = profiler.scope("ai_sensor_init_history");
    for (entity, actor, target, ground, range, cached_stats) in query.iter() {
        let reaction_time = cached_stats.map_or(actor.stats().reaction_time, |stats| stats.reaction_time);

        if reaction_time == 0 {
            continue;
        }

        let history = seeded_sensor_history(
            reaction_time,
            target,
            ground,
            range,
        );
        commands.entity(entity).insert(history);
    }
}

fn seeded_sensor_history(
    reaction_time: u8,
    target: &TargetSensor,
    ground: &GroundSensor,
    range: &RangeSensor,
) -> SensorHistory {
    let mut history = SensorHistory::new(reaction_time);

    for i in 0..32 {
        history.target_history[i] = (target.entity, target.dist2);
        history.ground_history[i] = ground.on;
        history.range_history[i] = (range.in_melee, range.in_aggro);
    }

    history
}
