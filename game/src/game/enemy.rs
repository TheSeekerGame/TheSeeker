use crate::game::gentstate::*;
use crate::prelude::*;
use bevy_xpbd_2d::SubstepSchedule;
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
        // app.add_systems(
        //     GameTickUpdate,
        //     test_spawn.run_if(in_state(AppState::InGame)),
        // );
        // app.add_systems(OnEnter(GameState::Paused), debug_enemy);
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

//could have a GentStateSet if it doesnt need to be as granular
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

fn test_spawn(
    q: Query<&Transform, With<super::player::PlayerGent>>,
    mut commands: Commands,
    input: Res<Input<KeyCode>>,
) {
    if input.just_pressed(KeyCode::I) {
        for transform in q.iter() {
            commands
                .spawn(EnemyBlueprint)
                .insert(TransformBundle::from_transform(
                    // transform.with_translation(Vec3::new(0., 10., 0.)),
                    *transform,
                ));
        }
    }
}

fn setup_enemy(q: Query<(&Transform, Entity), Added<EnemyBlueprint>>, mut commands: Commands) {
    for (xf_gent, e_gent) in q.iter() {
        // println!("{:?} enemy", xf_gent);
        let e_gfx = commands.spawn(()).id();
        commands.entity(e_gent).insert((
            EnemyGentBundle {
                marker: EnemyGent { e_gfx },
                phys: GentPhysicsBundle {
                    rb: RigidBody::Kinematic,
                    //need to find a way to offset this one px toward back of enemys facing
                    //direction
                    collider: Collider::cuboid(22.0, 10.0),
                },
            },
            Facing::Right,
            // GentStateBundle::<Idle>::default(),
            GentStateBundle::<Walking> {
                state: Walking {
                    current_walking_ticks: 0,
                    max_walking_ticks: 300,
                },
                transitions: TransitionsFrom::<Walking>::default(),
            },
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
        // println!("enemy spawned")
    }
}

struct EnemyBehaviorPlugin;

impl Plugin for EnemyBehaviorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                enemy_walking.run_if(any_with_components::<Walking, EnemyGent>()),
                // enemy_idle.run_if(any_with_components::<Idle, EnemyGent>()),
                // enemy_run.run_if(any_with_components::<Running, EnemyGent>()),
                // enemy_jump.run_if(any_with_components::<Jumping, EnemyGent>()),
                // enemy_move,
                // enemy_grounded.run_if(any_with_components::<Grounded, EnemyGent>()),
                // enemy_falling.run_if(any_with_components::<Falling, EnemyGent>()),
                //     ),
                sprite_flip,
            )
                .run_if(in_state(AppState::InGame)),
        );
        // app.add_systems(
        //     SubstepSchedule,
        //     //probably can switch to generic collision system
        //     enemy_collisions.in_set(SubstepSet::SolveUserConstraints),
        // );
    }
}

// fn debug_enemy(world: &World, query: Query<Entity, With<EnemyGent>>) {
//     for entity in query.iter() {
//         let components = world.inspect_entity(entity);
//         for component in components.iter() {
//             println!("{:?}", component.name());
//         }
//     }
// }

//do i want it to have enum substate? or patrolling + idle + moving + retargeting
#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
struct Patrolling;
impl GentState for Patrolling {}
impl Transitionable<Aggroed> for Patrolling {}
// impl Transitionable<Idle> for Patrolling {}

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
struct Walking {
    current_walking_ticks: u32,
    max_walking_ticks: u32,
}
impl GentState for Walking {}
impl Transitionable<Idle> for Walking {}

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
struct Aggroed;
impl GentState for Aggroed {}

//ai Intents
//need way to check edges of platform
fn enemy_patrolling(query: Query<(&EnemyGent, &Facing), With<Patrolling>>) {
    for (enemy, facing) in query.iter() {

        //spatial query for range? line of sight?
        
        //facing.direction()? -> f32 -1,1
        //on player enter range, transition to agro
    }
}

//animation/behavior state
fn enemy_walking(
    mut query: Query<(
        Entity,
        &EnemyGent,
        &GlobalTransform,
        &mut Facing,
        &mut LinearVelocity,
        &mut Walking,
        &mut TransitionsFrom<Walking>,
    )>,
    spatial_query: SpatialQuery,
) {
    for (entity, enemy, g_transform, mut facing, mut velocity, mut walking, mut transitions) in
        query.iter_mut()
    {
        if walking.current_walking_ticks == 0 {
            velocity.x = -20.;
        }
        if walking.current_walking_ticks >= walking.max_walking_ticks {
            velocity.x = 0.;
            transitions.push(Walking::new_transition(Idle::default()))
        }
        let ray_origin = Vec2::new(g_transform.translation().x - 10. * facing.direction(), g_transform.translation().y - 9.);
        if let Some(first_hit) = spatial_query.cast_ray(
            //offset 10 x from center toward facing direction
            // g_transform.translation().truncate(),
            ray_origin,
            Vec2::NEG_Y,
            //change
            100.,
            true,
            //switch this to only wall/floor entities?
            SpatialQueryFilter::new().without_entities([entity]),
        ) {
            if first_hit.time_of_impact > 0.0 {
                *facing = match *facing {
                    Facing::Right => Facing::Left,
                    Facing::Left => Facing::Right,
                };
                velocity.x *= -1.
            };
            // println!("{:?}", first_hit);
        };
        walking.current_walking_ticks += 1;

        // println!("{:?}", velocity);
        //move in facing direction, update distance walked
        //when reach end of distance walked, transition out of walking
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
                (
                    transition_from::<Walking>.run_if(any_with_components::<Walking, EnemyGent>()),
                    // transition_from::<Idle>.run_if(any_with_component::<Idle>()),
                    // transition_from::<Running>.run_if(any_with_component::<Running>()),
                    // transition_from::<Grounded>.run_if(any_with_component::<Grounded>()),
                    // transition_from::<Jumping>.run_if(any_with_component::<Jumping>()),
                    // transition_from::<Falling>.run_if(any_with_component::<Falling>()),
                ),
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
                enemy_walking_animation,
                // player_falling_animation,
                // player_jumping_animation,
                // player_running_animation,
            )
                .in_set(EnemyStateSet::Animation)
                // .after(EnemyStateSet::Transition)
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
            println!("should be animating");
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
            println!("should be animating");
        }
    }
}
