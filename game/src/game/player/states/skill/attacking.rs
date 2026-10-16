//! The basic attack: a melee swing whose hitbox follows the animation, or a volley of
//! arrows, plus the pogo bounce after a downward air swing lands.
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use rand::Rng;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::effects::{GhostColorMode, GhostingSource};
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::{
    groups, Collider, CollisionGroups, LinearVelocity, PhysicsWorld, ShapeCast,
    TileCollisionWorld, ENEMY, PLAYER_ATTACK,
};
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::{GameTickUpdate, GameTime};

use super::animation_hitbox;
use crate::camera::GameplayPixelScale;
use crate::game::combat::damage_source::apply_damage_modifications;
use crate::game::combat::sparks::WeaponHitSlot;
use crate::game::combat::{
    DamageSource, DownwardAttack, Hit, Pushback, SelfPushback, Stealthed,
};
use crate::game::effects::stealthed::StealthEffect;
use crate::game::gentstate::Facing;
use crate::game::physics::projectile::{Arrow, PreviousPosition, Projectile};
use crate::game::physics::Knockback;
use crate::game::player::autoaim::{
    melee_target_below, ranged_basic_attack_auto_aim_facing, visible_reach,
    LiveEnemies, RangedAttackFacingLock,
};
use crate::game::player::foreground::{
    finish_action, ActionMut, ActionTag, ForegroundAction,
};
use crate::game::player::passives::{self, pulse_echo::PulseEcho};
use crate::game::player::player_anim::set_direction_slots;
use crate::game::player::sensors::CeilingSensor;
use crate::game::player::states::{
    transition_locomotion, AttackVariant, Attacking, Falling,
    HorizontalInputIntent, Jumping, Running, WallSide,
};
use crate::game::player::weapon::{WeaponAttack, WeaponType};
use crate::game::player::{
    Dashing, JumpCount, Passives, Player, PlayerBehaviorSet, PlayerStatMod,
    PlayerStateSet, PlayerStats,
};

const ARROW_LIFETIME_TICKS: u32 = 192;
const ARROW_SPEED: f32 = 500.0;
const ARROW_HALF_EXTENTS: Vec2 = Vec2::new(4.0, 1.0);
/// Arrows fly in one of this many lanes spread sideways across ±`ARROW_LANE_SPREAD`
/// pixels, preferring lanes that reach an enemy before a wall.
const ARROW_LANE_COUNT: usize = 5;
const ARROW_LANE_SPREAD: f32 = 4.0;

/// Upward displacement per tick while bouncing off an enemy after a downward air swing.
const POGO_VELOCITIES: [f32; 24] = [
    0.818, 0.781, 0.745, 0.708, 0.672, 0.635, 0.599, 0.563, 0.526, 0.490,
    0.453, 0.417, 0.380, 0.344, 0.307, 0.271, 0.234, 0.198, 0.161, 0.125,
    0.089, 0.052, 0.016, 0.000,
];

#[derive(Component, Debug)]
pub struct Pogo {
    tick: u32,
}

impl Pogo {
    /// Applies this tick's bounce speed; false once the curve is spent.
    fn step(&mut self, velocity: &mut Vec2) -> bool {
        velocity.y = POGO_VELOCITIES[(self.tick as usize).min(POGO_VELOCITIES.len() - 1)];
        self.tick += 1;
        (self.tick as usize) < POGO_VELOCITIES.len()
    }
}

pub struct AttackingStatePlugin;

impl Plugin for AttackingStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (attacking_enter_system, attacking_update_system)
                .chain()
                .in_set(PlayerStateSet::Behavior)
                .in_set(PlayerBehaviorSet::Execution),
        );
        // Bounce once the damage pipeline has tagged the hit.
        app.add_systems(
            GameTickUpdate,
            start_pogo_on_downward_attack_hit.after(apply_damage_modifications),
        );
        app.add_systems(
            GameTickUpdate,
            pogo_update_system
                .in_set(PlayerStateSet::Behavior)
                .in_set(PlayerBehaviorSet::Execution),
        );
    }
}

/// A blow being dealt: who strikes, facing which way, how hard, and whether from stealth.
#[derive(Clone, Copy)]
struct Strike {
    owner: Entity,
    facing: Facing,
    damage: f32,
    stealthed: bool,
}

/// What a bow shot reads from the world to aim.
#[derive(SystemParam)]
struct BowWorld<'w> {
    physics: PhysicsWorld<'w>,
    tiles: Option<Res<'w, TileCollisionWorld>>,
    pixel_scale: Option<Res<'w, GameplayPixelScale>>,
}

/// Starts an attack: aims it, plays its animation, and spawns its hitbox or arrows.
fn attacking_enter_system(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            ActionMut<Attacking>,
            &GlobalTransform,
            &mut Facing,
            Option<&Falling>,
            &Gent,
            Option<&PlayerStatMod>,
            Option<&Passives>,
            Has<Running>,
            Has<Jumping>,
            Has<StealthEffect>,
        ),
        (With<Player>, Added<ActionTag<Attacking>>),
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>>,
    enemies: LiveEnemies,
    time: Res<GameTime>,
    bow_world: BowWorld,
) {
    for (
        entity,
        mut attacking,
        transform,
        mut facing,
        falling,
        gent,
        stat_mod,
        passives,
        is_running,
        is_jumping,
        is_stealthed,
    ) in &mut query
    {
        let position = transform.translation();
        let is_airborne = is_jumping || falling.is_some();
        let weapon = attacking.weapon;
        if weapon == WeaponType::Bow {
            *facing = aim_bow(*facing, falling, position.xy(), &bow_world, &enemies);
            commands
                .entity(entity)
                .insert(RangedAttackFacingLock::new(*facing, time.tick()));
        }
        let variant = swing_variant(
            weapon,
            attacking.variant,
            is_airborne,
            position.xy(),
            &enemies,
        );
        attacking.variant = variant;

        let attack = weapon.attack();
        let animation = if variant == AttackVariant::Down || is_airborne {
            attack.air_animation
        } else if is_running {
            attack.run_animation
        } else {
            attack.idle_animation
        };
        if let Ok(mut player) = gfx_query.get_mut(gent.e_gfx) {
            play_attack_animation(&mut player, attacking.execution_mut(), &facing, animation, variant);
        }

        let Some(stat_mod) = stat_mod else {
            continue;
        };
        let strike = Strike {
            owner: entity,
            facing: *facing,
            damage: attack.damage * stat_mod.damage,
            stealthed: is_stealthed,
        };
        if weapon == WeaponType::Bow {
            let reach = lane_reach(bow_world.pixel_scale.as_deref(), time.seconds_per_tick() as f32);
            shoot_volley(&mut commands, strike, position, passives, &bow_world, reach);
            continue;
        }
        let hitbox = spawn_swing(&mut commands, strike, gent, attack, variant);
        // Downward swings echo downward; others echo ahead.
        let echo_direction = if variant == AttackVariant::Down {
            Vec2::NEG_Y
        } else {
            Vec2::new(facing.direction(), 0.0)
        };
        if let Some(echo) = passives.and_then(|passives| PulseEcho::of(passives, stat_mod.damage, is_stealthed)) {
            commands.entity(hitbox).insert(echo.on_swing(echo_direction));
        }
        attacking.execution_mut().own(hitbox);
    }
}

/// Bows shoot away from a wall the player slides down, then turn toward the nearest enemy.
fn aim_bow(
    facing: Facing,
    falling: Option<&Falling>,
    position: Vec2,
    bow_world: &BowWorld,
    enemies: &LiveEnemies,
) -> Facing {
    let facing = match falling.and_then(|falling| falling.wall_slide) {
        Some(WallSide::Left) => Facing::Right,
        Some(WallSide::Right) => Facing::Left,
        None => facing,
    };
    ranged_basic_attack_auto_aim_facing(position, facing, bow_world.pixel_scale.as_deref(), enemies)
}

/// The swing direction: bows always shoot forward, grounded swings can't go down, and an
/// airborne forward swing turns downward onto an enemy below.
fn swing_variant(
    weapon: WeaponType,
    requested: AttackVariant,
    is_airborne: bool,
    position: Vec2,
    enemies: &LiveEnemies,
) -> AttackVariant {
    if !weapon.is_melee() || (requested == AttackVariant::Down && !is_airborne) {
        return AttackVariant::Forward;
    }
    if requested == AttackVariant::Forward && is_airborne && melee_target_below(position, enemies) {
        return AttackVariant::Down;
    }
    requested
}

fn play_attack_animation(
    player: &mut ScriptPlayer<SpriteAnimation>,
    execution: &mut ForegroundAction,
    facing: &Facing,
    key: &str,
    variant: AttackVariant,
) {
    set_direction_slots(player, facing);
    execution.play(player, key);
    player.set_presentation_slot(execution.id, "AttackTransition", false);
    player.set_presentation_slot(execution.id, "DownwardAttack", variant == AttackVariant::Down);
}

/// A melee swing's hitbox, shaped by the animation and active for the swing's window.
/// Downward swings only happen in the air, where a hit starts a pogo bounce.
fn spawn_swing(
    commands: &mut Commands,
    strike: Strike,
    gent: &Gent,
    attack: &WeaponAttack,
    variant: AttackVariant,
) -> Entity {
    let lifetime = attack.swing(variant).hitbox_ticks;
    let direction = strike.facing.direction();
    commands
        .spawn((
            animation_hitbox(
                strike.owner,
                gent,
                DamageSource::new(lifetime, strike.owner, strike.damage),
                attack.hit_slot,
            ),
            Pushback(attack.pushback.horizontal(direction)),
            SelfPushback(attack.self_pushback.horizontal(-direction)),
        ))
        .insert_if(Stealthed, || strike.stealthed)
        .insert_if(DownwardAttack, || variant == AttackVariant::Down)
        .id()
}

/// Fires one arrow per direction (more with multishot passives), each in a lane that
/// reaches an enemy when any does.
fn shoot_volley(
    commands: &mut Commands,
    strike: Strike,
    origin: Vec3,
    passives: Option<&Passives>,
    bow_world: &BowWorld,
    reach: f32,
) {
    let forward = Vec2::X * strike.facing.direction();
    let directions = passives.map_or_else(
        || vec![forward],
        |passives| passives::projectile_directions(passives, forward),
    );
    let bow = WeaponType::Bow.attack();
    let collider = Collider::cuboid(ARROW_HALF_EXTENTS.x, ARROW_HALF_EXTENTS.y);
    let mut rng = rand::rng();
    for direction in directions.into_iter().filter_map(Vec2::try_normalize) {
        let lane = choose_arrow_lane(&mut rng, |offset| {
            let start = lane_origin(origin.xy(), direction, offset);
            lane_hits_enemy(bow_world, strike.owner, start, direction, &collider, reach)
        });
        let start = lane_origin(origin.xy(), direction, lane).extend(origin.z);
        commands
            .spawn(player_arrow(
                DamageSource::new(ARROW_LIFETIME_TICKS, strike.owner, strike.damage),
                Transform::from_translation(start),
                direction * ARROW_SPEED,
                bow.pushback.horizontal(strike.facing.direction()),
                "anim.player.BowBasicArrow",
            ))
            .insert_if(Stealthed, || strike.stealthed);
    }
}

/// A player arrow in flight: a bow shot, or the body of a Pulse Echo.
pub(crate) fn player_arrow(
    damage: DamageSource,
    transform: Transform,
    velocity: Vec2,
    pushback: Knockback,
    animation: &str,
) -> impl Bundle {
    let mut player = ScriptPlayer::<SpriteAnimation>::default();
    player.play_key(animation);
    (
        damage.with_max_targets(1),
        Projectile { vel: LinearVelocity(velocity) },
        Arrow,
        PreviousPosition(transform.translation.xy()),
        Collider::cuboid(ARROW_HALF_EXTENTS.x, ARROW_HALF_EXTENTS.y),
        groups::player_attack(),
        Pushback(pushback),
        player,
        Sprite { texture_atlas: Some(TextureAtlas::default()), ..default() },
        transform,
        GlobalTransform::default(),
        Visibility::Visible,
        InheritedVisibility::default(),
        GhostingSource {
            spawn_interval_ticks: 3,
            ghost_lifetime_ticks: 19,
            initial_alpha: 0.3,
            color_mode: GhostColorMode::Tint(Color::srgb(3.0, 3.0, 3.0)),
            ..default()
        },
        WeaponHitSlot { slot_name: WeaponType::Bow.attack().hit_slot.to_owned() },
    )
}

/// How far lane probes look: as far as an arrow flies, but no farther than the player sees.
fn lane_reach(pixel_scale: Option<&GameplayPixelScale>, seconds_per_tick: f32) -> f32 {
    let flight = ARROW_SPEED * ARROW_LIFETIME_TICKS as f32 * seconds_per_tick;
    flight.min(visible_reach(pixel_scale))
}

fn lane_origin(origin: Vec2, direction: Vec2, offset: f32) -> Vec2 {
    let lateral_axis = Vec2::new(-direction.y, direction.x);
    origin + lateral_axis * offset
}

/// Picks one random offset per lane, then a random lane among those that reach an enemy,
/// or among all lanes when none does.
fn choose_arrow_lane(rng: &mut impl Rng, mut hits_enemy: impl FnMut(f32) -> bool) -> f32 {
    let offsets = lane_offsets(rng);
    let hits: Vec<f32> = offsets.into_iter().filter(|&offset| hits_enemy(offset)).collect();
    if hits.is_empty() {
        offsets[rng.random_range(0..ARROW_LANE_COUNT)]
    } else {
        hits[rng.random_range(0..hits.len())]
    }
}

fn lane_offsets(rng: &mut impl Rng) -> [f32; ARROW_LANE_COUNT] {
    let lane_width = ARROW_LANE_SPREAD * 2.0 / ARROW_LANE_COUNT as f32;
    std::array::from_fn(|lane| {
        -ARROW_LANE_SPREAD + (lane as f32 + rng.random_range(0.0..1.0)) * lane_width
    })
}

fn lane_hits_enemy(
    bow_world: &BowWorld,
    owner: Entity,
    origin: Vec2,
    direction: Vec2,
    collider: &Collider,
    reach: f32,
) -> bool {
    let cast = ShapeCast {
        origin,
        dir: direction,
        shape: collider,
        max_toi: reach,
        interaction: CollisionGroups::new(PLAYER_ATTACK, ENEMY),
        exclude: Some(owner),
    };
    let enemy_distance = bow_world
        .physics
        .cast_shape(cast, |_| true)
        .map(|(_, hit)| hit.time_of_impact);
    let wall_distance = bow_world
        .tiles
        .as_deref()
        .filter(|tiles| tiles.is_ready())
        .and_then(|tiles| tiles.ray_cast_solid(origin, direction, reach))
        .map_or(f32::INFINITY, |hit| hit.time_of_impact);
    enemy_before_wall(enemy_distance, wall_distance)
}

fn enemy_before_wall(enemy_distance: Option<f32>, wall_distance: f32) -> bool {
    enemy_distance.is_some_and(|distance| distance < wall_distance)
}

/// Ends the attack after its duration; the player stops unless bouncing.
fn attacking_update_system(
    mut commands: Commands,
    mut query: Query<
        (Entity, ActionMut<Attacking>, &mut LinearVelocity, Has<Pogo>),
        With<Player>,
    >,
) {
    for (entity, mut attacking, mut velocity, is_bouncing) in &mut query {
        attacking.tick += 1;
        if attacking.tick < attacking.duration {
            continue;
        }
        if !is_bouncing {
            velocity.0 = Vec2::ZERO;
        }
        finish_action(&mut commands, entity, attacking.execution().id);
    }
}

/// A downward air swing's first hit bounces the attacker and restores their jumps.
fn start_pogo_on_downward_attack_hit(
    mut commands: Commands,
    hits: Query<&DamageSource, (Added<Hit>, With<DownwardAttack>)>,
    mut players: Query<(&mut LinearVelocity, Option<&mut JumpCount>), With<Player>>,
) {
    for attack in &hits {
        let Ok((mut velocity, jump_count)) = players.get_mut(attack.owner) else {
            continue;
        };
        velocity.0.y = POGO_VELOCITIES[0];
        commands.entity(attack.owner).insert(Pogo { tick: 0 });
        if let Some(mut jump_count) = jump_count {
            jump_count.reset();
        }
    }
}

/// Follows the bounce curve with normal air control, until the curve ends or the player
/// hits a ceiling.
fn pogo_update_system(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &mut LinearVelocity,
            &mut Pogo,
            Option<&mut Falling>,
            Option<&Jumping>,
            &CeilingSensor,
            &HorizontalInputIntent,
            &PlayerStats,
            &PlayerStatMod,
            Has<ActionTag<Dashing>>,
        ),
        With<Player>,
    >,
) {
    for (
        entity,
        mut velocity,
        mut pogo,
        falling,
        jumping,
        ceiling,
        intent,
        stats,
        stat_mod,
        is_dashing,
    ) in &mut query
    {
        let blocked = ceiling.is_touching_ceiling;
        if !blocked && !is_dashing {
            velocity.0.x = air_control(intent.movement, stats, stat_mod);
        }
        if !blocked && pogo.step(&mut velocity.0) {
            continue;
        }
        velocity.0.y = 0.0;
        end_pogo(&mut commands, entity, falling, jumping);
    }
}

/// Horizontal speed while bouncing, the same as normal movement.
fn air_control(movement: f32, stats: &PlayerStats, stat_mod: &PlayerStatMod) -> f32 {
    if movement == 0.0 {
        0.0
    } else {
        movement * stats.max_move_velocity * stat_mod.speed
    }
}

/// Ends the bounce in a fresh fall, keeping the jump count of an ongoing jump.
fn end_pogo(
    commands: &mut Commands,
    entity: Entity,
    falling: Option<Mut<Falling>>,
    jumping: Option<&Jumping>,
) {
    if let Some(mut falling) = falling {
        falling.fall_ticks = 0;
        falling.wall_slide = None;
    } else {
        let jump_count = jumping.map_or(0, |jumping| jumping.jump_count);
        transition_locomotion(commands, entity, Falling::from_jump(jump_count));
    }
    commands.entity(entity).remove::<Pogo>();
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use rand::{rngs::StdRng, SeedableRng};
    use theseeker_engine::physics::EntityCollisionWorld;

    use super::*;
    use crate::game::enemy::Enemy;
    use crate::game::player::foreground;

    /// A player starting `attack`, optionally in the air, after the enter system ran.
    fn attack(attack: Attacking, airborne: bool, enemy_at: Option<Vec2>) -> (World, Entity) {
        let mut world = World::new();
        world.init_resource::<GameTime>();
        world.init_resource::<EntityCollisionWorld>();
        let gfx = world.spawn(ScriptPlayer::<SpriteAnimation>::default()).id();
        let player = world
            .spawn((
                Player,
                GlobalTransform::default(),
                Facing::Right,
                Gent { e_gfx: gfx, e_effects_gfx: gfx },
                PlayerStatMod::new(),
            ))
            .id();
        if airborne {
            world.entity_mut(player).insert(Falling::new());
        }
        if let Some(position) = enemy_at {
            world.spawn((Enemy, GlobalTransform::from_translation(position.extend(0.0)), Collider::cuboid(4.0, 4.0)));
        }
        foreground::start_action(&mut world, player, attack);
        world.run_system_once(attacking_enter_system).expect("attack systems run");
        (world, player)
    }

    fn state(world: &World, player: Entity) -> &Attacking {
        world.get::<ForegroundAction>(player).and_then(|action| action.state()).expect("attacking")
    }

    #[test]
    fn a_swing_takes_its_timing_and_knockback_from_the_weapon() {
        let (mut world, player) = attack(Attacking::new(AttackVariant::Up, WeaponType::Sword), false, None);

        let (damage, push, recoil, slot, parent) = world
            .query::<(&DamageSource, &Pushback, &SelfPushback, &WeaponHitSlot, &ChildOf)>()
            .single(&world)
            .expect("one swing hitbox");
        assert_eq!((damage.max_lifetime, damage.damage), (16, 33.0));
        assert_eq!((push.strength, push.max_ticks), (Vec2::new(90.0 / 96.0, 0.0), 12));
        assert_eq!((recoil.strength, recoil.max_ticks), (Vec2::new(-90.0 / 96.0, 0.0), 3));
        assert_eq!((slot.slot_name.as_str(), parent.parent()), ("SwordHit", player));
        assert_eq!(state(&world, player).duration, 36);
    }

    #[test]
    fn an_airborne_forward_swing_turns_down_onto_an_enemy_below() {
        let enemy = Some(Vec2::new(0.0, -30.0));
        let (mut world, player) = attack(Attacking::new(AttackVariant::Forward, WeaponType::Hammer), true, enemy);

        let damage = world
            .query_filtered::<&DamageSource, With<DownwardAttack>>()
            .single(&world)
            .expect("a downward swing");
        assert_eq!(damage.max_lifetime, 30);
        // The attack keeps the length of the swing that was requested.
        assert_eq!((state(&world, player).variant, state(&world, player).duration), (AttackVariant::Down, 56));
    }

    #[test]
    fn grounded_swings_cannot_go_down() {
        let (mut world, player) = attack(Attacking::new(AttackVariant::Down, WeaponType::Sword), false, None);

        assert_eq!(state(&world, player).variant, AttackVariant::Forward);
        assert_eq!(world.query_filtered::<(), With<DownwardAttack>>().iter(&world).count(), 0);
    }

    #[test]
    fn the_bow_shoots_one_forward_arrow_and_locks_facing() {
        let (mut world, player) = attack(Attacking::new(AttackVariant::Down, WeaponType::Bow), false, None);

        let (damage, projectile) = world
            .query::<(&DamageSource, &Projectile)>()
            .single(&world)
            .expect("one arrow");
        assert_eq!((damage.max_lifetime, damage.max_targets, damage.damage), (192, 1, 22.0));
        assert_eq!(projectile.vel.0, Vec2::new(500.0, 0.0));
        assert_eq!(state(&world, player).variant, AttackVariant::Forward);
        assert!(world.get::<RangedAttackFacingLock>(player).is_some());
    }

    #[test]
    fn lane_offsets_cover_every_lane() {
        let mut rng = StdRng::seed_from_u64(5);
        let lane_width = ARROW_LANE_SPREAD * 2.0 / ARROW_LANE_COUNT as f32;

        for (lane, offset) in lane_offsets(&mut rng).into_iter().enumerate() {
            let min = -ARROW_LANE_SPREAD + lane as f32 * lane_width;
            assert!((min..min + lane_width).contains(&offset));
        }
    }

    #[test]
    fn lane_choice_prefers_a_lane_that_hits() {
        let mut rng = StdRng::seed_from_u64(11);
        let mut probes = 0;

        let lane = choose_arrow_lane(&mut rng, |offset| {
            probes += 1;
            offset > 1.0
        });

        assert_eq!(probes, ARROW_LANE_COUNT);
        assert!(lane > 1.0);
    }

    #[test]
    fn lane_choice_stays_random_when_no_lane_hits() {
        let mut rng = StdRng::seed_from_u64(19);

        let lane = choose_arrow_lane(&mut rng, |_| false);

        assert!((-ARROW_LANE_SPREAD..ARROW_LANE_SPREAD).contains(&lane));
    }

    #[test]
    fn each_direction_uses_its_own_lateral_axis() {
        let origin = Vec2::new(4.0, -3.0);
        let directions = [
            Vec2::X,
            Vec2::from_angle(15.0_f32.to_radians()),
            Vec2::from_angle(-15.0_f32.to_radians()),
        ];

        for direction in directions {
            let displacement = lane_origin(origin, direction, 5.0) - origin;
            assert!(displacement.dot(direction).abs() < 0.000_01);
            assert!((displacement.length() - 5.0).abs() < 0.000_01);
        }
    }

    #[test]
    fn walls_block_enemies_at_or_beyond_impact() {
        assert!(enemy_before_wall(Some(9.0), 10.0));
        assert!(!enemy_before_wall(Some(10.0), 10.0));
        assert!(!enemy_before_wall(Some(11.0), 10.0));
        assert!(!enemy_before_wall(None, f32::INFINITY));
    }

    #[test]
    fn lane_reach_stops_at_visible_range_or_arrow_flight() {
        let normal_tick = lane_reach(None, 1.0 / 96.0);
        let short_flight = lane_reach(None, 0.000_5);

        assert_eq!(normal_tick, visible_reach(None));
        assert!((short_flight - 48.0).abs() < 0.000_01);
    }
}
