#[cfg(feature = "dev")]
use bevy_inspector_egui::quick::FilterQueryInspectorPlugin;
use rand::distributions::Standard;
use rapier2d::geometry::SharedShape;
use rapier2d::parry::query::TOIStatus;
use rapier2d::prelude::{Group, InteractionGroups};
use theseeker_engine::animation::SpriteAnimationBundle;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::ballistics_math::ballistic_speed;
use theseeker_engine::gent::{Gent, GentPhysicsBundle, TransformGfxFromGent};
use theseeker_engine::physics::{
    into_vec2, update_sprite_colliders, AnimationCollider, Collider,
    LinearVelocity, PhysicsWorld, ShapeCaster, ENEMY, ENEMY_ATTACK,
    ENEMY_INSIDE, GROUND, PLAYER, SENSOR,
};
use theseeker_engine::script::ScriptPlayer;

use super::physics::Knockback;
use super::player::{Player, PlayerConfig, StatusModifier, Stealthing};
use crate::game::attack::particles::ArcParticleEffectHandle;
use crate::game::attack::*;
use crate::game::gentstate::*;
use crate::game::{attack::arc_attack::Projectile, player::EnemiesNearby};
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
    const MAX: usize = 5;
    const COOLDOWN: u32 = 620;
    const RANGE: f32 = 500.;

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
#[component(storage = "SparseSet")]
pub struct EnemyBlueprint {
    /// Hp added from spawner due to number of enemies killed.
    bonus_hp: u32,
}

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

#[derive(Bundle)]
pub struct EnemyEffectsGfxBundle {
    marker: EnemyEffectGfx,
    gent2gfx: TransformGfxFromGent,
    sprite: SpriteSheetBundle,
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

//TODO:only spawn when all from spawner have died, increase scaling, when 5 are cleared, up spider
//tier one at a time
//when ranged should be capped at 2 per spawner
//only tick cooldown when spawner is cleared
fn spawn_enemy(
    mut spawner_q: Query<(&Transform, &mut EnemySpawner)>,
    //dead enemies to clear
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
                    spawner.threshold_next = thread_rng().gen_range(2..4);
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
                            //generate a random roll, max 2 per spawner
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
                                    EnemyBlueprintBundle {
                                        marker: EnemyBlueprint {
                                            bonus_hp: 20 * killed,
                                        },
                                    },
                                    slot.tier.clone(),
                                    role,
                                    TransformBundle::from_transform(*transform),
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
        Entity,
        Ref<EnemyBlueprint>,
    )>,
    mut commands: Commands,
) {
    for (mut xf_gent, tier, e_gent, bp) in q.iter_mut() {
        if !bp.is_added() {
            continue;
        }
        // TODO: ensure proper z order
        xf_gent.translation.z = 14.0 * 0.000001;
        xf_gent.translation.y += 2.0; // Sprite offset so it looks like it is standing on the ground
        let health = (100 + bp.bonus_hp) * *tier as u32;
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
                    // need to find a way to offset this one px toward back of enemys facing
                    // direction
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
                current: health,
                max: health,
            },
            Facing::Right,
            Patrolling,
            Idle,
            Waiting::new(12),
            AddQueue::default(),
            TransitionQueue::default(), // },
            StateDespawnMarker,
        ));
        commands.entity(e_gfx).insert((
            EnemyGfxBundle {
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
            },
            StateDespawnMarker,
        ));
        let mut animation: ScriptPlayer<SpriteAnimation> =
            ScriptPlayer::default();
        animation.play_key("anim.spider.Sparks");
        animation.set_slot("Start", true);
        commands.entity(e_effects_gfx).insert((
            EnemyEffectsGfxBundle {
                marker: EnemyEffectGfx { e_gent },
                gent2gfx: TransformGfxFromGent {
                    pixel_aligned: false,
                    gent: e_gent,
                },
                sprite: SpriteSheetBundle {
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
                        // assign_group,
                        check_player_range,
                        (
                            patrolling.run_if(any_with_component::<Patrolling>),
                            aggro.run_if(any_with_component::<Aggroed>),
                            waiting.run_if(any_with_component::<Waiting>),
                            defense.run_if(any_with_component::<Defense>),
                            ranged_attack
                                .run_if(any_with_component::<RangedAttack>),
                            melee_attack
                                .run_if(any_with_component::<MeleeAttack>),
                            // pushback_attack
                            //     .run_if(any_with_component::<PushbackAttack>),
                        ),
                        (
                            walking.run_if(any_with_component::<Walking>),
                            // retreating.run_if(any_with_component::<Retreating>),
                            chasing.run_if(any_with_component::<Chasing>),
                        ),
                    )
                        .chain(),
                )
                    .run_if(in_state(AppState::InGame))
                    .in_set(EnemyStateSet::Behavior)
                    .before(update_sprite_colliders),
                (move_collide, remove_inside)
                    .chain()
                    .in_set(EnemyStateSet::Collisions),
            ),
        );
    }
}

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
struct Patrolling;
impl GentState for Patrolling {}
impl Transitionable<Aggroed> for Patrolling {
    type Removals = Patrolling;
}

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
struct Walking {
    ticks: u32,
    max_ticks: u32,
}
impl GentState for Walking {}
impl GenericState for Walking {}

// #[derive(Component, Debug)]
// #[component(storage = "SparseSet")]
// struct Retreating {
//     ticks: u32,
//     max_ticks: u32,
// }
// impl GentState for Retreating {}
// impl GenericState for Retreating {}

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
    type Removals = Aggroed;
}

#[derive(Component, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct Defense;
// pub struct Defense {
//     cooldown_ticks: u32,
// }
// impl Defense {
//     const COOLDOWN: u32 = 30;
// }

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
    // const MAX: u32 = 10;
    // const STARTUP: u32 = 7;
    const MAX: u32 = 5;
    const STARTUP: u32 = 3;
}
impl GentState for MeleeAttack {}
impl GenericState for MeleeAttack {}

// #[derive(Component, Default, Debug)]
// #[component(storage = "SparseSet")]
// struct PushbackAttack {
//     ticks: u32,
// }
// impl PushbackAttack {
//     // const RECOVERY: u32 = 7;
//     const MAX: u32 = 10;
//     const STARTUP: u32 = 5;
// }
// impl GentState for PushbackAttack {}
// impl GenericState for PushbackAttack {}

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

// Spider upgrade/scaling tier
#[derive(Component, Default, Debug, Reflect, Clone, Copy)]
enum Tier {
    #[default]
    Base = 1,
    Two = 2,
    Three = 3,
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
impl Role {
    pub fn check_range(&self, distance: f32) -> Range {
        match self {
            Role::Melee => {
                if distance <= Range::MELEE_MELEE {
                    Range::Melee
                } else if distance <= Range::MELEE_AGGRO {
                    Range::Aggro
                } else if distance <= Range::MELEE_DEAGGRO {
                    Range::Deaggro
                } else {
                    Range::Far
                }
            },
            Role::Ranged => {
                if distance <= Range::RANGED_MELEE {
                    Range::Melee
                } else if distance <= Range::RANGED_AGGRO {
                    Range::Aggro
                } else if distance <= Range::RANGED_RANGED {
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

impl Range {
    const RANGED_AGGRO: f32 = 50.;
    // const RANGED_DEAGGRO: f32 = 70.;
    const RANGED_MELEE: f32 = 20.;
    const RANGED_RANGED: f32 = 60.;

    const MELEE_AGGRO: f32 = 50.;
    const MELEE_DEAGGRO: f32 = 70.;
    const MELEE_MELEE: f32 = 12.;
    // const MELEE_RANGED: f32 = 60.;

    // for players passive enemies nearby buff
    const NEARBY: f32 = 20.;
}

/// Component that indicates that the player is inside of this enemy,
/// and has its usual collision layer membership modified to ENEMY_INSIDE
/// it is removed once the player stops intersecting in the remove_inside system
#[derive(Component)]
pub struct Inside;

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
            Has<MeleeAttack>,
            Has<Defense>,
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
        //reset every tick
        **enemies_nearby = 0;

        for (
            mut range,
            mut target,
            mut facing,
            role,
            trans,
            is_aggroed,
            is_meleeing,
            is_defending,
        ) in query.iter_mut()
        {
            //TODO: still update enemies nearby in stealth?
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
            if is_aggroed && !is_meleeing && !is_defending {
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
            //set nearby enemies for passive buff
            if distance < Range::NEARBY {
                **enemies_nearby += 1;
            };
        }
    // if there is no player
    } else {
        for (mut range, mut target, _, _, _, _, _, _) in query.iter_mut() {
            *range = Range::None;
            target.0 = None;
        }
    }
}

// check if any other enemies are nearby, if so assign to group
// fn _assign_group(
//     query: Query<(Entity, &GlobalTransform, Has<Grouped>), With<Enemy>>,
//     spatial_query: Res<PhysicsWorld>,
//     mut commands: Commands,
// ) {
//     for (entity, transform, is_grouped) in query.iter() {
//         let project_from = transform.translation().truncate();
//         if let Some((other, projection)) = spatial_query.point_project(
//             project_from,
//             InteractionGroups::new(SENSOR, ENEMY_HURT),
//             Some(entity),
//         ) {
//             let closest = project_from
//                 .distance([projection.point.x, projection.point.y].into());
//             if closest < Range::GROUPED && !is_grouped {
//                 commands.entity(entity).insert(Grouped);
//             } else if closest >= Range::GROUPED && is_grouped {
//                 commands.entity(entity).remove::<Grouped>();
//             }
//         } else {
//             commands.entity(entity).remove::<Grouped>();
//         };
//     }
// }

// #[derive(Component, Debug)]
// struct Grouped;

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

fn waiting(mut query: Query<&mut Waiting, With<Enemy>>) {
    for mut waiting in query.iter_mut() {
        waiting.ticks += 1;
    }
}

fn defense(
    mut query: Query<
        (&Range, &mut TransitionQueue),
        (With<Enemy>, With<Defense>),
    >,
) {
    for (range, mut transitions) in query.iter_mut() {
        if !matches!(range, Range::Melee) {
            transitions.push(Defense::new_transition(
                Waiting::default(),
            ));
        }
    }
}

// fn pushback_attack(
//     mut query: Query<
//         (
//             Entity,
//             &Facing,
//             &Gent,
//             &mut PushbackAttack,
//             &mut TransitionQueue,
//         ),
//         With<Enemy>,
//     >,
//     mut commands: Commands,
// ) {
//     for (entity, facing, gent, mut attack, mut transitions) in query.iter_mut()
//     {
//         attack.ticks += 1;
//         if attack.ticks == PushbackAttack::STARTUP * 8 {
//             commands
//                 .spawn((
//                     Collider::empty(InteractionGroups {
//                         memberships: SENSOR,
//                         filter: PLAYER,
//                     }),
//                     TransformBundle::from_transform(Transform::default()),
//                     AnimationCollider(gent.e_gfx),
//                     Attack::new(8, entity),
//                     Pushback(Knockback::new(
//                         Vec2::new(-facing.direction() * 100., 0.),
//                         16,
//                     )),
//                 ))
//                 .set_parent(entity);
//         }
//         if attack.ticks >= PushbackAttack::MAX * 8 {
//             transitions.push(PushbackAttack::new_transition(
//                 Defense::default(),
//             ))
//         }
//     }
// }

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
            // each "substate" of aggro should return back to waiting when with wants to return control
            // to aggro
            With<Waiting>,
        ),
    >,
) {
    for (role, range, target, mut velocity, mut transitions) in query.iter_mut()
    {
        if let Some(p_entity) = target.0 {
            let mut rng = rand::thread_rng();
            // return to patrol if out of aggro range
            if matches!(range, Range::Far) {
                transitions.push(Aggroed::new_transition(Patrolling));
            } else if matches!(range, Range::Melee) {
                match role {
                    Role::Melee => transitions.push(Waiting::new_transition(
                        MeleeAttack::default(),
                    )),
                    Role::Ranged => {
                        velocity.x = 0.;
                        transitions.push(Waiting::new_transition(
                            Defense::default(),
                        ));
                    },
                }
            } else if matches!(role, Role::Melee) {
                transitions.push(Waiting::new_transition(Chasing));
            } else if matches!(role, Role::Ranged) {
                transitions.push(Waiting::new_transition(RangedAttack {
                    target: p_entity,
                    ticks: 0,
                }));
            }

            // if there is no player it should also return to patrol state
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
            &Tier,
            &mut RangedAttack,
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
        tier,
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
            trans_q.push(RangedAttack::new_transition(
                Waiting::default(),
            ));
            add_q.add(Idle);
        }
        // if player isnt alive, do nothing, we will transition back once animation finishes
        let Ok(transform) = player_query.get(attack.target) else {
            continue;
        };
        if attack.ticks == RangedAttack::STARTUP * 8 {
            // cast a ray midway between enemy and player to find height of ceiling
            // we want to avoid
            let mid_pt = (enemy_transform.translation().xy()
                + transform.translation.xy())
                * 0.5;
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
                    ceiling =
                        mid_pt.y + hit.toi - enemy_transform.translation().y;
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

            let relative_height =
                enemy_transform.translation().y - transform.translation.y;
            let delta_x =
                transform.translation.x - enemy_transform.translation().x;
            let gravity = config.fall_accel * time.hz as f32;
            let rng_factor = 1.0;
            let mut speed = ballistic_speed(
                Range::RANGED_RANGED,
                gravity,
                relative_height,
            ) * rng_factor as f32;
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
                } else {
                    if i == max_attempts - 1 {
                        warn!("No solution for ballistic trajectory, even after increased speed to {speed}, using default trajectory!")
                    } else {
                        speed *= 1.15;
                    }
                }
            }
            // spawn in the new projectile:
            commands
                .spawn((
                    Attack::new(1000, entity, 20. * *tier as u32 as f32)
                        .set_stat_mod(StatusModifier::basic_ice_spider()),
                    final_solution,
                    Collider::cuboid(
                        5.,
                        5.,
                        InteractionGroups::new(ENEMY_ATTACK, PLAYER),
                    ),
                    TransformBundle::from(Transform::from_translation(
                        enemy_transform.translation(),
                    )),
                ))
                .with_lingering_particles(particle_effect.0.clone());
        }
        if matches!(range, Range::Melee) {
            trans_q.push(RangedAttack::new_transition(
                Defense::default(),
            ));
        }
    }
}

fn melee_attack(
    mut query: Query<
        (
            Entity,
            &mut MeleeAttack,
            &Tier,
            &Facing,
            &mut TransitionQueue,
            &Gent,
        ),
        With<Enemy>,
    >,
    mut commands: Commands,
) {
    for (entity, mut attack, tier, facing, mut trans_q, gent) in
        query.iter_mut()
    {
        attack.ticks += 1;
        if attack.ticks == 8 * MeleeAttack::STARTUP {
            // spawn attack hitbox collider as child
            let collider = commands
                .spawn((
                    Collider::empty(InteractionGroups {
                        memberships: SENSOR,
                        filter: PLAYER,
                    }),
                    TransformBundle::from_transform(Transform::default()),
                    AnimationCollider(gent.e_gfx),
                    Attack::new(8, entity, 20. * *tier as u32 as f32),
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
            // TODO: remove addqueue
        ),
        (
            With<Enemy>,
            // Without<Retreating>,
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
            Navigation::Grounded => {},
        }
        walking.ticks += 1;
    }
}

// fn retreating(
//     mut query: Query<
//         (
//             &Range,
//             &Facing,
//             &mut Navigation,
//             &mut LinearVelocity,
//             &mut Retreating,
//             &mut TransitionQueue,
//         ),
//         (
//             With<Enemy>,
//             Without<Walking>,
//             Without<Knockback>,
//         ),
//     >,
//     player_query: Query<Entity, With<Player>>,
// ) {
//     for (
//         range,
//         facing,
//         mut nav,
//         mut velocity,
//         mut retreating,
//         mut transitions,
//     ) in query.iter_mut()
//     {
//         velocity.x = 12. * facing.direction();
//         if matches!(*nav, Navigation::Blocked)
//             || retreating.ticks > retreating.max_ticks
//         {
//             velocity.x = 0.;
//             *nav = Navigation::Grounded;
//             match range {
//                 Range::Melee => {
//                     transitions.push(Retreating::new_transition(
//                         Defense::default(),
//                     ));
//                 },
//                 Range::Ranged | Range::Aggro => transitions.push(
//                     Retreating::new_transition(RangedAttack {
//                         target: player_query.get_single().expect("no player"),
//                         ticks: 0,
//                     }),
//                 ),
//                 _ => {
//                     transitions.push(Retreating::new_transition(
//                         // RangedAttack {
//                         //     target: player_query.get_single().expect("no player"),
//                         //     ticks: 0,
//                         // },
//                         Waiting::default(),
//                     ))
//                 },
//             }
//         } else if matches!(range, Range::Melee) {
//             velocity.x = 0.;
//             transitions.push(Retreating::new_transition(
//                 Defense::default(),
//             ));
//         } else if matches!(range, Range::Ranged)
//             || matches!(range, Range::Aggro)
//         {
//             velocity.x = 0.;
//             transitions.push(Retreating::new_transition(
//                 RangedAttack {
//                     target: player_query.get_single().expect("no player"),
//                     ticks: 0,
//                 },
//             ));
//         }
//
//         retreating.ticks += 1;
//     }
// }

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
        (
            With<Enemy>,
            With<Chasing>,
            Without<Knockback>,
        ),
    >,
) {
    for (target, facing, role, range, mut nav, mut velocity, mut transitions) in
        query.iter_mut()
    {
        // only melee chase
        if !matches!(role, Role::Melee) {
            continue;
        }
        if let Some(p_entity) = target.0 {
            // check if we need to transition
            match *range {
                Range::Melee => {
                    velocity.x = 0.;
                    transitions.push(Chasing::new_transition(
                        MeleeAttack::default(),
                    ));
                },
                Range::Ranged | Range::Aggro | Range::Deaggro => {
                    velocity.x = -35. * facing.direction();
                    // if we cant get any closer because of edge
                    if let Navigation::Blocked = *nav {
                        velocity.x = 0.;
                        *nav = Navigation::Grounded;
                        // println!("chasing but blocked");
                        transitions.push(Chasing::new_transition(
                            Waiting::default(),
                        ));
                    }
                },
                _ => {
                    velocity.x = 0.;
                    transitions.push(Chasing::new_transition(
                        Waiting::default(),
                    ))
                },
            }
        // if there is no target, stop chasing
        } else {
            velocity.x = 0.;
            transitions.push(Chasing::new_transition(
                Waiting::default(),
            ));
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
    for (mut linear_velocity, mut transform, mut nav, collider, is_knocked) in
        query.iter_mut()
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
        while let Ok(shape_dir) = Direction2d::new(linear_velocity.0) {
            if let Some((e, first_hit)) = spatial_query.shape_cast(
                transform.translation.xy(),
                shape_dir,
                &*shape,
                linear_velocity.length() / time.hz as f32 + 0.5,
                InteractionGroups {
                    memberships: ENEMY,
                    // combination of the PLAYER + GROUND Groups
                    filter: Group::from_bits_truncate(0b10001),
                },
                None,
            ) {
                if first_hit.status != TOIStatus::Penetrating {
                    let sliding_plane = into_vec2(first_hit.normal1);
                    projected_velocity = linear_velocity.xy()
                        - sliding_plane
                            * linear_velocity.xy().dot(sliding_plane);
                    linear_velocity.0 = projected_velocity;
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

        // Raycast from underground directly below the enemy in direction of movement, detecting the edges of a platform from
        // inside
        if let Some((entity, first_hit)) = spatial_query.ray_cast(
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
            if !is_knocked {
                *nav = Navigation::Blocked;
            }
            projected_velocity.x = first_hit.toi * dir;
        }

        transform.translation = (transform.translation.xy()
            + projected_velocity * (1.0 / time.hz as f32))
            .extend(z);
    }
}

fn remove_inside(
    mut query: Query<
        (Entity, &GlobalTransform, &mut Collider),
        (With<Inside>, With<Enemy>),
    >,
    mut commands: Commands,
    spatial_query: Res<PhysicsWorld>,
) {
    for (entity, transform, mut collider) in query.iter_mut() {
        let intersections = spatial_query.intersect(
            transform.translation().xy(),
            collider.0.shape(),
            InteractionGroups {
                memberships: ENEMY_INSIDE,
                filter: PLAYER,
            },
            Some(entity),
        );
        if intersections.is_empty() {
            collider.0.set_collision_groups(InteractionGroups {
                memberships: ENEMY,
                filter: Group::all(),
            });
            commands.entity(entity).remove::<Inside>();
        }
    }
}

/// Increments the global KillCount, removes most components from the Enemy
/// after a set amount of ticks transitions to the Decay state
pub fn dead(
    mut query: Query<(Entity, &mut Dead, &Gent), With<Enemy>>,
    mut kill_count: ResMut<KillCount>,
    mut commands: Commands,
) {
    for (entity, mut dead, gent) in query.iter_mut() {
        if dead.ticks == 0 {
            **kill_count += 1;
            commands.entity(entity).retain::<(
                TransformBundle,
                Gent,
                Dead,
                Enemy,
                Role,
                Tier,
            )>();
        }
        if dead.ticks == 8 * 7 {
            commands.entity(entity).remove::<Dead>().insert(Decay);
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
                enemy_idle_animation,
                enemy_defense_animation,
                enemy_walking_animation,
                enemy_chasing_animation,
                // enemy_retreat_animation,
                enemy_ranged_attack_animation,
                enemy_melee_attack_animation,
                // enemy_pushback_attack_animation,
                enemy_death_animation,
                enemy_decay_animation,
                enemy_sparks_on_hit_animation,
                enemy_decay_visibility,
                sprite_flip,
            )
                .in_set(EnemyStateSet::Animation)
                .after(EnemyStateSet::Transition)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

fn enemy_sparks_on_hit_animation(
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
                hit_gfx.clear_slots();
                hit_gfx.set_slot(
                    format!("Spark{picked_spark}").as_str(),
                    true,
                );
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

fn enemy_idle_animation(
    i_query: Query<(&Gent, &Role, &Tier), (Added<Idle>, With<Enemy>)>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    for (gent, role, tier) in i_query.iter() {
        if let Ok(mut enemy) = gfx_query.get_mut(gent.e_gfx) {
            enemy.play_key((enemy_anim_prefix(role, tier) + ".Idle").as_str());
        }
    }
}

fn enemy_walking_animation(
    i_query: Query<(&Gent, &Role, &Tier), (Added<Walking>, With<Enemy>)>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    for (gent, role, tier) in i_query.iter() {
        if let Ok(mut enemy) = gfx_query.get_mut(gent.e_gfx) {
            enemy.play_key((enemy_anim_prefix(role, tier) + ".Walk").as_str());
        }
    }
}

fn enemy_chasing_animation(
    i_query: Query<(&Gent, &Tier), (Added<Chasing>, With<Enemy>)>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    for (gent, tier) in i_query.iter() {
        if let Ok(mut enemy) = gfx_query.get_mut(gent.e_gfx) {
            let key = match tier {
                Tier::Base => "anim.smallspider.Chase",
                Tier::Two => "anim.smallspider2.Chase",
                Tier::Three => "anim.smallspider3.Chase",
            };
            enemy.play_key(key);
        }
    }
}

fn enemy_ranged_attack_animation(
    i_query: Query<(&Gent, &Tier), (Added<RangedAttack>, With<Enemy>)>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    for (gent, tier) in i_query.iter() {
        if let Ok(mut enemy) = gfx_query.get_mut(gent.e_gfx) {
            let key = match tier {
                Tier::Base => "anim.spider.RangedAttack",
                Tier::Two => "anim.spider2.RangedAttack",
                Tier::Three => "anim.spider3.RangedAttack",
            };
            enemy.play_key(key);
        }
    }
}

fn enemy_melee_attack_animation(
    i_query: Query<(&Gent, &Tier), (Added<MeleeAttack>, With<Enemy>)>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    for (gent, tier) in i_query.iter() {
        if let Ok(mut enemy) = gfx_query.get_mut(gent.e_gfx) {
            let key = match tier {
                Tier::Base => "anim.smallspider.MeleeAttack",
                Tier::Two => "anim.smallspider2.MeleeAttack",
                Tier::Three => "anim.smallspider3.MeleeAttack",
            };
            enemy.play_key(key);
        }
    }
}

// fn enemy_pushback_attack_animation(
//     i_query: Query<&Gent, (Added<PushbackAttack>, With<Enemy>)>,
//     mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
// ) {
//     for gent in i_query.iter() {
//         if let Ok(mut enemy) = gfx_query.get_mut(gent.e_gfx) {
//             enemy.play_key("anim.spider.DefensiveAttack");
//         }
//     }
// }

fn enemy_defense_animation(
    i_query: Query<(&Gent, &Tier), (Added<Defense>, With<Enemy>)>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    for (gent, tier) in i_query.iter() {
        if let Ok(mut enemy) = gfx_query.get_mut(gent.e_gfx) {
            let key = match tier {
                Tier::Base => "anim.spider.Defense",
                Tier::Two => "anim.spider2.Defense",
                Tier::Three => "anim.spider3.Defense",
            };
            enemy.play_key(key);
        }
    }
}

// no longer used?
// fn enemy_retreat_animation(
//     i_query: Query<&Gent, (Added<Retreating>, With<Enemy>)>,
//     mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
// ) {
//     for gent in i_query.iter() {
//         if let Ok(mut enemy) = gfx_query.get_mut(gent.e_gfx) {
//             enemy.play_key("anim.spider.Retreat");
//         }
//     }
// }

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

fn enemy_anim_prefix(role: &Role, tier: &Tier) -> String {
    let r = match role {
        Role::Ranged => "spider",
        Role::Melee => "smallspider",
    };
    let t = match tier {
        Tier::Base => "",
        Tier::Two => "2",
        Tier::Three => "3",
    };
    "anim.".to_owned() + r + t
}

fn sprite_flip(
    query: Query<(&Facing, &Gent), With<Enemy>>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    for (facing, gent) in query.iter() {
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            match facing {
                Facing::Right => {
                    // TODO: toggle facing script action
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
