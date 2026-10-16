use super::*;

const GROUND_ROLL_PAST_TARGET_SPEED: f32 = 0.84;

#[derive(Component, Default)]
pub(super) struct GroundMotor;

#[derive(Component, Default)]
pub(super) struct MovementState {
    movement_type: MovementType,
    blocked_behavior: MotionBlockedBehavior,
    roll: RollPastTargetState,
    pub(super) frame_steps: Vec<crate::game::actors::action::TickDisplacement>,
    pub(super) walking_playback: theseeker_engine::animation::PlaybackId,
}

impl MovementState {
    pub(super) fn set_blocked_behavior(
        &mut self,
        blocked_behavior: MotionBlockedBehavior,
    ) {
        self.blocked_behavior = blocked_behavior;
    }

    pub(super) fn set_motion_type(&mut self, movement_type: MovementType) {
        if self.movement_type == movement_type {
            return;
        }
        self.movement_type = movement_type;
        if movement_type != MovementType::Rolling {
            self.roll.reset();
        }
    }

    pub(super) fn set_idle(&mut self) {
        self.set_motion_type(MovementType::Idle);
    }

    pub(super) fn stop(&mut self, velocity: &mut LinearVelocity) {
        velocity.0 = Vec2::ZERO;
        self.set_idle();
        self.frame_steps.clear();
    }

    fn stop_roll(&mut self, velocity: &mut LinearVelocity) {
        velocity.0.x = 0.0;
        self.roll.reset();
    }

    fn roll_attack_ready(&self) -> bool {
        self.movement_type == MovementType::Rolling && self.roll.finished
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub(super) enum MovementType {
    #[default]
    Idle,
    Walking,
    Chasing,
    Rolling,
}

#[derive(Default, Debug, Clone, Copy)]
struct RollPastTargetState {
    active: bool,
    direction: f32,
    finished: bool,
}

impl RollPastTargetState {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn begin(&mut self, direction: f32) {
        self.active = true;
        self.direction = if direction == 0.0 { 1.0 } else { direction };
        self.finished = false;
    }

    fn finish(&mut self) {
        self.active = false;
        self.finished = true;
    }
}

fn facing_from_world_dir(dir: f32) -> Facing {
    if dir >= 0.0 {
        Facing::Right
    } else {
        Facing::Left
    }
}

pub(super) fn update_ground_roll_observations(
    mut query: Query<
        (&MovementState, &mut ActorController),
        (With<Enemy>, With<GroundMotor>, Without<AiPaused>),
    >,
) {
    for (movement_state, mut controller) in query.iter_mut() {
        controller.observations.roll_ready = movement_state.roll_attack_ready();
    }
}

pub(super) fn sync_defense_state(
    mut query: Query<
        (
            Entity,
            &ActorController,
            Has<Defense>,
            Option<&DefenseInvulnerabilityTiming>,
            Option<&theseeker_engine::animation::PlaybackState>,
            Option<&mut Invulnerable>,
        ),
        (
            With<Enemy>,
            Without<crate::game::effects::frozen::Frozen>,
        ),
    >,
    mut commands: Commands,
) {
    for (
        entity,
        controller,
        has_defense,
        invulnerability_timing,
        animation_signals,
        maybe_invulnerable,
    ) in query.iter_mut()
    {
        let is_defending = controller.activity == crate::game::actors::controller::Activity::Defense;
        if is_defending && !has_defense {
            commands.entity(entity).insert(Defense);
        } else if !is_defending && has_defense {
            commands.entity(entity).remove::<Defense>();
        }

        sync_defense_invulnerability(
            entity,
            is_defending,
            invulnerability_timing,
            animation_signals,
            maybe_invulnerable,
            &mut commands,
        );
    }
}

fn sync_defense_invulnerability(
    entity: Entity,
    is_defending: bool,
    timing: Option<&DefenseInvulnerabilityTiming>,
    animation_signals: Option<&theseeker_engine::animation::PlaybackState>,
    maybe_invulnerable: Option<Mut<'_, Invulnerable>>,
    commands: &mut Commands,
) {
    let should_be_invulnerable =
        is_defending && defense_invulnerability_ready(timing, animation_signals);

    if should_be_invulnerable {
        if let Some(mut invulnerable) = maybe_invulnerable {
            invulnerable.add_source(InvulnerabilitySource::Defense);
        } else {
            commands.entity(entity).insert(Invulnerable::from_source(
                InvulnerabilitySource::Defense,
            ));
        }
        return;
    }

    let Some(mut invulnerable) = maybe_invulnerable else {
        return;
    };
    invulnerable.remove_source(InvulnerabilitySource::Defense);
    if invulnerable.is_empty() {
        commands.entity(entity).remove::<Invulnerable>();
    }
}

fn defense_invulnerability_ready(
    timing: Option<&DefenseInvulnerabilityTiming>,
    animation_signals: Option<&theseeker_engine::animation::PlaybackState>,
) -> bool {
    let Some(timing) = timing else {
        return true;
    };

    let Some(signals) = animation_signals else {
        return false;
    };

    if let Some(anim_key) = timing.anim_key.as_deref() {
        if signals.current_key.as_deref() != Some(anim_key) {
            return false;
        }
    }

    signals
        .current_frame
        .is_some_and(|frame| frame.0 >= timing.from_frame)
}

fn roll_target_dx(
    target_sensor: &TargetSensor,
    player_query: &Query<&Transform, With<Player>>,
    transform: &Transform,
) -> Option<f32> {
    let target_entity = target_sensor.entity?;
    let target_transform = player_query.get(target_entity).ok()?;
    Some(target_transform.translation.x - transform.translation.x)
}

pub(super) fn ground_motion_motor(
    mut query: Query<
        (
            &mut LinearVelocity,
            &mut MovementState,
            &ActorController,
            &mut Facing,
            &Gent,
            &mut Navigation,
            &mut TurnCooldown,
            &crate::game::actors::sensors::CachedArchetypeStats,
            &TargetSensor,
            &Transform,
            Has<Knockback>,
        ),
        (
            With<Enemy>,
            With<GroundMotor>,
            Without<crate::game::effects::frozen::Frozen>,
        ),
    >,
    signal_query: Query<&theseeker_engine::animation::PlaybackState>,
    player_query: Query<&Transform, With<Player>>,
    mut profiler: TickProfiler,
) {
    let _profile = profiler.scope("ground_motion_motor");
    for (
        mut velocity,
        mut movement_state,
        actor,
        mut facing,
        gent,
        mut navigation,
        mut turn_cooldown,
        cached_stats,
        target_sensor,
        transform,
        is_knocked,
    ) in query.iter_mut()
    {
        if is_knocked {
            movement_state.stop(&mut velocity);
            continue;
        }

        match movement_state.movement_type {
            MovementType::Walking => {
                tick_ground_walking(
                    &mut movement_state,
                    &mut velocity,
                    &mut facing,
                    &mut navigation,
                    &mut turn_cooldown,
                    GroundWalkTick {
                        signals: signal_query.get(gent.e_gfx).ok(),
                        speed: cached_stats.walk_speed,
                        motion: &actor.definition.walking,
                    },
                );
            },
            MovementType::Chasing => {
                tick_ground_chasing(
                    &mut movement_state,
                    &mut velocity,
                    &mut navigation,
                    cached_stats.chase_speed,
                );
            },
            MovementType::Rolling => {
                tick_ground_rolling(
                    &mut movement_state,
                    &mut velocity,
                    &mut facing,
                    &mut navigation,
                    cached_stats.melee_range,
                    roll_target_dx(target_sensor, &player_query, transform),
                );
            },
            MovementType::Idle => {},
        }
    }
}

struct GroundWalkTick<'a> {
    signals: Option<&'a theseeker_engine::animation::PlaybackState>,
    speed: f32,
    motion: &'a crate::game::actors::motion::ResolvedWalkMotion,
}

fn tick_ground_walking(
    movement_state: &mut MovementState,
    velocity: &mut LinearVelocity,
    facing: &mut Facing,
    navigation: &mut Navigation,
    turn_cooldown: &mut TurnCooldown,
    tick: GroundWalkTick,
) {
    handle_walking_blocked_turnaround(
        movement_state,
        facing,
        navigation,
        turn_cooldown,
    );
    use crate::game::actors::motion::ResolvedWalkMotion;
    tick.motion.collect_steps(tick.signals, movement_state.walking_playback, facing.direction(), &mut movement_state.frame_steps);
    velocity.0.x = match tick.motion {
        ResolvedWalkMotion::Continuous => facing.direction() * tick.speed,
        ResolvedWalkMotion::FrameSteps(_) => 0.0,
    };
}

fn handle_walking_blocked_turnaround(
    movement_state: &mut MovementState,
    facing: &mut Facing,
    navigation: &mut Navigation,
    turn_cooldown: &mut TurnCooldown,
) {
    if movement_state.blocked_behavior != MotionBlockedBehavior::TurnAround
        || !matches!(*navigation, Navigation::Blocked)
    {
        return;
    }

    if turn_cooldown.timer == 0 {
        *facing = opposite_facing(*facing);
        turn_cooldown.timer = 48;
        movement_state.roll.reset();
    }
    *navigation = Navigation::Grounded;
}

fn opposite_facing(facing: Facing) -> Facing {
    match facing {
        Facing::Right => Facing::Left,
        Facing::Left => Facing::Right,
    }
}

fn tick_ground_chasing(
    movement_state: &mut MovementState,
    velocity: &mut LinearVelocity,
    navigation: &mut Navigation,
    chase_speed: f32,
) {
    if movement_state.blocked_behavior == MotionBlockedBehavior::Stop
        && matches!(*navigation, Navigation::Blocked)
    {
        movement_state.stop(velocity);
        *navigation = Navigation::Grounded;
        return;
    }

    if velocity.0.x != 0.0 {
        velocity.0.x = velocity.0.x.signum() * chase_speed;
    }
}

fn tick_ground_rolling(
    movement_state: &mut MovementState,
    velocity: &mut LinearVelocity,
    facing: &mut Facing,
    navigation: &mut Navigation,
    melee_range: f32,
    target_dx: Option<f32>,
) {
    let Some(dx) = target_dx else {
        movement_state.stop_roll(velocity);
        return;
    };

    if finish_roll_near_target(movement_state, velocity, facing, dx, melee_range) {
        return;
    }
    begin_roll_if_needed(movement_state, *facing, dx);
    *facing = facing_from_world_dir(movement_state.roll.direction);
    if finish_blocked_or_passed_roll(movement_state, velocity, facing, navigation, dx) {
        return;
    }

    velocity.0.x = movement_state.roll.direction * GROUND_ROLL_PAST_TARGET_SPEED;
}

fn finish_roll_near_target(
    movement_state: &mut MovementState,
    velocity: &mut LinearVelocity,
    facing: &mut Facing,
    dx: f32,
    melee_range: f32,
) -> bool {
    if !movement_state.roll.finished {
        return false;
    }
    if dx.abs() <= melee_range {
        velocity.0.x = 0.0;
        *facing = facing_from_world_dir(dx);
        return true;
    }

    movement_state.roll.finished = false;
    false
}

fn begin_roll_if_needed(
    movement_state: &mut MovementState,
    facing: Facing,
    dx: f32,
) {
    if movement_state.roll.active {
        return;
    }

    let direction = if dx.abs() > 0.01 {
        dx.signum()
    } else {
        facing.direction()
    };
    movement_state.roll.begin(direction);
}

fn finish_blocked_or_passed_roll(
    movement_state: &mut MovementState,
    velocity: &mut LinearVelocity,
    facing: &mut Facing,
    navigation: &mut Navigation,
    dx: f32,
) -> bool {
    let blocked = movement_state.blocked_behavior == MotionBlockedBehavior::Stop
        && matches!(*navigation, Navigation::Blocked);
    let passed = dx * movement_state.roll.direction < -12.0;
    if !(blocked || passed) {
        return false;
    }

    movement_state.roll.finish();
    movement_state.stop_roll(velocity);
    if blocked {
        *navigation = Navigation::Grounded;
    }
    *facing = facing_from_world_dir(dx);
    true
}
