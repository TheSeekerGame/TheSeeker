use theseeker_engine::animation::SpriteAnimationBundle;
use crate::game::actors::motion::{
    MotionBlockedBehavior,
};
use theseeker_engine::assets::animation::SpriteAnimation;

use bevy::ecs::change_detection::Mut;
use bevy::camera::visibility::RenderLayers;
use theseeker_engine::effects::{
    FadeCurve, GhostColorMode, GhostMovement, GhostingSource, ScaleCurve,
};
use theseeker_engine::gent::{Gent, GentPhysicsBundle, TransformGfxFromGent};
use theseeker_engine::physics::{
    geometry::ProbeDirection, inside::EnemyInsidePlayer as Inside, Collider, CollisionGroups, Group,
    LinearVelocity, PhysicsSet, PhysicsWorld, TileCollisionWorld, ENEMY_INSIDE, GROUNDED_THRESHOLD,
};
use theseeker_engine::script::ScriptPlayer;

use super::physics::Knockback;
use super::player::Player;
use crate::game::effects::stealthed::StealthEffect;
use crate::game::physics::{gravity_velocity, jump_velocity, JUMP_VELOCITIES};
use crate::game::player::skills::portable_tent::TentInteriorOccupant;
use theseeker_engine::profile::TickProfiler;
use crate::graphics::pixel_perfect::pixel_perfect_layers;

use crate::game::combat::{
    DamageInfo, DamageWindow, Health, Invulnerable, InvulnerabilitySource,
    KillCount,
};
use crate::game::gentstate::*;
use crate::graphics::projectile_particles::ArcParticleEffectHandle;
use crate::levelgen::foliage::FoliageInteractor;

use crate::prelude::*;

const DEFAULT_DEATH_TICKS: u32 = 8 * 7;
const ENEMY_FALL_VELOCITIES: &[f32] =
    crate::game::physics::GRAVITY_VELOCITIES;

use crate::game::actors::sensors::{
    AiTarget, AiTargetInvisible, GroundedCheck, HealthCheck,
};
use crate::game::actors::{
    AiPaused, DistanceJitter, ActorController, PatrolTimeScale, TargetSensor, TurnCooldown,
};

mod actuation;
#[cfg(test)]
mod integration_tests;
use actuation::enemy_actor_actuator;
mod actor_descriptors;
mod actor_setup;
mod animation_lifecycle;
mod collision;
mod contact_damage;
mod corpse_loot;
mod death;
mod flying_motor;
mod ground_motor;
mod jump_motor;
mod projectiles;
mod spawn;
mod tentacle;
mod trap_controller;
use actor_descriptors::{
    move_flying_actor_with_wall_nudge,
    spawn_attached_enemy_damage_source, spawn_enemy_attack_from_descriptor,
    spawn_enemy_projectile_from_descriptor, AttachedEnemyDamageSourceSpawn,
    EnemyAttackSpawn, EnemyProjectileSpawn,
    ProjectileCache,
};
pub use actor_setup::spawn_enemy_actor_blueprint;
use actor_setup::{setup_enemy_actor, spawn_spider_actor_blueprint, SpiderActorSetup};
use animation_lifecycle::EnemyAnimationPlugin;
use contact_damage::{
    contact_damage_system, ContactDamage, ContactPushback,
};
use collision::{enemy_gravity, move_collide, remove_inside};
use corpse_loot::{activate_corpse_loot, CorpseLootPlugin};
pub(crate) use corpse_loot::{
    corpse_loot_distance_squared, CorpseLoot,
};
pub(crate) use death::dead;
pub(crate) use death::Decay;
pub use death::OpiTombstone;
use flying_motor::{
    bat_death_fall, flying_attack_damage_events, flying_motor_system,
    flying_move_collide, health_regen, rotate_void_gfx, set_flying_mode,
    FlyingMotor,
};
use ground_motor::{
    ground_motion_motor, sync_defense_state, update_ground_roll_observations,
    GroundMotor, MovementState, MovementType,
};
use jump_motor::{
    jump_motor_system, jump_motor_anger_trigger,
    jump_motor_move_collide,
};
use projectiles::EnemyProjectileRuntimePlugin;
pub use spawn::{SpiderBiome, Tier};
use spawn::Role;
use trap_controller::{
    stationary_trap_controller, stationary_trap_detection_sensor,
};
pub use tentacle::{Tentacle, TentaclePlugin};
use tentacle::TentacleState;

pub(crate) use spawn::reset_enemy_spawn_runtime;

pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    #[expect(
        clippy::too_many_lines,
        reason = "This Bevy plugin method declares enemy system registration and schedule ordering in one auditable block."
    )]
    fn build(&self, app: &mut App) {
        app.init_resource::<spawn::EnemySpawnRuntime>();
        app.configure_sets(
            GameTickUpdate,
            (
                EnemyStateSet::Sensors.after(PhysicsSet),
                EnemyStateSet::Decisions.after(EnemyStateSet::Sensors),
                EnemyStateSet::Actuator.after(EnemyStateSet::Decisions),
                EnemyStateSet::Death.after(EnemyStateSet::Actuator),
                EnemyStateSet::Transition.after(EnemyStateSet::Death),
                EnemyStateSet::Animation.after(EnemyStateSet::Transition),
                EnemyStateSet::Collisions.after(EnemyStateSet::Animation),
            )
                .chain()
                .run_if(in_state(AppState::InGame)),
        );

        app.configure_sets(
            GameTickUpdate,
            EnemyStateSet::Sensors
                .after(theseeker_engine::animation::AnimationSet::Signals),
        );

        app.add_systems(
            GameTickUpdate,
            setup_enemy_actor
                .run_if(in_state(GameState::Playing))
                .before(EnemyStateSet::Transition)
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            Update,
            spawn::build_enemy_spawn_catalog
                .after(crate::levelgen::sync_tile_collision_world)
                .in_set(GameTickSet::Pre)
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            GameTickUpdate,
            spawn::run_enemy_spawn_director
                .run_if(not(resource_exists::<crate::game::actors::population_probe::PopulationProbe>))
                .before(EnemyStateSet::Sensors)
                .run_if(in_state(GameState::Playing))
                .run_if(in_state(AppState::InGame)),
        );
        app.add_plugins((
            EnemyRuntimePlugin,
            EnemyAnimationPlugin,
            CorpseLootPlugin,
            EnemyProjectileRuntimePlugin,
            TentaclePlugin,
        ));
        app.register_type::<Role>();
        app.register_type::<Navigation>();

        app.add_systems(
            GameTickUpdate,
            (
                mark_player_as_target,
                update_player_target_visibility,
            )
                .before(EnemyStateSet::Sensors)
                .run_if(in_state(AppState::InGame)),
        );

        app.add_systems(
            GameTickUpdate,
            contact_damage_system
                .after(EnemyStateSet::Collisions)
                .before(crate::game::combat::damage_source::determine_damage_targets)
                .run_if(in_state(AppState::InGame)),
        );

        app.add_systems(
            GameTickUpdate,
            jump_motor_anger_trigger
                .after(crate::game::combat::damage_source::apply_damage)
                .run_if(in_state(AppState::InGame)),
        );

        app.add_systems(
            GameTickPost,
            rotate_void_gfx
                .after(theseeker_engine::gent::transform_gfx_from_gent)
                .in_set(theseeker_engine::presentation::PresentationSet::Prepare)
                .run_if(in_state(AppState::InGame)),
        );

        {
            use crate::game::actors::sensors::*;

            app.add_systems(
                GameTickUpdate,
                (
                    sensor_target,
                    update_ground_roll_observations,
                    sensor_ground::<Navigation>,
                    sensor_range,
                    stationary_trap_detection_sensor,
                    sensor_health::<Health>,
                    update_sensor_history,
                    (
                        update_perceived_sensors,
                        copy_actual_to_perceived,
                    ),
                )
                    .chain() // Ensure history is updated before perceived sensors
                    .in_set(EnemyStateSet::Sensors)
                    .run_if(in_state(AppState::InGame)),
            );

            app.add_systems(
                GameTickUpdate,
                initialize_sensor_history
                    .after(setup_enemy_actor)
                    .before(EnemyStateSet::Sensors)
                    .run_if(in_state(AppState::InGame)),
            );

            app.add_systems(
            GameTickUpdate,
            (
                enemy_actor_actuator,
                stationary_trap_controller.before(
                    crate::game::combat::damage_source::determine_damage_targets,
                ),
                ground_motion_motor,
                flying_motor_system,
                sync_defense_state,
                jump_motor_system,
                actuation::finish_actor_tick,
            )
                    .chain()
                    .in_set(EnemyStateSet::Actuator)
                    .run_if(in_state(AppState::InGame)),
            );
        }
    }
}

fn sync_motion_facing(facing: &mut Facing, velocity: Vec2) {
    if velocity.x.abs() <= f32::EPSILON {
        return;
    }

    *facing = if velocity.x > 0.0 {
        Facing::Right
    } else {
        Facing::Left
    };
}

fn random_patrol_candidate(
    home: Vec2,
    controller: &mut ActorController,
    min_radius: f32,
    max_radius: f32,
) -> Vec2 {
    let angle = actor_rand_unit(controller) * std::f32::consts::TAU;
    let radius_span = (max_radius - min_radius).max(0.0);
    let radius = min_radius + actor_rand_unit(controller) * radius_span;
    home + Vec2::from_angle(angle) * radius
}

fn first_patrol_target(
    controller: &mut ActorController,
    attempts: u8,
    mut candidate: impl FnMut(&mut ActorController) -> Vec2,
    mut accept: impl FnMut(Vec2) -> bool,
) -> Option<Vec2> {
    for _ in 0..attempts {
        let candidate = candidate(controller);
        if accept(candidate) {
            return Some(candidate);
        }
    }
    None
}

fn despawn_entity_if_exists(commands: &mut Commands, entity: Entity) {
    // Another queued cleanup can remove the actor or its hierarchy first.
    commands.entity(entity).try_despawn();
}

fn despawn_enemy_with_gfx(
    commands: &mut Commands,
    entity: Entity,
    gent: &Gent,
) {
    despawn_entity_if_exists(commands, gent.e_gfx);
    despawn_entity_if_exists(commands, gent.e_effects_gfx);
    despawn_entity_if_exists(commands, entity);
}

#[derive(SystemSet, Clone, PartialEq, Eq, Debug, Hash)]
pub enum EnemyStateSet {
    Sensors,    // Gather world info into sensor components
    Decisions,  // Select activity from typed observations
    Actuator,   // Apply action and locomotion requests
    Death,      // Retain and tick enemies that have entered death
    Collisions, // Physics resolution
    Transition, // State transition handling
    Animation,  // Animation updates
}

#[derive(Component)]
pub struct Enemy;

#[derive(Component)]
pub struct EnemyGfx;

#[derive(Component)]
pub struct NoXpDrops;

struct EnemyRuntimePlugin;

impl Plugin for EnemyRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                spawn::release_dead_spawn_leases.before(dead),
                dead.run_if(any_with_component::<Dead>),
            )
                .in_set(EnemyStateSet::Death)
                .run_if(in_state(AppState::InGame)),
        );

        app.add_systems(
            GameTickUpdate,
            (
                enemy_gravity, // Pure gravity application
                crate::game::physics::knockback
                    .run_if(any_with_component::<Knockback>),
                jump_motor_move_collide,
                move_collide,
                flying_move_collide,
                bat_death_fall,
                remove_inside,
                health_regen,
            )
                .chain() // Chain all physics systems to ensure proper ordering
                .in_set(EnemyStateSet::Collisions)
                .run_if(in_state(AppState::InGame)),
        );

        app.add_systems(
            GameTickUpdate,
            flying_attack_damage_events
                .after(crate::game::combat::damage_source::apply_damage)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

#[derive(Component, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct Defense;

#[derive(Component)]
pub struct LargeFreezeTarget;


#[derive(Component)]
struct DefenseInvulnerabilityTiming {
    from_frame: u32,
    anim_key: Option<String>,
}

#[derive(Component, Reflect, Debug)]
enum Navigation {
    Grounded,
    Falling {
        fall_ticks: u32, // Tick counter for velocity curve
        jumping: bool,   // Keep existing field
    },
    Blocked,
}

fn actor_lcg_next(state: &mut u32) -> u32 {
    *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    *state
}

fn actor_rand_unit(controller: &mut ActorController) -> f32 {
    actor_lcg_next(&mut controller.rng_state) as f32 / u32::MAX as f32
}

fn actor_rand_range(controller: &mut ActorController, min: u16, max: u16) -> u16 {
    if max <= min {
        return min;
    }
    let span = (max - min + 1) as u32;
    (actor_lcg_next(&mut controller.rng_state) % span + min as u32) as u16
}

fn set_enemy_anim_slot(
    gfx_query: &mut Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
    gent: &Gent,
    slot: &str,
    value: bool,
) {
    if let Ok(mut anim) = gfx_query.get_mut(gent.e_gfx) {
        anim.set_slot(slot, value);
    }
}

fn curve_velocity(curve: &[f32], tick: u32) -> f32 {
    let Some(last) = curve.last().copied() else {
        return 0.0;
    };
    curve.get(tick as usize).copied().unwrap_or(last)
}

impl GroundedCheck for Navigation {
    fn is_grounded(&self) -> bool {
        matches!(self, Navigation::Grounded)
    }
}

impl HealthCheck for Health {
    fn is_zero(&self) -> bool {
        self.current == 0
    }
}

pub fn mark_player_as_target(
    mut commands: Commands,
    query: Query<Entity, (With<Player>, Without<AiTarget>)>,
) {
    for entity in query.iter() {
        commands.entity(entity).insert(AiTarget);
    }
}

pub fn update_player_target_visibility(
    mut commands: Commands,
    player_query: Query<
        (
            Entity,
            Has<StealthEffect>,
            Has<TentInteriorOccupant>,
            Has<AiTargetInvisible>,
        ),
        With<Player>,
    >,
) {
    for (entity, is_stealthed, is_inside_tent, is_invisible) in
        player_query.iter()
    {
        let should_be_invisible = is_stealthed || is_inside_tent;
        match (should_be_invisible, is_invisible) {
            (true, false) => {
                commands.entity(entity).insert(AiTargetInvisible);
            },
            (false, true) => {
                commands.entity(entity).remove::<AiTargetInvisible>();
            },
            _ => {}
        }
    }
}

#[cfg(test)]
mod collision_tests;

pub(crate) fn spawn_catalog_ready(world: &World) -> bool {
    world.contains_resource::<spawn::EnemySpawnCatalog>()
}
