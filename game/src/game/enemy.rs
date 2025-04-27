#[cfg(feature = "dev")]
use bevy_inspector_egui::quick::FilterQueryInspectorPlugin;
use rand::distr::StandardUniform;
use rand::thread_rng;
use rapier2d::geometry::SharedShape;
use rapier2d::parry::query::TOIStatus;
use rapier2d::prelude::{Group, InteractionGroups};
use theseeker_engine::animation::SpriteAnimationBundle;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::assets::config::{update_field, DynamicConfig};
use theseeker_engine::ballistics_math::ballistic_speed;
use theseeker_engine::gent::{Gent, GentPhysicsBundle, TransformGfxFromGent};
use theseeker_engine::physics::{
    into_vec2, update_sprite_colliders, AnimationCollider, Collider,
    LinearVelocity, PhysicsWorld, ShapeCaster, ENEMY, ENEMY_ATTACK,
    GROUND, PLAYER, SENSOR,
};
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
use crate::game::gentstate::{Patrolling, Chasing, Defending};
use crate::game::gentstate::Transitionable;

// Handle to the archetype asset used during spawning
use crate::game::enemy::archetype::EnemyArchetypeHandle;
use crate::game::enemy::archetype::apply_enemy_archetype;

pub struct EnemyPlugin;
pub mod archetype;
pub mod components;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        // Register the new EnemyArchetypeAsset so Bevy can load
        // `assets/enemies/archetypes.toml`.
        use bevy_common_assets::toml::TomlAssetPlugin;
        use crate::game::enemy::archetype::EnemyArchetypeAsset;

        app.init_asset::<EnemyArchetypeAsset>();
        app.add_plugins(TomlAssetPlugin::<EnemyArchetypeAsset>::new(&["*.arch.toml"]));

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
        app.add_plugins((
            EnemyBehaviorPlugin,
            EnemyTransitionPlugin,
            EnemyAnimationPlugin,
        ));

        // ------------------------------------------------------------------
        // Phase-2: convert handle-only entities into full enemies once the
        // archetype asset is available.
        // ------------------------------------------------------------------

        // Handle any entities already present at startup (e.g. placed in LDtk)
        app.add_systems(Startup, apply_enemy_archetype);

        // And handle ones spawned during gameplay (e.g. from spawners)
        app.add_systems(
            GameTickUpdate,
            apply_enemy_archetype
                .before(EnemyStateSet::Behavior)
                .run_if(in_state(AppState::InGame)),
        );

        app.register_type::<Range>();
        app.register_type::<Navigation>();
    }
}

// pub fn debug_enemy(world: &World, query: Query<Entity, With<Gent>>) {
//     for entity in query.iter() {
//         let components = world.inspect_entity(entity);
//         println!("enemy");
//         for component in components.iter() {
//             println!("{:?}", component.name());
//         }
//     }
// }

#[derive(SystemSet, Clone, PartialEq, Eq, Debug, Hash)]
pub enum EnemyStateSet {
    Behavior,
    Collisions,
    Transition,
    Animation,
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
    const COOLDOWN: u32 = 4000;
    const MAX: usize = 3;
    const RANGE: f32 = 400.;

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
    // Add slots or upgrade the next slot (tier system removed in refactor)
    Upgrade,
    // Ready to spawn
    Ready,
    Spawned,
    Cooldown,
}

#[derive(Debug)]
pub struct SpawnSlot {
    pub enemy: Option<Entity>,
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
    sprite: SpriteBundle,
    animation: SpriteAnimationBundle,
}

#[derive(Bundle)]
pub struct EnemyEffectsGfxBundle {
    marker: EnemyEffectGfx,
    gent2gfx: TransformGfxFromGent,
    sprite: SpriteBundle,
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

// TODO:only spawn when all from spawner have died, increase scaling, when 5 are cleared, up spider
// moved tier logic removed for refactor (was one at a time)
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
    asset_server: Res<AssetServer>,
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
                    // TODO: get rid of threshold_next if we decide to continue with spawning every
                    // clear
                    spawner.threshold_next = 1;
                    // spawner.threshold_next = thread_rng().gen_range(2..4);
                    // add a slot
                    if spawner.slots.len() < EnemySpawner::MAX {
                        spawner.slots.push(SpawnSlot {
                            enemy: None,
                        })
                    // previously: or increase tier of the next slot (logic removed)
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
                        for slot in spawner.slots.iter_mut() {
                            let e = commands
                                .spawn((
                                    TransformBundle::from_transform(*transform),
                                    Role::Ranged,
                                    EnemyArchetypeHandle::key("RangedSpider", &asset_server),
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
        &Role,
        Entity,
        Ref<EnemyBlueprint>,
    )>,
    mut commands: Commands,
) {
    for (mut xf_gent, role, e_gent, bp) in q.iter_mut() {
        if !bp.is_added() {
            continue;
        }
        // TODO: ensure proper z order
        xf_gent.translation.z = 14.0 * 0.000001;
        xf_gent.translation.y += 2.0; // Sprite offset so it looks like it is standing on the ground
        let health = 100; // Placeholder health until Archetypes are implemented
        let e_gfx = commands.spawn(()).id();
        let e_effects_gfx = commands.spawn(()).id();
        commands.entity(e_gent).insert((
            Name::new("Enemy"),
            EnemyGentBundle {
                enemy: Enemy,
                marker: Gent {
                    e_gfx,
                    e_effects_gfx,
                },
                phys: GentPhysicsBundle {
                    collider: Collider::cuboid(
                        16.0,
                        10.0,
                        InteractionGroups {
                            memberships: ENEMY,
                            filter: Group::all(),
                        },
                    ),
                    shapecast: ShapeCaster {
                        shape: SharedShape::cuboid(22.0, 10.0),
                        direction: Dir2::NEG_Y,
                        origin: Vec2::new(0.0, -2.0),
                        max_toi: 0.0,
                        interaction: InteractionGroups {
                            memberships: ENEMY,
                            filter: GROUND,
                        },
                    },
                    linear_velocity: LinearVelocity(Vec2::ZERO),
                },
            },
            Navigation::Grounded,
            Range::None,
            Target(None),
            Health {
                current: health,
                max: health,
            },
            Facing::Right,
            Patrolling,
            Idle,
            Waiting::new(12),
            AddQueue::default(),
            TransitionQueue::default(),
            StateDespawnMarker,
        ));
        commands.entity(e_gfx).insert((
            EnemyGfxBundle {
                marker: EnemyGfx { e_gent },
                gent2gfx: TransformGfxFromGent {
                    pixel_aligned: false,
                    gent: e_gent,
                },
                sprite: SpriteBundle {
                    sprite: Sprite {
                        texture_atlas: Some(TextureAtlas::default()),
                        ..default()
                    },
                    transform: *xf_gent,
                    ..Default::default()
                },
                animation: Default::default(),
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
                },
                sprite: SpriteBundle {
                    sprite: Sprite {
                        texture_atlas: Some(TextureAtlas::default()),
                        ..default()
                    },
                    transform: xf_gent.with_translation(Vec3::new(0., 0., 1.)),
                    ..Default::default()
                },
                animation: SpriteAnimationBundle { player: animation },
            },
            StateDespawnMarker,
        ));
        commands.entity(e_gfx).remove::<EnemyBlueprint>();
    }
}

struct EnemyBehaviorPlugin;

impl Plugin for EnemyBehaviorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                (
                    decay_despawn.run_if(any_with_component::<Decay>),
                    dead.run_if(any_with_component::<Dead>),
                    (
                        check_player_range,
                        (
                            patrolling.run_if(any_with_component::<Patrolling>),
                            aggro.run_if(any_with_component::<Aggroed>),
                            waiting.run_if(any_with_component::<Waiting>),
                            defense.run_if(any_with_component::<Defending>),
                            ranged_attack
                                .run_if(any_with_component::<LegacyRangedAttack>),
                        ),
                        (
                            walking.run_if(any_with_component::<Walking>),
                            falling,
                        ),
                    )
                        .chain(),
                )
                    .run_if(in_state(AppState::InGame))
                    .in_set(EnemyStateSet::Behavior)
                    .before(update_sprite_colliders),
                move_collide.in_set(EnemyStateSet::Collisions),
            ),
        );
    }
}

#[derive(Component, Debug, Default)]
#[component(storage = "SparseSet")]
struct Aggroed;

impl GentState for Aggroed {}
impl Transitionable<Patrolling> for Aggroed {
    type Removals = Aggroed;
}

#[derive(Component, Debug)]
#[component(storage = "SparseSet")]
pub struct Defense;

impl GentState for Defense {}
impl GenericState for Defense {}

#[derive(Component, Debug)]
#[component(storage = "SparseSet")]
struct LegacyRangedAttack {
    target: Entity,
    ticks: u32,
}
impl LegacyRangedAttack {
    const STARTUP: u32 = 6;
}
impl GentState for LegacyRangedAttack {}
impl GenericState for LegacyRangedAttack {}

#[derive(Component, Default)]
#[component(storage = "SparseSet")]
struct Waiting {
    ticks: u32,
    max_ticks: u32,
}
impl Waiting {
    pub fn new(max: u32) -> Self {
        Waiting {
            ticks: 0,
            max_ticks: max,
        }
    }
}
impl GentState for Waiting {}
impl GenericState for Waiting {}

#[derive(Component, Reflect)]
enum Navigation {
    Grounded,
    Falling { jumping: bool },
    Blocked,
}

#[derive(Component, Reflect)]
enum Role {
    Ranged,
}

impl Role {
    pub fn check_range(
        &self,
        distance: f32,
    ) -> Range {
        match self {
            Role::Ranged => {
                // Placeholder values until Archetypes are implemented
                let range_ranged_melee = 29.0;
                let range_ranged_aggro = 100.0;
                let range_ranged_ranged = 100.0;
                if distance <= range_ranged_melee {
                    Range::Melee
                } else if distance <= range_ranged_aggro {
                    Range::Aggro
                } else if distance <= range_ranged_ranged {
                    Range::Ranged
                } else {
                    Range::Far
                }
            },
        }
    }
}

#[derive(Component, Debug, Reflect, PartialEq, Eq)]
enum Range {
    Melee,
    Ranged,
    Aggro,
    Deaggro,
    Far,
    None,
}

#[derive(Component, Debug, Deref)]
struct Target(Option<Entity>);

/// Component added to Decaying enemy
/// not a GentState because it shouldnt transition to or from anything else
#[derive(Component)]
struct Decay;

// Check how far the player is, set our range, set our target if applicable, turn to face player if
// in range
// TODO: check x and y distance independently?
fn check_player_range(
    mut query: Query<
        (
            &mut Range,
            &mut Target,
            &mut Facing,
            &Role,
            &GlobalTransform,
            Has<Aggroed>,
            Has<Defending>,
        ),
        With<Enemy>,
    >,
    mut player_query: Query<
        (
            Entity,
            &GlobalTransform,
            Option<&Stealthing>,
            &mut EnemiesNearby,
        ),
        (Without<Enemy>, With<Player>),
    >,
) {
    if let Ok((player_e, player_trans, player_stealth, mut enemies_nearby)) =
        player_query.get_single_mut()
    {
        // reset every tick
        **enemies_nearby = 0;

        for (
            mut range,
            mut target,
            mut facing,
            role,
            trans,
            is_aggroed,
            is_defending,
        ) in query.iter_mut()
        {
            // TODO: still update enemies nearby in stealth?
            if player_stealth.is_some() {
                *range = Range::Deaggro;
                target.0 = None;
                continue;
            }
            let distance = trans
                .translation()
                .truncate()
                .distance(player_trans.translation().truncate());

            // face player
            if is_aggroed && !is_defending {
                if trans.translation().x > player_trans.translation().x {
                    *facing = Facing::Right;
                } else if trans.translation().x < player_trans.translation().x {
                    *facing = Facing::Left;
                }
            }

            // set range
            *range = role.check_range(distance);
            // set target
            target.0 = match *range {
                Range::Melee | Range::Aggro | Range::Ranged => Some(player_e),
                Range::Deaggro | Range::Far | Range::None => None,
            };
            // set nearby enemies for passive buff
            if distance < 50.0 {
                **enemies_nearby += 1;
            };
        }
    // if there is no player
    } else {
        for (mut range, mut target, _, _, _, _, _) in query.iter_mut() {
            *range = Range::None;
            target.0 = None;
        }
    }
}

fn patrolling(
    mut query: Query<
        (
            &Range,
            &mut TransitionQueue,
            &mut AddQueue,
            Option<&Waiting>,
        ),
        (With<Patrolling>, With<Enemy>),
    >,
) {
    for (range, mut transitions, mut additions, maybe_waiting) in
        query.iter_mut()
    {
        match range {
            Range::Aggro | Range::Melee => {
                transitions.push(Patrolling::new_transition(Aggroed));
                transitions.push(Walking::new_transition(
                    Waiting::default(),
                ));
            },
            Range::Deaggro | Range::Ranged | Range::Far => {
                if let Some(waiting) = maybe_waiting {
                    if waiting.ticks >= waiting.max_ticks {
                        transitions.push(Waiting::new_transition(Walking {
                            max_ticks: rand::thread_rng().gen_range(
                                24..300,
                            ),
                            ticks: 0,
                        }));
                    }
                }
            },
            Range::None => {
                if let Some(waiting) = maybe_waiting {
                    if waiting.ticks >= 15 * 8 {
                        additions.add(Idle);
                    }
                }
            },
        }
    }
}

fn waiting(mut query: Query<&mut Waiting, With<Enemy>>) {
    for mut waiting in query.iter_mut() {
        waiting.ticks += 1;
    }
}

fn defense(
    mut query: Query<
        (&Range, &mut TransitionQueue),
        (With<Enemy>, With<Defending>),
    >,
) {
    for (range, mut transitions) in query.iter_mut() {
        if !matches!(range, Range::Melee) {
            transitions.push(Defending::new_transition(
                Waiting::default(),
            ));
        }
    }
}

fn aggro(
    mut query: Query<
        (
            &Role,
            &Range,
            &Target,
            &mut LinearVelocity,
            &mut TransitionQueue,
        ),
        (
            With<Enemy>,
            With<Aggroed>,
            With<Waiting>,
        ),
    >,
) {
    for (role, range, target, mut velocity, mut transitions) in query.iter_mut()
    {
        if let Some(p_entity) = target.0 {
            // return to patrol if out of aggro range
            if matches!(range, Range::Far) {
                transitions.push(Aggroed::new_transition(Patrolling));
            } else if matches!(range, Range::Melee) {
                match role {
                    Role::Ranged => {
                        velocity.x = 0.;
                        transitions.push(Waiting::new_transition(Defending));
                    },
                }
            } else if matches!(role, Role::Ranged) {
                transitions.push(Waiting::new_transition(LegacyRangedAttack {
                    target: p_entity,
                    ticks: 0,
                }));
            }
        } else {
            transitions.push(Aggroed::new_transition(Patrolling));
        }
    }
}

fn ranged_attack(
    spatial_query: Res<PhysicsWorld>,
    mut query: Query<
        (
            Entity,
            &GlobalTransform,
            &Range,
            &mut LegacyRangedAttack,
            &mut LinearVelocity,
            &mut TransitionQueue,
            &mut AddQueue,
        ),
        (With<Enemy>, Without<Knockback>),
    >,
    player_query: Query<&Transform, With<Player>>,
    mut commands: Commands,
    config: Res<PlayerConfig>,
    time: Res<GameTime>,
    particle_effect: Res<ArcParticleEffectHandle>,
) {
    for (
        entity,
        enemy_transform,
        range,
        mut attack,
        mut velocity,
        mut trans_q,
        mut add_q,
    ) in query.iter_mut()
    {
        if attack.ticks == 0 {
            velocity.x = 0.;
        }
        attack.ticks += 1;
        if attack.ticks >= 15 * 8 {
            trans_q.push(LegacyRangedAttack::new_transition(
                Waiting::default(),
            ));
            add_q.add(Idle);
        }
        // if player isnt alive, do nothing, we will transition back once animation finishes
        let Ok(transform) = player_query.get(attack.target) else {
            continue;
        };
        if attack.ticks == LegacyRangedAttack::STARTUP * 8 {
            // cast a ray midway between enemy and player to find height of ceiling
            // we want to avoid
            let mid_pt = (enemy_transform.translation().xy()
                + transform.translation.xy())
                * 0.5;
            // how far is the ceiling above the mid point of the projectile trajectory?
            let mut ceiling = f32::MAX;
            if let Some((_hit_e, hit)) = spatial_query.ray_cast(
                mid_pt,
                Vec2::new(0.0, 1.0),
                f32::MAX,
                true,
                InteractionGroups::new(ENEMY, GROUND),
                None,
            ) {
                // only count it if the ray didn't start underground
                if hit.toi != 0.0 {
                    ceiling =
                        mid_pt.y + hit.toi - enemy_transform.translation().y;
                } else {
                    // if it did start underground, fire another one to find how far underground, and then fire again from there + 0.1
                    if let Some((_hit_e, hit)) = spatial_query.ray_cast(
                        mid_pt,
                        Vec2::new(0.0, 1.0),
                        f32::MAX,
                        false,
                        InteractionGroups::new(ENEMY, GROUND),
                        None,
                    ) {
                        if let Some((_hit_e, hit_2)) = spatial_query.ray_cast(
                            mid_pt + Vec2::new(0.0, hit.toi + 0.001),
                            Vec2::new(0.0, 1.0),
                            f32::MAX,
                            true,
                            InteractionGroups::new(ENEMY, GROUND),
                            None,
                        ) {
                            ceiling = mid_pt.y + hit.toi + hit_2.toi + 0.001
                                - enemy_transform.translation().y;
                        }
                    }
                }
            }

            // account for projectile width
            ceiling -= 5.0;

            let relative_height =
                enemy_transform.translation().y - transform.translation.y;
            let delta_x =
                transform.translation.x - enemy_transform.translation().x;
            let gravity = config.fall_accel * time.hz as f32;
            let mut speed = ballistic_speed(
                100.0,
                gravity,
                relative_height,
            );
            let max_attempts = 10;
            // Define default arc as 50ish degree shot with in the direction of the player
            let mut final_solution = Projectile {
                vel: LinearVelocity(Vec2::new(
                    134.0 * delta_x.signum(),
                    151.0,
                )),
            };
            for i in 0..max_attempts {
                if let Some(mut projectile) = Projectile::with_vel(
                    transform.translation.xy(),
                    enemy_transform.translation().xy(),
                    speed,
                    gravity,
                ) {
                    let max_proj_h = projectile.vel.y.powi(2) / (2.0 * gravity);
                    if max_proj_h >= ceiling {
                        let max_vel_y = (ceiling * (2.0 * gravity)).sqrt();
                        let max_vel_x =
                            max_vel_y / projectile.vel.y * projectile.vel.x;
                        let max_vel = Vec2::new(max_vel_x, max_vel_y).length();
                        if let Some(projectile_2) = Projectile::with_vel(
                            transform.translation.xy(),
                            enemy_transform.translation().xy(),
                            max_vel,
                            gravity,
                        ) {
                            projectile = projectile_2
                        } else {
                            // attempts to fire anyway, even though ceiling will always block the shot
                        }
                    }
                    final_solution = projectile;
                    break;
                } else if i == max_attempts - 1 {
                    warn!("No solution for ballistic trajectory, even after increased speed to {speed}, using default trajectory!")
                } else {
                    speed *= 1.15;
                }
            }
            // spawn in the new projectile:
            commands
                .spawn((
                    Attack::new(
                        192,
                        entity,
                        20.0,
                    )
                    .with_max_targets(1)
                    .set_stat_mod(StatusModifier::basic_ice_spider()),
                    final_solution,
                    Collider::cuboid(
                        5.,
                        5.,
                        InteractionGroups::new(ENEMY_ATTACK, PLAYER),
                    ),
                    TransformBundle::from(Transform::from_translation(
                        enemy_transform.translation().truncate().extend(1.0),
                    )),
                    VisibilityBundle::default(),
                ))
                .with_lingering_particles(particle_effect.0.clone());
        }
        if matches!(range, Range::Melee) {
            trans_q.push(LegacyRangedAttack::new_transition(Defending));
        }
    }
}

fn walking(
    mut query: Query<
        (
            &mut Navigation,
            &mut Facing,
            &mut LinearVelocity,
            &mut Walking,
            &mut TransitionQueue,
            &mut AddQueue,
        ),
        (
            With<Enemy>,
            Without<Knockback>,
        ),
    >,
) {
    for (
        mut nav,
        mut facing,
        mut velocity,
        mut walking,
        mut transitions,
        mut add_q,
    ) in query.iter_mut()
    {
        // set initial velocity
        velocity.x = -20.0 * facing.direction();
        if walking.ticks >= walking.max_ticks {
            velocity.x = 0.;
            transitions.push(Walking::new_transition(Waiting {
                ticks: 0,
                max_ticks: 240,
            }));
            add_q.add(Idle);
            continue;
        }
        // Turn around if we get to the edge/wall
        match *nav {
            Navigation::Blocked => {
                velocity.x *= -1.;
                *nav = Navigation::Grounded;
                *facing = match *facing {
                    Facing::Right => Facing::Left,
                    Facing::Left => Facing::Right,
                }
            },
            Navigation::Grounded | Navigation::Falling { .. } => {},
        }
        walking.ticks += 1;
    }
}

const GROUNDED_THRESHOLD: f32 = 1.0;
const GROUND_BUFFER: f32 = -1.0;
fn falling(
    spatial_query: Res<PhysicsWorld>,
    mut query: Query<
        (
            Entity,
            &mut LinearVelocity,
            &mut Transform,
            &ShapeCaster,
            &mut Navigation,
            &Collider,
            &Gent,
            &Role,
        ),
        With<Enemy>,
    >,
    players: Query<&Transform, (With<Player>, Without<Enemy>)>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    for (
        entity,
        mut velocity,
        mut transform,
        _,
        mut nav,
        collider,
        gent,
        role,
    ) in query.iter_mut()
    {
        if matches!(*nav, Navigation::Falling { .. }) {
            if let Some((e, toi)) = spatial_query.shape_cast(
                transform.translation.xy(),
                Dir2::new_unchecked(Vec2::new(0., -1.)),
                collider.0.shape(),
                GROUNDED_THRESHOLD,
                InteractionGroups {
                    memberships: ENEMY,
                    filter: GROUND,
                },
                Some(entity),
            ) {
                // println!("coll {toi:?}");
                // If we are not a player
                if players.get(e).is_err() {
                    // If we are close to the ground
                    if velocity.y < 0. && toi.witness2[1] < 0. {
                        println!(
                            "hit ground {toi:?} {} {:?}",
                            transform.translation, velocity
                        );
                        *nav = Navigation::Grounded;
                        transform.translation.y =
                            transform.translation.y - toi.witness2[1] - toi.toi
                                + GROUND_BUFFER;
                        velocity.y = 0.;
                        if let Ok(mut enemy_anim) =
                            gfx_query.get_mut(gent.e_gfx)
                        {
                            enemy_anim.play_key("anim.spider.Walk");
                        }
                        continue;
                    }
                }
            }
            if let Ok(ptrans) = players.get_single() {
                if ptrans.translation.y < transform.translation.y {
                    *nav = Navigation::Falling { jumping: false };
                }
            }
            match *nav {
                Navigation::Falling { jumping: true } => {
                    velocity.y -= 3.5;
                },
                Navigation::Falling { jumping: false } => {
                    velocity.y -= 4.5;
                },
                _ => unreachable!(),
            }

            if let Ok(mut enemy_anim) = gfx_query.get_mut(gent.e_gfx) {
                enemy_anim.set_slot("jump", velocity.y > 0.);
                enemy_anim.set_slot("fall", velocity.y < 0.);
            }
        } else if matches!(*nav, Navigation::Grounded) {
            if spatial_query
                .shape_cast(
                    transform.translation.xy(),
                    Dir2::new_unchecked(Vec2::new(0., -1.)),
                    collider.0.shape(),
                    GROUNDED_THRESHOLD
                        + collider.0.shape().compute_local_aabb().extents()[1]
                            / 2.
                        + GROUND_BUFFER,
                    InteractionGroups {
                        memberships: ENEMY,
                        filter: GROUND,
                    },
                    Some(entity),
                )
                .is_none()
            {
                println!("refalling");
                *nav = Navigation::Falling { jumping: false };
                if let Ok(mut enemy_anim) = gfx_query.get_mut(gent.e_gfx) {
                    enemy_anim.play_key("anim.spider.Idle");
                }
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
            Has<Chasing>,
        ),
        With<Enemy>,
    >,
    time: Res<GameTime>,
    spatial_query: Res<PhysicsWorld>,
) {
    for (
        mut linear_velocity,
        mut transform,
        mut nav,
        collider,
        is_knocked,
        is_chasing,
    ) in query.iter_mut()
    {
        let shape = collider.0.shared_shape().clone();
        let dir = linear_velocity.x.signum();
        let x_len = linear_velocity.x.abs();
        // TODO: should be based on collider half extent x
        let front = transform.translation.x + 10. * dir;
        let z = transform.translation.z;
        let mut projected_velocity = linear_velocity.xy();

        // Simplified version of the player collisions
        // If the enemy encounters a collision with the player, wall or edge of platform, it sets
        // the Navigation component to Navigation::Blocked
        while let Ok(shape_dir) = Dir2::new(linear_velocity.0) {
            if let Some((_entity, first_hit)) = spatial_query.shape_cast(
                transform.translation.xy(),
                shape_dir,
                &*shape,
                linear_velocity.length() / time.hz as f32 + 0.5,
                InteractionGroups {
                    memberships: ENEMY,
                    // Ground group
                    filter: Group::from_bits_truncate(0b10000),
                },
                None,
            ) {
                if first_hit.status != TOIStatus::Penetrating {
                    let sliding_plane = into_vec2(first_hit.normal1);
                    projected_velocity = linear_velocity.xy()
                        - sliding_plane
                            * linear_velocity.xy().dot(sliding_plane);
                    linear_velocity.0 = projected_velocity;
                    if !is_knocked
                        && !matches!(*nav, Navigation::Falling { .. })
                    {
                        *nav = Navigation::Blocked;
                    }
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
            // TODO: should be based on collider half extent y + a little
            Vec2::new(front, transform.translation.y - 10.),
            Vec2::new(dir, 0.),
            x_len / time.hz as f32,
            false,
            InteractionGroups {
                memberships: ENEMY,
                filter: GROUND,
            },
            None,
        ) {
            if !is_knocked && !matches!(*nav, Navigation::Falling { .. }) {
                *nav = Navigation::Blocked;
            }
            projected_velocity.x = first_hit.toi * dir;
        }

        transform.translation = (transform.translation.xy()
            + projected_velocity * (1.0 / time.hz as f32))
            .extend(z);
    }
}

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
                TransformBundle,
                Gent,
                Dead,
                Enemy,
                Role,
            )>();
        }
        if dead.ticks == 8 * 7 {
            commands
                .entity(entity)
                .remove::<Dead>()
                .insert(Decay)
                .remove_parent();
        }
        dead.ticks += 1;
    }
}

/// Despawns the gent after enemy enters Decay state
/// the gfx entity is despawned with a script action after the decay animation finishes playing
///
/// Also moves the Decay marker to the gfx entity so we can adjust the rate
fn decay_despawn(
    query: Query<(Entity, &Gent), (With<Enemy>, With<Decay>)>,
    gfx_query: Query<Entity, With<EnemyGfx>>,
    mut commands: Commands,
) {
    for (entity, gent) in query.iter() {
        if let Ok(gfx_entity) = gfx_query.get(gent.e_gfx) {
            commands.entity(gfx_entity).insert(Decay);
        }
        commands.entity(entity).despawn_recursive();
    }
}

struct EnemyTransitionPlugin;

impl Plugin for EnemyTransitionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                transition.run_if(any_with_component::<Enemy>),
                add_states.run_if(any_with_component::<AddQueue>),
                apply_deferred,
            )
                .chain()
                .in_set(EnemyStateSet::Transition)
                .after(EnemyStateSet::Behavior)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

struct EnemyAnimationPlugin;

impl Plugin for EnemyAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                sprite_flip,
            )
                .in_set(EnemyStateSet::Animation)
                .after(EnemyStateSet::Transition)
                .run_if(in_state(AppState::InGame)),
        );
    }
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

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
struct Walking {
    ticks: u32,
    max_ticks: u32,
}
impl GentState for Walking {}
impl GenericState for Walking {}

impl Transitionable<Aggroed> for Patrolling {
    type Removals = Patrolling;
}
