use bevy_hanabi::{ParticleEffect, ParticleEffectBundle};
#[cfg(feature = "dev")]
use bevy_inspector_egui::quick::FilterQueryInspectorPlugin;
use rand::distributions::Standard;
use rapier2d::geometry::SharedShape;
use rapier2d::parry::query::TOIStatus;
use rapier2d::prelude::{Group, InteractionGroups};
use theseeker_engine::animation::SpriteAnimationBundle;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::{Gent, GentPhysicsBundle, TransformGfxFromGent};
use theseeker_engine::physics::{
    into_vec2, Collider, LinearVelocity, PhysicsWorld, ShapeCaster, ENEMY, ENEMY_ATTACK, GROUND,
    PLAYER, SENSOR,
};
use theseeker_engine::script::ScriptPlayer;

use super::player::{Player, PlayerConfig};
use crate::game::attack::arc_attack::Projectile;
use crate::game::attack::particles::ArcParticleEffectHandle;
use crate::game::attack::*;
use crate::game::gentstate::*;
use crate::graphics::particles_util::BuildParticles;
use crate::prelude::*;

pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (setup_enemy.run_if(in_state(GameState::Playing)))
                .before(EnemyStateSet::Transition)
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            GameTickUpdate,
            spawn_enemy.after(setup_enemy),
        );
        app.add_plugins((
            EnemyBehaviorPlugin,
            EnemyTransitionPlugin,
            EnemyAnimationPlugin,
        ));
        app.register_type::<Range>();
        app.register_type::<Role>();
        app.register_type::<Navigation>();
    }
}

pub fn debug_enemy(world: &World, query: Query<Entity, With<Gent>>) {
    for entity in query.iter() {
        let components = world.inspect_entity(entity);
        println!("enemy");
        for component in components.iter() {
            println!("{:?}", component.name());
        }
    }
}

#[derive(SystemSet, Clone, PartialEq, Eq, Debug, Hash)]
pub enum EnemyStateSet {
    Behavior,
    Collisions,
    Transition,
    Animation,
}

#[derive(Bundle, LdtkEntity, Default)]
pub struct EnemyBlueprintBundle {
    marker: EnemyBlueprint,
}

#[derive(Bundle, LdtkEntity, Default)]
pub struct EnemySpawnerBundle {
    marker: EnemySpawner,
}

#[derive(Component, Default)]
pub struct EnemySpawner {
    pub enemy: Option<Entity>,
    pub cooldown_ticks: u32,
}

impl EnemySpawner {
    const COOLDOWN: u32 = 620;
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
    sprite: SpriteSheetBundle,
    animation: SpriteAnimationBundle,
}

#[derive(Component)]
pub struct EnemyGfx {
    e_gent: Entity,
}

fn spawn_enemy(
    mut spawner_q: Query<(&Transform, &mut EnemySpawner)>,
    enemy_q: Query<Entity, (With<Enemy>, Without<EnemySpawner>)>,
    mut commands: Commands,
) {
    for (transform, mut spawner) in spawner_q.iter_mut() {
        if let Some(enemy) = spawner.enemy {
            if !enemy_q.get(enemy).is_ok() {
                spawner.cooldown_ticks += 1;
                if spawner.cooldown_ticks >= EnemySpawner::COOLDOWN {
                    spawner.enemy = None;
                }
            }
        } else {
            let id = commands
                .spawn((
                    EnemyBlueprintBundle::default(),
                    TransformBundle::from_transform(*transform),
                ))
                .id();
            spawner.enemy = Some(id);
            spawner.cooldown_ticks = 0;
        }
    }
}

fn setup_enemy(
    mut q: Query<(&mut Transform, Entity), Added<EnemyBlueprint>>,
    mut commands: Commands,
) {
    for (mut xf_gent, e_gent) in q.iter_mut() {
        //TODO: ensure propper z order
        xf_gent.translation.z = 14.;
        let e_gfx = commands.spawn(()).id();
        commands.entity(e_gent).insert((
            Name::new("Enemy"),
            EnemyGentBundle {
                enemy: Enemy,
                marker: Gent { e_gfx },
                phys: GentPhysicsBundle {
                    //need to find a way to offset this one px toward back of enemys facing
                    //direction
                    collider: Collider::cuboid(
                        22.0,
                        10.0,
                        InteractionGroups {
                            memberships: ENEMY,
                            filter: Group::all(),
                        },
                    ),
                    shapecast: ShapeCaster {
                        shape: SharedShape::cuboid(22.0, 10.0),
                        direction: Direction2d::NEG_Y,
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
                current: 100,
                max: 100,
            },
            Role::random(),
            Facing::Right,
            Patrolling,
            Idle,
            Waiting::new(12),
            AddQueue::default(),
            TransitionQueue::default(), // },
        ));
        commands.entity(e_gfx).insert((EnemyGfxBundle {
            marker: EnemyGfx { e_gent },
            gent2gfx: TransformGfxFromGent {
                pixel_aligned: false,
                gent: e_gent,
            },
            sprite: SpriteSheetBundle {
                transform: *xf_gent,
                ..Default::default()
            },
            animation: Default::default(),
        },));
    }
}

struct EnemyBehaviorPlugin;

impl Plugin for EnemyBehaviorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                ((
                    assign_group,
                    check_player_range,
                    (
                        patrolling.run_if(any_with_component::<Patrolling>),
                        aggro.run_if(any_with_component::<Aggroed>),
                        waiting.run_if(any_with_component::<Waiting>),
                        defense.run_if(any_with_component::<Defense>),
                        ranged_attack.run_if(any_with_component::<RangedAttack>),
                        melee_attack.run_if(any_with_component::<MeleeAttack>),
                        pushback_attack.run_if(any_with_component::<PushbackAttack>),
                    ),
                    (
                        walking.run_if(any_with_component::<Walking>),
                        retreating.run_if(any_with_component::<Retreating>),
                        chasing.run_if(any_with_component::<Chasing>),
                    ),
                )
                    .chain(),)
                    .run_if(in_state(AppState::InGame))
                    .in_set(EnemyStateSet::Behavior),
                move_collide.in_set(EnemyStateSet::Collisions),
            ),
        );
    }
}

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
struct Patrolling;
impl GentState for Patrolling {}
impl Transitionable<Aggroed> for Patrolling {
    //TODO:
    // type Removals = (Patrolling, Waiting);
    type Removals = (Patrolling);
}

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
struct Walking {
    ticks: u32,
    max_ticks: u32,
}
impl GentState for Walking {}
impl GenericState for Walking {}

#[derive(Component, Debug)]
#[component(storage = "SparseSet")]
struct Retreating {
    ticks: u32,
    max_ticks: u32,
}
impl GentState for Retreating {}
impl GenericState for Retreating {}

#[derive(Component, Debug, Default)]
#[component(storage = "SparseSet")]
struct Chasing;
impl GentState for Chasing {}
impl GenericState for Chasing {}

#[derive(Component, Debug)]
#[component(storage = "SparseSet")]
struct Aggroed;

impl GentState for Aggroed {}
impl Transitionable<Patrolling> for Aggroed {
    // type Removals = (Aggroed, RangedAttack);
    type Removals = (Aggroed);
}

#[derive(Component, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct Defense {
    cooldown_ticks: u32,
}
impl Defense {
    const COOLDOWN: u32 = 30;
}

impl GentState for Defense {}
impl GenericState for Defense {}

#[derive(Component, Debug)]
#[component(storage = "SparseSet")]
struct RangedAttack {
    target: Entity,
    ticks: u32,
}
impl RangedAttack {
    const STARTUP: u32 = 6;
    // const RANGE: f32 = 40.;
}
impl GentState for RangedAttack {}
impl GenericState for RangedAttack {}

#[derive(Component, Debug, Default)]
#[component(storage = "SparseSet")]
struct MeleeAttack {
    ticks: u32,
}
impl MeleeAttack {
    // const RECOVERY: u32 = 9;
    const MAX: u32 = 10;
    const STARTUP: u32 = 7;
}
impl GentState for MeleeAttack {}
impl GenericState for MeleeAttack {}

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
struct PushbackAttack {
    ticks: u32,
}
impl PushbackAttack {
    // const RECOVERY: u32 = 7;
    const MAX: u32 = 10;
    const STARTUP: u32 = 5;
}
impl GentState for PushbackAttack {}
impl GenericState for PushbackAttack {}

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
    // Falling,
    Blocked,
}

#[derive(Component, Reflect)]
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

impl Distribution<Role> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Role {
        let index: u8 = rng.gen_range(0..=1);
        match index {
            0 => Role::Melee,
            1 => Role::Ranged,
            _ => unreachable!(),
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
// Target entity, distance
struct Target(Option<Entity>);

impl Range {
    const MELEE: f32 = 16.;
    const AGGRO: f32 = 50.;
    const RANGED: f32 = 60.;
    const DEAGGRO: f32 = 70.;
    const GROUPED: f32 = 30.;
}

//Check how far the player is, set our range, set our target if applicable, turn to face player if
//in range
fn check_player_range(
    mut query: Query<
        (
            &mut Range,
            &mut Target,
            &mut Facing,
            &GlobalTransform,
            Has<Aggroed>,
            Has<MeleeAttack>,
        ),
        With<Enemy>,
    >,
    player_query: Query<(Entity, &GlobalTransform), (Without<Enemy>, With<Player>)>,
) {
    for (mut range, mut target, mut facing, trans, is_aggroed, is_meleeing) in query.iter_mut() {
        if let Ok((player_e, player_trans)) = player_query.get_single() {
            let distance = trans
                .translation()
                .truncate()
                .distance(player_trans.translation().truncate());

            if is_aggroed && !is_meleeing {
                // }
                //if we are in AGGRO range, face the player
                // if distance <= Range::AGGRO {
                if trans.translation().x > player_trans.translation().x {
                    *facing = Facing::Right;
                } else if trans.translation().x < player_trans.translation().x {
                    *facing = Facing::Left;
                }
            }

            //set range and target
            if distance <= Range::MELEE {
                *range = Range::Melee;
                target.0 = Some(player_e);
            } else if distance <= Range::AGGRO {
                *range = Range::Aggro;
                target.0 = Some(player_e);
            } else if distance <= Range::RANGED {
                *range = Range::Ranged;
                target.0 = Some(player_e);
            } else if distance <= Range::DEAGGRO {
                *range = Range::Deaggro;
                target.0 = Some(player_e);
                // target.0 = None;
            } else {
                *range = Range::Far;
                target.0 = None;
            }
        //if there is no player
        } else {
            *range = Range::None;
            target.0 = None;
        }
        // dbg!(range);
    }
}

//check if any other enemies are nearby, if so assign to group
fn assign_group(
    query: Query<(Entity, &GlobalTransform, Has<Grouped>), With<Enemy>>,
    spatial_query: Res<PhysicsWorld>,
    mut commands: Commands,
) {
    for (entity, transform, is_grouped) in query.iter() {
        let project_from = transform.translation().truncate();
        if let Some((other, projection)) = spatial_query.point_project(
            project_from,
            InteractionGroups::new(SENSOR, ENEMY),
            Some(entity),
        ) {
            let closest = project_from.distance([projection.point.x, projection.point.y].into());
            if closest < Range::GROUPED && !is_grouped {
                commands.entity(entity).insert(Grouped);
            } else if closest >= Range::GROUPED && is_grouped {
                commands.entity(entity).remove::<Grouped>();
            }
        } else {
            commands.entity(entity).remove::<Grouped>();
        };
    }
}

#[derive(Component, Debug)]
struct Grouped;

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
    for (range, mut transitions, mut additions, maybe_waiting) in query.iter_mut() {
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
                            max_ticks: rand::thread_rng().gen_range(24..300),
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

fn waiting(mut query: Query<(&mut Waiting), With<Enemy>>) {
    for mut waiting in query.iter_mut() {
        waiting.ticks += 1;
    }
}

fn defense(
    mut query: Query<
        (
            &Range,
            &mut Defense,
            &mut TransitionQueue,
        ),
        With<Enemy>,
    >,
) {
    for (range, mut defense, mut transitions) in query.iter_mut() {
        if !matches!(range, Range::Melee) {
            transitions.push(Defense::new_transition(
                Waiting::default(),
            ));
            continue;
        }
        defense.cooldown_ticks += 1;
        if defense.cooldown_ticks == Defense::COOLDOWN * 8 {
            transitions.push(Defense::new_transition(
                PushbackAttack::default(),
            ));
            defense.cooldown_ticks = 0;
        }
    }
}

fn pushback_attack(
    mut query: Query<
        (
            Entity,
            &Facing,
            &mut PushbackAttack,
            &mut TransitionQueue,
        ),
        With<Enemy>,
    >,
    mut commands: Commands,
) {
    for (entity, facing, mut attack, mut transitions) in query.iter_mut() {
        attack.ticks += 1;
        if attack.ticks == PushbackAttack::STARTUP * 8 {
            commands
                .spawn((
                    TransformBundle::from_transform(Transform::from_xyz(
                        10. * -facing.direction(),
                        0.,
                        0.,
                    )),
                    Collider::cuboid(
                        10.,
                        10.,
                        InteractionGroups {
                            memberships: ENEMY_ATTACK,
                            filter: PLAYER,
                        },
                    ),
                    Attack::new(8, entity),
                    Pushback {
                        direction: -facing.direction(),
                        strength: 100.,
                    },
                ))
                .set_parent(entity);
        }
        if attack.ticks >= PushbackAttack::MAX * 8 {
            transitions.push(PushbackAttack::new_transition(
                Defense::default(),
            ))
        }
    }
}

fn aggro(
    mut query: Query<
        (
            &Range,
            &Target,
            Has<Grouped>,
            &mut TransitionQueue,
        ),
        (
            With<Enemy>,
            With<Aggroed>,
            //each "substate" of aggro should return back to waiting when with wants to return control
            //to aggro
            With<Waiting>,
        ),
    >,
) {
    for (range, target, is_grouped, mut transitions) in query.iter_mut() {
        if target.0.is_some() {
            let mut rng = rand::thread_rng();
            //return to patrol if out of aggro range
            if matches!(range, Range::Far) {
                transitions.push(Aggroed::new_transition(Patrolling));
            } else if !is_grouped {
                transitions.push(Waiting::new_transition(Retreating {
                    ticks: 0,
                    max_ticks: rng.gen_range(24..300),
                }));
            } else if is_grouped {
                if matches!(range, Range::Melee) {
                    transitions.push(Waiting::new_transition(
                        MeleeAttack::default(),
                    ));
                } else {
                    transitions.push(Waiting::new_transition(Chasing));
                }
            }
        //if there is no player it should also return to patrol state
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
            &mut RangedAttack,
            &mut LinearVelocity,
            &mut TransitionQueue,
            &mut AddQueue,
            Has<Grouped>,
        ),
        With<Enemy>,
    >,
    player_query: Query<(&Transform), With<Player>>,
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
        is_grouped,
    ) in query.iter_mut()
    {
        if attack.ticks == 0 {
            velocity.x = 0.;
        }
        attack.ticks += 1;
        // if attack.ticks >= 15 * 8 || !matches!(range, Range::Ranged) {
        if attack.ticks >= 15 * 8 {
            trans_q.push(RangedAttack::new_transition(
                Waiting::default(),
            ));
            add_q.add(Idle);
        }
        //if player isnt alive, do nothing, we will transiton back once animation finishes
        let Ok(transform) = player_query.get(attack.target) else {
            continue;
        };
        if attack.ticks == RangedAttack::STARTUP * 8 {
            // cast a ray midway between enemy and player to find height of ceiling
            // we want to avoid
            let mid_pt = (enemy_transform.translation().xy() + transform.translation.xy()) * 0.5;
            // how far is the ceiling above the mid point of the projectile trajectory?
            let mut ceiling = f32::MAX;
            if let Some((hit_e, hit)) = spatial_query.ray_cast(
                mid_pt,
                Vec2::new(0.0, 1.0),
                f32::MAX,
                true,
                InteractionGroups::new(ENEMY, GROUND),
                None,
            ) {
                // only count it if the ray didn't start underground
                if hit.toi != 0.0 {
                    ceiling = mid_pt.y + hit.toi - enemy_transform.translation().y;
                } else {
                    // if it did start underground, fire another one to find how far underground, and then fire again from there + 0.1
                    if let Some((hit_e, hit)) = spatial_query.ray_cast(
                        mid_pt,
                        Vec2::new(0.0, 1.0),
                        f32::MAX,
                        false,
                        InteractionGroups::new(ENEMY, GROUND),
                        None,
                    ) {
                        if let Some((hit_e, hit_2)) = spatial_query.ray_cast(
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

            let gravity = config.fall_accel * time.hz as f32;

            if let Some(mut projectile) = Projectile::with_vel(
                transform.translation.xy(),
                enemy_transform.translation().xy(),
                200.0,
                gravity,
            ) {
                let max_proj_h = projectile.vel.y.powi(2) / (2.0 * gravity);
                //println!("ceiling_h: {ceiling}, estimated_h: {max_proj_h}");
                if max_proj_h >= ceiling {
                    //if projectile would hit ceiling, lower available power proportionally
                    let max_vel_y = (ceiling * (2.0 * gravity)).sqrt();
                    let max_vel_x = max_vel_y / projectile.vel.y * projectile.vel.x;
                    let max_vel = Vec2::new(max_vel_x, max_vel_y).length();
                    //println!("trying again with new max vel: {max_vel}");
                    if let Some(mut projectile_2) = Projectile::with_vel(
                        transform.translation.xy(),
                        enemy_transform.translation().xy(),
                        max_vel,
                        gravity,
                    ) {
                        projectile = projectile_2
                    } else {
                        //println!("can't find solution, ceiling too low");
                    }
                }
                commands.spawn((
                    Attack::new(1000, entity),
                    projectile,
                    Collider::cuboid(
                        5.,
                        5.,
                        InteractionGroups::new(ENEMY_ATTACK, PLAYER),
                    ),
                    TransformBundle::from(Transform::from_translation(
                        enemy_transform.translation(),
                    )),
                ));
            } else {
                warn!("No solution for ballistic trajectory, use a higher projectile velocity!")
            }
        }
        if matches!(range, Range::Melee) {
            if is_grouped {
                trans_q.push(RangedAttack::new_transition(
                    MeleeAttack::default(),
                ));
            } else {
                trans_q.push(RangedAttack::new_transition(
                    Defense::default(),
                ));
            }
        }
    }
}

fn melee_attack(
    mut query: Query<
        (
            Entity,
            &mut MeleeAttack,
            &Facing,
            &mut TransitionQueue,
        ),
        With<Enemy>,
    >,
    mut commands: Commands,
) {
    for (entity, mut attack, facing, mut trans_q) in query.iter_mut() {
        attack.ticks += 1;
        if attack.ticks == 8 * MeleeAttack::STARTUP {
            //spawn attack hitbox collider as child
            let collider = commands
                .spawn((
                    Collider::cuboid(
                        //todo, half extents correct?
                        10.,
                        10.,
                        InteractionGroups {
                            memberships: SENSOR,
                            filter: PLAYER,
                        },
                    ),
                    TransformBundle::from_transform(Transform::from_xyz(
                        10. * -facing.direction(),
                        0.,
                        0.,
                    )),
                    Attack::new(8, entity),
                ))
                .set_parent(entity)
                .id();
        }
        if attack.ticks >= MeleeAttack::MAX * 8 {
            trans_q.push(MeleeAttack::new_transition(
                Waiting::default(),
            ))
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
            //TODO: remove addqueue
        ),
        (With<Enemy>, Without<Retreating>),
    >,
) {
    for (mut nav, mut facing, mut velocity, mut walking, mut transitions, mut add_q) in
        query.iter_mut()
    {
        //set initial velocity
        velocity.x = -20. * facing.direction();
        if walking.ticks >= walking.max_ticks {
            velocity.x = 0.;
            transitions.push(Walking::new_transition(Waiting {
                ticks: 0,
                max_ticks: 240,
            }));
            add_q.add(Idle);
            continue;
        }
        //Turn around if we get to the edge/wall
        match *nav {
            Navigation::Blocked => {
                velocity.x *= -1.;
                *nav = Navigation::Grounded;
                *facing = match *facing {
                    Facing::Right => Facing::Left,
                    Facing::Left => Facing::Right,
                }
            },
            Navigation::Grounded => {},
        }
        walking.ticks += 1;
    }
}

fn retreating(
    mut query: Query<
        (
            &Range,
            &Facing,
            &mut Navigation,
            &mut LinearVelocity,
            &mut Retreating,
            &mut TransitionQueue,
        ),
        (With<Enemy>, Without<Walking>),
    >,
    player_query: Query<(Entity), With<Player>>,
) {
    for (range, facing, mut nav, mut velocity, mut retreating, mut transitions) in query.iter_mut()
    {
        velocity.x = 12. * facing.direction();
        if matches!(*nav, Navigation::Blocked) || retreating.ticks > retreating.max_ticks {
            velocity.x = 0.;
            *nav = Navigation::Grounded;
            match range {
                Range::Melee => {
                    transitions.push(Retreating::new_transition(
                        Defense::default(),
                    ));
                },
                Range::Ranged | Range::Aggro => transitions.push(Retreating::new_transition(
                    RangedAttack {
                        target: player_query.get_single().expect("no player"),
                        ticks: 0,
                    },
                )),
                _ => transitions.push(Retreating::new_transition(
                    // RangedAttack {
                    //     target: player_query.get_single().expect("no player"),
                    //     ticks: 0,
                    // },
                    Waiting::default(),
                )),
            }
        } else if matches!(range, Range::Melee) {
            velocity.x = 0.;
            transitions.push(Retreating::new_transition(
                Defense::default(),
            ));
        } else if matches!(range, Range::Ranged) || matches!(range, Range::Aggro) {
            velocity.x = 0.;
            transitions.push(Retreating::new_transition(
                RangedAttack {
                    target: player_query.get_single().expect("no player"),
                    ticks: 0,
                },
            ));
        }

        retreating.ticks += 1;
    }
}

fn chasing(
    mut query: Query<
        (
            &Target,
            &Facing,
            &Role,
            &Range,
            &mut Navigation,
            &mut LinearVelocity,
            &mut TransitionQueue,
        ),
        (With<Enemy>, With<Chasing>),
    >,
) {
    for (target, facing, role, range, mut nav, mut velocity, mut transitions) in query.iter_mut() {
        if let Some(p_entity) = target.0 {
            //TODO: how should blocked from knockback interact with movement ai?
            let target_range = match role {
                Role::Ranged => Range::Ranged,
                Role::Melee => Range::Melee,
            };

            //if we are outside our target range, walk closer.
            //TODO: change to add random offsets?
            if *range != target_range {
                velocity.x = -35. * facing.direction();
                //if we cant get any closer because of edge
                if let Navigation::Blocked = *nav {
                    velocity.x = 0.;
                    *nav = Navigation::Grounded;
                    // println!("chasing but blocked");
                    transitions.push(Chasing::new_transition(RangedAttack {
                        target: p_entity,
                        ticks: 0,
                    }));
                }
            } else {
                velocity.x = 0.;
                match role {
                    Role::Melee => transitions.push(Chasing::new_transition(
                        MeleeAttack::default(),
                    )),
                    Role::Ranged => transitions.push(Chasing::new_transition(RangedAttack {
                        target: p_entity,
                        ticks: 0,
                    })),
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
        ),
        With<Enemy>,
    >,
    time: Res<GameTime>,
    spatial_query: Res<PhysicsWorld>,
) {
    for (mut linear_velocity, mut transform, mut nav, collider, is_knocked) in query.iter_mut() {
        let shape = collider.0.shared_shape().clone();
        let dir = linear_velocity.x.signum();
        let x_len = linear_velocity.x.abs();
        let front = transform.translation.x + 10. * dir;
        let z = transform.translation.z;
        let interaction = InteractionGroups {
            memberships: ENEMY,
            filter: Group::from_bits_truncate(0b10001),
            // filter: GROUND,
        };
        while let Ok(shape_dir) = Direction2d::new(linear_velocity.0) {
            if let Some((e, first_hit)) = spatial_query.shape_cast(
                transform.translation.xy(),
                shape_dir,
                &*shape,
                linear_velocity.length() / time.hz as f32 + 0.5,
                interaction,
                None,
            ) {
                if first_hit.status != TOIStatus::Penetrating {
                    let sliding_plane = into_vec2(first_hit.normal1);
                    let projected_velocity = linear_velocity.xy()
                        - sliding_plane * linear_velocity.xy().dot(sliding_plane);
                    linear_velocity.0 = projected_velocity;
                    let new_pos =
                        transform.translation.xy() + (shape_dir.xy() * (first_hit.toi - 0.01));
                    transform.translation.x = new_pos.x;
                    transform.translation.y = new_pos.y;
                    if !is_knocked {
                        *nav = Navigation::Blocked;
                    }
                } else {
                    break;
                }
            } else {
                break;
            };
        }
        //its easy to get stuck on the corner of an enemy...

        //if Navigation::Grounded
        //no support for air right now
        //cast from underground in direction of movement
        let mut projected_velocity = linear_velocity;

        if let Some((entity, first_hit)) = spatial_query.ray_cast(
            Vec2::new(front, transform.translation.y - 10.),
            Vec2::new(dir, 0.),
            x_len / time.hz as f32,
            false,
            interaction,
            None,
        ) {
            if !is_knocked {
                *nav = Navigation::Blocked;
            }
            projected_velocity.x = first_hit.toi * dir;
        }

        transform.translation = (transform.translation.xy()
            + projected_velocity.xy() * (1.0 / time.hz as f32))
            .extend(z);
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
                enemy_idle_animation,
                enemy_defense_animation,
                enemy_walking_animation,
                enemy_chasing_animation,
                enemy_retreat_animation,
                enemy_ranged_attack_animation,
                enemy_melee_attack_animation,
                enemy_pushback_attack_animation,
                sprite_flip,
            )
                .in_set(EnemyStateSet::Animation)
                .after(EnemyStateSet::Transition)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

fn enemy_idle_animation(
    i_query: Query<&Gent, (Added<Idle>, With<Enemy>)>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    for gent in i_query.iter() {
        if let Ok(mut enemy) = gfx_query.get_mut(gent.e_gfx) {
            enemy.play_key("anim.spider.Idle");
        }
    }
}

fn enemy_walking_animation(
    i_query: Query<&Gent, ((Added<Walking>), (With<Enemy>))>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    for gent in i_query.iter() {
        if let Ok(mut enemy) = gfx_query.get_mut(gent.e_gfx) {
            enemy.play_key("anim.spider.Walk");
        }
    }
}

fn enemy_chasing_animation(
    i_query: Query<&Gent, ((Added<Chasing>), (With<Enemy>))>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    for gent in i_query.iter() {
        if let Ok(mut enemy) = gfx_query.get_mut(gent.e_gfx) {
            enemy.play_key("anim.spider.Chase");
        }
    }
}

fn enemy_ranged_attack_animation(
    i_query: Query<&Gent, (Added<RangedAttack>, With<Enemy>)>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    for gent in i_query.iter() {
        if let Ok(mut enemy) = gfx_query.get_mut(gent.e_gfx) {
            enemy.play_key("anim.spider.RangedAttack");
        }
    }
}

fn enemy_melee_attack_animation(
    i_query: Query<&Gent, (Added<MeleeAttack>, With<Enemy>)>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    for gent in i_query.iter() {
        if let Ok(mut enemy) = gfx_query.get_mut(gent.e_gfx) {
            enemy.play_key("anim.spider.OffensiveAttack");
        }
    }
}

fn enemy_pushback_attack_animation(
    i_query: Query<&Gent, (Added<PushbackAttack>, With<Enemy>)>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    for gent in i_query.iter() {
        if let Ok(mut enemy) = gfx_query.get_mut(gent.e_gfx) {
            enemy.play_key("anim.spider.DefensiveAttack");
        }
    }
}

fn enemy_defense_animation(
    i_query: Query<&Gent, (Added<Defense>, With<Enemy>)>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    for gent in i_query.iter() {
        if let Ok(mut enemy) = gfx_query.get_mut(gent.e_gfx) {
            enemy.play_key("anim.spider.Defense");
        }
    }
}

fn enemy_retreat_animation(
    i_query: Query<&Gent, (Added<Retreating>, With<Enemy>)>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    for gent in i_query.iter() {
        if let Ok(mut enemy) = gfx_query.get_mut(gent.e_gfx) {
            enemy.play_key("anim.spider.Retreat");
        }
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
                    //TODO: toggle facing script action
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
