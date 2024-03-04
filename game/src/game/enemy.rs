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
        app.add_plugins((
            // EnemyBehaviorPlugin,
            // EnemyTransitionPlugin,
            // EnemyAnimationPlugin,
        ));
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

fn setup_enemy(q: Query<(&Transform, Entity), Added<EnemyBlueprint>>, mut commands: Commands) {
    for (xf_gent, e_gent) in q.iter() {
        let e_gfx = commands.spawn(()).id();
        commands.entity(e_gent).insert((
            EnemyGentBundle {
                marker: EnemyGent { e_gfx },
                phys: GentPhysicsBundle {
                    rb: RigidBody::Kinematic,
                    collider: Collider::cuboid(6.0, 10.0),
                    shapecast: ShapeCaster::new(
                        Collider::cuboid(6.0, 10.0),
                        Vec2::new(0.0, -2.0),
                        0.0,
                        Vec2::NEG_Y.into(),
                    ),
                },
            },
            GentStateBundle::<Falling>::default(),
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
        // app.add_systems(
        //     GameTickUpdate,
        // (
        // enemy_idle.run_if(any_with_components::<Idle, EnemyGent>()),
        // enemy_run.run_if(any_with_components::<Running, EnemyGent>()),
        // enemy_jump.run_if(any_with_components::<Jumping, EnemyGent>()),
        // enemy_move,
        // enemy_grounded.run_if(any_with_components::<Grounded, EnemyGent>()),
        // enemy_falling.run_if(any_with_components::<Falling, EnemyGent>()),
        //     ),
        // );
        // app.add_systems(
        //     SubstepSchedule,
        //     //probably can switch to generic collision system
        //     enemy_collisions.in_set(SubstepSet::SolveUserConstraints),
        // );
    }
}

struct EnemyAnimationPlugin;

impl Plugin for EnemyAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                enemy_idle_animation,
                // player_falling_animation,
                // player_jumping_animation,
                // player_running_animation,
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
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            player.play_key("anim.enemy.Idle")
        }
    }
}
