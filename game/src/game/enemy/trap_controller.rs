use super::*;
use crate::game::actors::definition::EnemyTrapSetup;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StationaryTrapMode {
    Idle,
    Prepare,
    AttackLeft,
    AttackRight,
    EatLeft,
    EatRight,
}

#[derive(Clone, Copy, Debug)]
struct StationaryTrapConfig {
    body_half_extents: Vec2,
    body_offset_y: f32,
    detection_half_extents: Vec2,
    attack_extra_x: f32,
    attack_height: f32,
    kill_event: &'static str,
}

impl StationaryTrapConfig {
    fn from_setup(body_half_extents: Vec2, setup: Option<EnemyTrapSetup>) -> Self {
        let body_offset_y = setup
            .and_then(|setup| setup.body_offset_y)
            .unwrap_or(body_half_extents.y);
        Self {
            body_half_extents,
            body_offset_y,
            detection_half_extents: setup
                .and_then(|setup| setup.detection_half_extents)
                .map(Vec2::from_array)
                .unwrap_or(body_half_extents),
            attack_extra_x: setup
                .and_then(|setup| setup.attack_extra_x)
                .unwrap_or(0.0),
            attack_height: setup
                .and_then(|setup| setup.attack_height)
                .unwrap_or(body_half_extents.y * 2.0),
            kill_event: "kill",
        }
    }
}

#[derive(Component)]
pub(super) struct StationaryTrapController {
    config: StationaryTrapConfig,
    killed: bool,
    attack_direction: Option<TrapDirection>,
}

impl StationaryTrapController {
    pub(super) fn new(
        body_half_extents: Vec2,
        setup: Option<EnemyTrapSetup>,
    ) -> Self {
        Self {
            config: StationaryTrapConfig::from_setup(body_half_extents, setup),
            killed: false,
            attack_direction: None,
        }
    }


}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrapDirection {
    Left,
    Right,
}

#[derive(Component)]
pub(super) struct TrapPrey;

type TrapPreyQueryFilter = (With<TrapPrey>, Without<StationaryTrapController>, Without<Dead>);
type TrapPlayerQueryFilter = (With<Player>, Without<Dead>);
type TrapEnemyQueryFilter = (With<Enemy>, Without<StationaryTrapController>, Without<Dead>);

#[derive(Clone, Copy)]
struct TrapBounds {
    min: Vec2,
    max: Vec2,
}

impl TrapBounds {
    fn contains(self, point: Vec2) -> bool {
        in_box(point, self.min, self.max)
    }
}

#[derive(Clone, Copy)]
struct TrapDetectionBoxes {
    left: TrapBounds,
    right: TrapBounds,
}

#[derive(Clone, Copy, Default)]
struct TrapDetection {
    left: bool,
    right: bool,
}

impl TrapDetection {
    fn record_position(&mut self, position: Vec2, boxes: TrapDetectionBoxes) {
        if boxes.left.contains(position) {
            self.left = true;
        }
        if boxes.right.contains(position) {
            self.right = true;
        }
    }

}

#[derive(Clone, Copy)]
struct TrapKillAttempt {
    owner: Entity,
    bounds: TrapBounds,
}

fn trap_box(
    transform: &Transform,
    config: StationaryTrapConfig,
    direction: TrapDirection,
) -> (Vec2, Vec2) {
    let trap_pos =
        transform.translation.xy() + Vec2::new(0.0, -config.body_offset_y);
    let h = config.detection_half_extents;
    match direction {
        TrapDirection::Left => (
            trap_pos + Vec2::new(-h.x, -h.y),
            trap_pos + Vec2::new(0.0, h.y),
        ),
        TrapDirection::Right => (
            trap_pos + Vec2::new(0.0, -h.y),
            trap_pos + Vec2::new(h.x, h.y),
        ),
    }
}

fn trap_bounds(
    transform: &Transform,
    config: StationaryTrapConfig,
    direction: TrapDirection,
) -> TrapBounds {
    let (min, max) = trap_box(transform, config, direction);
    TrapBounds { min, max }
}

fn trap_detection_boxes(
    transform: &Transform,
    config: StationaryTrapConfig,
) -> TrapDetectionBoxes {
    TrapDetectionBoxes {
        left: trap_bounds(transform, config, TrapDirection::Left),
        right: trap_bounds(transform, config, TrapDirection::Right),
    }
}

fn stationary_trap_attack_collider(
    config: StationaryTrapConfig,
    direction: TrapDirection,
) -> Collider {
    let min_y = -config.body_offset_y;
    let max_y = min_y + config.attack_height;
    let base_min_x = -config.body_half_extents.x;
    let base_max_x = config.body_half_extents.x;
    let (min_x, max_x) = match direction {
        TrapDirection::Left => (
            base_min_x - config.attack_extra_x,
            base_max_x,
        ),
        TrapDirection::Right => (
            base_min_x,
            base_max_x + config.attack_extra_x,
        ),
    };

    Collider::multi_rect(vec![Rect::from_corners(
        Vec2::new(min_x, min_y),
        Vec2::new(max_x, max_y),
    )])
}

pub(super) fn stationary_trap_detection_sensor(
    mut trap_query: Query<(
        &Transform,
        &StationaryTrapController,
        &mut ActorController,
    ), Without<AiPaused>>,
    player_query: Query<
        (&Transform, &Health),
        (
            With<Player>,
            Without<Dead>,
            Without<AiTargetInvisible>,
        ),
    >,
    bat_query: Query<
        (&Transform, &Health),
        TrapPreyQueryFilter,
    >,
) {
    for (trap_tf, trap, mut controller) in trap_query.iter_mut() {
        let config = trap.config;
        let boxes = trap_detection_boxes(trap_tf, config);
        let mut detection = TrapDetection::default();
        record_player_trap_detection(
            &mut detection,
            player_query.iter().next(),
            boxes,
        );
        record_enemy_trap_detections(&mut detection, &bat_query, boxes);
        resolve_trap_detection_tie(&mut detection, &mut controller.rng_state);

        controller.observations.detected_left = detection.left;
        controller.observations.detected_right = detection.right;
    }
}

fn record_player_trap_detection(
    detection: &mut TrapDetection,
    player: Option<(&Transform, &Health)>,
    boxes: TrapDetectionBoxes,
) {
    let Some((player_tf, player_health)) = player else {
        return;
    };
    if player_health.current == 0 {
        return;
    }

    detection.record_position(player_tf.translation.xy(), boxes);
}

fn record_enemy_trap_detections(
    detection: &mut TrapDetection,
    enemy_query: &Query<(&Transform, &Health), TrapPreyQueryFilter>,
    boxes: TrapDetectionBoxes,
) {
    for (enemy_tf, enemy_health) in enemy_query.iter() {
        if enemy_health.current == 0 {
            continue;
        }

        detection.record_position(enemy_tf.translation.xy(), boxes);
    }
}

fn resolve_trap_detection_tie(
    detection: &mut TrapDetection,
    rng_state: &mut u32,
) {
    if !detection.left || !detection.right {
        return;
    }

    *rng_state =
        rng_state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    let choose_left = (*rng_state & 1) == 0;
    if choose_left {
        detection.right = false;
    } else {
        detection.left = false;
    }
}

fn in_box(point: Vec2, min: Vec2, max: Vec2) -> bool {
    point.x >= min.x && point.x <= max.x && point.y >= min.y && point.y <= max.y
}


#[expect(clippy::too_many_arguments, reason = "Bevy system dependencies are expressed as explicit system parameters.")]
pub(super) fn stationary_trap_controller(
    mut opi_query: Query<
        (
            Entity,
            &mut StationaryTrapController,
            &mut DamageWindow,
            &mut theseeker_engine::physics::CollisionGroups,
            &mut Collider,
            &Transform,
            &mut Facing,
            &mut ActorController,
            Option<&theseeker_engine::animation::PlaybackState>,
        ),
        (
            With<StationaryTrapController>,
            Without<Dead>,
            Without<crate::game::effects::frozen::Frozen>,
        ),
    >,
    player_query: Query<Entity, TrapPlayerQueryFilter>,
    enemy_query: Query<Entity, TrapEnemyQueryFilter>,
    transform_query: Query<&Transform>,
    mut health_query: Query<(&mut Health, Has<Invulnerable>)>,
    mut damage_events: MessageWriter<DamageInfo>,
    mut commands: Commands,
    mut profiler: TickProfiler,
) {
    let _profile = profiler.scope("stationary_trap_controller");
    for (
        opi_entity,
        mut kill_state,
        mut damage_window,
        mut collision_groups,
        mut collider,
        opi_tf,
        mut facing,
        mut controller,
        animation_signals,
    ) in opi_query.iter_mut()
    {
        apply_stationary_trap_state(
            &mut kill_state,
            &mut damage_window,
            &mut collision_groups,
            &mut collider,
            &mut facing,
            actuation::trap_intent(controller.activity),
        );

        if let Some(attempt) =
            trap_kill_attempt(opi_entity, &kill_state, opi_tf, animation_signals.filter(|signals| signals.playback == controller.playback))
        {
            kill_state.killed = resolve_trap_kill_targets(
                attempt,
                &player_query,
                &enemy_query,
                &transform_query,
                &mut health_query,
                &mut damage_events,
                &mut commands,
            );
        }

        controller.observations.killed_target = kill_state.killed;
    }
}

fn apply_stationary_trap_state(
    kill_state: &mut StationaryTrapController,
    damage_window: &mut DamageWindow,
    collision_groups: &mut theseeker_engine::physics::CollisionGroups,
    collider: &mut Collider,
    facing: &mut Facing,
    mode: StationaryTrapMode,
) {
    apply_stationary_trap_facing(mode, facing);

    let attack_direction = trap_attack_direction(mode);
    damage_window.open = attack_direction.is_some();
    set_stationary_trap_collision_groups(collision_groups, attack_direction);
    sync_stationary_trap_collider(kill_state, collider, attack_direction);
}

fn apply_stationary_trap_facing(
    mode: StationaryTrapMode,
    facing: &mut Facing,
) {
    match mode {
        StationaryTrapMode::AttackLeft | StationaryTrapMode::EatLeft => {
            *facing = Facing::Left;
        },
        StationaryTrapMode::AttackRight | StationaryTrapMode::EatRight => {
            *facing = Facing::Right;
        },
        StationaryTrapMode::Idle | StationaryTrapMode::Prepare => {},
    }
}

fn trap_attack_direction(mode: StationaryTrapMode) -> Option<TrapDirection> {
    match mode {
        StationaryTrapMode::AttackLeft => Some(TrapDirection::Left),
        StationaryTrapMode::AttackRight => Some(TrapDirection::Right),
        StationaryTrapMode::Idle
        | StationaryTrapMode::Prepare
        | StationaryTrapMode::EatLeft
        | StationaryTrapMode::EatRight => None,
    }
}

fn set_stationary_trap_collision_groups(
    collision_groups: &mut theseeker_engine::physics::CollisionGroups,
    attack_direction: Option<TrapDirection>,
) {
    *collision_groups = if attack_direction.is_some() {
        theseeker_engine::physics::groups::enemy_body()
    } else {
        theseeker_engine::physics::CollisionGroups::new(
            theseeker_engine::physics::GROUND,
            theseeker_engine::physics::GROUND,
        )
    };
}

fn sync_stationary_trap_collider(
    kill_state: &mut StationaryTrapController,
    collider: &mut Collider,
    attack_direction: Option<TrapDirection>,
) {
    match (attack_direction, kill_state.attack_direction) {
        (Some(desired), Some(current)) if current == desired => {},
        (Some(desired), _) => {
            kill_state.attack_direction = Some(desired);
            kill_state.killed = false;
            *collider =
                stationary_trap_attack_collider(kill_state.config, desired);
        },
        (None, Some(_)) => {
            kill_state.attack_direction = None;
            kill_state.killed = false;
            *collider = Collider::cuboid(
                kill_state.config.body_half_extents.x,
                kill_state.config.body_half_extents.y,
            );
        },
        (None, None) => {},
    }
}

fn trap_kill_attempt(
    owner: Entity,
    kill_state: &StationaryTrapController,
    transform: &Transform,
    animation_signals: Option<&theseeker_engine::animation::PlaybackState>,
) -> Option<TrapKillAttempt> {
    let direction = kill_state.attack_direction?;
    if kill_state.killed {
        return None;
    }
    if !trap_animation_has_kill_event(animation_signals, kill_state.config) {
        return None;
    }

    Some(TrapKillAttempt {
        owner,
        bounds: trap_bounds(transform, kill_state.config, direction),
    })
}

fn trap_animation_has_kill_event(
    animation_signals: Option<&theseeker_engine::animation::PlaybackState>,
    config: StationaryTrapConfig,
) -> bool {
    animation_signals
        .map(|signals| signals.has_event(config.kill_event))
        .unwrap_or(false)
}

fn resolve_trap_kill_targets(
    attempt: TrapKillAttempt,
    player_query: &Query<Entity, TrapPlayerQueryFilter>,
    enemy_query: &Query<Entity, TrapEnemyQueryFilter>,
    transform_query: &Query<&Transform>,
    health_query: &mut Query<(&mut Health, Has<Invulnerable>)>,
    damage_events: &mut MessageWriter<DamageInfo>,
    commands: &mut Commands,
) -> bool {
    let mut killed_any = false;
    for target in player_query.iter().chain(enemy_query.iter()) {
        killed_any |= try_kill_trap_target(
            target,
            attempt,
            transform_query,
            health_query,
            damage_events,
            commands,
        );
    }
    killed_any
}

fn try_kill_trap_target(
    target: Entity,
    attempt: TrapKillAttempt,
    transform_query: &Query<&Transform>,
    health_query: &mut Query<(&mut Health, Has<Invulnerable>)>,
    damage_events: &mut MessageWriter<DamageInfo>,
    commands: &mut Commands,
) -> bool {
    let Ok(target_tf) = transform_query.get(target) else {
        return false;
    };
    if !attempt.bounds.contains(target_tf.translation.xy()) {
        return false;
    }

    let Ok((mut health, is_invulnerable)) = health_query.get_mut(target) else {
        return false;
    };
    if health.current == 0 {
        return false;
    }

    let dealt = if is_invulnerable {
        0.0
    } else {
        let dealt = health.current as f32;
        health.current = 0;
        commands.entity(target).insert(NoXpDrops);
        dealt
    };
    damage_events.write(DamageInfo {
        owner: attempt.owner,
        source: attempt.owner,
        target,
        amount: dealt,
        crit: false,
        stealthed: false,
        backstab: false,
    });

    !is_invulnerable
}
