use crate::game::actors::{ActorAction, action::ActiveAction, lunge::{LungeRuntime, MotionContact}};
use super::*;
use crate::game::actors::definition::{
    EnemyFlyingAttackKind as AssetFlyingAttackKind,
    EnemyFlyingProfile, EnemyFlyingSetup,
    EnemyFlyingStalkKind as AssetFlyingStalkKind,
};

const DIVING_PATROL_RADIUS: f32 = 180.0;
const DIVING_MIN_WALL_DIST: f32 = 60.0;
const DIVING_SEARCH_ATTEMPTS: u8 = 12;
const DIVING_IDLE_WAIT_MAX: u16 = 96;
const DIVING_APPROACH_RATIO_MIN: f32 = 0.55;
const DIVING_APPROACH_RATIO_MAX: f32 = 1.8;
const DIVING_DIVE_TRIGGER_DIST: f32 = 60.0;
const BAT_DEATH_GFX_OFFSET_Y: f32 = 7.0;
const FLYING_COLLISION_WAIT_TICKS: u16 = 8;

const DIVING_COLLIDER_HALF_WIDTH: f32 = 3.75;
const DIVING_COLLIDER_HALF_HEIGHT: f32 = 2.0;

const PATROL_COLLIDER_HALF_WIDTH: f32 = 8.0;
const PATROL_COLLIDER_HALF_HEIGHT: f32 = 10.0;
const PATROL_RADIUS: f32 = 120.0;
const PATROL_MIN_WALL_DIST: f32 = 40.0;
const PATROL_SEARCH_ATTEMPTS: u8 = 8;
const PATROL_IDLE_WAIT_MIN: u16 = 144;
const PATROL_IDLE_WAIT_MAX: u16 = 288;

const SMOOTH_WAVE_COLLIDER_HALF_WIDTH: f32 = 8.0;
const SMOOTH_WAVE_COLLIDER_HALF_HEIGHT: f32 = 8.0;
const SMOOTH_WAVE_PATROL_RADIUS: f32 = 180.0;
const SMOOTH_WAVE_MIN_WALL_DIST: f32 = 48.0;
const SMOOTH_WAVE_SEARCH_ATTEMPTS: u8 = 12;
const SMOOTH_WAVE_TARGET_REACHED_DIST: f32 = 12.0;
const SMOOTH_WAVE_TARGET_MIN_DIST: f32 = 36.0;
const SMOOTH_WAVE_TURN_SPEED_PER_TICK: f32 = 0.025;
const SMOOTH_WAVE_PERIOD_TICKS: f32 = 256.0;
const SMOOTH_WAVE_STRENGTH: f32 = 1.0;
const HORIZONTAL_LINE_TARGET_REACHED_DIST: f32 = 5.0;
const HORIZONTAL_LINE_TARGET_DISTANCE: f32 = 104.0;
const HORIZONTAL_LINE_MIN_TARGET_DISTANCE: f32 = 36.0;
const HORIZONTAL_LINE_X_TOLERANCE: f32 = 8.0;
const HORIZONTAL_LINE_Y_TOLERANCE: f32 = 6.0;
pub(super) const VOID_FACE_Z_OFFSET: f32 = 0.000_000_2;
pub(super) const VOID_GHOST_LIFETIME_TICKS: u32 = 72;
pub(super) const VOID_GHOST_INTERVAL_TICKS: u32 = 5;
pub(super) const VOID_GHOST_MIN_SCALE: f32 = 0.55;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FlyingMode {
    Patrol,
    Stalk,
    Attack,
    Recover,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FlyingSteer {
    Direct,
    SmoothWave,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FlyingStalkKind {
    None,
    DiveSetup,
    HorizontalLine,
    Pursuit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FlyingAttackKind {
    None,
    Dash,
    Stationary,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct FlyingMotorConfig {
    patrol_speed: f32,
    patrol_radius: f32,
    min_patrol_radius: f32,
    min_wall_dist: f32,
    patrol_probe_half_extents: Vec2,
    search_attempts: u8,
    idle_wait_min: u16,
    idle_wait_max: u16,
    target_reached_dist: f32,
    target_min_dist: f32,
    unchecked_target_fallback: bool,
    require_right_wall_clearance: bool,
    nudge_blocked_home: bool,
    steer: FlyingSteer,
    turn_speed: f32,
    wave_period_ticks: f32,
    wave_strength: f32,
    stalk_kind: FlyingStalkKind,
    attack_kind: FlyingAttackKind,
    approach_ratio_min: f32,
    approach_ratio_max: f32,
    dive_trigger_dist: f32,
    attack_collider_half_extents: Vec2,
    horizontal_target_distance: f32,
    horizontal_min_target_distance: f32,
    horizontal_x_tolerance: f32,
    horizontal_y_tolerance: f32,
}

impl FlyingMotorConfig {
    pub(super) fn from_setup(
        setup: Option<EnemyFlyingSetup>,
        patrol_speed: f32,
    ) -> Self {
        let profile = setup
            .map(|setup| setup.profile)
            .unwrap_or(EnemyFlyingProfile::Patrol);
        let mut config = match profile {
            EnemyFlyingProfile::Diving => Self::diving(
                patrol_speed,
            ),
            EnemyFlyingProfile::Patrol => Self::patrol(patrol_speed),
            EnemyFlyingProfile::SmoothWave => Self::smooth_wave(patrol_speed),
        };
        if let Some(setup) = setup {
            config.apply_setup(setup);
        }
        config
    }

    pub(super) fn diving(
        patrol_speed: f32,
    ) -> Self {
        Self {
            patrol_speed,
            patrol_radius: DIVING_PATROL_RADIUS,
            min_patrol_radius: 0.0,
            min_wall_dist: DIVING_MIN_WALL_DIST,
            patrol_probe_half_extents: Vec2::splat(DIVING_MIN_WALL_DIST * 0.5),
            search_attempts: DIVING_SEARCH_ATTEMPTS,
            idle_wait_min: 0,
            idle_wait_max: DIVING_IDLE_WAIT_MAX,
            target_reached_dist: 4.0,
            target_min_dist: 0.0,
            unchecked_target_fallback: false,
            require_right_wall_clearance: false,
            nudge_blocked_home: true,
            steer: FlyingSteer::Direct,
            turn_speed: 0.0,
            wave_period_ticks: 1.0,
            wave_strength: 0.0,
            stalk_kind: FlyingStalkKind::DiveSetup,
            attack_kind: FlyingAttackKind::Dash,
            approach_ratio_min: DIVING_APPROACH_RATIO_MIN,
            approach_ratio_max: DIVING_APPROACH_RATIO_MAX,
            dive_trigger_dist: DIVING_DIVE_TRIGGER_DIST,
            attack_collider_half_extents: Vec2::new(
                DIVING_COLLIDER_HALF_WIDTH,
                DIVING_COLLIDER_HALF_HEIGHT,
            ),
            horizontal_target_distance: 0.0,
            horizontal_min_target_distance: 0.0,
            horizontal_x_tolerance: 0.0,
            horizontal_y_tolerance: 0.0,
        }
    }

    pub(super) fn patrol(patrol_speed: f32) -> Self {
        Self {
            patrol_speed,
            patrol_radius: PATROL_RADIUS,
            min_patrol_radius: PATROL_RADIUS * 0.3,
            min_wall_dist: PATROL_MIN_WALL_DIST,
            patrol_probe_half_extents: Vec2::new(
                PATROL_COLLIDER_HALF_WIDTH,
                PATROL_COLLIDER_HALF_HEIGHT,
            ),
            search_attempts: PATROL_SEARCH_ATTEMPTS,
            idle_wait_min: PATROL_IDLE_WAIT_MIN,
            idle_wait_max: PATROL_IDLE_WAIT_MAX,
            target_reached_dist: 4.0,
            target_min_dist: 0.0,
            unchecked_target_fallback: false,
            require_right_wall_clearance: true,
            nudge_blocked_home: false,
            steer: FlyingSteer::Direct,
            turn_speed: 0.0,
            wave_period_ticks: 1.0,
            wave_strength: 0.0,
            stalk_kind: FlyingStalkKind::None,
            attack_kind: FlyingAttackKind::None,
            approach_ratio_min: 0.0,
            approach_ratio_max: 0.0,
            dive_trigger_dist: 0.0,
            attack_collider_half_extents: Vec2::ZERO,
            horizontal_target_distance: 0.0,
            horizontal_min_target_distance: 0.0,
            horizontal_x_tolerance: 0.0,
            horizontal_y_tolerance: 0.0,
        }
    }

    pub(super) fn smooth_wave(patrol_speed: f32) -> Self {
        Self {
            patrol_speed,
            patrol_radius: SMOOTH_WAVE_PATROL_RADIUS,
            min_patrol_radius: SMOOTH_WAVE_TARGET_MIN_DIST,
            min_wall_dist: SMOOTH_WAVE_MIN_WALL_DIST,
            patrol_probe_half_extents: Vec2::new(
                SMOOTH_WAVE_COLLIDER_HALF_WIDTH,
                SMOOTH_WAVE_COLLIDER_HALF_HEIGHT,
            ),
            search_attempts: SMOOTH_WAVE_SEARCH_ATTEMPTS,
            idle_wait_min: 0,
            idle_wait_max: 0,
            target_reached_dist: SMOOTH_WAVE_TARGET_REACHED_DIST,
            target_min_dist: SMOOTH_WAVE_TARGET_MIN_DIST,
            unchecked_target_fallback: true,
            require_right_wall_clearance: true,
            nudge_blocked_home: true,
            steer: FlyingSteer::SmoothWave,
            turn_speed: SMOOTH_WAVE_TURN_SPEED_PER_TICK,
            wave_period_ticks: SMOOTH_WAVE_PERIOD_TICKS,
            wave_strength: SMOOTH_WAVE_STRENGTH,
            stalk_kind: FlyingStalkKind::None,
            attack_kind: FlyingAttackKind::None,
            approach_ratio_min: 0.0,
            approach_ratio_max: 0.0,
            dive_trigger_dist: 0.0,
            attack_collider_half_extents: Vec2::ZERO,
            horizontal_target_distance: 0.0,
            horizontal_min_target_distance: 0.0,
            horizontal_x_tolerance: 0.0,
            horizontal_y_tolerance: 0.0,
        }
    }

    fn apply_setup(&mut self, setup: EnemyFlyingSetup) {
        if let Some(value) = setup.patrol_radius {
            self.patrol_radius = value;
        }
        if let Some([min, max]) = setup.idle_wait {
            self.idle_wait_min = min;
            self.idle_wait_max = max;
        }
        if let Some(stalk) = setup.stalk {
            self.apply_stalk_setup(stalk);
        }
        if let Some(attack) = setup.attack {
            self.apply_attack_setup(attack);
        }
    }

    fn apply_stalk_setup(
        &mut self,
        setup: crate::game::actors::definition::EnemyFlyingStalkSetup,
    ) {
        match setup.kind {
            AssetFlyingStalkKind::DiveSetup => {
                self.apply_dive_stalk_setup(&setup);
            },
            AssetFlyingStalkKind::HorizontalLine => {
                self.stalk_kind = FlyingStalkKind::HorizontalLine;
                self.target_reached_dist = HORIZONTAL_LINE_TARGET_REACHED_DIST;
                self.unchecked_target_fallback = true;
                self.require_right_wall_clearance = false;
                self.nudge_blocked_home = true;
                let target_distance = setup
                    .target_distance
                    .unwrap_or(HORIZONTAL_LINE_TARGET_DISTANCE);
                self.horizontal_target_distance = target_distance;
                self.min_patrol_radius = target_distance * 0.5;
                self.target_min_dist = target_distance * 0.4;
                self.horizontal_min_target_distance = setup
                    .min_target_distance
                    .unwrap_or(HORIZONTAL_LINE_MIN_TARGET_DISTANCE);
                self.horizontal_x_tolerance = setup
                    .x_tolerance
                    .unwrap_or(HORIZONTAL_LINE_X_TOLERANCE);
                self.horizontal_y_tolerance = setup
                    .y_tolerance
                    .unwrap_or(HORIZONTAL_LINE_Y_TOLERANCE);
            },
            AssetFlyingStalkKind::None => {
                self.stalk_kind = FlyingStalkKind::None;
            },
            AssetFlyingStalkKind::Pursuit => {
                self.stalk_kind = FlyingStalkKind::Pursuit;
            },
        }


    }

    fn apply_dive_stalk_setup(
        &mut self,
        setup: &crate::game::actors::definition::EnemyFlyingStalkSetup,
    ) {
        self.stalk_kind = FlyingStalkKind::DiveSetup;
        if let Some(trigger_distance) = setup.trigger_distance {
            self.dive_trigger_dist = trigger_distance;
        }
        if let Some([min, max]) = setup.approach_ratio {
            self.approach_ratio_min = min;
            self.approach_ratio_max = max;
        }
    }

    fn apply_attack_setup(
        &mut self,
        setup: crate::game::actors::definition::EnemyFlyingAttackSetup,
    ) {
        match setup.kind {
            AssetFlyingAttackKind::Dash => {
                self.apply_dash_attack_setup(&setup);
            },
            AssetFlyingAttackKind::Stationary => {
                self.attack_kind = FlyingAttackKind::Stationary;
                self.attack_collider_half_extents = Vec2::ZERO;
            },
            AssetFlyingAttackKind::None => {
                self.attack_kind = FlyingAttackKind::None;
                self.attack_collider_half_extents = Vec2::ZERO;
            },
        }
    }

    fn apply_dash_attack_setup(
        &mut self,
        setup: &crate::game::actors::definition::EnemyFlyingAttackSetup,
    ) {
        self.attack_kind = FlyingAttackKind::Dash;
        if let Some([half_width, half_height]) = setup.collider_half_extents {
            self.attack_collider_half_extents = Vec2::new(half_width, half_height);
        }
    }
}

#[derive(Component)]
pub(super) struct FlyingMotor {
    home: Vec2,
    attack_dir: Vec2,
    move_angle: f32,
    has_move_angle: bool,
    wave_tick: u32,
    config: FlyingMotorConfig,
}

impl FlyingMotor {
    pub(super) fn new(home: Vec2, config: FlyingMotorConfig) -> Self {
        Self {
            home,
            attack_dir: Vec2::new(0.0, -1.0),
            move_angle: 0.0,
            has_move_angle: false,
            wave_tick: 0,
            config,
        }
    }


}

#[derive(Component)]
pub(super) struct BatDeath {
    landed: bool,
    fall_ticks: u32,
    ground_y: Option<f32>,
    probe_half_extents: Vec2,
    gfx_offset_y: f32,
}

impl BatDeath {
    pub(super) fn new(
        probe_half_extents: Option<Vec2>,
        gfx_offset_y: Option<f32>,
    ) -> Self {
        Self {
            landed: false,
            fall_ticks: 0,
            ground_y: None,
            probe_half_extents: probe_half_extents.unwrap_or(Vec2::new(
                DIVING_COLLIDER_HALF_WIDTH,
                DIVING_COLLIDER_HALF_HEIGHT,
            )),
            gfx_offset_y: gfx_offset_y.unwrap_or(BAT_DEATH_GFX_OFFSET_Y),
        }
    }

    pub(super) fn has_landed(&self) -> bool {
        self.landed
    }

    pub(super) fn probe_collider(&self) -> Collider {
        Collider::cuboid(
            self.probe_half_extents.x,
            self.probe_half_extents.y,
        )
    }
}

impl Default for BatDeath {
    fn default() -> Self {
        Self::new(None, None)
    }
}

#[derive(Component)]
pub(super) struct OrientToVelocity;

#[derive(Component)]
pub(super) struct HealthRegen {
    amount: u32,
    interval_ticks: u32,
    tick_counter: u32,
}

impl HealthRegen {
    pub(super) const fn new(amount: u32, interval_ticks: u32) -> Self {
        Self {
            amount,
            interval_ticks,
            tick_counter: 0,
        }
    }
}

pub(super) fn rotate_void_gfx(
    void_query: Query<
        (&Gent, &LinearVelocity, &FlyingMotor),
        (With<OrientToVelocity>, Without<Dead>),
    >,
    mut gfx_query: Query<(&mut Transform, Option<&mut Sprite>)>,
) {
    for (gent, velocity, motor) in void_query.iter() {
        let movement = if velocity.0.length_squared() > f32::EPSILON {
            velocity.0
        } else if motor.has_move_angle {
            Vec2::from_angle(motor.move_angle)
        } else {
            Vec2::ZERO
        };
        let rotation = Quat::from_rotation_z(void_gfx_target_angle(movement));
        for gfx_entity in [gent.e_gfx, gent.e_effects_gfx] {
            apply_void_gfx_rotation(gfx_entity, rotation, &mut gfx_query);
        }
    }
}

fn apply_void_gfx_rotation(
    gfx_entity: Entity,
    rotation: Quat,
    gfx_query: &mut Query<(&mut Transform, Option<&mut Sprite>)>,
) {
    let Ok((mut transform, sprite)) = gfx_query.get_mut(gfx_entity) else {
        return;
    };

    transform.rotation = rotation;
    if let Some(mut sprite) = sprite {
        sprite.flip_x = false;
        sprite.flip_y = false;
    }
}

fn void_gfx_target_angle(velocity: Vec2) -> f32 {
    if velocity.length_squared() <= f32::EPSILON {
        return 0.0;
    }

    velocity.y.atan2(velocity.x)
}

fn flying_clear_observations(actor: &mut ActorController) {
    actor.observations.aligned = false;
}

fn flying_set_attack_ready(actor: &mut ActorController, ready: bool) {
    actor.observations.aligned = ready;
}

pub(super) fn set_flying_mode(
    mode: FlyingMode,
    _motor: &mut FlyingMotor,
    controller: &mut ActorController,
    velocity: &mut LinearVelocity,
    gent: &Gent,
    gfx_query: &mut Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
    _commands: &mut Commands,
) {
    flying_clear_observations(controller);
    if mode != FlyingMode::Attack {
        velocity.0 = Vec2::ZERO;
    }
    if matches!(mode, FlyingMode::Patrol | FlyingMode::Stalk) {
        set_enemy_anim_slot(gfx_query, gent, "AttackImpact", false);
    }
}

fn flying_patrol_target_allowed(
    collision_world: &TileCollisionWorld,
    cfg: FlyingMotorConfig,
    candidate: Vec2,
    current: Vec2,
) -> bool {
    if cfg.target_min_dist > 0.0
        && candidate.distance_squared(current) < cfg.target_min_dist.powi(2)
    {
        return false;
    }
    if collision_world.overlaps_solid(candidate, cfg.patrol_probe_half_extents) {
        return false;
    }
    !cfg.require_right_wall_clearance
        || collision_world.distance(
            ProbeDirection::Right,
            candidate,
            cfg.patrol_probe_half_extents,
            cfg.min_wall_dist,
        ) >= cfg.min_wall_dist
}

fn flying_unchecked_patrol_target(
    motor: &FlyingMotor,
    current: Vec2,
    controller: &mut ActorController,
) -> Vec2 {
    if let Some(candidate) = first_patrol_target(
        controller,
        motor.config.search_attempts,
        |controller| {
            random_patrol_candidate(
                motor.home,
                controller,
                motor.config.min_patrol_radius,
                motor.config.patrol_radius,
            )
        },
        |candidate| {
            motor.config.target_min_dist <= 0.0
                || candidate.distance_squared(current)
                    >= motor.config.target_min_dist.powi(2)
        },
    ) {
        return candidate;
    }

    let angle = actor_rand_unit(controller) * std::f32::consts::TAU;
    current + Vec2::from_angle(angle) * motor.config.target_min_dist.max(1.0)
}

fn flying_pick_patrol_target(
    motor: &FlyingMotor,
    current: Vec2,
    controller: &mut ActorController,
    collision_world: Option<&TileCollisionWorld>,
) -> Option<Vec2> {
    let cfg = motor.config;
    let Some(collision_world) = collision_world.filter(|world| world.is_ready())
    else {
        return cfg
            .unchecked_target_fallback
            .then(|| flying_unchecked_patrol_target(motor, current, controller));
    };

    if cfg.nudge_blocked_home
        && collision_world.overlaps_solid(motor.home, cfg.patrol_probe_half_extents)
    {
        let nudged = motor.home + Vec2::new(0.0, cfg.min_wall_dist);
        if flying_patrol_target_allowed(collision_world, cfg, nudged, current) {
            return Some(nudged);
        }
    }

    if let Some(candidate) = first_patrol_target(
        controller,
        cfg.search_attempts,
        |controller| {
            random_patrol_candidate(
                motor.home,
                controller,
                cfg.min_patrol_radius,
                cfg.patrol_radius,
            )
        },
        |candidate| {
            flying_patrol_target_allowed(collision_world, cfg, candidate, current)
        },
    ) {
        return Some(candidate);
    }

    cfg.unchecked_target_fallback
        .then(|| flying_unchecked_patrol_target(motor, current, controller))
}

fn flying_steer_velocity(motor: &mut FlyingMotor, target_delta: Vec2) -> Vec2 {
    let cfg = motor.config;
    if cfg.steer == FlyingSteer::Direct {
        return target_delta.normalize_or_zero() * cfg.patrol_speed;
    }

    let target_angle = if target_delta.length_squared() > f32::EPSILON {
        target_delta.y.atan2(target_delta.x)
    } else if motor.has_move_angle {
        motor.move_angle
    } else {
        0.0
    };

    if motor.has_move_angle {
        motor.move_angle =
            approach_angle(motor.move_angle, target_angle, cfg.turn_speed);
    } else {
        motor.move_angle = target_angle;
        motor.has_move_angle = true;
    }

    let forward = Vec2::from_angle(motor.move_angle);
    let side = Vec2::new(-forward.y, forward.x);
    let wave_phase =
        (motor.wave_tick as f32 / cfg.wave_period_ticks) * std::f32::consts::TAU;
    motor.wave_tick = motor.wave_tick.wrapping_add(1);
    (forward + side * wave_phase.sin() * cfg.wave_strength)
        .normalize_or_zero()
        * cfg.patrol_speed
}

fn approach_angle(current: f32, target: f32, max_step: f32) -> f32 {
    let delta = (target - current + std::f32::consts::PI)
        .rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    current + delta.clamp(-max_step, max_step)
}

fn flying_tick_patrol(
    motor: &mut FlyingMotor,
    controller: &mut ActorController,
    velocity: &mut LinearVelocity,
    current_pos: Vec2,
    collision_world: Option<&TileCollisionWorld>,
    is_knocked: bool,
) {
    if controller.roam_wait > 0 {
        velocity.0 = Vec2::ZERO;
        controller.roam_wait = controller.roam_wait.saturating_sub(1);
        return;
    }

    if controller.roam_target.is_none() {
        controller.roam_target =
            flying_pick_patrol_target(motor, current_pos, controller, collision_world);
        if controller.roam_target.is_none() {
            controller.roam_wait = actor_rand_range(
                controller,
                motor.config.idle_wait_min,
                motor.config.idle_wait_max,
            );
            return;
        }
    }

    let Some(target) = controller.roam_target else {
        return;
    };
    let delta = target - current_pos;
    if delta.length() < motor.config.target_reached_dist {
        controller.roam_target = None;
        controller.roam_wait = actor_rand_range(
            controller,
            motor.config.idle_wait_min,
            motor.config.idle_wait_max,
        );
        velocity.0 = Vec2::ZERO;
        return;
    }

    if !is_knocked {
        velocity.0 = flying_steer_velocity(motor, delta);
    }
}

fn flying_stalk_target(
    actor_pos: Vec2,
    player_pos: Vec2,
    collision_world: Option<&TileCollisionWorld>,
    cfg: FlyingMotorConfig,
) -> Option<Vec2> {
    match cfg.stalk_kind {
        FlyingStalkKind::DiveSetup => flying_dive_stalk_target(
            actor_pos,
            player_pos,
            collision_world,
            cfg,
        ),
        FlyingStalkKind::HorizontalLine => flying_horizontal_stalk_target(
            actor_pos,
            player_pos,
            collision_world,
            cfg,
        ),
        FlyingStalkKind::None => None,
        FlyingStalkKind::Pursuit => Some(player_pos),
    }
}

fn flying_dive_stalk_target(
    actor_pos: Vec2,
    player_pos: Vec2,
    collision_world: Option<&TileCollisionWorld>,
    cfg: FlyingMotorConfig,
) -> Option<Vec2> {
    let collision_world = collision_world.filter(|world| world.is_ready())?;
    let half_extents = cfg.patrol_probe_half_extents;

    let delta = actor_pos - player_pos;
    if delta.y > 8.0 && delta.length() <= cfg.dive_trigger_dist {
        let ratio = delta.x.abs() / delta.y.max(1.0);
        if ratio >= cfg.approach_ratio_min && ratio <= cfg.approach_ratio_max {
            return Some(actor_pos);
        }
    }

    let side = if delta.x.abs() < 4.0 {
        if actor_pos.x >= player_pos.x { 1.0 } else { -1.0 }
    } else {
        delta.x.signum()
    };
    let approach_offset =
        cfg.dive_trigger_dist * 0.85 * std::f32::consts::FRAC_1_SQRT_2;
    let ideal_target =
        player_pos + Vec2::new(approach_offset * side, approach_offset);

    if let Some(closer_target) =
        flying_closer_dive_target(delta, player_pos, collision_world, half_extents, cfg)
    {
        return Some(closer_target);
    }

    if !collision_world.overlaps_solid(ideal_target, half_extents) {
        return Some(ideal_target);
    }

    let fallback_target =
        player_pos + Vec2::new(approach_offset * -side, approach_offset);
    (!collision_world.overlaps_solid(fallback_target, half_extents))
        .then_some(fallback_target)
}

fn flying_closer_dive_target(
    delta: Vec2,
    player_pos: Vec2,
    collision_world: &TileCollisionWorld,
    half_extents: Vec2,
    cfg: FlyingMotorConfig,
) -> Option<Vec2> {
    if delta.y <= 8.0 {
        return None;
    }

    let current_ratio = delta.x.abs() / delta.y.max(1.0);
    if current_ratio < cfg.approach_ratio_min
        || current_ratio > cfg.approach_ratio_max
    {
        return None;
    }

    let target_dist = cfg.dive_trigger_dist * 0.85;
    let closer_target = player_pos + delta.normalize_or_zero() * target_dist;
    (!collision_world.overlaps_solid(closer_target, half_extents))
        .then_some(closer_target)
}

fn flying_horizontal_stalk_target(
    actor_pos: Vec2,
    player_pos: Vec2,
    collision_world: Option<&TileCollisionWorld>,
    cfg: FlyingMotorConfig,
) -> Option<Vec2> {
    let side = if actor_pos.x >= player_pos.x { 1.0 } else { -1.0 };
    let target = player_pos + Vec2::new(
        cfg.horizontal_target_distance * side,
        0.0,
    );

    let Some(collision_world) =
        collision_world.filter(|world| world.is_ready())
    else {
        return Some(target);
    };

    if !collision_world.overlaps_solid(target, cfg.patrol_probe_half_extents) {
        return Some(target);
    }

    let fallback = player_pos + Vec2::new(
        cfg.horizontal_target_distance * -side,
        0.0,
    );
    if !collision_world.overlaps_solid(fallback, cfg.patrol_probe_half_extents) {
        return Some(fallback);
    }

    let current_side_target = Vec2::new(actor_pos.x, player_pos.y);
    (!collision_world.overlaps_solid(
        current_side_target,
        cfg.patrol_probe_half_extents,
    ))
    .then_some(current_side_target)
}

fn flying_is_aligned_for_attack(
    actor_pos: Vec2,
    player_pos: Vec2,
    cfg: FlyingMotorConfig,
) -> bool {
    match cfg.stalk_kind {
        FlyingStalkKind::DiveSetup => flying_is_aligned_for_dive(
            actor_pos,
            player_pos,
            cfg,
        ),
        FlyingStalkKind::HorizontalLine => {
            let delta = player_pos - actor_pos;
            let x_dist = delta.x.abs();
            delta.y.abs() <= cfg.horizontal_y_tolerance
                && x_dist >= cfg.horizontal_min_target_distance
                && (x_dist - cfg.horizontal_target_distance).abs()
                    <= cfg.horizontal_x_tolerance
        },
        FlyingStalkKind::None | FlyingStalkKind::Pursuit => false,
    }
}

fn flying_is_aligned_for_dive(
    actor_pos: Vec2,
    player_pos: Vec2,
    cfg: FlyingMotorConfig,
) -> bool {
    let delta = player_pos - actor_pos;
    if delta.y >= -8.0 || delta.length() > cfg.dive_trigger_dist {
        return false;
    }
    let ratio = (delta.x.abs() / delta.y.abs().max(1.0)).abs();
    ratio >= cfg.approach_ratio_min && ratio <= cfg.approach_ratio_max
}

fn flying_reset_attack_ready(controller: &mut ActorController) {
    flying_set_attack_ready(controller, false);
}

fn flying_attack_dir(
    target_sensor: &TargetSensor,
    player_query: &Query<&Transform, With<Player>>,
    transform: &Transform,
    motor: &FlyingMotor,
) -> Vec2 {
    target_sensor
        .entity
        .and_then(|entity| player_query.get(entity).ok())
        .map(|player| player.translation.xy() - transform.translation.xy())
        .filter(|delta| delta.length_squared() > 0.1)
        .map(glam::Vec2::normalize)
        .unwrap_or(motor.attack_dir.normalize_or_zero())
        .normalize_or_zero()
}

fn flying_face_target_in_aggro(
    facing: &mut Facing,
    target_sensor: &TargetSensor,
    player_query: &Query<&Transform, With<Player>>,
    transform: &Transform,
    motor: &FlyingMotor,
    stats: &crate::game::actors::sensors::CachedArchetypeStats,
) -> bool {
    if motor.config.attack_kind != FlyingAttackKind::Stationary
        || target_sensor.dist2 > stats.vision_range.powi(2)
    {
        return false;
    }

    let Some(target_entity) = target_sensor.entity else {
        return false;
    };
    let Ok(player_transform) = player_query.get(target_entity) else {
        return false;
    };

    let dx = player_transform.translation.x - transform.translation.x;
    *facing = if dx >= 0.0 {
        Facing::Right
    } else {
        Facing::Left
    };
    true
}

fn flying_begin_attack(
    start: &FlyingAttackStart<'_>,
    motor: &mut FlyingMotor,
    active: &mut ActiveAction,
    player_query: &Query<&Transform, With<Player>>,
    commands: &mut Commands,
) {
    let Some(motion) = active.definition.motion else { return; };
    let dir = flying_attack_dir(start.target_sensor, player_query, start.transform, motor);
    motor.attack_dir = if dir.length_squared() > 0.0 { dir } else { Vec2::NEG_Y };
    active.lunge = Some(LungeRuntime::new(motion, motor.attack_dir, start.transform.translation.xy()));
    let half_extents = motor.config.attack_collider_half_extents;
    let offset = motor.attack_dir.normalize_or_zero() * (half_extents.y * 0.5);
    active.owned_entities.push(spawn_attached_enemy_damage_source(commands, AttachedEnemyDamageSourceSpawn {
        owner: start.entity, damage: active.damage, lifetime_ticks: 999,
        collider: Collider::cuboid(half_extents.x, half_extents.y), offset: offset.extend(0.0),
    }));
}

fn flying_mark_impact(
    action: &mut ActorAction,
    velocity: &mut LinearVelocity,
    gent: &Gent,
    gfx_query: &mut Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
    commands: &mut Commands,
    contact: MotionContact,
) {
    let Some(active) = &mut action.0 else { return; };
    let Some(lunge) = &mut active.lunge else { return; };
    if !lunge.impact(contact) { return; }
    velocity.0 = Vec2::ZERO;
    active.cleanup(commands);
    if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
        active.present(&mut player, crate::game::actors::action::ActionPresentation::Impact);
    }
}

struct FlyingStalkTick<'a> {
    actor_pos: Vec2,
    player_pos: Option<Vec2>,
    collision_world: Option<&'a TileCollisionWorld>,
    is_knocked: bool,
}

fn flying_tick_stalk_mode(
    motor: &mut FlyingMotor,
    controller: &mut ActorController,
    velocity: &mut LinearVelocity,
    tick: FlyingStalkTick<'_>,
) {
    let Some(player_pos) = tick.player_pos else {
        flying_reset_attack_ready(controller);
        stop_flying_if_not_knocked(velocity, tick.is_knocked);
        return;
    };

    update_flying_stalk_velocity(
        motor,
        velocity,
        tick.actor_pos,
        player_pos,
        tick.collision_world,
        tick.is_knocked,
    );
    update_flying_attack_windup(
        motor,
        controller,
        tick.actor_pos,
        player_pos,
    );
}

fn update_flying_stalk_velocity(
    motor: &FlyingMotor,
    velocity: &mut LinearVelocity,
    actor_pos: Vec2,
    player_pos: Vec2,
    collision_world: Option<&TileCollisionWorld>,
    is_knocked: bool,
) {
    let Some(target) =
        flying_stalk_target(actor_pos, player_pos, collision_world, motor.config)
    else {
        return;
    };

    let delta = target - actor_pos;
    if delta.length() > 2.0 {
        set_flying_velocity_if_not_knocked(
            velocity,
            delta.normalize() * motor.config.patrol_speed.max(0.1),
            is_knocked,
        );
    } else {
        stop_flying_if_not_knocked(velocity, is_knocked);
    }
}

fn update_flying_attack_windup(
    motor: &mut FlyingMotor,
    controller: &mut ActorController,
    actor_pos: Vec2,
    player_pos: Vec2,
) {
    if !flying_is_aligned_for_attack(actor_pos, player_pos, motor.config) {
        flying_reset_attack_ready(controller);
        return;
    }

    flying_set_attack_ready(controller, true);
}

struct FlyingAttackStart<'a> {
    entity: Entity,
    entered: bool,
    target_sensor: &'a TargetSensor,
    transform: &'a Transform,
}

fn flying_tick_attack_mode(
    start: FlyingAttackStart<'_>,
    motor: &mut FlyingMotor,
    action: &mut ActorAction,
    velocity: &mut LinearVelocity,
    is_knocked: bool,
    player_query: &Query<&Transform, With<Player>>,
    commands: &mut Commands,
) {
    let Some(active) = &mut action.0 else { return; };
    if start.entered { flying_begin_attack(&start, motor, active, player_query, commands); }
    let displacement = active.lunge.as_ref().map(LungeRuntime::displacement).unwrap_or_default();
    if !is_knocked { active.motion_intent(displacement).apply(active.id, velocity); }
}

fn set_flying_velocity_if_not_knocked(
    velocity: &mut LinearVelocity,
    target_velocity: Vec2,
    is_knocked: bool,
) {
    if !is_knocked {
        velocity.0 = target_velocity;
    }
}

fn stop_flying_if_not_knocked(
    velocity: &mut LinearVelocity,
    is_knocked: bool,
) {
    set_flying_velocity_if_not_knocked(velocity, Vec2::ZERO, is_knocked);
}

fn cap_flying_roam_speed(
    target_sensor: &TargetSensor,
    velocity: &mut LinearVelocity,
    motor: &FlyingMotor,
    cached_stats: &crate::game::actors::sensors::CachedArchetypeStats,
) {
    if target_sensor.entity.is_some() || velocity.0.length_squared() <= 0.0 {
        return;
    }

    let max_speed = cached_stats
        .vision_range
        .max(motor.config.patrol_speed * 2.0);
    if velocity.0.length() > max_speed {
        velocity.0 = velocity.0.normalize() * max_speed;
    }
}

pub(super) fn flying_motor_system(
    collision_world: Option<Res<TileCollisionWorld>>,
    mut query: Query<
        (
            Entity,
            &mut ActorController,
            &mut LinearVelocity,
            &mut Facing,
            &TargetSensor,
            &Transform,
            &mut FlyingMotor,
            &mut ActorAction,
            &crate::game::actors::sensors::CachedArchetypeStats,
            Has<Knockback>,
        ),
        (
            With<Enemy>,
            Without<Dead>,
            Without<crate::game::effects::frozen::Frozen>,
        ),
    >,
    player_query: Query<&Transform, With<Player>>,
    mut commands: Commands,
    mut profiler: TickProfiler,
) {
    let _profile = profiler.scope("flying_motor");
    let collision_world = collision_world.as_deref();
    for (
        entity,
        mut controller,
        mut velocity,
        mut facing,
        target_sensor,
        transform,
        mut motor,
        mut action,
        cached_stats,
        is_knocked,
    ) in query.iter_mut()
    {
        let entered = controller.entered;

        match actuation::flying_intent(&controller, controller.activity) {
            FlyingMode::Patrol => {
                flying_tick_patrol(
                    &mut motor,
                    &mut controller,
                    &mut velocity,
                    transform.translation.xy(),
                    collision_world,
                    is_knocked,
                );
            },
            FlyingMode::Stalk => {
                let player_pos = target_sensor
                    .entity
                    .and_then(|e| player_query.get(e).ok())
                    .map(|t| t.translation.xy());
                flying_tick_stalk_mode(
                    &mut motor,
                    &mut controller,
                    &mut velocity,
                    FlyingStalkTick {
                        actor_pos: transform.translation.xy(),
                        player_pos,
                        collision_world,
                                        is_knocked,
                    },
                );
            },
            FlyingMode::Attack => {
                flying_tick_attack_mode(
                    FlyingAttackStart {
                        entity,
                        target_sensor,
                        transform,
                        entered,
                    },
                    &mut motor,
                    &mut action,
                    &mut velocity,
                    is_knocked,
                    &player_query,
                    &mut commands,
                );
            },
            FlyingMode::Recover => {
                stop_flying_if_not_knocked(&mut velocity, is_knocked);
            },
        }

        if !flying_face_target_in_aggro(
            &mut facing,
            target_sensor,
            &player_query,
            transform,
            &motor,
            cached_stats,
        ) {
            sync_motion_facing(&mut facing, velocity.0);
        }
        cap_flying_roam_speed(target_sensor, &mut velocity, &motor, cached_stats);
    }
}

pub(super) fn flying_move_collide(
    collision_world: Option<Res<TileCollisionWorld>>,
    mut query: Query<
        (
            &mut Transform,
            &Collider,
            &mut LinearVelocity,
            &Gent,
            &mut FlyingMotor,
            &mut ActorController,
            &mut ActorAction,
        ),
        (
            With<FlyingMotor>,
            Without<Dead>,
            Without<crate::game::effects::frozen::Frozen>,
        ),
    >,
    mut commands: Commands,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
    mut profiler: TickProfiler,
) {
    let _profile = profiler.scope("enemy_flying_move_collide");
    let collision_world =
        collision_world.as_deref().filter(|world| world.is_ready());

    for (mut transform, collider, mut velocity, gent, motor, mut controller, mut action) in query.iter_mut() {
        handle_flying_collision(
            move_flying_actor_with_wall_nudge(
                collision_world,
                &mut transform,
                collider,
                &mut velocity,
            ),
            (&motor, &mut controller),
            &mut action,
            &mut velocity,
            gent,
            &mut gfx_query,
            &mut commands,
        );
    }
}

fn handle_flying_collision(
    collision: Option<super::actor_descriptors::FlyingCollision>,
    roam: (&FlyingMotor, &mut ActorController),
    action: &mut ActorAction,
    velocity: &mut LinearVelocity,
    gent: &Gent,
    gfx_query: &mut Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
    commands: &mut Commands,
) {
    let Some(collision) = collision else {
        return;
    };

    let (motor, controller) = roam;
    controller.roam_target = None;
    if collision.normal != Vec2::ZERO
        && (motor.config.idle_wait_min > 0 || motor.config.idle_wait_max > 0)
    {
        controller.roam_wait = FLYING_COLLISION_WAIT_TICKS;
    }
    flying_mark_impact(action, velocity, gent, gfx_query, commands, MotionContact::Tile);
}

pub(super) fn flying_attack_damage_events(
    mut damage_events: MessageReader<DamageInfo>,
    mut query: Query<
        (
            &mut ActorAction,
            &mut LinearVelocity,
            &Gent,
        ),
        (
            With<FlyingMotor>,
            Without<crate::game::effects::frozen::Frozen>,
        ),
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
    mut commands: Commands,
) {
    for evt in damage_events.read() {
        if let Ok(impact) = query.get_mut(evt.owner) {
            mark_flying_impact_for_damage(
                impact,
                &mut gfx_query,
                &mut commands,
            );
        }
    }
}

type FlyingImpactItem<'a> = (
    Mut<'a, ActorAction>,
    Mut<'a, LinearVelocity>,
    &'a Gent,
);

fn mark_flying_impact_for_damage(
    impact: FlyingImpactItem<'_>,
    gfx_query: &mut Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
    commands: &mut Commands,
) {
    let (mut action, mut velocity, gent) = impact;
    flying_mark_impact(&mut action, &mut velocity, gent, gfx_query, commands, MotionContact::ConfirmedHit);
}

pub(super) fn bat_death_fall(
    collision_world: Option<Res<TileCollisionWorld>>,
    mut query: Query<
        (
            &mut Transform,
            &mut LinearVelocity,
            &Gent,
            &mut BatDeath,
        ),
        (With<BatDeath>, With<Dead>),
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    let collision_world =
        collision_world.as_deref().filter(|world| world.is_ready());

    for (mut transform, mut velocity, gent, mut death) in query.iter_mut() {
        tick_bat_death_fall(
            &mut transform,
            &mut velocity,
            gent,
            &mut death,
            collision_world,
            &mut gfx_query,
        );
    }
}

fn tick_bat_death_fall(
    transform: &mut Transform,
    velocity: &mut LinearVelocity,
    gent: &Gent,
    death: &mut BatDeath,
    collision_world: Option<&TileCollisionWorld>,
    gfx_query: &mut Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    if death.landed {
        return;
    }
    if let Some(ground_y) = death.ground_y {
        land_bat_death(transform, velocity, gent, death, ground_y, gfx_query);
        return;
    }

    let vel_y = curve_velocity(ENEMY_FALL_VELOCITIES, death.fall_ticks);
    velocity.0 = Vec2::new(0.0, vel_y);
    let landed_this_tick = try_land_bat_death(
        collision_world,
        transform,
        velocity,
        gent,
        death,
        vel_y,
        gfx_query,
    );
    if !landed_this_tick {
        transform.translation.y += vel_y;
    }
    death.fall_ticks = death.fall_ticks.saturating_add(1);
}

fn try_land_bat_death(
    collision_world: Option<&TileCollisionWorld>,
    transform: &mut Transform,
    velocity: &mut LinearVelocity,
    gent: &Gent,
    death: &mut BatDeath,
    vel_y: f32,
    gfx_query: &mut Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) -> bool {
    let Some(collision_world) = collision_world else {
        return false;
    };

    let max_distance = vel_y.abs() + GROUNDED_THRESHOLD;
    let ground_dist = collision_world.distance(
        ProbeDirection::Down,
        transform.translation.xy(),
        death.probe_half_extents,
        max_distance,
    );
    if ground_dist >= max_distance {
        return false;
    }

    let ground_pos = transform.translation.y - ground_dist;
    death.ground_y = Some(ground_pos);
    land_bat_death(transform, velocity, gent, death, ground_pos, gfx_query);
    true
}

fn land_bat_death(
    transform: &mut Transform,
    velocity: &mut LinearVelocity,
    gent: &Gent,
    death: &mut BatDeath,
    ground_y: f32,
    gfx_query: &mut Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    transform.translation.y = ground_y;
    velocity.0 = Vec2::ZERO;
    death.landed = true;
    set_enemy_anim_slot(gfx_query, gent, "HitGround", true);
}

pub(super) fn sync_bat_death_gfx_offset(
    bat_query: Query<(&Gent, &BatDeath, Has<Dead>, Has<Decay>)>,
    mut gfx_query: Query<&mut TransformGfxFromGent>,
) {
    for (gent, death, is_dead, is_decay) in bat_query.iter() {
        let target = Some(if is_dead || is_decay {
            Vec3::new(0.0, death.gfx_offset_y, 0.0)
        } else {
            Vec3::ZERO
        });
        sync_bat_death_gfx_offset_for_gent(gent, target, &mut gfx_query);
    }
}

fn sync_bat_death_gfx_offset_for_gent(
    gent: &Gent,
    target: Option<Vec3>,
    gfx_query: &mut Query<&mut TransformGfxFromGent>,
) {
    let Ok(mut gfx) = gfx_query.get_mut(gent.e_gfx) else {
        return;
    };
    if gfx.offset != target {
        gfx.offset = target;
    }
}

pub(super) fn health_regen(
    mut query: Query<
        (&mut Health, &mut HealthRegen),
        (
            Without<Dead>,
            Without<crate::game::effects::frozen::Frozen>,
        ),
    >,
) {
    for (mut health, mut regen) in query.iter_mut() {
        regen.tick_counter += 1;
        if regen.tick_counter >= regen.interval_ticks {
            regen.tick_counter = 0;
            health.current =
                health.current.saturating_add(regen.amount).min(health.max);
        }
    }
}
