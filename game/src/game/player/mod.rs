mod player_anim;
mod player_behaviour;

use bevy::transform::TransformSystem::TransformPropagate;
use leafwing_input_manager::axislike::VirtualAxis;
use leafwing_input_manager::prelude::*;
use player_anim::PlayerAnimationPlugin;
use player_behaviour::PlayerBehaviorPlugin;
use rapier2d::geometry::{Group, InteractionGroups};
use rapier2d::parry::query::TOIStatus;
use theseeker_engine::animation::SpriteAnimationBundle;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::assets::config::{update_field, DynamicConfig};
use theseeker_engine::gent::{Gent, GentPhysicsBundle, TransformGfxFromGent};
use theseeker_engine::physics::{
    into_vec2, AnimationCollider, Collider, LinearVelocity, PhysicsWorld, ShapeCaster, ENEMY,
    GROUND, PLAYER, PLAYER_ATTACK,
};
use theseeker_engine::script::ScriptPlayer;

use crate::game::attack::*;
use crate::game::gentstate::*;
use crate::prelude::*;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PlayerConfig::default());
        app.add_systems(GameTickUpdate, load_player_config);
        app.add_systems(
            GameTickUpdate,
            (setup_player.run_if(in_state(GameState::Playing)))
                .before(PlayerStateSet::Transition)
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            OnEnter(GameState::Paused),
            (
                debug_player,
                crate::game::enemy::debug_enemy,
            )
                .chain(),
        )
        .add_plugins((
            InputManagerPlugin::<PlayerAction>::default(),
            PlayerBehaviorPlugin,
            PlayerTransitionPlugin,
            PlayerAnimationPlugin,
        ));

        #[cfg(feature = "dev")]
        app.add_systems(
            GameTickUpdate,
            debug_player_states
                .run_if(in_state(GameState::Playing))
                .after(PlayerStateSet::Transition),
        );
    }
}

/// set to order the player behavior, state transitions, and animations relative to eachother
#[derive(SystemSet, Clone, PartialEq, Eq, Debug, Hash)]
pub enum PlayerStateSet {
    Behavior,
    Collisions,
    Transition,
    Animation,
}

//TODO: change to player spawnpoint
#[derive(Bundle, LdtkEntity, Default)]
pub struct PlayerBlueprintBundle {
    marker: PlayerBlueprint,
}

#[derive(Bundle)]
pub struct PlayerGentBundle {
    player: Player,
    marker: Gent,
    phys: GentPhysicsBundle,
    coyote_time: CoyoteTime,
}

#[derive(Bundle)]
pub struct PlayerGfxBundle {
    marker: PlayerGfx,
    gent2gfx: TransformGfxFromGent,
    sprite: SpriteSheetBundle,
    animation: SpriteAnimationBundle,
}

#[derive(Component, Default)]
pub struct PlayerBlueprint;

#[derive(Component)]
pub struct Player;

#[derive(Component)]
pub struct PlayerGfx {
    pub e_gent: Entity,
}

#[derive(Actionlike, PartialEq, Eq, Clone, Copy, Hash, Debug, Reflect)]
pub enum PlayerAction {
    Move,
    Jump,
    Attack,
    Dash,
    Whirl,
}

fn debug_player_states(
    query: Query<
        AnyOf<(
            Ref<Running>,
            Ref<Idle>,
            Ref<Falling>,
            Ref<Jumping>,
            Ref<Grounded>,
            Ref<Dashing>,
            Ref<CanDash>,
        )>,
        With<Player>,
    >,
) {
    for states in query.iter() {
        // println!("{:?}", states);
        let (running, idle, falling, jumping, grounded, dashing, can_dash) = states;
        let mut states_string: String = String::new();
        if let Some(running) = running {
            if running.is_added() {
                states_string.push_str("added running, ");
            }
        }
        if let Some(idle) = idle {
            if idle.is_added() {
                states_string.push_str("added idle, ");
            }
        }
        if let Some(falling) = falling {
            if falling.is_added() {
                states_string.push_str("added falling, ");
            }
        }
        if let Some(jumping) = jumping {
            if jumping.is_added() {
                states_string.push_str("added jumping, ");
            }
        }
        if let Some(grounded) = grounded {
            if grounded.is_added() {
                states_string.push_str("added grounded, ");
            }
        }
        if let Some(dashing) = dashing {
            if dashing.is_added() {
                states_string.push_str("added dashing, ");
            }
        }
        if let Some(can_dash) = can_dash {
            if can_dash.is_added() {
                states_string.push_str("added can_dash, ");
            }
        }
        if !states_string.is_empty() {
            println!("{}", states_string);
        }

        // let components = entity.archetype().sparse_set_components();
        // for item in components {
        // print!("{:?}", item);
        // }
    }
}

// fn debug_player(world: &World, query: Query<Entity, With<PlayerGfx>>) {
fn debug_player(world: &World, query: Query<Entity, With<Player>>) {
    for entity in query.iter() {
        let components = world.inspect_entity(entity);
        for component in components.iter() {
            println!("{:?}", component.name());
        }
    }
}

fn setup_player(
    mut q: Query<(&mut Transform, Entity), Added<PlayerBlueprint>>,
    mut commands: Commands,
) {
    for (mut xf_gent, e_gent) in q.iter_mut() {
        //TODO: proper way of ensuring z is correct
        xf_gent.translation.z = 15.;
        println!("{:?}", xf_gent);
        let e_gfx = commands.spawn(()).id();
        commands.entity(e_gent).insert((
            Name::new("Player"),
            PlayerGentBundle {
                player: Player,
                marker: Gent { e_gfx },
                phys: GentPhysicsBundle {
                    collider: Collider::cuboid(
                        4.0,
                        10.0,
                        InteractionGroups {
                            memberships: PLAYER,
                            //should be more specific
                            filter: Group::all(),
                        },
                    ),
                    shapecast: ShapeCaster {
                        shape: Collider::cuboid(4.0, 10.0, InteractionGroups::none())
                            .0
                            .shared_shape()
                            .clone(),
                        origin: Vec2::new(0.0, 0.0),
                        max_toi: f32::MAX,
                        direction: Direction2d::NEG_Y,
                        interaction: InteractionGroups {
                            memberships: PLAYER,
                            filter: GROUND,
                        },
                    },
                    linear_velocity: LinearVelocity(Vec2::ZERO),
                },
                coyote_time: Default::default(),
            },
            Facing::Right,
            Health {
                current: 1600,
                max: 1600,
            },
            //have to use builder here *i think* because of different types between keycode and
            //axis
            InputManagerBundle::<PlayerAction> {
                action_state: ActionState::default(),
                input_map: InputMap::default()
                    .insert(PlayerAction::Jump, KeyCode::Space)
                    .insert(PlayerAction::Jump, KeyCode::KeyW)
                    .insert(PlayerAction::Jump, KeyCode::ArrowUp)
                    .insert(
                        PlayerAction::Move,
                        VirtualAxis::from_keys(KeyCode::KeyA, KeyCode::KeyD),
                    )
                    .insert(
                        PlayerAction::Move,
                        VirtualAxis::from_keys(KeyCode::ArrowLeft, KeyCode::ArrowRight),
                    )
                    .insert(PlayerAction::Attack, KeyCode::Enter)
                    .insert(PlayerAction::Attack, KeyCode::KeyJ)
                    .insert(PlayerAction::Dash, KeyCode::ShiftLeft)
                    .insert(
                        PlayerAction::Whirl,
                        KeyCode::ControlLeft,
                    )
                    .build(),
            },
            Falling,
            CanDash {
                remaining_cooldown: 0.0,
            },
            WallSlideTime(f32::MAX),
            HitFreezeTime(u32::MAX, None),
            WhirlAbility {
                active: false,
                active_ticks: 0,
                energy: 0.0,
                attack_entity: None,
            },
            TransitionQueue::default(),
        ));
        commands.entity(e_gfx).insert((PlayerGfxBundle {
            marker: PlayerGfx { e_gent },
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
        // println!("player spawned")
    }
}

struct PlayerTransitionPlugin;

impl Plugin for PlayerTransitionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (transition.run_if(any_with_component::<TransitionQueue>),)
                .in_set(PlayerStateSet::Transition)
                .after(PlayerStateSet::Behavior)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

// States
// states are components which are added to the entity on transition.
// an entity can be in multiple states at once, eg Grounded and Running/Idle
// Impl Playerstate for each state
// Impl Transitionable<T: GentState> for each state that that should be able to be transitioned
// from by a state
// pub trait GentState: Component<Storage = SparseStorage> {}

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct Idle;
impl GentState for Idle {}
impl GenericState for Idle {}

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct Running;
impl GentState for Running {}
impl GenericState for Running {}

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct Falling;
impl GentState for Falling {}
impl GenericState for Falling {}

#[derive(Component, Debug)]
#[component(storage = "SparseSet")]
pub struct Jumping;
impl GentState for Jumping {}
impl GenericState for Jumping {}

#[derive(Component, Default, Debug)]
#[component(storage = "SparseSet")]
pub struct Grounded;
impl GentState for Grounded {}
//cant be Idle or Running if not Grounded
impl Transitionable<Jumping> for Grounded {
    type Removals = (Grounded, Idle, Running);
}
//cant be Idle or Running if not Grounded
impl Transitionable<Falling> for Grounded {
    type Removals = (Grounded, Idle, Running);
}

#[derive(Component, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct Attacking {
    ticks: u32,
}
impl Attacking {
    const MAX: u32 = 4;
}
impl GentState for Attacking {}

impl Transitionable<CanAttack> for Attacking {
    type Removals = (Attacking);
}

#[derive(Component, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct CanAttack;
impl GentState for CanAttack {}

impl Transitionable<Attacking> for CanAttack {
    type Removals = (CanAttack);
}

#[derive(Component, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct Dashing {
    duration: f32,
}

impl GentState for Dashing {}
impl Transitionable<CanDash> for Dashing {
    type Removals = (Dashing);
}

#[derive(Component, Debug)]
#[component(storage = "SparseSet")]
pub struct CanDash {
    remaining_cooldown: f32,
}
impl CanDash {
    pub fn new(config: &PlayerConfig) -> Self {
        Self {
            remaining_cooldown: config.dash_cooldown_duration,
        }
    }
}
impl GentState for CanDash {}
impl Transitionable<Dashing> for CanDash {
    type Removals = (
        CanDash,
        Running,
        Jumping,
        Falling,
        Idle,
        Attacking,
        CanAttack,
    );
}

// Pseudo-States
// Not quite the same as states, these components enable certain behaviours when attached,
// and provide storage for that behaviours state

/// If a player attack lands, locks their velocity for the configured number of ticks'
//Tracks the attack entity which last caused the hirfreeze affect. and ticks since triggered
// (this way the same attack doesn't trigger it multiple times)
#[derive(Component, Default, Debug)]
pub struct HitFreezeTime(u32, Option<Entity>);

#[derive(Component, Default, Debug)]
pub struct CoyoteTime(f32);

/// Indicates that sliding is tracked for this entity
#[derive(Component, Default, Debug)]
pub struct WallSlideTime(f32);
impl WallSlideTime {
    /// Player is sliding if f32 value is less then the coyote time
    /// f32 starts incrementing when the player stops pressing into the wall
    fn sliding(&self, cfg: &PlayerConfig) -> bool {
        self.0 <= cfg.max_coyote_time * 2.0
    }
}

/// Indicates that sliding is tracked for this entity
#[derive(Component, Default, Debug)]
pub struct WhirlAbility {
    active: bool,
    active_ticks: u32,
    energy: f32,
    attack_entity: Option<Entity>,
}

#[derive(Resource, Debug, Default)]
pub struct PlayerConfig {
    /// The maximum horizontal velocity the player can move at.
    ///
    /// (in pixels/second)
    max_move_vel: f32,

    /// The maximum downward velocity the player can fall at.
    ///
    /// (in pixels/second)
    max_fall_vel: f32,

    /// The initial acceleration applied to the player for the first tick they start moving.
    ///
    /// (in pixels/second^2)
    move_accel_init: f32,

    /// The acceleration applied to the player while they continue moving horizontally.
    ///
    /// (in pixels/second^2)
    move_accel: f32,

    /// How much velocity does the player have at the moment they jump?
    ///
    /// (in pixels/second)
    jump_vel_init: f32,

    /// How fast does the player accelerate downward while holding down the jump button?
    ///
    /// (in pixels/tick^2)
    jump_fall_accel: f32,

    /// How fast does the player accelerate downward while in the falling state?
    /// (ie: after releasing the jump key)
    ///
    /// (in pixels/tick^2)
    /// Note: sets the games global_gravity! (affects projectiles and other things that fall)
    pub fall_accel: f32,

    /// How many seconds does our characters innate hover boots work?
    max_coyote_time: f32,

    /// Onlly applies in the downward y direction while the player is falling
    /// and trying to walk into the wall
    sliding_friction: f32,

    /// How many ticks is the players velocity locked to zero after landing an attack?
    hitfreeze_ticks: u32,

    /// How many seconds does our character dash for?
    dash_duration: f32,

    /// How many pixels/s do they dash with?
    dash_velocity: f32,

    /// How long before the player can dash again?
    dash_cooldown_duration: f32,

    max_whirl_energy: f32,

    /// Spends this much energy per second when using whirl
    whirl_cost: f32,

    /// Spends this much energy per second when not using whirl
    whirl_regen: f32,
}

fn load_player_config(
    mut ev_asset: EventReader<AssetEvent<DynamicConfig>>,
    cfgs: Res<Assets<DynamicConfig>>,
    preloaded: Res<PreloadedAssets>,
    mut player_config: ResMut<PlayerConfig>,
    mut commands: Commands,
    mut initialized_config: Local<bool>,
) {
    // convert from asset key string to bevy handle
    let Some(cfg_handle) = preloaded.get_single_asset::<DynamicConfig>("cfg.player") else {
        return;
    };
    // The reason we do this here instead of in an AssetEvent::Added match arm, is because
    // the Added match arm fires before preloaded updates with the asset key; as a result
    // you can't tell what specific DynamicConfig loaded in like that.
    if !*initialized_config {
        if let Some(cfg) = cfgs.get(cfg_handle.clone()) {
            update_player_config(&mut player_config, cfg);
            println!("init:");
            dbg!(&player_config);
        }
        *initialized_config = true;
    }
    for ev in ev_asset.read() {
        match ev {
            AssetEvent::Modified { id } => {
                if let Some(cfg) = cfgs.get(*id) {
                    if cfg_handle.id() == *id {
                        println!("before:");
                        dbg!(&player_config);
                        update_player_config(&mut player_config, cfg);
                        println!("after:");
                        dbg!(&player_config);
                    }
                }
            },
            _ => {},
        }
    }
}

#[rustfmt::skip]
fn update_player_config(config: &mut PlayerConfig, cfg: &DynamicConfig) {
    let mut errors = Vec::new();

    update_field(&mut errors, &cfg.0, "max_move_vel", |val| config.max_move_vel = val);
    update_field(&mut errors, &cfg.0, "max_fall_vel", |val| config.max_fall_vel = val);
    update_field(&mut errors, &cfg.0, "move_accel_init", |val| config.move_accel_init = val);
    update_field(&mut errors, &cfg.0, "move_accel", |val| config.move_accel = val);
    update_field(&mut errors, &cfg.0, "jump_vel_init", |val| config.jump_vel_init = val);
    update_field(&mut errors, &cfg.0, "jump_fall_accel", |val| config.jump_fall_accel = val);
    update_field(&mut errors, &cfg.0, "fall_accel", |val| config.fall_accel = val);
    update_field(&mut errors, &cfg.0, "max_coyote_time", |val| config.max_coyote_time = val);
    update_field(&mut errors, &cfg.0, "sliding_friction", |val| config.sliding_friction = val);
    update_field(&mut errors, &cfg.0, "hitfreeze_ticks", |val| config.hitfreeze_ticks = val as u32);
    update_field(&mut errors, &cfg.0, "dash_duration", |val| config.dash_duration = val);
    update_field(&mut errors, &cfg.0, "dash_velocity", |val| config.dash_velocity = val);
    update_field(&mut errors, &cfg.0, "dash_cooldown_duration", |val| config.dash_cooldown_duration = val);
    update_field(&mut errors, &cfg.0, "max_whirl_energy", |val| config.max_whirl_energy = val);
    update_field(&mut errors, &cfg.0, "whirl_cost", |val| config.whirl_cost = val);
    update_field(&mut errors, &cfg.0, "whirl_regen", |val| config.whirl_regen = val);

   for error in errors{
       warn!("failed to load player cfg value: {}", error);
   }
}
