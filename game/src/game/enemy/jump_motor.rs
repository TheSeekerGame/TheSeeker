//! The jump motor of hopping enemies: they idle, crouch, and leap toward the player or at
//! random, may jump again in the air, and fall. A player's hit angers every hopper near the one
//! hit, which then chases the player.
use bevy::ecs::query::QueryData;
use theseeker_engine::physics::geometry::Axis;

use super::*;
use crate::game::actors::definition::{EnemyJumpAngerSetup, EnemyJumpSetup};
use crate::game::actors::hopper::next_random;
use crate::game::effects::frozen::Frozen;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum JumpMotorMode {
    Idle,
    Jump,
    AirJump,
    Fall,
    Death,
}

#[derive(Clone, Copy, Debug)]
struct JumpMotorConfig {
    idle_ticks: [u32; 2],
    jump_prep_ticks: u32,
    /// Ticks into a rise before it may be cut short at random.
    jump_cancel_lock_ticks: u32,
    horizontal_speed: f32,
    fall_multiplier: f32,
    anger: Option<EnemyJumpAngerSetup>,
}

impl JumpMotorConfig {
    fn from_setup(setup: Option<EnemyJumpSetup>) -> Self {
        let default = Self::default();
        let setup = setup.unwrap_or_default();
        Self {
            idle_ticks: setup.idle_ticks.unwrap_or(default.idle_ticks),
            jump_prep_ticks: setup.jump_prep_ticks.unwrap_or(default.jump_prep_ticks),
            jump_cancel_lock_ticks: setup.jump_cancel_lock_ticks.unwrap_or(default.jump_cancel_lock_ticks),
            horizontal_speed: setup.horizontal_speed.unwrap_or(default.horizontal_speed),
            fall_multiplier: setup.fall_multiplier.unwrap_or(default.fall_multiplier),
            anger: setup.anger,
        }
    }
}

impl Default for JumpMotorConfig {
    fn default() -> Self {
        Self {
            idle_ticks: [96, 192],
            jump_prep_ticks: 12,
            jump_cancel_lock_ticks: 16,
            horizontal_speed: 0.5,
            fall_multiplier: 1.0,
            anger: None,
        }
    }
}

#[derive(Component)]
pub(super) struct JumpMotor {
    config: JumpMotorConfig,
    jump_dir: f32,
    /// Ticks in the current mode; for a jump, its preparation and then its rise.
    jump_ticks: u32,
    fall_ticks: u32,
    jump_done: bool,
}

impl Default for JumpMotor {
    fn default() -> Self {
        Self::new(None)
    }
}

impl JumpMotor {
    pub(super) fn new(setup: Option<EnemyJumpSetup>) -> Self {
        Self { config: JumpMotorConfig::from_setup(setup), jump_dir: 0.0, jump_ticks: 0, fall_ticks: 0, jump_done: false }
    }

    /// Ends any jump and restarts the fall.
    fn settle(&mut self) {
        self.fall_ticks = 0;
        self.jump_done = false;
    }

    /// Falls one tick along the gravity curve.
    fn fall(&mut self, velocity: &mut LinearVelocity, nav: &mut Navigation, jumping: bool) {
        velocity.0.y = gravity_velocity(self.fall_ticks) * self.config.fall_multiplier;
        *nav = Navigation::Falling { fall_ticks: self.fall_ticks, jumping };
        self.fall_ticks = self.fall_ticks.saturating_add(1);
    }
}

/// Angers every hopper within the anger radius of one the player hits.
pub(super) fn jump_motor_anger_trigger(
    mut damage: MessageReader<DamageInfo>,
    mut hoppers: Query<(&Transform, &JumpMotor, &mut ActorController), Without<Frozen>>,
    players: Query<(), With<Player>>,
) {
    let hits: Vec<(Vec2, f32)> = damage
        .read()
        .filter(|hit| players.contains(hit.owner))
        .filter_map(|hit| {
            let (transform, motor, _) = hoppers.get(hit.target).ok()?;
            Some((transform.translation.xy(), motor.config.anger?.radius))
        })
        .collect();
    for (center, radius) in hits {
        let near = |(transform, motor, _): &(&Transform, &JumpMotor, Mut<ActorController>)| {
            motor.config.anger.is_some() && transform.translation.xy().distance(center) <= radius
        };
        for (_, _, mut controller) in hoppers.iter_mut().filter(near) {
            controller.hopper.anger_active = true;
        }
    }
}

/// Moves hoppers through the tiles. Landing, or falling onto ground within reach, settles them
/// on it; a rise into the ceiling ends the jump; ground dropping away starts a fall.
pub(super) fn jump_motor_move_collide(
    collision_world: Option<Res<TileCollisionWorld>>,
    mut query: Query<(&mut Transform, &mut LinearVelocity, &Collider, &mut Navigation, &mut JumpMotor), Without<Frozen>>,
    mut profiler: TickProfiler,
) {
    let _profile = profiler.scope("enemy_jump_motor_move_collide");
    let collision_world = collision_world.as_deref().filter(|world| world.is_ready());
    for (mut transform, mut velocity, collider, mut nav, mut motor) in &mut query {
        let z = transform.translation.z;
        let half_extents = collider.half_extents();
        let Some(tiles) = collision_world.filter(|_| half_extents.min_element() > 0.0) else {
            transform.translation = (transform.translation.xy() + velocity.0).extend(z);
            continue;
        };
        let attempted = velocity.0;
        let result = tiles.resolve_movement(transform.translation.xy(), attempted, half_extents);
        let (mut position, mut moved) = (result.position, result.velocity);
        let ground_below = |at: Vec2| tiles.distance(ProbeDirection::Down, at, half_extents, GROUNDED_THRESHOLD);
        let snap = (!result.grounded && attempted.y <= 0.0)
            .then(|| ground_below(result.position))
            .filter(|&distance| distance < GROUNDED_THRESHOLD);
        if let Some(distance) = snap {
            position.y -= distance;
            moved.y = 0.0;
        }
        if result.grounded || snap.is_some() {
            *nav = Navigation::Grounded;
            motor.settle();
        }
        if result.hit_ceiling && attempted.y > 0.0 {
            motor.jump_done = true;
            moved.y = 0.0;
        }
        if matches!(*nav, Navigation::Grounded) && !result.grounded && ground_below(position) >= GROUNDED_THRESHOLD {
            *nav = Navigation::Falling { fall_ticks: 0, jumping: false };
            motor.fall_ticks = 0;
        }
        transform.translation = position.extend(z);
        velocity.0 = moved;
    }
}

/// What the jump motor drives.
#[derive(QueryData)]
#[query_data(mutable)]
pub(super) struct JumpBody {
    motor: &'static mut JumpMotor,
    velocity: &'static mut LinearVelocity,
    nav: &'static mut Navigation,
    transform: &'static Transform,
    facing: &'static mut Facing,
    collider: &'static Collider,
    controller: &'static mut ActorController,
}

/// Drives each hopper in the mode its controller's activity asks for, and reports what the
/// controller decides on: whether an idle is over, a jump is done, and an air jump is ready.
pub(super) fn jump_motor_system(
    collision_world: Option<Res<TileCollisionWorld>>,
    mut bodies: Query<JumpBody, (Without<Dead>, Without<Frozen>)>,
    players: Query<&Transform, (With<Player>, Without<JumpMotor>)>,
) {
    let tiles = collision_world.as_deref().filter(|world| world.is_ready());
    let player = players.iter().next().map(|transform| transform.translation.xy());
    for mut body in &mut bodies {
        let mode = actuation::jump_intent(body.controller.activity);
        if body.controller.entered {
            body.enter(mode, player);
        }
        let air_jump = body.run(mode, tiles, player);
        let idle_ready = mode == JumpMotorMode::Idle && body.motor.jump_ticks > body.controller.hopper.idle_ready_tick;
        let jump_done = body.motor.jump_done;
        let observations = &mut body.controller.observations;
        observations.idle_ready = idle_ready;
        observations.jump_done = jump_done;
        observations.air_jump = air_jump;
    }
}

impl JumpBodyItem<'_, '_> {
    fn enter(&mut self, mode: JumpMotorMode, player: Option<Vec2>) {
        match mode {
            JumpMotorMode::Idle => self.enter_idle(player),
            JumpMotorMode::Jump => self.enter_jump(player, false),
            JumpMotorMode::AirJump => self.enter_jump(player, true),
            JumpMotorMode::Fall => self.motor.settle(),
            JumpMotorMode::Death => {},
        }
    }

    /// Starts an idle. A chasing hopper cuts it short to leap at the player at once.
    fn enter_idle(&mut self, player: Option<Vec2>) {
        let position = self.transform.translation.xy();
        let motor = &mut *self.motor;
        let controller = &mut *self.controller;
        motor.jump_ticks = 0;
        motor.settle();
        let escape_distance = motor.config.anger.map_or(0.0, |anger| anger.escape_distance);
        let chasing = controller.hopper.enter_idle(&mut controller.rng_state, motor.config.idle_ticks, player, position,
            escape_distance);
        if chasing {
            motor.jump_ticks = controller.hopper.idle_ready_tick + 1;
            motor.jump_dir = player.map_or(motor.jump_dir, |player| (player.x - position.x).signum());
        }
    }

    /// Starts a jump; one from the ground also picks its direction.
    fn enter_jump(&mut self, player: Option<Vec2>, air: bool) {
        let x = self.transform.translation.x;
        let motor = &mut *self.motor;
        let controller = &mut *self.controller;
        motor.jump_ticks = 0;
        motor.jump_done = false;
        controller.hopper.air_jump_used |= air;
        controller.hopper.air_jump_checked |= air;
        if air {
            return;
        }
        motor.jump_dir = controller.hopper.direction(&mut controller.rng_state, player.map(|player| player.x - x));
        *self.facing = if motor.jump_dir >= 0.0 { Facing::Right } else { Facing::Left };
        motor.fall_ticks = 0;
    }

    /// Runs the body a tick in `mode`; returns whether an air jump is ready.
    fn run(&mut self, mode: JumpMotorMode, tiles: Option<&TileCollisionWorld>, player: Option<Vec2>) -> bool {
        match mode {
            JumpMotorMode::Idle => {
                self.velocity.0 = Vec2::ZERO;
                *self.nav = Navigation::Grounded;
                self.motor.jump_ticks = self.motor.jump_ticks.saturating_add(1);
            },
            JumpMotorMode::Jump | JumpMotorMode::AirJump => self.jump(mode == JumpMotorMode::AirJump, tiles, player),
            JumpMotorMode::Fall => {
                self.velocity.0.x = self.motor.jump_dir * self.motor.config.horizontal_speed;
                self.motor.fall(&mut self.velocity, &mut self.nav, false);
                return self.air_jump_ready();
            },
            JumpMotorMode::Death => {
                self.velocity.0 = Vec2::ZERO;
                *self.nav = Navigation::Grounded;
                self.motor.jump_done = false;
            },
        }
        false
    }

    /// Crouches through the jump's preparation, holding still on the ground or falling in the
    /// air, then rises along the jump curve until the jump ends, and falls.
    fn jump(&mut self, air: bool, tiles: Option<&TileCollisionWorld>, player: Option<Vec2>) {
        let preparing = self.motor.jump_ticks < self.motor.config.jump_prep_ticks;
        if preparing && air {
            self.motor.fall(&mut self.velocity, &mut self.nav, true);
        } else if preparing {
            self.velocity.0.y = 0.0;
            *self.nav = Navigation::Grounded;
        } else {
            self.leap(tiles, player);
        }
        self.motor.jump_ticks = self.motor.jump_ticks.saturating_add(1);
    }

    /// Rises along the jump curve until the jump ends, then falls.
    fn leap(&mut self, tiles: Option<&TileCollisionWorld>, player: Option<Vec2>) {
        self.velocity.0.x = self.motor.jump_dir * self.motor.config.horizontal_speed;
        let ascent = self.motor.jump_ticks - self.motor.config.jump_prep_ticks;
        let rise = if self.motor.jump_done { None } else { self.rise(ascent, tiles, player) };
        match rise {
            Some(speed) => {
                self.velocity.0.y = speed;
                self.motor.fall_ticks = 0;
                *self.nav = Navigation::Falling { fall_ticks: 0, jumping: true };
            },
            None => self.motor.fall(&mut self.velocity, &mut self.nav, false),
        }
    }

    /// The rise's speed `ascent` ticks into it, unless the jump ends there: cut short, stopped
    /// by the ceiling, or at the top of the jump curve.
    fn rise(&mut self, ascent: u32, tiles: Option<&TileCollisionWorld>, player: Option<Vec2>) -> Option<f32> {
        let speed = jump_velocity(ascent);
        let cut_short = self.cut_short(ascent, player);
        let half_extents = self.collider.half_extents();
        let blocked = tiles.filter(|_| speed > 0.0 && half_extents.min_element() > 0.0).is_some_and(|tiles| {
            let position = self.transform.translation.xy();
            tiles.resolve_axis(Axis::Y, position, speed + GROUNDED_THRESHOLD, half_extents).1
        });
        let speed = if blocked { 0.0 } else { speed };
        let top = (speed <= 0.0 && ascent > 1) || ascent + 1 >= JUMP_VELOCITIES.len() as u32;
        self.motor.jump_done = cut_short || blocked || top;
        (!self.motor.jump_done).then_some(speed)
    }

    /// Whether the jump is cut short `ascent` ticks into its rise: past its lock, at random and
    /// likelier the later; or, for an angry hopper, once it has passed the player.
    fn cut_short(&mut self, ascent: u32, player: Option<Vec2>) -> bool {
        let last_ascent = (JUMP_VELOCITIES.len() as u32).saturating_sub(1).max(1);
        let chance = (ascent.min(last_ascent) * 178 / last_ascent) as u8;
        let at_random = ascent >= self.motor.config.jump_cancel_lock_ticks
            && chance > 0
            && ((next_random(&mut self.controller.rng_state) >> 24) as u8) < chance;
        let Some(target) = player.map(|player| player.x) else {
            return at_random;
        };
        let x = self.transform.translation.x;
        let dir = self.motor.jump_dir;
        let beyond = if dir > 0.0 { x >= target } else { x <= target };
        let passed = beyond || (target - x).signum() != dir;
        at_random || (self.controller.hopper.anger_active && ascent >= 10 && passed)
    }

    /// Whether the falling hopper may jump again in the air; if so, its next jump starts
    /// afresh.
    fn air_jump_ready(&mut self) -> bool {
        let controller = &mut *self.controller;
        let ready = controller.hopper.try_air_jump(&mut controller.rng_state, self.motor.fall_ticks);
        if ready {
            self.motor.jump_done = false;
            self.motor.jump_ticks = 0;
        }
        ready
    }
}
