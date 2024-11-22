mod player_anim;
mod player_behaviour;
use bevy::utils::hashbrown::HashMap;
use leafwing_input_manager::axislike::VirtualAxis;
use leafwing_input_manager::{
    action_state::ActionState, input_map::InputMap, Actionlike,
    InputManagerBundle,
};
use player_anim::PlayerAnimationPlugin;
use player_behaviour::PlayerBehaviorPlugin;
use rapier2d::geometry::{Group, InteractionGroups};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;
use theseeker_engine::animation::SpriteAnimationBundle;
use theseeker_engine::assets::config::{update_field, DynamicConfig};
use theseeker_engine::gent::{Gent, GentPhysicsBundle, TransformGfxFromGent};
use theseeker_engine::input::InputManagerPlugin;
use theseeker_engine::physics::{
    Collider, LinearVelocity, ShapeCaster, GROUND, PLAYER,
};

use super::physics::Knockback;
use crate::game::attack::*;
use crate::game::gentstate::*;
use crate::prelude::*;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PlayerConfig::default());
        app.add_systems(
            GameTickUpdate,
            (
                load_player_config,
                load_player_stats
                    .before(PlayerStateSet::Behavior)
                    .after(load_player_config)
                    .run_if(resource_changed::<PlayerConfig>),
            ),
        );
        app.add_systems(Startup, load_dash_asset);
        app.add_systems(
            GameTickUpdate,
            ((setup_player, despawn_dead_player)
                .run_if(in_state(GameState::Playing)))
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
    Stealth,
}

#[derive(Component, Debug, Deref, DerefMut)]
pub struct Passives {
    #[deref]
    pub current: HashSet<Passive>,
    pub locked: Vec<Passive>,
}

impl Default for Passives {
    fn default() -> Self {
        let passives: Vec<Passive> = Passive::iter().collect();
        Passives {
            current: HashSet::with_capacity(5),
            locked: passives,
        }
    }
}

impl Passives {
    //TODO: pass in slice of passives, filter the locked passives on it
    fn new_with(passive: Passive) -> Self {
        Passives::default()
    }
    fn gain(&mut self) {
        //TODO: add checks for no passives remaining
        //TODO add limit on gaining past max passive slots?
        //does nothing if there are no more passives to gain
        let mut rng = rand::thread_rng();
        if !self.locked.is_empty() {
            let i = rng.gen_range(0..self.locked.len());
            let passive = self.locked.swap_remove(i);
            self.current.insert(passive);
        }
    }
}

#[derive(Debug, Eq, PartialEq, Hash, EnumIter)]
pub enum Passive {
    /// Heal when killing an enemy
    Absorption,
    /// Crit every 3rd and 5th hit when low health
    CritResolve,
    Backstab,
    CrowdCtrl,
    Unmoving,
    Speedy,
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
        let (running, idle, falling, jumping, grounded, dashing, can_dash) =
            states;
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
    mut q: Query<(&mut Transform, Entity, &Parent), Added<PlayerBlueprint>>,
    parent_query: Query<Entity, With<Children>>,
    mut commands: Commands,
    config: Res<PlayerConfig>,
) {
    for (mut xf_gent, e_gent, parent) in q.iter_mut() {
        //TODO: proper way of ensuring z is correct
        xf_gent.translation.z = 15.0 * 0.000001;
        println!("{:?}", xf_gent);
        let e_gfx = commands.spawn(()).id();
        let e_effects_gfx = commands.spawn(()).id();
        commands.entity(e_gent).insert((
            Name::new("Player"),
            PlayerGentBundle {
                player: Player,
                marker: Gent {
                    e_gfx,
                    e_effects_gfx,
                },
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
                        shape: Collider::cuboid(
                            4.0,
                            10.0,
                            InteractionGroups::none(),
                        )
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
                current: config.max_health,
                max: config.max_health,
            },
            //have to use builder here *i think* because of different types between keycode and
            //axis
            InputManagerBundle::<PlayerAction> {
                action_state: ActionState::default(),
                input_map: InputMap::default()
                    .with(PlayerAction::Jump, KeyCode::Space)
                    .with(PlayerAction::Jump, KeyCode::KeyW)
                    .with(PlayerAction::Jump, KeyCode::ArrowUp)
                    .with(
                        PlayerAction::Move,
                        VirtualAxis::from_keys(KeyCode::KeyA, KeyCode::KeyD),
                    )
                    .with(
                        PlayerAction::Move,
                        VirtualAxis::from_keys(
                            KeyCode::ArrowLeft,
                            KeyCode::ArrowRight,
                        ),
                    )
                    .with(PlayerAction::Attack, KeyCode::Enter)
                    .with(PlayerAction::Attack, KeyCode::KeyJ)
                    .with(PlayerAction::Dash, KeyCode::KeyK)
                    .with(PlayerAction::Whirl, KeyCode::KeyL)
                    .with(PlayerAction::Stealth, KeyCode::KeyI),
            },
            //bundling things up becuase we reached max tuple
            (
                Falling,
                CanDash {
                    remaining_cooldown: 0.0,
                },
                CanStealth {
                    remaining_cooldown: 0.0,
                },
            ),
            WallSlideTime(f32::MAX),
            HitFreezeTime(u32::MAX, None),
            JumpCount(0),
            WhirlAbility::default(),
            Crits::new(2.0),
            TransitionQueue::default(),
            StateDespawnMarker,
            Passives::default(),
        ));
        //unparent from the level
        if let Ok(parent) = parent_query.get(parent.get()) {
            commands.entity(parent).remove_children(&[e_gent]);
        }
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
    }
}

fn despawn_dead_player(
    query: Query<(Entity, &Gent), (With<Dead>, With<Player>)>,
    mut commands: Commands,
) {
    for (entity, gent) in query.iter() {
        commands.entity(gent.e_gfx).despawn_recursive();
        commands.entity(entity).despawn_recursive();
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
    type Removals = (Grounded, Idle, Running, Whirling);
}
//cant be Idle or Running if not Grounded
impl Transitionable<Falling> for Grounded {
    type Removals = (Grounded, Idle, Running, Whirling);
}

#[derive(Component, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct Attacking {
    pub ticks: u32,
    followup: bool,
}
impl Attacking {
    pub const MAX: u32 = 4;
}
impl GentState for Attacking {}

impl Transitionable<CanAttack> for Attacking {
    type Removals = Attacking;
}

#[derive(Component, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct CanAttack {
    pub immediate: bool,
}
impl GentState for CanAttack {}

impl Transitionable<Attacking> for CanAttack {
    type Removals = CanAttack;
}
impl Transitionable<Whirling> for CanAttack {
    type Removals = CanAttack;
}

#[derive(Component, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct Whirling {
    pub attack_entity: Option<Entity>,
    pub ticks: u32,
}
impl GentState for Whirling {}

impl Transitionable<CanAttack> for Whirling {
    type Removals = (Whirling, Attacking);
}
impl Whirling {
    const MIN_TICKS: u32 = 48;
}

#[derive(Component, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct Dashing {
    duration: f32,
}

impl GentState for Dashing {}
impl Transitionable<CanDash> for Dashing {
    type Removals = (Dashing, Whirling);
}

#[derive(Component, Debug)]
#[component(storage = "SparseSet")]
pub struct CanDash {
    pub remaining_cooldown: f32,
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

#[derive(Component, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct Stealthing {
    duration: f32,
}

impl GentState for Stealthing {}
impl Transitionable<CanStealth> for Stealthing {
    type Removals = Stealthing;
}
#[derive(Component, Debug)]
#[component(storage = "SparseSet")]
pub struct CanStealth {
    pub remaining_cooldown: f32,
}
impl CanStealth {
    pub fn new(config: &PlayerConfig) -> Self {
        Self {
            remaining_cooldown: config.stealth_cooldown,
        }
    }
}
impl GentState for CanStealth {}
impl Transitionable<Stealthing> for CanStealth {
    type Removals = (CanStealth,);
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

#[derive(Component, Default, Debug)]
pub struct JumpCount(u8);

/// Indicates that sliding is tracked for this entity
#[derive(Component, Default, Debug)]
pub struct WallSlideTime(f32);
impl WallSlideTime {
    /// Player is sliding if f32 value is less then the coyote time
    /// f32 starts incrementing when the player stops pressing into the wall
    fn sliding(&self, cfg: &PlayerConfig) -> bool {
        self.0 <= cfg.max_coyote_time * 2.0
    }
    fn strict_sliding(&self, cfg: &PlayerConfig) -> bool {
        self.0 <= cfg.max_coyote_time * 1.0
    }
}

///Tracks the cooldown for the available energy for the players whirl
#[derive(Component, Default, Debug)]
pub struct WhirlAbility {
    pub energy: f32,
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

    /// How many seconds does our character stealth for?
    stealth_duration: f32,

    /// How many seconds does our character stealth for?
    pub stealth_cooldown: f32,

    /// How many pixels/s do they dash with?
    dash_velocity: f32,

    /// How long before the player can dash again?
    pub dash_cooldown_duration: f32,

    pub max_whirl_energy: f32,

    /// Spends this much energy per second when using whirl
    whirl_cost: f32,

    /// Spends this much energy per second when not using whirl
    whirl_regen: f32,

    /// How much max health the player has
    pub max_health: u32,

    /// Pushback velocity on wall jumps
    wall_pushback: f32,

    /// Ticks for wall pushback velocity; determines how long movement is locked for
    wall_pushback_ticks: u32,

    /// Self pushback velocity on basic melee hits
    melee_self_pushback: f32,

    /// Ticks for melee self pushback velocity; determines how long movement is locked for
    melee_self_pushback_ticks: u32,

    /// Knockback velocity applied to enemy on basic melee hit
    melee_pushback: f32,

    /// Ticks for melee knockback velocity; determines how long movement is locked for
    melee_pushback_ticks: u32,

    /// How many kills to trigger a passive gain
    passive_gain_rate: u32,
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
    let Some(cfg_handle) =
        preloaded.get_single_asset::<DynamicConfig>("cfg.player")
    else {
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
    update_field(&mut errors, &cfg.0, "stealth_duration", |val| config.stealth_duration = val);
    update_field(&mut errors, &cfg.0, "stealth_cooldown", |val| config.stealth_cooldown = val);
    update_field(&mut errors, &cfg.0, "max_whirl_energy", |val| config.max_whirl_energy = val);
    update_field(&mut errors, &cfg.0, "whirl_cost", |val| config.whirl_cost = val);
    update_field(&mut errors, &cfg.0, "whirl_regen", |val| config.whirl_regen = val);
    update_field(&mut errors, &cfg.0, "max_health", |val| config.max_health = val as u32);
    update_field(&mut errors, &cfg.0, "wall_pushback", |val| config.wall_pushback = val);
    update_field(&mut errors, &cfg.0, "wall_pushback_ticks", |val| config.wall_pushback_ticks = val as u32);
    update_field(&mut errors, &cfg.0, "melee_self_pushback", |val| config.melee_self_pushback = val);
    update_field(&mut errors, &cfg.0, "melee_self_pushback_ticks", |val| config.melee_self_pushback_ticks = val as u32);
    update_field(&mut errors, &cfg.0, "melee_pushback", |val| config.melee_pushback = val);
    update_field(&mut errors, &cfg.0, "melee_pushback_ticks", |val| config.melee_pushback_ticks = val as u32);
    update_field(&mut errors, &cfg.0, "passive_gain_rate", |val| config.passive_gain_rate = val as u32);

    for error in errors{
       warn!("failed to load player cfg value: {}", error);
   }
}

fn load_player_stats(mut commands: Commands, player_config: Res<PlayerConfig>) {
    commands.insert_resource(PlayerStats::init_from_config(
        &player_config,
    ));
}

/// Extend with additional parameter Stats
#[derive(Clone, Copy, Eq, PartialEq, Hash, Debug)]
pub enum StatType {
    MoveVelMax,
    MoveAccelInit,
    MoveAccel,
}
/// For now, Status Modifier is implemented so that only one Status Modifier is active at a time.
/// However, a single Status Modifier can modify multiple Stats.
/// scalar and delta will use the same coefficient for all Stats if there is only one.
#[derive(Component, Clone, Debug)]
pub struct StatusModifier {
    status_types: Vec<StatType>,

    /// Multiplying Factor on Stat, e.g. 102.0 * 0.5 = 51.0
    scalar: Vec<f32>,
    /// Offseting Value on Stat, e.g. 100.0 - 10.0 = 90.0
    delta: Vec<f32>,

    effect_col: Color,

    time_remaining: f32,
}

//TODO: move to attack
impl StatusModifier {
    pub fn basic_ice_spider() -> Self {
        Self {
            status_types: vec![
                StatType::MoveVelMax,
                StatType::MoveAccel,
                StatType::MoveAccelInit,
            ],
            scalar: vec![0.5],
            delta: vec![],
            //            effect_col: Color::hex("C2C9C9").unwrap(),
            effect_col: Color::hex("0099CC").unwrap(), // For More Visible Effect
            time_remaining: 2.0,
        }
    }
}

#[derive(Resource)]
pub struct PlayerStats {
    pub base_stats: HashMap<StatType, f32>,
    pub effective_stats: HashMap<StatType, f32>,
}

impl PlayerStats {
    pub fn init_from_config(config: &PlayerConfig) -> Self {
        let stats = HashMap::from_iter(vec![
            (
                StatType::MoveVelMax,
                config.max_move_vel,
            ),
            (StatType::MoveAccel, config.move_accel),
            (
                StatType::MoveAccelInit,
                config.move_accel_init,
            ),
        ]);

        Self {
            base_stats: stats.clone(),
            effective_stats: stats,
        }
    }

    pub fn get(&self, stat: StatType) -> f32 {
        self.effective_stats[&stat]
    }

    pub fn reset_stats(&mut self) {
        self.effective_stats = self.base_stats.clone();
    }

    pub fn update_stats(&mut self, modifier: &StatusModifier) {
        self.effective_stats.clear();

        let base_scalar = match modifier.scalar.len() {
            0 => Some(1.0),
            1 => Some(modifier.scalar[0]),
            _ => None,
        };
        let base_delta = match modifier.delta.len() {
            0 => Some(0.0),
            1 => Some(modifier.delta[0]),
            _ => None,
        };

        for (i, stat) in modifier.status_types.iter().enumerate() {
            let val = self.base_stats[stat]
                * base_scalar.unwrap_or_else(|| modifier.scalar[i])
                + base_delta.unwrap_or_else(|| modifier.delta[i]);

            self.effective_stats.insert(*stat, val);
        }

        println!("{:?}", self.effective_stats);
    }
}

fn player_new_stats_mod(
    mut query: Query<(Entity, &mut StatusModifier)>,
    mut gfx_query: Query<(&PlayerGfx, &mut Sprite)>,
    mut player_stats: ResMut<PlayerStats>,
    time: Res<Time<Virtual>>,
    mut commands: Commands,
) {
    for (p_gfx, mut sprite) in gfx_query.iter_mut() {
        let Ok((entity, mut modifier)) = query.get_mut(p_gfx.e_gent) else {
            return;
        };

        if modifier.is_changed() {
            player_stats.update_stats(&modifier);
        }

        sprite.color = modifier.effect_col.clone();

        modifier.time_remaining -= time.delta_seconds();

        if modifier.time_remaining < 0. {
            commands.entity(entity).remove::<StatusModifier>();
            player_stats.reset_stats();

            sprite.color = Color::WHITE;
        }
    }
}
#[derive(Component)]
pub struct DashIcon {
    time: f32,
    init_a: f32,
}

#[derive(Resource)]
pub struct DashIconAssetHandle(Handle<Image>);

pub fn load_dash_asset(assets: Res<AssetServer>, mut commands: Commands) {
    let tex: Handle<Image> = assets.load("animations/player/movement/Dash.png");

    commands.insert_resource(DashIconAssetHandle(tex));
}

pub fn player_dash_fx(
    mut query: Query<
        (
            &GlobalTransform,
            &Facing,
            //            &LinearVelocity,
            Ref<Dashing>,
            Option<&Stealthing>,
        ),
        With<Player>,
    >,
    config: Res<PlayerConfig>,
    time: Res<GameTime>,
    mut commands: Commands,
    asset: Res<DashIconAssetHandle>,
) {
    for (global_tr, facing, dashing, stealthing_maybe) in query.iter() {
        let pos = global_tr.translation();

        // Code for potentially interpolating position
        // Not deemed necessary due to sufficient sprite overlap
        //let dir = Vec3::new(
        //    config.dash_velocity * facing.direction(),
        //    0.0,
        //    0.0,
        //);
        //let lpos = pos + dir * 0.5;
        //if dashing.is_added() {
        //
        //} else
        {
            //let dir = Vec2::new(
            //    config.dash_velocity * facing.direction(),
            //    0.0,
            //);

            let tex: Handle<Image> = asset.0.clone();
            let t = time.time_in_seconds() as f32 + config.dash_duration;

            let init_a = match stealthing_maybe {
                Some(_) => 0.2,
                None => 0.5,
            };

            commands.spawn((
                SpriteBundle {
                    sprite: Sprite {
                        flip_x: facing.direction() < 0.,
                        ..default()
                    },
                    transform: Transform::from_translation(pos),
                    texture: tex.clone(),
                    ..default()
                },
                DashIcon { time: t, init_a },
            ));
        }
    }
}

pub fn dash_icon_fx(
    mut commands: Commands,
    mut query: Query<(Entity, &DashIcon, &mut Sprite)>,
    time: Res<GameTime>,
    config: Res<PlayerConfig>,
) {
    for (entity, icon, mut sprite) in query.iter_mut() {
        let d = time.time_in_seconds() as f32 - icon.time;

        let r = d / config.dash_duration;

        if r >= 1.0 {
            commands.entity(entity).despawn();
        } else {
            sprite.color.set_a((1.0 - r) * icon.init_a);
        }
    }
}

///Resets the players cooldowns/energy on hit of a stealthed critical hit
pub fn on_hit_stealth_reset(
    query: Query<&Attack, (Added<Hit>, With<Crit>, With<Stealthed>)>,
    mut attacker_skills: Query<(
        Option<&mut CanDash>,
        Option<&mut WhirlAbility>,
        Option<&mut CanStealth>,
    )>,
    config: Res<PlayerConfig>,
) {
    for attack in query.iter() {
        if let Ok((
            mut maybe_can_dash,
            mut maybe_whirl_ability,
            mut maybe_can_stealth,
        )) = attacker_skills.get_mut(attack.attacker)
        {
            if let Some(ref mut can_dash) = maybe_can_dash {
                can_dash.remaining_cooldown = 0.;
            }
            if let Some(ref mut whirl_ability) = maybe_whirl_ability {
                whirl_ability.energy = config.max_whirl_energy;
            }
            if let Some(ref mut can_stealth) = maybe_can_stealth {
                can_stealth.remaining_cooldown = 0.;
            }
        }
    }
}

///Exits player Stealthing state when a stealthed attack first hits
pub fn on_hit_exit_stealthing(
    query: Query<&Attack, (Added<Hit>, With<Stealthed>)>,
    mut attacker_query: Query<(&Gent, &mut TransitionQueue), With<Player>>,
    mut sprites: Query<&mut Sprite, Without<Player>>,
    config: Res<PlayerConfig>,
) {
    for attack in query.iter() {
        if let Ok((gent, mut transitions)) =
            attacker_query.get_mut(attack.attacker)
        {
            let mut sprite = sprites.get_mut(gent.e_gfx).unwrap();
            sprite.color = sprite.color.with_a(1.0);
            transitions.push(Stealthing::new_transition(
                CanStealth::new(&config),
            ));
        }
    }
}
