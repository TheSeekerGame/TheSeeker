use crate::game::attack::Attack;
use crate::game::gentstate::*;
use crate::game::player::PlayerGent;
use crate::prelude::*;
use rapier2d::geometry::SharedShape;
use theseeker_engine::physics::{Collider, LinearVelocity, PhysicsWorld, ShapeCaster};
use theseeker_engine::{
    animation::SpriteAnimationBundle,
    assets::animation::SpriteAnimation,
    gent::{GentPhysicsBundle, TransformGfxFromGent},
    script::ScriptPlayer,
};

pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (setup_enemy.run_if(in_state(GameState::Playing)))
                .before(EnemyStateSet::Transition)
                .run_if(in_state(AppState::InGame)),
        );
        app.add_plugins((
            EnemyBehaviorPlugin,
            EnemyTransitionPlugin,
            EnemyAnimationPlugin,
        ));
    }
}

pub fn debug_enemy(world: &World, query: Query<Entity, With<EnemyGent>>) {
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
    Transition,
    Animation,
}

#[derive(Bundle, LdtkEntity, Default)]
pub struct EnemyBlueprintBundle {
    marker: EnemyBlueprint,
}

#[derive(Component, Default)]
pub struct EnemyBlueprint;

#[derive(Bundle)]
pub struct EnemyGentBundle {
    marker: EnemyGent,
    phys: GentPhysicsBundle,
}

#[derive(Component)]
pub struct EnemyGent {
    e_gfx: Entity,
}

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

fn setup_enemy(
    mut q: Query<(&mut Transform, Entity), Added<EnemyBlueprint>>,
    mut commands: Commands,
) {
    for (mut xf_gent, e_gent) in q.iter_mut() {
        //TODO: ensure propper z order
        xf_gent.translation.z = 14.;
        // println!("{:?} enemy", xf_gent);
        let e_gfx = commands.spawn(()).id();
        commands.entity(e_gent).insert((
            EnemyGentBundle {
                marker: EnemyGent { e_gfx },
                phys: GentPhysicsBundle {
                    //need to find a way to offset this one px toward back of enemys facing
                    //direction
                    collider: Collider::cuboid(22.0, 10.0),
                    shapecast: ShapeCaster {
                        shape: SharedShape::cuboid(22.0, 10.0),
                        // Vec2::NEG_Y.into(),,
                        vec: Vec2::NEG_Y,
                        offset: Vec2::new(0.0, -2.0),
                        max_toi: 0.0,
                    },
                },
            },
            Role::Melee,
            Facing::Right,
            AddQueue::default(),
            Patrolling::default(),
            Walking {
                current_walking_ticks: 0,
                max_walking_ticks: 300,
            }, // transitions: TransitionsFrom::<Walking>::default(),
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
                (
                    (
                        patrolling.run_if(any_with_component::<Patrolling>),
                        aggro.run_if(any_with_component::<Aggroed>),
                        waiting.run_if(any_with_component::<Waiting>),
                        ranged_attack.run_if(any_with_component::<RangedAttack>),
                        melee_attack.run_if(any_with_component::<MeleeAttack>),
                    ),
                    walking.run_if(any_with_component::<Walking>),
                )
                    .chain(),
                sprite_flip,
            )
                .chain()
                .run_if(in_state(AppState::InGame)),
        );
    }
}

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
struct Patrolling;
impl GentState for Patrolling {}
impl Transitionable<Aggroed> for Patrolling {
    type Removals = (Patrolling, Waiting);
}

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
struct Walking {
    current_walking_ticks: u32,
    max_walking_ticks: u32,
}
impl GentState for Walking {}
impl GenericState for Walking {}

#[derive(Component, Debug)]
#[component(storage = "SparseSet")]
struct Aggroed {
    target: Entity,
}
impl GentState for Aggroed {}
impl Transitionable<Patrolling> for Aggroed {
    type Removals = (Aggroed, RangedAttack);
}

#[derive(Component, Debug)]
#[component(storage = "SparseSet")]
struct RangedAttack {
    target: Entity,
}
impl GentState for RangedAttack {}

#[derive(Component, Debug)]
#[component(storage = "SparseSet")]
struct MeleeAttack {
    target: Entity,
    current_ticks: u32,
}
impl MeleeAttack {
    //frames
    const STARTUP: u32 = 7;
    const RECOVERY: u32 = 9;
    const MAX: u32 = 10;
}
impl GentState for MeleeAttack {}
impl GenericState for MeleeAttack {}

#[derive(Component, Default)]
#[component(storage = "SparseSet")]
struct Waiting {
    current_waiting_ticks: u32,
    max_waiting_ticks: u32,
}
impl GentState for Waiting {}
impl Transitionable<Walking> for Waiting {
    type Removals = (Waiting, Idle);
}

#[derive(Component)]
enum Role {
    Melee,
    Ranged,
}

//check if in group

//ai Intents
//need way to check edges of platform
fn patrolling(
    mut query: Query<
        (
            &EnemyGent,
            &GlobalTransform,
            &Facing,
            &mut TransitionQueue,
            Option<&Waiting>,
        ),
        With<Patrolling>,
    >,
    player_query: Query<(Entity, &GlobalTransform), (Without<EnemyGent>, With<PlayerGent>)>,
) {
    let aggro_distance = 60.;
    if let Ok((player_gent, player_trans)) = player_query.get_single() {
        for (enemy, trans, facing, mut transitions, maybe_waiting) in query.iter_mut() {
            let distance = trans
                .translation()
                .truncate()
                .distance(player_trans.translation().truncate());
            //if player comes close, aggro
            //
            if distance < aggro_distance {
                println!("should transition to aggroed");
                transitions.push(Patrolling::new_transition(Aggroed {
                    target: player_gent,
                }));
            } else if maybe_waiting.is_some() {
                let waiting = maybe_waiting.unwrap();
                if waiting.current_waiting_ticks >= waiting.max_waiting_ticks {
                    transitions.push(Waiting::new_transition(Walking {
                        max_walking_ticks: 240,
                        current_walking_ticks: 0,
                    }));
                }
            }
        }
    }
}

fn waiting(mut query: Query<(&mut Waiting), With<EnemyGent>>) {
    for mut waiting in query.iter_mut() {
        waiting.current_waiting_ticks += 1;
    }
}

//in aggro the enemy should not turn around at edge, rather pause
fn aggro(
    mut query: Query<
        (
            &Aggroed,
            &mut Facing,
            &GlobalTransform,
            &mut TransitionQueue,
            &mut AddQueue,
            Has<MeleeAttack>,
        ),
        (With<EnemyGent>),
    >,
    player_query: Query<(&GlobalTransform), (Without<EnemyGent>, With<PlayerGent>)>,
) {
    let aggro_distance = 60.;
    for (aggroed, mut facing, trans, mut transitions, mut add_q, maybe_attacking) in
        query.iter_mut()
    {
        if let Ok(player_trans) = player_query.get(aggroed.target) {
            //face player
            if trans.translation().x > player_trans.translation().x {
                *facing = Facing::Right;
            } else if trans.translation().x < player_trans.translation().x {
                *facing = Facing::Left;
            }
            //return to patrol
            let distance = trans
                .translation()
                .truncate()
                .distance(player_trans.translation().truncate());
            if distance > aggro_distance {
                //and if not mid attack?
                transitions.push(Aggroed::new_transition(
                    Patrolling::default(),
                ));
                add_q.add(Waiting::default());
            } else if !maybe_attacking {
                // transitions.push(Walking::new_transition(RangedAttack {
                //     target: aggroed.target,
                // }));
                transitions.push(Walking::new_transition(MeleeAttack {
                    target: aggroed.target,
                    current_ticks: 0,
                }));
            }
        }
        //if there is no player it should also return to patrol state
    }
}

fn ranged_attack(mut query: Query<(&RangedAttack, &mut LinearVelocity), With<EnemyGent>>) {
    for (attack, mut velocity) in query.iter_mut() {
        velocity.x = 0.;
    }
}

fn melee_attack(
    mut query: Query<
        (
            Entity,
            &mut MeleeAttack,
            &mut LinearVelocity,
            &Facing,
            &GlobalTransform,
            &mut TransitionQueue,
        ),
        With<EnemyGent>,
    >,
    mut commands: Commands,
) {
    for (entity, mut attack, mut velocity, facing, transform, mut trans_q) in query.iter_mut() {
        velocity.x = 0.;
        attack.current_ticks += 1;
        //tick till end of attack startup frames (looks to be end of frame 7)
        // println!(
        //     "current_ticks {:?}",
        //     attack.current_ticks
        // );
        // println!("startup {:?}", MeleeAttack::STARTUP);
        // if attack.current_ticks % 8 * MeleeAttack::STARTUP == 0 {
        if attack.current_ticks == 8 * MeleeAttack::STARTUP {
            //spawn attack hitbox collider as child
            println!("collider should be spawned");
            //why isnt transform working after setting parent?
            let collider = commands
                .spawn((
                    Collider::cuboid(28., 10.),
                    Attack::new(8),
                    // TransformBundle::from_transform(Transform::from_xyz(
                    //     10. * facing.direction() + transform.translation().x,
                    //     transform.translation().y,
                    //     0.,
                    // )),
                ))
                .set_parent(entity)
                .id();
            // commands.entity(entity).add_child(collider);
        }
        if attack.current_ticks >= MeleeAttack::MAX * 8 {
            trans_q.push(MeleeAttack::new_transition(
                Waiting::default(),
            ))
        }

        //intersection check, check against player collider
        //if hit, deal damage
    }
}

//animation/behavior state
fn walking(
    mut query: Query<(
        Entity,
        &EnemyGent,
        &GlobalTransform,
        &mut Facing,
        &mut LinearVelocity,
        &mut Walking,
        &mut TransitionQueue,
        &mut AddQueue,
        Option<&Aggroed>,
    )>,
    spatial_query: Res<PhysicsWorld>,
) {
    for (
        entity,
        enemy,
        g_transform,
        mut facing,
        mut velocity,
        mut walking,
        mut transitions,
        mut add_q,
        maybe_aggroed,
    ) in query.iter_mut()
    {
        // if walking.current_walking_ticks == 0 {
        //     velocity.x = -20. * facing.direction();
        // }
        velocity.x = -20. * facing.direction();
        if walking.current_walking_ticks >= walking.max_walking_ticks {
            velocity.x = 0.;
            transitions.push(Walking::new_transition(Waiting {
                current_waiting_ticks: 0,
                max_waiting_ticks: 240,
            }));
            add_q.add(Idle::default());
            continue;
        }
        let ray_origin = Vec2::new(
            g_transform.translation().x - 10. * facing.direction(),
            g_transform.translation().y - 9.,
        );
        if let Some(first_hit) = spatial_query.cast_ray(
            //offset 10 x from center toward facing direction
            // g_transform.translation().truncate(),
            ray_origin,
            Direction2d::NEG_Y,
            //change
            100.,
            true,
            //switch this to only wall/floor entities?
            //TODO: use layers
            SpatialQueryFilter::from_excluded_entities([entity]),
        ) {
            if first_hit.time_of_impact > 0.0 {
                //if not aggro turn around to walk away from edge
                if maybe_aggroed.is_none() {
                    *facing = match *facing {
                        Facing::Right => Facing::Left,
                        Facing::Left => Facing::Right,
                    };
                    velocity.x *= -1.;
                } else {
                    velocity.x = 0.;
                }
            };
            // println!("{:?}", first_hit);
        };
        walking.current_walking_ticks += 1;
    }
}

fn sprite_flip(
    query: Query<(&Facing, &EnemyGent)>,
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

struct EnemyTransitionPlugin;

impl Plugin for EnemyTransitionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                transition.run_if(any_matching::<(
                    With<TransitionQueue>,
                    With<EnemyGent>,
                )>()),
                add_states.run_if(any_matching::<(
                    With<AddQueue>,
                    With<EnemyGent>,
                )>()),
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
                enemy_walking_animation,
                enemy_ranged_attack_animation,
                enemy_melee_attack_animation,
            )
                .in_set(EnemyStateSet::Animation)
                .after(EnemyStateSet::Transition)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

fn enemy_idle_animation(
    i_query: Query<&EnemyGent, Added<Idle>>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    for gent in i_query.iter() {
        if let Ok(mut enemy) = gfx_query.get_mut(gent.e_gfx) {
            enemy.play_key("anim.spider.Idle");
        }
    }
}

fn enemy_walking_animation(
    i_query: Query<&EnemyGent, Added<Walking>>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    for gent in i_query.iter() {
        if let Ok(mut enemy) = gfx_query.get_mut(gent.e_gfx) {
            enemy.play_key("anim.spider.Walk");
        }
    }
}

fn enemy_ranged_attack_animation(
    i_query: Query<&EnemyGent, Added<RangedAttack>>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    for gent in i_query.iter() {
        if let Ok(mut enemy) = gfx_query.get_mut(gent.e_gfx) {
            enemy.play_key("anim.spider.RangedAttack");
        }
    }
}

fn enemy_melee_attack_animation(
    i_query: Query<&EnemyGent, Added<MeleeAttack>>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
) {
    for gent in i_query.iter() {
        if let Ok(mut enemy) = gfx_query.get_mut(gent.e_gfx) {
            enemy.play_key("anim.spider.OffensiveAttack");
        }
    }
}
