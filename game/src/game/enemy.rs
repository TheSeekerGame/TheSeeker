#[cfg(feature = "dev")]
use bevy_inspector_egui::quick::FilterQueryInspectorPlugin;
use rand::distr::StandardUniform;
use rand::thread_rng;
use theseeker_engine::animation::SpriteAnimationBundle;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::ballistics_math::ballistic_speed;
use theseeker_engine::gent::{Gent, GentPhysicsBundle, TransformGfxFromGent};
use theseeker_engine::physics::{
    into_vec2, update_sprite_colliders, AnimationCollider, Collider,
    LinearVelocity, PhysicsWorld, ShapeCaster, ENEMY, ENEMY_ATTACK,
    ENEMY_INSIDE, GROUND, PLAYER, PLAYER_ATTACK, SENSOR, CollisionGroups, Group, GROUNDED_THRESHOLD, GROUND_BUFFER,
    ColliderShapeAccess,
};
use bevy_rapier2d::rapier::prelude::SharedShape;
use bevy_rapier2d::prelude::ShapeCastStatus;
use theseeker_engine::script::ScriptPlayer;

use super::physics::Knockback;
use super::player::player_weapon::CurrentWeapon;
use super::player::{Player, PlayerConfig, StatusModifier, Stealthing};
use crate::game::attack::arc_attack::Projectile;
use crate::game::attack::particles::ArcParticleEffectHandle;
use crate::game::attack::*;
use crate::game::gentstate::*;
use crate::game::player::EnemiesNearby;
use crate::graphics::particles_util::BuildParticles;
use crate::prelude::*;
use theseeker_engine::physics::inside::EnemyInsidePlayer as Inside;

use theseeker_engine::ai::{FsmInstance, TargetSensor, GroundSensor, RangeSensor, TurnCooldown, HealthSensor};
use theseeker_engine::ai::sensors::{CachedArchetypeStats, AiTarget, AiTargetInvisible, GroundedCheck, HealthCheck};

pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        // Schedule ordering: Sensors → Brain → Actuator → legacy systems
        app.configure_sets(
            GameTickUpdate,
            (
                EnemyStateSet::Sensors,
                EnemyStateSet::Brain.after(EnemyStateSet::Sensors),
                EnemyStateSet::Actuator.after(EnemyStateSet::Brain),
                EnemyStateSet::Behavior.after(EnemyStateSet::Actuator),
                EnemyStateSet::Transition.after(EnemyStateSet::Behavior),
                EnemyStateSet::Animation.after(EnemyStateSet::Transition),
                EnemyStateSet::Collisions.after(EnemyStateSet::Animation),
            ).chain().run_if(in_state(AppState::InGame)),
        );
        
        // Ensure sensors run after animation loop detection but before loop clear
        app.configure_sets(
            GameTickUpdate,
            EnemyStateSet::Sensors
                .after(theseeker_engine::animation::AnimationSet::LoopDetection)
                .before(theseeker_engine::animation::AnimationSet::LoopClear),
        );
        
        app.add_systems(
            GameTickUpdate,
            (setup_enemy.run_if(in_state(GameState::Playing)))
                .before(EnemyStateSet::Transition)
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            GameTickUpdate,
            spawn_enemies.after(setup_enemy),
        );
        app.insert_resource(EnemyConfig::default());
        app.add_plugins((
            EnemyBehaviorPlugin,
            EnemyAnimationPlugin,
        ));
        app.register_type::<Role>();
        app.register_type::<Navigation>();

        // Add player marking systems
        app.add_systems(
            GameTickUpdate,
            (mark_player_as_target, update_player_target_visibility)
                .before(EnemyStateSet::Sensors)
                .run_if(in_state(AppState::InGame)),
        );

        {
            use theseeker_engine::ai::systems::ai_brain_system;
            use theseeker_engine::ai::sensors::*;

            // Sensor systems gather world info → sensor components
            app.add_systems(
                GameTickUpdate,
                (
                    sensor_target,
                    sensor_ground::<Navigation>,
                    sensor_range,
                    sensor_health::<Health>,
                    sensor_slots,
                    sensor_reset_timer_on_anim_loop,
                )
                    .in_set(EnemyStateSet::Sensors)
                    .run_if(in_state(AppState::InGame)),
            );
            
            // Trigger state actions for newly spawned enemies
            app.add_systems(
                GameTickUpdate,
                trigger_initial_state_actions
                    .after(setup_enemy)
                    .before(EnemyStateSet::Sensors)
                    .run_if(in_state(AppState::InGame)),
            );

            // Brain evaluates FSM rules → queues actions
            app.add_systems(
                GameTickUpdate,
                ai_brain_system
                    .in_set(EnemyStateSet::Brain)
                    .run_if(in_state(AppState::InGame)),
            );

            // Actuator executes queued actions → world changes
            app.add_systems(
                GameTickUpdate,
                (enemy_ai_actuator_game, clear_projectile_cache_on_death)
                    .chain()
                    .in_set(EnemyStateSet::Actuator)
                    .run_if(in_state(AppState::InGame)),
            );
        }
    }
}

#[derive(Resource, Debug)]
struct EnemyConfig {
    fall_accel: f32,
    start_hp: u32,
    range_melee_melee: f32,
    range_melee_aggro: f32,
}

// Fallback defaults when archetype stats missing
impl Default for EnemyConfig {
    fn default() -> Self {
        Self {
            fall_accel: 4.5,
            start_hp: 100,
            range_melee_melee: 12.0,
            range_melee_aggro: 100.0,
        }
    }
}

#[derive(SystemSet, Clone, PartialEq, Eq, Debug, Hash)]
pub enum EnemyStateSet {
    Sensors,    // Gather world info into sensor components
    Brain,      // Evaluate FSM rules using sensor data
    Actuator,   // Execute actions from FSM
    Behavior,   // Legacy behavior systems
    Collisions, // Physics resolution
    Transition, // State transition handling
    Animation,  // Animation updates
}

#[derive(Bundle, LdtkEntity, Default)]
pub struct EnemyBlueprintBundle {
    pub(crate) marker: EnemyBlueprint,
}

#[derive(Bundle, LdtkEntity, Default)]
pub struct EnemySpawnerBundle {
    marker: EnemySpawner,
}

/// Enemy spawner, cooldown starts ticking once all spawned enemies have been killed
#[derive(Component, Default, Debug)]
pub struct EnemySpawner {
    // slots for enemies to spawn, shouldnt grow past EnemySpawner::MAX
    pub slots: Vec<SpawnSlot>,
    // tracks total killed enemies of this spawner
    pub killed: u32,
    // clears, used to track when to next upgrade
    pub clears: u32,
    // after an upgrade, how many more clears till the next upgrade
    pub threshold_next: u32,
    // cooldown increases to EnemySpawner::COOLDOWN before spawning new batch of enemies
    pub cooldown_ticks: u32,
    pub spawn_state: SpawnerState,
    // the next slot to buff
    pub next_buff_index: usize,
}

impl EnemySpawner {
    /// Maximum number of enemy slots per spawner
    const MAX: usize = 4;
    /// Distance from player required to spawn enemies (in pixels)
    const RANGE: f32 = 320.;
    /// Ticks to wait before spawning next wave after clearing
    const COOLDOWN: u32 = 8 * 8;

    fn is_cleared(&self) -> bool {
        self.slots
            .iter()
            .filter(|x| x.enemy.is_some())
            .collect::<Vec<_>>()
            .is_empty()
    }
}

#[derive(Default, Debug)]
pub enum SpawnerState {
    #[default]
    // Add slots or upgrade the Tier of the next slot
    Upgrade,
    // Ready to spawn
    Ready,
    Spawned,
    Cooldown,
}

#[derive(Debug)]
pub struct SpawnSlot {
    pub enemy: Option<Entity>,
    pub tier: Tier,
}

#[derive(Component, Default)]
pub struct EnemyBlueprint;

#[derive(Bundle)]
pub struct EnemyGentBundle {
    enemy: Enemy,
    marker: Gent,
    phys: GentPhysicsBundle,
}

#[derive(Component)]
pub struct Enemy;

#[derive(Bundle)]
pub struct EnemyGfxBundle {
    marker: EnemyGfx,
    gent2gfx: TransformGfxFromGent,
    sprite: Sprite,
    transform: Transform,
    global_transform: GlobalTransform,
    visibility: Visibility,
    inherited_visibility: InheritedVisibility,
    view_visibility: ViewVisibility,
    animation: SpriteAnimationBundle,
}

#[derive(Bundle)]
pub struct EnemyEffectsGfxBundle {
    marker: EnemyEffectGfx,
    gent2gfx: TransformGfxFromGent,
    sprite: Sprite,
    transform: Transform,
    global_transform: GlobalTransform,
    visibility: Visibility,
    inherited_visibility: InheritedVisibility,
    view_visibility: ViewVisibility,
    animation: SpriteAnimationBundle,
}

#[derive(Component)]
pub struct EnemyGfx {
    e_gent: Entity,
}
#[derive(Component)]
pub struct EnemyEffectGfx {
    e_gent: Entity,
}

// Spawner scaling occurs automatically once all enemies are cleared.
// when ranged should be capped at 2 per spawner
// only tick cooldown when spawner is cleared
fn spawn_enemies(
    mut spawner_q: Query<(&Transform, &mut EnemySpawner)>,
    // dead enemies to clear
    enemy_q: Query<
        Entity,
        (
            With<Enemy>,
            With<Dead>,
            Without<EnemySpawner>,
        ),
    >,
    player_query: Query<&Transform, (Without<Enemy>, With<Player>)>,
    mut commands: Commands,
) {
    let p_transform = player_query.get_single();
    for (transform, mut spawner) in spawner_q.iter_mut() {
        let mut killed = spawner.killed;

        // check if enemies are dead and update kill count
        for slot in spawner.slots.iter_mut() {
            if let Some(enemy) = slot.enemy {
                // clear dead enemy
                if enemy_q.get(enemy).is_ok() {
                    slot.enemy = None;
                    killed += 1;
                }
            }
        }
        spawner.killed = killed;

        loop {
            match spawner.spawn_state {
                SpawnerState::Upgrade => {
                    // set number of clears till next upgrade 2 or 3
                    spawner.threshold_next = 1;
                    // spawner.threshold_next = thread_rng().gen_range(2..4);
                    // add a slot
                    if spawner.slots.len() < EnemySpawner::MAX {
                        spawner.slots.push(SpawnSlot {
                            enemy: None,
                            tier: Tier::Base,
                        })
                    // or increase tier of the next slot
                    } else {
                        let i = spawner.next_buff_index % EnemySpawner::MAX;
                        if let Some(mut slot_to_buff) = spawner.slots.get_mut(i)
                        {
                            slot_to_buff.tier = match slot_to_buff.tier {
                                Tier::Base => Tier::Two,
                                Tier::Two => Tier::Three,
                                Tier::Three => Tier::Three,
                            };
                            spawner.next_buff_index += 1;
                        }
                    }
                    spawner.spawn_state = SpawnerState::Ready;
                },
                SpawnerState::Ready => {
                    if if let Ok(ptrans) = p_transform {
                        transform
                            .translation
                            .truncate()
                            .distance(ptrans.translation.truncate())
                            > EnemySpawner::RANGE
                    } else {
                        true
                    } {
                        let mut ranged_role = 0;
                        for slot in spawner.slots.iter_mut() {
                            // generate a random roll, max 2 per spawner
                            let role = if ranged_role < 2 {
                                let r = Role::random();
                                if matches!(r, Role::Ranged) {
                                    ranged_role += 1;
                                };
                                r
                            } else {
                                Role::Melee
                            };

                            let e = commands
                                .spawn((
                                    EnemyBlueprintBundle::default(),
                                    slot.tier,
                                    role,
                                    *transform,
                                    GlobalTransform::default(),
                                ))
                                .id();
                            slot.enemy = Some(e);
                        }
                        spawner.spawn_state = SpawnerState::Spawned;
                    } else {
                        break;
                    }
                },
                SpawnerState::Spawned => {
                    if spawner.is_cleared() {
                        spawner.spawn_state = SpawnerState::Cooldown;
                        spawner.clears += 1;
                    } else {
                        break;
                    };
                },
                SpawnerState::Cooldown => {
                    spawner.cooldown_ticks += 1;
                    if spawner.cooldown_ticks >= EnemySpawner::COOLDOWN {
                        spawner.cooldown_ticks = 0;
                        if spawner.clears >= spawner.threshold_next {
                            spawner.clears = 0;
                            spawner.spawn_state = SpawnerState::Upgrade;
                        } else {
                            spawner.spawn_state = SpawnerState::Ready;
                        }
                    } else {
                        break;
                    }
                },
            }
        }
    }
}

fn setup_enemy(
    mut q: Query<(
        &mut Transform,
        &Tier,
        &Role,
        Entity,
        Ref<EnemyBlueprint>,
    )>,
    mut commands: Commands,
    enemy_config: Res<EnemyConfig>,
    _arch_assets: Res<Assets<theseeker_engine::ai::EnemyArchetype>>,
    _preloaded: Res<PreloadedAssets>,
    fsm_assets: Res<Assets<theseeker_engine::ai::CompiledFsm>>,
    level_seed: Res<theseeker_engine::ai::LevelSeed>,
) {
    for (mut xf_gent, tier, role, e_gent, bp) in q.iter_mut() {
        if !bp.is_added() {
            continue;
        }
        // Z-ordering layers:
        // - Base: 0.000014 (below player at 0.000015)
        // - +0.0000001 for melee (in front of ranged)
        // - +0.00000001/2 for higher tiers
        xf_gent.translation.z = 14.0 * 0.000001;
        // Make melee spiders appear in front of ranged ones
        if let Role::Melee = role {
            xf_gent.translation.z += 0.0000001;
        }
        // Make higher tier spiders appear in front of lower tier ones
        xf_gent.translation.z += match tier {
            Tier::Base => 0.0,
            Tier::Two => 0.00000001,
            Tier::Three => 0.00000002,
        };
        xf_gent.translation.y += 2.0; // Sprite offset so it looks like it is standing on the ground
        // Resolve health from archetype stats, fallback to config * tier
        let expected_id = format!("spider_{}{}",
            if matches!(role, Role::Melee) { "small" } else { "big" },
            match tier {
                Tier::Base => "",
                Tier::Two => "_t2",
                Tier::Three => "_t3",
            }
        );

        let health = _arch_assets
            .iter()
            .find(|(_, arch)| arch.id == expected_id)
            .and_then(|(_, arch)| arch.stats.as_ref().map(|s| s.spawn_hp as u32))
            .unwrap_or(enemy_config.start_hp * *tier as u32);
        let e_gfx = commands.spawn(()).id();
        let e_effects_gfx = commands.spawn(()).id();

        
        {
            // Base components that are always needed
            commands.entity(e_gent).insert((
                Name::new("Enemy"),
                Enemy,
                Gent {
                    e_gfx,
                    e_effects_gfx,
                },
                GentPhysicsBundle {
                    collider: Collider::cuboid(4.0, 2.5),
                    shapecast: ShapeCaster {
                        shape: SharedShape::cuboid(22.0, 10.0),
                        direction: Dir2::NEG_Y,
                        origin: Vec2::new(0.0, -2.0),
                        max_toi: 0.0,
                        interaction: CollisionGroups::new(ENEMY, GROUND),
                    },
                    linear_velocity: LinearVelocity(Vec2::ZERO),
                },
                theseeker_engine::physics::groups::enemy_body(),
                Navigation::Grounded,
                Health {
                    current: health,
                    max: health,
                },
                // Random initial facing for patrol variety
                if rand::thread_rng().gen_bool(0.5) { Facing::Left } else { Facing::Right },
                StateDespawnMarker,
                // Keep Role and Tier for death/decay animations and other systems
                *role,
                *tier,
            ));
            
            // Add the new AI components via ScriptBundle
            if let Some(bundle) = theseeker_engine::ai::ScriptBundle::from_arch(
                &expected_id,
                e_gent,
                &_arch_assets,
                &fsm_assets,
                &_preloaded,
                level_seed.0,
            ) {
                commands.entity(e_gent).insert(bundle);
                
                // Cache archetype stats to avoid per-frame asset lookups
                let arch = _arch_assets
                    .iter()
                    .find(|(_, a)| &a.id == &expected_id)
                    .map(|(_, a)| a);
                    
                let cached_stats = if let Some(arch) = arch {
                    let stats = arch.stats.as_ref();
                    theseeker_engine::ai::sensors::CachedArchetypeStats {
                        vision_range: stats.map(|s| s.vision_range).filter(|&v| v > 0.0).unwrap_or(enemy_config.range_melee_aggro),
                        melee_range: stats.map(|s| s.melee_range).filter(|&v| v > 0.0).unwrap_or(enemy_config.range_melee_melee),
                        fall_accel: stats.map(|s| s.fall_accel).unwrap_or(enemy_config.fall_accel),
                        needs_line_of_sight: arch.id.starts_with("spider_big"),
                    }
                } else {
                    // Fallback stats
                    theseeker_engine::ai::sensors::CachedArchetypeStats {
                        vision_range: enemy_config.range_melee_aggro,
                        melee_range: enemy_config.range_melee_melee,
                        fall_accel: enemy_config.fall_accel,
                        needs_line_of_sight: matches!(role, Role::Ranged),
                    }
                };
                commands.entity(e_gent).insert(cached_stats);
                
                // Add ProjectileCache for ranged enemies to optimize ballistic calculations
                if matches!(role, Role::Ranged) {
                    commands.entity(e_gent).insert(ProjectileCache::new());
                }
            } else {
                error!("Failed to create ScriptBundle for archetype: {}", expected_id);
            }
        }
        
        // Create the sprite WITHOUT an initial animation - let FSM handle it
        let sprite_animation = ScriptPlayer::<SpriteAnimation>::default();
        
        commands.entity(e_gfx).insert((
            EnemyGfxBundle {
                marker: EnemyGfx { e_gent },
                gent2gfx: TransformGfxFromGent {
                    pixel_aligned: false,
                    gent: e_gent,
                    offset: Some(Vec3::new(0.0, 5.0, 0.0)),
                },
                sprite: Sprite {
                    texture_atlas: Some(TextureAtlas::default()),
                    ..Default::default()
                },
                transform: *xf_gent,
                global_transform: GlobalTransform::default(),
                visibility: Visibility::Visible,
                inherited_visibility: InheritedVisibility::VISIBLE,
                view_visibility: ViewVisibility::default(),
                animation: SpriteAnimationBundle { player: sprite_animation },
            },
            StateDespawnMarker,
        ));
        let mut animation = ScriptPlayer::<SpriteAnimation>::default();
        animation.play_key("anim.spider.Sparks");
        commands.entity(e_effects_gfx).insert((
            EnemyEffectsGfxBundle {
                marker: EnemyEffectGfx { e_gent },
                gent2gfx: TransformGfxFromGent {
                    pixel_aligned: false,
                    gent: e_gent,
                    offset: Some(Vec3::new(0.0, 2.5, 0.0)),
                },
                sprite: Sprite {
                    texture_atlas: Some(TextureAtlas::default()),
                    ..Default::default()
                },
                transform: xf_gent.with_translation(Vec3::new(0., 0., 1.)),
                global_transform: GlobalTransform::default(),
                visibility: Visibility::Visible,
                inherited_visibility: InheritedVisibility::VISIBLE,
                view_visibility: ViewVisibility::default(),
                animation: SpriteAnimationBundle { player: animation },
            },
            StateDespawnMarker,
        ));
        commands.entity(e_gent).remove::<EnemyBlueprint>();
    }
}

struct EnemyBehaviorPlugin;

impl Plugin for EnemyBehaviorPlugin {
    fn build(&self, app: &mut App) {
        // Death & decay systems (shared by both AI approaches)
        app.add_systems(
            GameTickUpdate,
                (
                    decay_despawn.run_if(any_with_component::<Decay>),
                    dead.run_if(any_with_component::<Dead>),
            )
                .run_if(in_state(AppState::InGame))
                .in_set(EnemyStateSet::Behavior),
        );
        
        // Physics systems
        app.add_systems(
            GameTickUpdate,
            (
                enemy_gravity,  // Pure gravity application
                move_collide,
                remove_inside,
            )
                .chain()  // Chain all physics systems to ensure proper ordering
                .in_set(EnemyStateSet::Collisions)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

// Defense component is used by attack system for damage reduction
// Keep it available for both old and new AI
#[derive(Component, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct Defense;

#[derive(Component, Reflect)]
enum Navigation {
    Grounded,
    Falling { jumping: bool },
    Blocked,
}

#[derive(Component, Reflect, Copy, Clone)]
enum Role {
    Melee,
    Ranged,
}

impl Role {
    fn random() -> Role {
        let mut rng = rand::thread_rng();
        rng.gen()
    }
}

// Spider upgrade/scaling tier
#[derive(Component, Default, Debug, Reflect, Clone, Copy)]
pub enum Tier {
    #[default]
    Base = 1,
    Two = 3,
    Three = 9,
}

impl Distribution<Role> for StandardUniform {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Role {
        let index: u8 = rng.gen_range(0..=1);
        match index {
            0 => Role::Melee,
            1 => Role::Ranged,
            _ => unreachable!(),
        }
    }
}

/// Component added to Decaying enemy
/// not a GentState because it shouldnt transition to or from anything else
#[derive(Component)]
struct Decay;

/// Increments the global KillCount, removes most components from the Enemy
/// after a set amount of ticks transitions to the Decay state
pub fn dead(
    mut query: Query<(Entity, &mut Dead), With<Enemy>>,
    mut kill_count: ResMut<KillCount>,
    mut commands: Commands,
) {
    for (entity, mut dead) in query.iter_mut() {
        if dead.ticks == 0 {
            **kill_count += 1;
            commands.entity(entity).retain::<(
                Transform,
                GlobalTransform,
                Dead,
                Enemy,
                Role,
                Tier,
                Gent,
            )>();
        }
        if dead.ticks == 8 * 7 {
            commands.entity(entity).remove::<Dead>().insert(Decay).remove_parent();
        }
        dead.ticks += 1;
    }
}

// Pure gravity system - applies fall_accel without side effects
fn enemy_gravity(
    spatial_query: PhysicsWorld,
    mut query: Query<(
        Entity,
        &mut LinearVelocity,
        &mut Transform,
        &mut Navigation,
        &Collider,
        &theseeker_engine::ai::sensors::CachedArchetypeStats,
    ), (With<Enemy>, With<FsmInstance>)>,
) {
    for (entity, mut velocity, mut transform, mut nav, collider, stats) in query.iter_mut() {
        match *nav {
            Navigation::Falling { jumping } => {
                // Apply gravity consistently
                velocity.0.y -= stats.fall_accel;
                
                // Check for ground collision
                if let Some((_, toi)) = spatial_query.shape_cast(
                    transform.translation.xy(),
                    Dir2::new_unchecked(Vec2::new(0., -1.)),
                    collider.shape(),
                    GROUNDED_THRESHOLD
                        + collider
                            .shape()
                            .compute_local_aabb()
                            .half_extents()
                            .y
                        + GROUND_BUFFER,
                    CollisionGroups::new(ENEMY, GROUND),
                    Some(entity),
                ) {
                    if velocity.0.y < 0. {
                        if let Some(details) = toi.details {
                            let witness_y = details.witness2.y;
                            // Only land if witness point is below the collider origin (ground contact)
                            if witness_y < 0.0 {
                                *nav = Navigation::Grounded;
                                // Adjust vertical position so collider rests on ground exactly like pre-refactor behaviour
                                transform.translation.y = transform.translation.y - witness_y - toi.time_of_impact + GROUND_BUFFER;
                                velocity.0.y = 0.0;
                            }
                        }
                    }
                }
            },
            Navigation::Grounded => {
                // Verify ground contact - transition to falling if no ground detected
                if spatial_query
                    .shape_cast(
                        transform.translation.xy(),
                        Dir2::new_unchecked(Vec2::new(0., -1.)),
                        collider.shape(),
                        GROUNDED_THRESHOLD
                            + collider
                                .shape()
                                .compute_local_aabb()
                                .half_extents()
                                .y
                            + GROUND_BUFFER,
                        CollisionGroups::new(ENEMY, GROUND),
                        Some(entity),
                    )
                    .is_none()
                {
                    // We're not actually on ground, transition to falling
                    *nav = Navigation::Falling { jumping: false };
                    // Initialize falling with zero velocity
                    velocity.0.y = 0.0;
                }
            },
            Navigation::Blocked => {
                // No special handling needed for blocked state
            }
        }
    }
}

fn move_collide(
    mut query: Query<
        (
            &mut LinearVelocity,
            &mut Transform,
            &mut Navigation,
            &Collider,
            Has<Knockback>,
        ),
        With<Enemy>,
    >,
    time: Res<GameTime>,
    spatial_query: PhysicsWorld,
) {
    for (
        mut linear_velocity,
        mut transform,
        mut nav,
        collider,
        is_knocked,
    ) in query.iter_mut()
    {
        let shape = collider.shared_shape().clone();
        let dir = linear_velocity.0.x.signum();
        let x_len = linear_velocity.0.x.abs();
        let half_extent_x = collider.shape().compute_local_aabb().half_extents().x;
        let front = transform.translation.x + (half_extent_x + 2.0) * dir;
        let z = transform.translation.z;
        let mut projected_velocity = linear_velocity.0;

        // Simplified version of the player collisions
        // If the enemy encounters a collision with the player, wall or edge of platform, it sets
        // the Navigation component to Navigation::Blocked
        while let Ok(shape_dir) = Dir2::new(linear_velocity.0) {
            if let Some((_entity, first_hit)) = spatial_query.shape_cast(
                transform.translation.xy(),
                shape_dir,
                &*shape,
                linear_velocity.0.length() / time.hz as f32 + 0.5,
                CollisionGroups::new(ENEMY, GROUND),
                None,
            ) {
                if first_hit.status != ShapeCastStatus::PenetratingOrWithinTargetDist {
                    let sliding_plane = into_vec2(first_hit.details.map(|d| d.normal1).unwrap_or_default());
                    projected_velocity = linear_velocity.0
                        - sliding_plane
                            * linear_velocity.0.dot(sliding_plane);
                    // println!("b {is_chasing} {transform:?}");
                    // if is_chasing {
                    //     println!("chasing noblock");
                    //     break;
                    // } else {
                    linear_velocity.0 = projected_velocity;
                    if !is_knocked
                        && !matches!(*nav, Navigation::Falling { .. })
                    {
                        *nav = Navigation::Blocked;
                    }
                    // }
                } else {
                    break;
                }
            } else {
                break;
            };
        }

        // Raycast from underground directly below the enemy in direction of movement, detecting the edges of a platform from
        // inside
        if let Some((_entity, first_hit)) = spatial_query.ray_cast(
            Vec2::new(front, transform.translation.y - 10.),
            Vec2::new(dir, 0.),
            x_len / time.hz as f32,
            false,
            CollisionGroups::new(ENEMY, GROUND),
            None,
        ) {
            if !is_knocked && !matches!(*nav, Navigation::Falling { .. }) {
                *nav = Navigation::Blocked;
            }
            projected_velocity.x = first_hit.time_of_impact * dir;
        }

        transform.translation = (transform.translation.xy()
            + projected_velocity * (1.0 / time.hz as f32))
            .extend(z);
    }
}

fn remove_inside(
    mut enemies: Query<
        (Entity, &GlobalTransform, &Collider),
        (With<Inside>, With<Enemy>),
    >,
    players_q: Query<(Entity, &GlobalTransform, &Collider), With<theseeker_engine::physics::inside::PlayerInsideEnemy>>,    
    mut commands: Commands,
    spatial_query: PhysicsWorld,
) {
    for (enemy_entity, transform, collider) in enemies.iter_mut() {
        // Collect players still intersecting this specific enemy
        let intersections = spatial_query.intersect(
            transform.translation().xy(),
            collider.shape(),
            theseeker_engine::physics::groups::groups(ENEMY_INSIDE),
            Some(enemy_entity),
        );
        let still_inside: std::collections::HashSet<Entity> = intersections.into_iter().collect();

        for (player_entity, p_transform, p_collider) in players_q.iter() {
            if !still_inside.contains(&player_entity) {
                // extra safeguard: confirm not intersecting to avoid premature clear
                let overlap = spatial_query.intersect(
                    p_transform.translation().xy(),
                    p_collider.shape(),
                    theseeker_engine::physics::groups::groups(ENEMY_INSIDE),
                    Some(player_entity),
                );
                if overlap.is_empty() {
                    theseeker_engine::physics::inside::clear(
                        &mut commands,
                        enemy_entity,
                        player_entity,
                    );
                }
            }
        }
    }
}

/// Despawns the gent after enemy enters Decay state
/// the gfx entity is despawned with a script action after the decay animation finishes playing
fn decay_despawn(
    query: Query<(Entity, &Gent), (With<Enemy>, With<Decay>)>,
    gfx_query: Query<Entity, With<EnemyGfx>>,
    mut commands: Commands,
) {
    for (entity, gent) in query.iter() {
        // First detach the graphics entity from hierarchy
        if let Ok(gfx_entity) = gfx_query.get(gent.e_gfx) {
            commands.entity(gfx_entity)
                .remove_parent()
                .insert(Decay);
        }
        
        // Then despawn the main entity without recursion since graphics is detached
        commands.entity(entity).despawn();
    }
}

struct EnemyAnimationPlugin;

impl Plugin for EnemyAnimationPlugin {
    fn build(&self, app: &mut App) {
        // Animation systems (shared by all enemies)
        app.add_systems(
            GameTickUpdate,
            (
                enemy_death_animation,
                enemy_decay_animation,
                enemy_decay_visibility,
                sprite_flip,
            )
                .in_set(EnemyStateSet::Animation)
                .after(EnemyStateSet::Transition)
                .run_if(in_state(AppState::InGame)),
        );
        

        
        // Hit effects run for both
        app.add_systems(
            GameTickUpdate,
                enemy_hit_sfx_gfx
                    .run_if(on_event::<DamageInfo>)
                    .in_set(RespondToDamageInfoSet),
        );
    }
}

fn enemy_hit_sfx_gfx(
    current_weapon: CurrentWeapon,
    i_query: Query<&Gent, With<Enemy>>,
    mut damage_events: EventReader<DamageInfo>,
    mut gfx_query: Query<
        &mut ScriptPlayer<SpriteAnimation>,
        With<EnemyEffectGfx>,
    >,
    player_facing_dir: Query<&Facing, With<Player>>,
) {
    for damage_info in damage_events.read() {
        if let Ok(enemy) = i_query.get(damage_info.target) {
            if let Ok(mut hit_gfx) = gfx_query.get_mut(enemy.e_effects_gfx) {
                let mut rng = thread_rng();
                let picked_spark = rng.gen_range(1..=6);
                hit_gfx.play_key("anim.spider.Sparks");
                // reset everything so as not to glitch audio?
                hit_gfx.clear_slots();
                hit_gfx.set_slot(
                    format!("Spark{picked_spark}").as_str(),
                    true,
                );
                // set hit
                hit_gfx.set_slot("AttackHit", true);
                // set weapon slot hit used for playing correct hit sfx
                let current_weapon_name = current_weapon.to_string();
                let slot = &format!("{current_weapon_name}Hit");
                hit_gfx.set_slot(slot, true);
                if let Ok(direction) = player_facing_dir.get_single() {
                    match direction {
                        Facing::Right => {
                            hit_gfx.set_slot("DirectionRight", true);
                            hit_gfx.set_slot("DirectionLeft", false);
                        },
                        Facing::Left => {
                            hit_gfx.set_slot("DirectionRight", false);
                            hit_gfx.set_slot("DirectionLeft", true);
                        },
                    };
                }
            }
        }
    }
}

fn enemy_death_animation(
    i_query: Query<(&Gent, &Role, &Tier), (Added<Dead>, With<Enemy>)>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    for (gent, role, tier) in i_query.iter() {
        if let Ok(mut enemy) = gfx_query.get_mut(gent.e_gfx) {
            enemy.play_key((enemy_anim_prefix(role, tier) + ".Death").as_str());
        }
    }
}

fn enemy_decay_animation(
    i_query: Query<(&Gent, &Role, &Tier), (Added<Decay>, With<Enemy>)>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    for (gent, role, tier) in i_query.iter() {
        if let Ok(mut enemy) = gfx_query.get_mut(gent.e_gfx) {
            enemy.play_key((enemy_anim_prefix(role, tier) + ".Decay").as_str());
        }
    }
}

/// Passes sprite screen visibility to the script slot that controls the decay rate
fn enemy_decay_visibility(
    mut gfx_query: Query<
        (
            &mut ScriptPlayer<SpriteAnimation>,
            &ViewVisibility,
        ),
        (With<EnemyGfx>, With<Decay>),
    >,
) {
    for (mut enemy, visible) in gfx_query.iter_mut() {
        enemy.set_slot("DecayRate", !visible.get());
    }
}

/// Outputs "anim.{enemy}{tier}"
fn enemy_anim_prefix(role: &Role, tier: &Tier) -> String {
    format!("anim.{}{}",
        match role {
            Role::Ranged => "spider",
            Role::Melee => "smallspider",
        },
        match tier {
            Tier::Base => "",
            Tier::Two => "2",
            Tier::Three => "3",
        }
    )
}

fn sprite_flip(
    query: Query<(&Facing, &Gent), With<Enemy>>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    for (facing, gent) in query.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            match facing {
                Facing::Right => {
                    player.set_slot("DirectionRight", true);
                    player.set_slot("DirectionLeft", false);
                },
                Facing::Left => {
                    player.set_slot("DirectionRight", false);
                    player.set_slot("DirectionLeft", true);
                },
            }
        }
    }
}

// AI Components & Systems

/// Ballistic projectile cache - avoids recalculating trajectories each frame
#[derive(Component, Default)]
struct ProjectileCache {
    /// Cached projectile velocity
    velocity: Option<LinearVelocity>,
    /// Player position when cache was calculated
    cached_player_pos: Vec2,
    /// Gravity value when cache was calculated
    cached_gravity: f32,
    /// Max allowed position delta before recalculation (px)
    position_tolerance: f32,
}

impl ProjectileCache {
    fn new() -> Self {
        Self {
            velocity: None,
            cached_player_pos: Vec2::ZERO,
            cached_gravity: 0.0,
            position_tolerance: 6.0, // Recalculate if player moves >6px for responsive targeting
        }
    }
    
    fn is_valid(&self, current_player_pos: Vec2, current_gravity: f32) -> bool {
        self.velocity.is_some() && 
        self.cached_player_pos.distance(current_player_pos) < self.position_tolerance &&
        (self.cached_gravity - current_gravity).abs() < 0.1 // Allow tiny floating point differences
    }
    
    fn clear(&mut self) {
        self.velocity = None;
    }
}

// Game-level actuator: executes AI actions affecting game-specific components
fn enemy_ai_actuator_game(
    mut query: Query<(
        Entity,
        &mut FsmInstance,
        &mut LinearVelocity,
        &mut Facing,
        &TargetSensor,
        &Gent,
        &Transform,
        &mut Navigation,
        &mut TurnCooldown,
        &theseeker_engine::ai::sensors::CachedArchetypeStats,
        Option<&mut ProjectileCache>,
    ), With<Enemy>>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
    player_query: Query<&Transform, With<Player>>,
    enemy_config: Res<EnemyConfig>,
    particle_effects: Res<ArcParticleEffectHandle>,
    _preloaded: Res<PreloadedAssets>,
    compiled_assets: Res<Assets<theseeker_engine::ai::CompiledFsm>>,
    mut commands: Commands,
) {
    for (entity, mut fsm, mut velocity, mut facing, target_sensor, gent, transform, mut navigation, mut turn_cooldown, cached_stats, mut projectile_cache) in query.iter_mut() {
        // Turn cooldown prevents wall-flipping spam
        if turn_cooldown.timer > 0 {
            turn_cooldown.timer = turn_cooldown.timer.saturating_sub(1);
        }
        
        // Process queued actions (FIFO). Drain keeps the underlying capacity, avoiding the
        // two small allocations incurred by `swap`.
        let brain_handle = fsm.brain.clone();
        let actions_drained: Vec<_> = fsm.actions.drain(..).collect();
        
        for action in actions_drained {
            match action {
                theseeker_engine::ai::CompiledAction::PlayAnim(key) => {
                    if let Ok(mut anim_player) = gfx_query.get_mut(gent.e_gfx) {
                        // Reset state_tick on animation change for at_state_tick actions
                        if fsm.current_anim_key.as_ref() != Some(&key.to_string()) {
                            // Reset state_tick when playing a new animation
                            fsm.state_tick = 0;
                            fsm.current_anim_key = Some(key.to_string());
                        }
                        anim_player.play_key(&key);
                        // Reset anim_tick when starting a new animation to synchronize frame timing
                        fsm.anim_tick = 0;
                    }
                },
                
                theseeker_engine::ai::CompiledAction::SetVel(vel) => {
                    velocity.0 = vel;
                },
                
                theseeker_engine::ai::CompiledAction::SetVelTowardsPlayer(speed) => {
                    // Calculate velocity towards player
                    if let Some(target_entity) = target_sensor.entity {
                        if let Ok(player_transform) = player_query.get(target_entity) {
                            let direction = (player_transform.translation - transform.translation).truncate();
                            if direction.length_squared() > 0.001 {
                                // Only move horizontally
                                let normalized_x = direction.x.signum();
                                velocity.0.x = normalized_x * speed;
                            }
                        }
                    }
                },
                
                theseeker_engine::ai::CompiledAction::SetVelFromFacing(speed) => {
                    // Set velocity based on current facing direction
                    // Note: facing.direction() returns -1.0 for Right, 1.0 for Left
                    // So we negate it to get the correct direction
                    velocity.0.x = -facing.direction() * speed;
                    
                    // Turn around at walls (with cooldown to prevent flipping)
                    if matches!(*navigation, Navigation::Blocked) && turn_cooldown.timer == 0 {
                        // Turn around
                        *facing = match *facing {
                            Facing::Right => Facing::Left,
                            Facing::Left => Facing::Right,
                        };
                        // Reset navigation state
                        *navigation = Navigation::Grounded;
                        // Update velocity with new facing
                        velocity.0.x = -facing.direction() * speed;
                        // Set cooldown to prevent rapid flipping (about 0.5 seconds)
                        turn_cooldown.timer = 48;
                    }
                },
                
                theseeker_engine::ai::CompiledAction::FacePlayer => {
                    // Face player (with turn cooldown)
                    if turn_cooldown.timer == 0 {
                        if let Some(target_entity) = target_sensor.entity {
                            if let Ok(player_transform) = player_query.get(target_entity) {
                                let dx = player_transform.translation.x - transform.translation.x;
                                // Note: Sprites are drawn facing LEFT by default, so the facing logic is inverted
                                let new_facing = if dx >= 0.0 { Facing::Left } else { Facing::Right };
                                
                                // Only update if actually changing direction
                                if new_facing != *facing {
                                    *facing = new_facing;
                                    turn_cooldown.timer = 48; // About 0.5 seconds
                                }
                            }
                        }
                    }
                },
                
                theseeker_engine::ai::CompiledAction::SpawnAttack { key, dmg } => {
                    if key == "melee_hit" {
                        // Spawn melee hitbox as child
                        let attack_entity = commands.spawn((
                            Collider::cuboid(8.0, 8.0), // Larger initial size to ensure hits register before AnimationCollider updates
                            theseeker_engine::physics::groups::enemy_attack(),
                            AnimationCollider(gent.e_gfx), // Links to sprite's magenta pixels
                            Transform::from_translation(Vec3::new(0.0, 5.0, 0.0)),
                            GlobalTransform::default(),
                            Attack::new(8, entity, dmg),
                        )).set_parent(entity).id();

                        // Ensure the enemy stops moving while performing melee attack
                        velocity.0 = Vec2::ZERO;
                    } else if key == "spider_ball" {
                        // Spawn ranged projectile (big spider attack)
                        let current_position = transform.translation.truncate();

                        // Create the projectile entity first – we'll attach velocity once we have it.
                        let projectile_entity = commands
                            .spawn((
                                Attack::new(480, entity, dmg), // lifetime in ticks
                                Collider::cuboid(2.5, 2.5),
                                theseeker_engine::physics::groups::enemy_attack(),
                                Transform::from_translation(current_position.extend(1.0)),
                                GlobalTransform::default(),
                                Visibility::Visible,
                                InheritedVisibility::VISIBLE,
                                ViewVisibility::default(),
                            ))
                            .with_lingering_particles(particle_effects.0.clone())
                            .id();

                        // Try to use cached projectile solution or compute a new one
                        let mut maybe_projectile = None;
                        
                        if let Some(target_entity) = target_sensor.entity {
                            if let Ok(player_transform) = player_query.get(target_entity) {
                                let player_pos = player_transform.translation.truncate();
                                
                                // Projectile cache for performance
                                // Get gravity from cached archetype stats for consistency
                                let archetype_gravity = cached_stats.fall_accel * 96.0;
                                    
                                let (need_recalculate, cached_velocity) = if let Some(ref mut cache) = projectile_cache {
                                    if cache.is_valid(player_pos, archetype_gravity) {
                                        // Use cached velocity
                                        (false, cache.velocity)
                                    } else {
                                        // Invalidate old cached velocity
                                        cache.velocity = None;
                                        (true, None)
                                    }
                                } else {
                                    (true, None)
                                };
                                
                                if !need_recalculate {
                                    // Use cached solution
                                    maybe_projectile = cached_velocity.map(|vel| 
                                        crate::game::attack::arc_attack::Projectile { vel }
                                    );
                                } else {
                                    // Calculate new projectile solution
                                    let gravity = archetype_gravity; // Use archetype-specific gravity for consistency

                                    // Start with a reasonable speed and keep increasing until we find a solution.
                                    let mut launch_speed = 200.0; // px/s
                                    for _ in 0..10 {
                                        if let Some(p) = crate::game::attack::arc_attack::Projectile::with_vel(
                                            player_pos,
                                            current_position,
                                            launch_speed,
                                            gravity,
                                        ) {
                                                // Cache the solution
                                                if let Some(ref mut cache) = projectile_cache {
                                                    cache.velocity = Some(p.vel);
                                                    cache.cached_player_pos = player_pos;
                                                    cache.cached_gravity = gravity;
                                                }
                                            maybe_projectile = Some(p);
                                            break;
                                        }
                                        launch_speed *= 1.15; // progressively try faster shots
                                    }

                                    // Fallback: fixed arc if ballistic solver fails
                                    if maybe_projectile.is_none() {
                                        let delta_x = player_pos.x - current_position.x;
                                        let fallback_vel = LinearVelocity(Vec2::new(134.0 * delta_x.signum(), 151.0));
                                            // Cache the fallback solution
                                            if let Some(ref mut cache) = projectile_cache {
                                                cache.velocity = Some(fallback_vel);
                                                cache.cached_player_pos = player_pos;
                                                cache.cached_gravity = gravity;
                                            }
                                        maybe_projectile = Some(crate::game::attack::arc_attack::Projectile { vel: fallback_vel });
                                    }
                                }
                            }
                        }

                        // If we didn't have a target (e.g., player stealthed), still fire forward using facing.
                        if maybe_projectile.is_none() {
                            let horiz_dir = -facing.direction(); // facing -> sprite left/right; negate to get +x is right
                            let fallback_vel = LinearVelocity(Vec2::new(134.0 * horiz_dir, 151.0));
                            maybe_projectile = Some(crate::game::attack::arc_attack::Projectile { vel: fallback_vel });
                            // Clear cache when firing without target
                            if let Some(ref mut cache) = projectile_cache {
                                cache.velocity = Some(fallback_vel);
                                cache.cached_player_pos = current_position; // approximate
                            }
                        }

                        // Attach the projectile component so the arc_projectile system will move it.
                        if let Some(projectile) = maybe_projectile {
                            commands.entity(projectile_entity).insert(projectile);
                        }
                    }
                },
                
                theseeker_engine::ai::CompiledAction::Cooldown { name, ticks } => {
                    if let Some(compiled) = compiled_assets.get(&brain_handle) {
                        if let Some(id) = compiled.inner.cooldown_id(&name) {
                            let idx = id as usize;
                            // Direct index - vector sized in ScriptBundle::from_arch
                            fsm.cooldowns[idx] = ticks;
                        }
                    }
                },

                theseeker_engine::ai::CompiledAction::Delayed { .. } => {
                    // Delayed actions are unwrapped in brain system
                },
                
                theseeker_engine::ai::CompiledAction::StateDelayed { .. } => {
                    // StateDelayed actions are unwrapped in brain system
                },
            }
        }
    }
}

/// Execute on_enter actions for newly spawned enemies (initial state setup)
fn trigger_initial_state_actions(
    mut query: Query<&mut FsmInstance, Added<FsmInstance>>,
    compiled_assets: Res<Assets<theseeker_engine::ai::CompiledFsm>>,
) {
    for mut fsm in query.iter_mut() {
        // Get the compiled FSM data
        let Some(compiled) = compiled_assets.get(&fsm.brain) else { 
            continue; 
        };
        
        // Execute on_enter actions for the initial logic state
        if let Some(actions) = compiled.inner.logic_state_actions.get(fsm.logic as usize) {
            fsm.actions.extend(actions.on_enter.iter().cloned());
        }
        
        // Execute on_enter actions for the initial movement state
        if let Some(actions) = compiled.inner.movement_state_actions.get(fsm.movement as usize) {
            fsm.actions.extend(actions.on_enter.iter().cloned());
        }
    }
}

/// Clear projectile cache when enemy dies to prevent stale data
fn clear_projectile_cache_on_death(
    mut query: Query<&mut ProjectileCache, Added<Dead>>,
) {
    for mut cache in query.iter_mut() {
        cache.clear();
    }
}

// Generic trait implementations for sensor systems
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

// Player target management - allows AI to find/track player
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
    added_stealth: Query<Entity, (With<Player>, Added<Stealthing>)>,
    mut removed_stealth: RemovedComponents<Stealthing>,
    player_query: Query<Entity, With<Player>>,
) {
    // Add AiTargetInvisible when stealth is added
    for entity in added_stealth.iter() {
        commands.entity(entity).insert(AiTargetInvisible);
    }
    
    // Remove AiTargetInvisible when stealth is removed
    for entity in removed_stealth.read() {
        if player_query.contains(entity) {
            commands.entity(entity).remove::<AiTargetInvisible>();
        }
    }
}
