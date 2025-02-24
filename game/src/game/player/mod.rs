mod player_action;
mod player_anim;
mod player_behaviour;
pub mod player_weapon;
use bevy::utils::hashbrown::HashMap;
use leafwing_input_manager::action_state::ActionState;
use player_action::PlayerActionPlugin;
use player_anim::PlayerAnimationPlugin;
use player_behaviour::PlayerBehaviorPlugin;
use player_weapon::PlayerWeaponPlugin;
use rapier2d::geometry::{Group, InteractionGroups};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;
use theseeker_engine::animation::SpriteAnimationBundle;
use theseeker_engine::assets::config::{update_field, DynamicConfig};
use theseeker_engine::gent::{Gent, GentPhysicsBundle, TransformGfxFromGent};
use theseeker_engine::physics::{
    Collider, LinearVelocity, ShapeCaster, GROUND, PLAYER,
};

use crate::game::attack::*;
use crate::game::gentstate::*;
use crate::game::pickups::DropTracker;
use crate::game::xp_orbs::XpOrbPickup;
use crate::prelude::*;

use super::game_over::GameOver;
use super::physics::Knockback;
use crate::game::enemy::Enemy;

pub use player_action::PlayerAction;
pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PlayerConfig::default());
        app.add_systems(
            GameTickUpdate,
            (
                load_player_config,
                load_player_stats.run_if(resource_changed::<PlayerConfig>),
                track_hits,
                player_update_stats_mod,
                player_update_passive_buffs,
            )
                .chain()
                .before(PlayerStateSet::Behavior),
        );
        app.add_systems(
            GameTickUpdate,
            on_xp_heal.after(PlayerStateSet::Behavior),
        );
        app.add_systems(GameTickUpdate, on_crit_heal);
        app.add_systems(GameTickUpdate, apply_vitality_overclock);
        app.add_systems(Startup, load_dash_asset);
        app.add_systems(
            GameTickUpdate,
            ((setup_player, despawn_dead_player)
                .run_if(in_state(GameState::Playing)))
            .after(PlayerStateSet::Transition)
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
            PlayerActionPlugin,
            PlayerBehaviorPlugin,
            PlayerTransitionPlugin,
            PlayerAnimationPlugin,
            PlayerWeaponPlugin,
        ));

        #[cfg(feature = "dev")]
        app.add_systems(
            GameTickUpdate,
            debug_player_states
                .run_if(in_state(GameState::Playing))
                .after(PlayerStateSet::Transition),
        );

        app.add_systems(GameTickUpdate, update_serpentring_health);
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

// TODO: change to player spawnpoint
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


#[derive(Component, Debug, Deref, DerefMut)]
pub struct Passives {
    #[deref]
    pub current: HashSet<Passive>,
    pub locked: Vec<Passive>,
}
impl Passives {
    /// Maximum number of passives player can hold at once
    pub const MAX: usize = 3;
}

impl Default for Passives {
    fn default() -> Self {
        let passives: Vec<Passive> = Passive::iter().collect();
        Passives {
            current: HashSet::with_capacity(Passives::MAX),
            locked: passives,
        }
    }
}

impl Passives {
    pub fn drop_random(&mut self) -> Option<Passive> {
        let mut rng = rand::thread_rng();

        // dont drop more passives if full
        if !self.locked.is_empty() && self.current.len() < Passives::MAX {
            let i = rng.gen_range(0..self.locked.len());
            let passive = self.locked.swap_remove(i);

            return Some(passive);
        }
        None
    }

    // TODO: return result?
    pub fn add_passive(&mut self, passive: Passive) {
        if self.current.len() < Passives::MAX {
            self.current.insert(passive);
        }
    }
}

// they could also be components...limit only by the pickup/gain function instead of sized hashmap
#[derive(Debug, Eq, PartialEq, Hash, EnumIter, Clone)]
pub enum Passive {
    /// Heal after killing an enemy
    Bloodstone,
    /// Crit on every 2nd and 3rd hit when on low health
    FlamingHeart,
    /// Deal double damage when backstabbing
    IceDagger,
    /// Defense scaling based on number of enemies nearby
    GlowingShard,
    /// Crits lower cooldown of all abilities by 0.5 seconds
    ObsidianNecklace,
    /// Doubled damage & defence while standing still,but halved while moving
    HeavyBoots,
    /// Move faster, get cooldown redudction, but take double damage
    SerpentRing,
    /// Sacrifice health but get increased cooldown reduction for every consecutive hit within 3 seconds
    FrenziedAttack,
    /// Deal more damage to packs of enemies
    PackKiller,
    /// Get increased defense when you're in the air, but become more vulnerable when on the ground.
    DeadlyFeather,
    /// Scale damage based on distance between you and nearest enemy
    Sharpshooter,
    /// Limits the damage taken from any attack to 1/3 of your maximum health.
    ProtectiveSpirit,
    /// Gain 1 extra jump.
    RabbitsFoot,
    /// Critical hits heal you
    CriticalRegeneration,
    /// Increases damage based on health percentage, at the cost of constant health degeneration.
    VitalityOverclock,
}

impl Passive {
    pub fn name(&self) -> &str {
        match self {
            Passive::Bloodstone => "Bloodstone",
            Passive::FlamingHeart => "Flaming Heart",
            Passive::IceDagger => "Ice Dagger",
            Passive::GlowingShard => "Glowing Shard",
            Passive::ObsidianNecklace => "Obsidian Necklace",
            Passive::HeavyBoots => "Heavy Boots",
            Passive::SerpentRing => "Serpent Ring",
            Passive::FrenziedAttack => "Frenzied Attack",
            Passive::PackKiller => "Pack Killer",
            Passive::DeadlyFeather => "Deadly Feather",
            Passive::Sharpshooter => "Sharpshooter",
            Passive::ProtectiveSpirit => "Protective Spirit",
            Passive::RabbitsFoot => "Elastic Accelerator",
            Passive::CriticalRegeneration => "Critical Regeneration",
            Passive::VitalityOverclock => "Vitality Overclock",
        }
    }

    pub fn description(&self) -> &str {
        match self {
            Passive::Bloodstone => "Heal after kills",
            Passive::FlamingHeart => "Crit on every 2nd and 3rd hit when on low health",
            Passive::IceDagger => "Deal double damage when backstabbing",
            Passive::GlowingShard => "Defense scaling based on number of enemies nearby",
            Passive::ObsidianNecklace => "Get +11% crit chance. Crits lower cooldown of all abilities by 0.5 seconds",
            Passive::HeavyBoots => "Doubled damage & defence while standing still,but halved while moving",
            Passive::SerpentRing => "Move faster, get cooldown redudction, but your life gets cut in half",
            Passive::FrenziedAttack => "Sacrifice health but get increased cooldown reduction for every consecutive hit within 3 seconds",
            Passive::PackKiller => "Deal more damage to packs of enemies",
            Passive::DeadlyFeather => "Deal 50% extra damage, get faster cdr, and +50% crit chance when you're in the air, but become more vulnerable when on the ground",
            Passive::Sharpshooter => "Scale damage based on distance between you and nearest enemy. Distance value only updated when Running.",
            Passive::ProtectiveSpirit => "Damage you take from any attack is limited to 1/3rd of your maximum health",
            Passive::RabbitsFoot => "Gain 1 extra jump, move 20% faster, and get +9% crit chance",
            Passive::CriticalRegeneration => "Critical hits heal you",
            Passive::VitalityOverclock => "Gain increased damage based on health percentage, at the cost of constant health degeneration",
        }
    }
}

#[cfg(feature = "dev")]
fn debug_player_states(
    query: Query<
        AnyOf<(
            Ref<Running>,
            Ref<Idle>,
            Ref<Falling>,
            Ref<Jumping>,
            Ref<Grounded>,
            Ref<Dashing>,
            Ref<DashStrike>,
            Ref<CanDash>,
        )>,
        With<Player>,
    >,
) {
    for states in query.iter() {
        // println!("{:?}", states);
        let (
            running,
            idle,
            falling,
            jumping,
            grounded,
            dashing,
            dash_strike,
            can_dash,
        ) = states;
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
                if dashing.is_down_dash() {
                    states_string.push_str("added down dashing, ");
                } else {
                    states_string.push_str("added dashing, ");
                }
            }
        }
        if let Some(dash_strike) = dash_strike {
            if dash_strike.is_added() {
                states_string.push_str("added dashing strike ");
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
        // TODO: proper way of ensuring z is correct
        xf_gent.translation.z = 15.0 * 0.000001;
        let e_gfx = commands.spawn(()).id();
        let e_effects_gfx = commands.spawn(()).id();
        let mut passives = Passives::default();
        // uncomment for testing
        // passives.gain(Passive::HeavyBoots);
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
                            // should be more specific
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
            // have to use builder here *i think* because of different types between keycode and
            // axis
        
            PlayerAction::input_manager_bundle(),
            // bundling things up because we reached max tuple
            (
                Falling,
                CanDash {
                    remaining_cooldown: 0.0,
                    total_cooldown: 0.0,
                },
                CanStealth {
                    remaining_cooldown: 0.0,
                },
            ),
            (
                PlayerStats::init_from_config(&config),
                // maybe consolidate with PlayerStats
                PlayerStatMod::new(),
                EnemiesNearby(0),
            ),
            WallSlideTime(f32::MAX),
            HitFreezeTime(u32::MAX, None),
            JumpCount(0),
            WhirlAbility::default(),
            Crits::new(2.0),
            TransitionQueue::default(),
            StateDespawnMarker,
            (passives, BuffTick::default()),
        ));
        // unparent from the level
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

        commands.init_resource::<DropTracker>();
    }
}

fn despawn_dead_player(
    query: Query<(Entity, &Gent), (With<Dead>, With<Player>)>,
    mut commands: Commands,
) {
    for (entity, gent) in query.iter() {
        commands.entity(gent.e_gfx).despawn_recursive();
        commands.entity(entity).despawn_recursive();
        commands.insert_resource(GameOver);
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
// cant be Idle or Running if not Grounded
impl Transitionable<Jumping> for Grounded {
    type Removals = (Grounded, Idle, Running, Whirling);
}
// cant be Idle or Running if not Grounded
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
    // pub const MAX: u32 = 4;
    // minimum amount of frames that should be played from attack animation
    pub const MIN: u32 = 2;
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

/// Differentiates between different types of dashing
//#[allow(clippy::enum_variant_names)]
#[derive(Clone, Copy, Eq, PartialEq, Hash, Debug, Default)]
pub enum DashType {
    #[default]
    Horizontal,
    Downward,
}

#[derive(Component, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct Dashing {
    duration: f32,
    hit: bool,
    hit_ground: bool,
    dash_type: DashType,
}
impl Dashing {
    pub fn from_action_state(action_state: &ActionState<PlayerAction>) -> Self {
        if action_state.pressed(&PlayerAction::Fall) {
            Self {
                dash_type: DashType::Downward,
                ..default()
            }
        } else {
            Self {
                dash_type: DashType::Horizontal,
                ..default()
            }
        }
    }

    pub fn dash_duration(&self, config: &PlayerConfig) -> f32 {
        match self.dash_type {
            DashType::Horizontal => config.dash_duration,
            DashType::Downward => config.dash_down_duration,
        }
    }

    pub fn is_down_dash(&self) -> bool {
        self.dash_type == DashType::Downward
    }

    pub fn is_horizontal_dash(&self) -> bool {
        self.dash_type == DashType::Horizontal
    }

    pub fn set_player_velocity(
        &self,
        velocity: &mut LinearVelocity,
        facing: &Facing,
        config: &PlayerConfig,
    ) {
        match self.dash_type {
            DashType::Horizontal => {
                velocity.x = config.dash_velocity * facing.direction();
                velocity.y = 0.0;
            },
            DashType::Downward => {
                velocity.x =
                    config.dash_down_horizontal_velocity * facing.direction();
                velocity.y = -config.dash_down_vertical_velocity;
            },
        };
    }
}

impl GentState for Dashing {}
impl Transitionable<CanDash> for Dashing {
    type Removals = (Dashing, Whirling);
}

impl Transitionable<DashStrike> for Dashing {
    type Removals = (Dashing, Whirling);
}

#[derive(Component, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct DashStrike {
    ticks: u32,
}

impl DashStrike {
    pub const MAX: u32 = 2;
}

impl GentState for DashStrike {}
impl Transitionable<CanDash> for DashStrike {
    type Removals = (DashStrike, Whirling, Dashing);
}

#[derive(Component, Debug)]
#[component(storage = "SparseSet")]
pub struct CanDash {
    pub remaining_cooldown: f32,
    pub total_cooldown: f32,
}
impl CanDash {
    pub fn new(config: &PlayerConfig, dash_type: &DashType) -> Self {
        let cooldown = match dash_type {
            DashType::Horizontal => config.dash_cooldown_duration,
            DashType::Downward => config.dash_down_cooldown_duration,
        };

        Self {
            remaining_cooldown: cooldown,
            total_cooldown: cooldown,
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
// Tracks the attack entity which last caused the hirfreeze affect. and ticks since triggered
// (this way the same attack doesn't trigger it multiple times)
#[allow(dead_code)]
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

    /// Checks that player is actually against the wall, rather then it being close
    /// enough time from the player having left the wall to still jump
    /// (ie: not wall_jump_coyote_time)
    fn is_pressed_against_wall(&self, time: &Res<GameTime>) -> bool {
        self.0 <= 1.0 / time.hz as f32
    }
}

/// Tracks the cooldown for the available energy for the players whirl
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

    /// The maximum horizontal velocity the player can move at while using the Hammer weapon.
    ///
    /// (in pixels/second)
    hammer_max_move_vel: f32,

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

    /// Only applies in the downward y direction while the player is falling
    /// and trying to walk into the wall
    sliding_friction: f32,

    /// How many ticks is the players velocity locked to zero after landing an attack?
    hitfreeze_ticks: u32,

    /// How many seconds does our character dash for?
    dash_duration: f32,

    /// How many seconds does our character dash for?
    dash_down_duration: f32,

    /// How many seconds does our character stealth for?
    stealth_duration: f32,

    /// How many seconds does our character stealth for?
    pub stealth_cooldown: f32,

    /// How many pixels/s do they dash with?
    dash_velocity: f32,

    /// How many pixels/s (horizontally) do they dash with when doing a downward dash?
    dash_down_horizontal_velocity: f32,

    /// How many pixels/s (vertically) do they dash with when doing a downward dash?
    dash_down_vertical_velocity: f32,

    /// How long before the player can dash again?
    pub dash_cooldown_duration: f32,

    /// How long before the player can dash again?
    pub dash_down_cooldown_duration: f32,

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

    /// Self pushback velocity on basic sword hits
    sword_self_pushback: f32,

    /// Ticks for sword self pushback velocity; determines how long movement is locked for
    sword_self_pushback_ticks: u32,

    /// Knockback velocity applied to enemy on basic sword hit
    sword_pushback: f32,

    /// Ticks for sword knockback velocity; determines how long movement is locked for
    sword_pushback_ticks: u32,

    /// Self pushback velocity on basic hammer hits
    hammer_self_pushback: f32,

    /// Ticks for hammer self pushback velocity; determines how long movement is locked for
    hammer_self_pushback_ticks: u32,

    /// Knockback velocity applied to enemy on basic hammer hit
    hammer_pushback: f32,

    /// Ticks for hammer knockback velocity; determines how long movement is locked for
    hammer_pushback_ticks: u32,

    /// Pushback velocity on basic bow shots
    bow_self_pushback: f32,

    /// Ticks for bow pushback velocity; determines how long movement is locked for
    bow_self_pushback_ticks: u32,

    /// Knockback velocity applied to enemy on basic bow hit
    bow_pushback: f32,

    /// Ticks for melee knockback velocity; determines how long movement is locked for
    bow_pushback_ticks: u32,

    /// Base sword attack damage
    sword_attack_damage: f32,

    /// Base hammer attack damage
    hammer_attack_damage: f32,

    /// Base bow attack damage
    bow_attack_damage: f32,

    /// Velocity of the projectiles fired by the Bow weapon
    arrow_velocity: f32,

    /// Default strength for on hit camera screen shake
    pub default_on_hit_screenshake_strength: f32,
    /// Default duration (in seconds) for on hit camera screen shake
    pub default_on_hit_screenshake_duration_secs: f32,
    /// Default frequency for on hit camera screen shake
    pub default_on_hit_screenshake_frequency: f32,

    /// Hammer-specific strength for on hit camera screen shake
    pub hammer_on_hit_screenshake_strength: f32,
    /// Hammer-specific duration (in seconds) for on hit camera screen shake
    pub hammer_on_hit_screenshake_duration_secs: f32,
    /// Hammer-specific frequency for on hit camera screen shake
    pub hammer_on_hit_screenshake_frequency: f32,

    /// How many kills to trigger a passive gain
    passive_gain_rate: u32,
}

fn load_player_config(
    mut ev_asset: EventReader<AssetEvent<DynamicConfig>>,
    cfgs: Res<Assets<DynamicConfig>>,
    preloaded: Res<PreloadedAssets>,
    mut player_config: ResMut<PlayerConfig>,
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
        }
        *initialized_config = true;
    }
    for ev in ev_asset.read() {
        if let AssetEvent::Modified { id } = ev {
            if let Some(cfg) = cfgs.get(*id) {
                if cfg_handle.id() == *id {
                    update_player_config(&mut player_config, cfg);
                }
            }
        }
    }
}

#[rustfmt::skip]
fn update_player_config(config: &mut PlayerConfig, cfg: &DynamicConfig) {
    let mut errors = Vec::new();
    update_field(&mut errors, &cfg.0, "max_move_vel", |val| config.max_move_vel = val);
    update_field(&mut errors, &cfg.0, "hammer_max_move_vel", |val| config.hammer_max_move_vel = val);
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
    update_field(&mut errors, &cfg.0, "dash_down_duration", |val| config.dash_down_duration = val);
    update_field(&mut errors, &cfg.0, "dash_velocity", |val| config.dash_velocity = val);
    update_field(&mut errors, &cfg.0, "dash_down_horizontal_velocity", |val| config.dash_down_horizontal_velocity = val);
    update_field(&mut errors, &cfg.0, "dash_down_vertical_velocity", |val| config.dash_down_vertical_velocity = val);
    update_field(&mut errors, &cfg.0, "dash_cooldown_duration", |val| config.dash_cooldown_duration = val);
    update_field(&mut errors, &cfg.0, "dash_down_cooldown_duration", |val| config.dash_down_cooldown_duration = val);
    update_field(&mut errors, &cfg.0, "stealth_duration", |val| config.stealth_duration = val);
    update_field(&mut errors, &cfg.0, "stealth_cooldown", |val| config.stealth_cooldown = val);
    update_field(&mut errors, &cfg.0, "max_whirl_energy", |val| config.max_whirl_energy = val);
    update_field(&mut errors, &cfg.0, "whirl_cost", |val| config.whirl_cost = val);
    update_field(&mut errors, &cfg.0, "whirl_regen", |val| config.whirl_regen = val);
    update_field(&mut errors, &cfg.0, "max_health", |val| config.max_health = val as u32);
    update_field(&mut errors, &cfg.0, "wall_pushback", |val| config.wall_pushback = val);
    update_field(&mut errors, &cfg.0, "wall_pushback_ticks", |val| config.wall_pushback_ticks = val as u32);
    update_field(&mut errors, &cfg.0, "sword_self_pushback", |val| config.sword_self_pushback = val);
    update_field(&mut errors, &cfg.0, "sword_self_pushback_ticks", |val| config.sword_self_pushback_ticks = val as u32);
    update_field(&mut errors, &cfg.0, "sword_pushback", |val| config.sword_pushback = val);
    update_field(&mut errors, &cfg.0, "sword_pushback_ticks", |val| config.sword_pushback_ticks = val as u32);
    update_field(&mut errors, &cfg.0, "hammer_self_pushback", |val| config.hammer_self_pushback = val);
    update_field(&mut errors, &cfg.0, "hammer_self_pushback_ticks", |val| config.hammer_self_pushback_ticks = val as u32);
    update_field(&mut errors, &cfg.0, "hammer_pushback", |val| config.hammer_pushback = val);
    update_field(&mut errors, &cfg.0, "hammer_pushback_ticks", |val| config.hammer_pushback_ticks = val as u32);
    update_field(&mut errors, &cfg.0, "sword_attack_damage", |val| config.sword_attack_damage = val);
    update_field(&mut errors, &cfg.0, "hammer_attack_damage", |val| config.hammer_attack_damage = val);
    update_field(&mut errors, &cfg.0, "bow_attack_damage", |val| config.bow_attack_damage = val);
    update_field(&mut errors, &cfg.0, "bow_self_pushback", |val| config.bow_self_pushback = val);
    update_field(&mut errors, &cfg.0, "bow_self_pushback_ticks", |val| config.bow_self_pushback_ticks = val as u32);
    update_field(&mut errors, &cfg.0, "bow_pushback", |val| config.bow_pushback = val);
    update_field(&mut errors, &cfg.0, "bow_pushback_ticks", |val| config.bow_pushback_ticks = val as u32);
    update_field(&mut errors, &cfg.0, "arrow_velocity", |val| config.arrow_velocity = val);
    update_field(&mut errors, &cfg.0, "default_on_hit_screenshake_strength", |val| config.default_on_hit_screenshake_strength = val);
    update_field(&mut errors, &cfg.0, "default_on_hit_screenshake_duration_secs", |val| config.default_on_hit_screenshake_duration_secs = val);
    update_field(&mut errors, &cfg.0, "default_on_hit_screenshake_frequency", |val| config.default_on_hit_screenshake_frequency = val);
    update_field(&mut errors, &cfg.0, "hammer_on_hit_screenshake_strength", |val| config.hammer_on_hit_screenshake_strength = val);
    update_field(&mut errors, &cfg.0, "hammer_on_hit_screenshake_duration_secs", |val| config.hammer_on_hit_screenshake_duration_secs = val);
    update_field(&mut errors, &cfg.0, "hammer_on_hit_screenshake_frequency", |val| config.hammer_on_hit_screenshake_frequency = val);
    update_field(&mut errors, &cfg.0, "passive_gain_rate", |val| config.passive_gain_rate = val as u32);

    for error in errors{
       warn!("failed to load player cfg value: {}", error);
   }
}

fn load_player_stats(
    player_config: Res<PlayerConfig>,
    mut stat_q: Query<&mut PlayerStats>,
) {
    stat_q.iter_mut().for_each(|mut stats| {
        *stats = PlayerStats::init_from_config(&player_config);
    });
}

/// Extend with additional parameter Stats
#[allow(clippy::enum_variant_names)]
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
    /// Offsetting Value on Stat, e.g. 100.0 - 10.0 = 90.0
    delta: Vec<f32>,

    effect_col: Color,

    time_remaining: f32,
}

// TODO: move to attack
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
            effect_col: Color::hex("7aa7ff").unwrap(), /* For More Visible Effect */
            time_remaining: 2.0,
        }
    }
}

#[derive(Component)]
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

    pub fn set(&mut self, stat: StatType, value: f32) {
        if let Some(effective_stat) = self.effective_stats.get_mut(&stat) {
            *effective_stat = value;
        }
    }

    pub fn reset_stat(&mut self, stat: StatType) {
        if let Some(effective_stat) = self.effective_stats.get_mut(&stat) {
            *effective_stat = self.base_stats[&stat];
        }
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
    }
}

#[derive(Component, Deref, DerefMut)]
pub struct EnemiesNearby(u32);

#[derive(Component, Debug)]
pub struct PlayerStatMod {
    pub attack: f32,
    pub defense: f32,
    pub speed: f32,
    pub cdr: f32,
    pub sharpshooter_multiplier: f32,
}

impl PlayerStatMod {
    fn new() -> PlayerStatMod {
        PlayerStatMod {
            attack: 1.,
            defense: 1.,
            speed: 1.,
            cdr: 1.,
            sharpshooter_multiplier: 1.,
        }
    }
}

fn player_update_passive_buffs(
    mut query: Query<(
        &Passives,
        &LinearVelocity,
        &GlobalTransform,
        &EnemiesNearby,
        &BuffTick,
        &mut PlayerStatMod,
        &Health,
        Option<&Grounded>,
        Has<Stealthing>,
        Option<&Idle>,
        Option<&Running>,
        Option<&Jumping>,
    )>,
    enemy_q: Query<&GlobalTransform, With<Enemy>>,
) {
    for (passives, vel, transform, enemies_nearby, buff_tick, mut stat_mod, health, grounded, is_stealth, idle, running, jumping) in query.iter_mut() {
        let mut attack = 1.;
        let mut defense = 1.;
        let mut speed = 1.;
        let mut cdr = 1.;
        let mut sharpshooter_mult = stat_mod.sharpshooter_multiplier;

        if passives.contains(&Passive::GlowingShard) {
            defense *= 1. + 1. * enemies_nearby.0 as f32;
        }
        if passives.contains(&Passive::SerpentRing) {
            speed *= 1.2;
            cdr *= 1.33;
        }
        if passives.contains(&Passive::RabbitsFoot) {
            speed *= 1.2;
        }
        if passives.contains(&Passive::HeavyBoots) {
            if vel.0.length() > 0.0001 {
                attack *= 0.5;
                defense *= 0.5;
            } else {
                attack *= 2.;
                defense *= 2.5;
            }
        }
        if passives.contains(&Passive::FrenziedAttack) {
            cdr *= 1. + (0.1 * buff_tick.stacks as f32).min(1.);
        }
        if passives.contains(&Passive::DeadlyFeather) {
            if grounded.is_none() {
                attack *= 1.5;
                cdr *= 1.3;
            } else {
                defense *= 0.5;
            }
        }
        if is_stealth {
            attack *= 2.;
        }

        if passives.contains(&Passive::Sharpshooter) {
            if running.is_some() {
                let player_pos = transform.translation().truncate();
                let mut nearest_distance = f32::MAX;

                for enemy_tf in enemy_q.iter() {
                    let enemy_pos = enemy_tf.translation().truncate();
                    let d = player_pos.distance(enemy_pos);
                    if d < nearest_distance {
                        nearest_distance = d;
                    }
                }

                sharpshooter_mult = if nearest_distance >= 150.0 {
                    3.0
                } else if nearest_distance >= 120.0 {
                    2.5
                } else if nearest_distance >= 90.0 {
                    2.0
                } else if nearest_distance >= 60.0 {
                    1.5
                } else if nearest_distance >= 30.0 {
                    1.25
                } else {
                    1.0
                };
            }
            attack *= sharpshooter_mult;
        }

        if passives.contains(&Passive::VitalityOverclock) {
            let ratio = health.current as f32 / health.max as f32;
            let multiplier = if ratio >= 0.9 {
                3.0
            } else if ratio >= 0.7 {
                2.0
            } else if ratio >= 0.5 {
                1.5
            } else if ratio >= 0.25 {
                1.25
            } else {
                1.0
            };
            attack *= multiplier;
        }

        *stat_mod = PlayerStatMod {
            attack,
            defense,
            speed,
            cdr,
            sharpshooter_multiplier: sharpshooter_mult,
        };
    }
}

fn player_update_stats_mod(
    mut query: Query<(
        Entity,
        &mut StatusModifier,
        &mut PlayerStats,
    )>,
    mut gfx_query: Query<(&PlayerGfx, &mut Sprite)>,
    // TODO: switch to ticks
    time: Res<Time<Virtual>>,
    mut commands: Commands,
) {
    for (p_gfx, mut sprite) in gfx_query.iter_mut() {
        let Ok((entity, mut modifier, mut player_stats)) =
            query.get_mut(p_gfx.e_gent)
        else {
            return;
        };

        if modifier.is_changed() {
            player_stats.update_stats(&modifier);
        }

        sprite.color = modifier.effect_col;

        // TODO: switch to ticks
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
    dash_duration: f32,
}

#[derive(Resource)]
pub struct DashIconAssetHandle {
    tex: Handle<Image>,
    atlas: TextureAtlas,
}
#[derive(Resource)]
pub struct DashDownIconAssetHandle {
    tex: Handle<Image>,
    atlas: TextureAtlas,
}

pub fn load_dash_asset(
    assets: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut commands: Commands,
) {
    let dash_tex: Handle<Image> =
        assets.load("animations/player/movement/Dash.png");
    let dash_layout =
        TextureAtlasLayout::from_grid(Vec2::new(96.0, 96.0), 1, 1, None, None);
    let dash_layout_handle = texture_atlas_layouts.add(dash_layout);

    let dash_down_tex: Handle<Image> =
        assets.load("animations/player/sword/DashDownSheet.png");
    let dash_down_layout =
        TextureAtlasLayout::from_grid(Vec2::new(48.0, 48.0), 6, 1, None, None);
    let dash_down_layout_handle = texture_atlas_layouts.add(dash_down_layout);

    commands.insert_resource(DashIconAssetHandle {
        tex: dash_tex,
        atlas: TextureAtlas {
            layout: dash_layout_handle,
            index: 0,
        },
    });

    commands.insert_resource(DashDownIconAssetHandle {
        tex: dash_down_tex,
        atlas: TextureAtlas {
            layout: dash_down_layout_handle,
            index: 1,
        },
    });
}

pub fn player_dash_fx(
    mut query: Query<
        (
            &GlobalTransform,
            &Facing,
            //            &LinearVelocity,
            &Dashing,
            Option<&Stealthing>,
        ),
        With<Player>,
    >,
    config: Res<PlayerConfig>,
    time: Res<GameTime>,
    mut commands: Commands,
    dash_asset: Res<DashIconAssetHandle>,
    dash_down_asset: Res<DashDownIconAssetHandle>,
) {
    for (global_tr, facing, dashing, stealthing_maybe) in query.iter() {
        let pos = global_tr.translation();

        let t = time.time_in_seconds() as f32 + dashing.dash_duration(&config);

        let init_a = match stealthing_maybe {
            Some(_) => 0.2,
            None => 0.5,
        };

        let (tex, atlas) = if dashing.is_down_dash() {
            (
                dash_down_asset.tex.clone(),
                dash_down_asset.atlas.clone(),
            )
        } else {
            (
                dash_asset.tex.clone(),
                dash_asset.atlas.clone(),
            )
        };

        commands.spawn((
            SpriteSheetBundle {
                sprite: Sprite {
                    flip_x: facing.direction() < 0.,
                    ..default()
                },
                transform: Transform::from_translation(pos),
                texture: tex.clone(),
                atlas,
                ..default()
            },
            DashIcon {
                time: t,
                init_a,
                dash_duration: dashing.dash_duration(&config),
            },
        ));
    }
}

pub fn dash_icon_fx(
    mut commands: Commands,
    mut query: Query<(Entity, &DashIcon, &mut Sprite)>,
    time: Res<GameTime>,
) {
    for (entity, icon, mut sprite) in query.iter_mut() {
        let d = time.time_in_seconds() as f32 - icon.time;

        let r = d / icon.dash_duration;

        if r >= 1.0 {
            commands.entity(entity).despawn();
        } else {
            sprite.color.set_a((1.0 - r) * icon.init_a);
        }
    }
}

pub fn on_crit_cooldown_reduce(
    attack_query: Query<&Attack, (With<Crit>, Added<Hit>)>,
    mut attacker_query: Query<(
        &Passives,
        Option<&mut CanDash>,
        Option<&mut WhirlAbility>,
        Option<&mut CanStealth>,
    )>,
) {
    for attack in attack_query.iter() {
        if let Ok((
            passives,
            mut maybe_can_dash,
            mut maybe_whirl_ability,
            mut maybe_can_stealth,
        )) = attacker_query.get_mut(attack.attacker)
        {
            if passives.contains(&Passive::ObsidianNecklace) {
                if let Some(ref mut can_dash) = maybe_can_dash {
                    can_dash.remaining_cooldown -= 0.5;
                }
                if let Some(ref mut whirl_ability) = maybe_whirl_ability {
                    whirl_ability.energy += 0.5;
                }
                if let Some(ref mut can_stealth) = maybe_can_stealth {
                    can_stealth.remaining_cooldown -= 0.5;
                }
            }
        }
    }
}

/// Resets the players cooldowns/energy on hit of a stealthed critical hit
pub fn on_stealth_hit_cooldown_reset(
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

/// Exits player Stealthing state when a stealthed attack first hits
pub fn on_hit_exit_stealthing(
    query: Query<&Attack, (With<Hit>, With<Stealthed>)>,
    mut attacker_query: Query<(&Gent, &mut TransitionQueue), With<Player>>,
    mut sprites: Query<&mut Sprite, Without<Player>>,
    config: Res<PlayerConfig>,
) {
    for attack in query.iter() {
        if let Ok((gent, mut transitions)) = attacker_query.get_mut(attack.attacker) {
            let mut sprite = sprites.get_mut(gent.e_gfx).unwrap();
            sprite.color = sprite.color.with_a(1.0);
            transitions.push(Stealthing::new_transition(CanStealth::new(&config)));
        }
    }
}

pub fn on_xp_heal(
    mut query: Query<(&Passives, &mut Health), With<Player>>,
    mut xp_event: EventReader<XpOrbPickup>,
) {
    if let Ok((passives, mut health)) = query.get_single_mut() {
        if passives.contains(&Passive::Bloodstone) {
            for _event in xp_event.read() {
                health.current = (health.current + 2).min(health.max);
            }
        }
    }
}

#[derive(Default, Component)]
pub struct BuffTick {
    pub falloff: u32,
    pub stacks: u32,
}

fn track_hits(
    mut query: Query<
        (
            Entity,
            &Passives,
            &mut Health,
            &mut BuffTick,
        ),
        With<Player>,
    >,
    mut damage_events: EventReader<DamageInfo>,
) {
    if let Ok((player_e, passives, mut health, mut buff)) =
        query.get_single_mut()
    {
        // tick falloff
        buff.falloff = buff.falloff.saturating_sub(1);
        if passives.contains(&Passive::FrenziedAttack) {
            for damage_info in damage_events.read() {
                if damage_info.attacker == player_e {
                    buff.falloff = 288;
                    buff.stacks += 1;
                    // leaves the player at minimum of 1 health
                    health.current =
                        health.current.saturating_sub(buff.stacks).max(1);
                }
            }
        }
        if buff.falloff == 0 {
            buff.stacks = 0
        }
    }
}

fn update_serpentring_health(
    query: Query<(&Passives, Option<&Grounded>), With<Player>>,
    mut health_q: Query<&mut Health, With<Player>>,
    config: Res<PlayerConfig>,
) {
    if let Ok((passives, grounded)) = query.get_single() {
        if let Ok(mut health) = health_q.get_single_mut() {
            if passives.contains(&Passive::SerpentRing) {
            
                // Halve the max health when grounded.
                let new_max = ((config.max_health as f32) / 4.0) as u32;
                health.max = new_max;
                // Ensure current health does not exceed the new maximum.
                if health.current > new_max {
                    health.current = new_max;
                }
            } else {
                health.max = config.max_health as u32;
            }
        }
    }
}

/// When an attack hit is critical, heal the attacker if they have CriticalRegeneration.
fn on_crit_heal(
    attack_query: Query<&Attack, (With<Crit>, Added<Hit>)>,
    mut player_query: Query<(&Passives, &mut Health), With<Player>>,
) {
    for attack in attack_query.iter() {
        if let Ok((passives, mut health)) = player_query.get_mut(attack.attacker) {
            if passives.contains(&Passive::CriticalRegeneration) {
                // Heal 2 points, capped at player's max health.
                health.current = (health.current + 24).min(health.max);
            }
        }
    }
}

/// Increases player's attack and applies constant health degeneration.
fn apply_vitality_overclock(
    mut query: Query<(&Passives, &mut Health, &mut PlayerStatMod), With<Player>>,
    mut tick: Local<u32>,
) {
    *tick += 1;
    for (passives, mut health, mut stat_mod) in query.iter_mut() {
        if passives.contains(&Passive::VitalityOverclock) {

            let mut deg_rate = 0;

            if  passives.contains(&Passive::SerpentRing) {
                deg_rate = 40
            } else {
                deg_rate = 20

            }
            // Every n ticks (depending on deg_tick rate), apply the health degeneration.
            if *tick % deg_rate == 0 && health.current > 1 {
                let mut deg = 1;
                health.current = health.current.saturating_sub(deg);
            }
        }
    }
}
