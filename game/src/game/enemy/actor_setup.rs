//! Enemy assembly: a blueprint names its catalog actor, and setup builds the body, its runtime, and
//! its graphics from the resolved definition.
use std::sync::Arc;

use super::*;
use super::animation_lifecycle::FacingSlots;
use super::death::DeathPolicy;
use super::flying_motor::{
    FlyingMotor, FlyingMotorConfig, HealthRegen, OrientToVelocity, VOID_FACE_Z_OFFSET, VOID_GHOST_INTERVAL_TICKS,
    VOID_GHOST_LIFETIME_TICKS, VOID_GHOST_MIN_SCALE,
};
use super::jump_motor::JumpMotor;
use super::trap_controller::{StationaryTrapController, TrapPrey};
use crate::game::actors::catalog::{ActorCatalog, ResolvedActor};
use crate::game::actors::definition::{EnemyArchetype, EnemyFacingSetup, EnemyGfxKind, EnemySetupController};
use crate::game::actors::sensors::CachedArchetypeStats;
use crate::game::actors::{ActorBundle, LevelSeed};

/// An enemy waiting for setup, named by its catalog actor.
#[derive(Component)]
pub struct EnemyActorBlueprint {
    arch_id: String,
    spider: Option<SpiderActorSetup>,
}

/// A spider's spawn-time traits.
#[derive(Clone, Copy)]
pub(super) struct SpiderActorSetup {
    pub role: Role,
    pub tier: Tier,
    pub biome: SpiderBiome,
    pub small: bool,
    pub large_freeze: bool,
    pub spawn_z: f32,
}

pub fn spawn_enemy_actor_blueprint(
    commands: &mut Commands,
    arch_id: impl Into<String>,
    transform: Transform,
) -> Entity {
    spawn_blueprint(commands, EnemyActorBlueprint { arch_id: arch_id.into(), spider: None }, transform)
}

pub(super) fn spawn_spider_actor_blueprint(
    commands: &mut Commands,
    arch_id: impl Into<String>,
    transform: Transform,
    spider: SpiderActorSetup,
) -> Entity {
    spawn_blueprint(commands, EnemyActorBlueprint { arch_id: arch_id.into(), spider: Some(spider) }, transform)
}

fn spawn_blueprint(commands: &mut Commands, blueprint: EnemyActorBlueprint, transform: Transform) -> Entity {
    commands.spawn((blueprint, transform, GlobalTransform::default(), StateDespawnMarker)).id()
}

/// Builds each new blueprint from its catalog actor.
pub(super) fn setup_enemy_actor(
    mut blueprints: Query<(Entity, &mut Transform, &EnemyActorBlueprint), Added<EnemyActorBlueprint>>,
    mut commands: Commands,
    catalog: Res<ActorCatalog>,
    level_seed: Res<LevelSeed>,
) {
    for (entity, mut transform, blueprint) in &mut blueprints {
        let Some(actor) = catalog.actors.get(&blueprint.arch_id) else {
            error!("Failed to resolve actor definition for archetype: {}", blueprint.arch_id);
            commands.entity(entity).despawn();
            continue;
        };
        let mut offset = actor.body.spawn_offset;
        if let Some(spider) = blueprint.spider {
            offset.z = spider.spawn_z;
        }
        transform.translation += offset;
        assemble_enemy(&mut commands, entity, *transform, blueprint, actor, level_seed.0);
        commands.entity(entity).remove::<EnemyActorBlueprint>();
    }
}

fn assemble_enemy(
    commands: &mut Commands,
    entity: Entity,
    transform: Transform,
    blueprint: &EnemyActorBlueprint,
    actor: &Arc<ResolvedActor>,
    level_seed: u32,
) {
    let graphics = (commands.spawn(()).id(), commands.spawn(()).id());
    let (body, stats, archetype) = (&actor.body, actor.stats(), &actor.archetype);
    let collider = Collider::cuboid(body.collider_half_extents.x, body.collider_half_extents.y);
    commands.entity(entity).insert((
        Name::new(body.name.clone()),
        Enemy,
        Gent { e_gfx: graphics.0, e_effects_gfx: graphics.1 },
        FoliageInteractor::body(&collider),
        GentPhysicsBundle { collider, linear_velocity: LinearVelocity(Vec2::ZERO) },
        theseeker_engine::physics::groups::enemy_body(),
        Health { current: stats.spawn_hp, max: stats.spawn_hp },
        spawn_facing(body.facing),
        StateDespawnMarker,
        DeathPolicy::from_setup(archetype.death.as_ref()),
    ));
    if let Some((right, left)) = &body.facing_slots {
        commands.entity(entity).insert(FacingSlots::new(right.clone(), left.clone()));
    }
    insert_body_runtime(commands, entity, actor, transform.translation.xy());
    let spider = blueprint.spider.map(|spider| (spider, SpiderJitter::new(entity, level_seed, spider.small)));
    let jitter = spider.as_ref().map(|(_, jitter)| jitter);
    commands.entity(entity).insert((ActorBundle::new(actor.clone(), entity, level_seed), cached_stats(actor, jitter)));
    if stats.dmg_ranged > 0 {
        commands.entity(entity).insert(ProjectileCache::new());
    }
    if let Some(from_frame) = archetype.defense.and_then(|defense| defense.invulnerable_from_frame) {
        let anim_key = archetype.anim.get("Defense").cloned();
        commands.entity(entity).insert(DefenseInvulnerabilityTiming { from_frame, anim_key });
    }
    if let Some((spider, jitter)) = &spider {
        insert_spider_traits(commands, entity, spider, jitter, stats.melee_range);
    }
    insert_graphics(commands, entity, graphics, transform, actor);
}

fn spawn_facing(setup: EnemyFacingSetup) -> Facing {
    match setup {
        EnemyFacingSetup::Right => Facing::Right,
        EnemyFacingSetup::Left => Facing::Left,
        EnemyFacingSetup::RandomHorizontal => {
            if rand::rng().random_bool(0.5) { Facing::Right } else { Facing::Left }
        },
    }
}

/// The body's motor and its standing effects.
fn insert_body_runtime(commands: &mut Commands, entity: Entity, actor: &ResolvedActor, home: Vec2) {
    let (archetype, body) = (&actor.archetype, &actor.body);
    let mut entity = commands.entity(entity);
    if body.trap_prey {
        entity.insert(TrapPrey);
    }
    match body.controller {
        EnemySetupController::Flying => {
            entity.insert(FlyingMotor::new(home, FlyingMotorConfig::from_setup(archetype.flying, actor.stats().walk_speed)))
        },
        EnemySetupController::Ground => entity.insert((Navigation::Grounded, GroundMotor, MovementState::default())),
        EnemySetupController::Jump => {
            entity.insert((Navigation::Grounded, JumpMotor::new(archetype.jump), MovementState::default()))
        },
        EnemySetupController::StationaryTrap => entity.insert((
            DamageWindow { open: false },
            StationaryTrapController::new(body.collider_half_extents, archetype.trap),
        )),
    };
    if let Some(regen) = archetype.health_regen {
        entity.insert(HealthRegen::new(regen.amount, regen.interval_ticks));
    }
    if let Some(contact) = archetype.contact_damage {
        let pushback = contact.pushback.map(|pushback| ContactPushback {
            horizontal: pushback.horizontal,
            vertical: pushback.vertical,
            ticks: pushback.ticks,
        });
        entity.insert(ContactDamage::new(contact.damage, contact.cooldown_ticks, pushback));
    }
}

/// Perception stats after a spider's jitter, raised to the body's floors.
fn cached_stats(actor: &ResolvedActor, jitter: Option<&SpiderJitter>) -> CachedArchetypeStats {
    let (stats, body) = (actor.stats(), &actor.body);
    let vision = jitter.map_or(stats.vision_range, |jitter| jitter.vision_range(stats.vision_range));
    let melee = jitter.map_or(stats.melee_range, |jitter| jitter.melee_range(stats.melee_range));
    CachedArchetypeStats {
        vision_range: vision.max(body.min_vision_range),
        melee_range: melee.max(body.min_melee_range),
        walk_speed: stats.walk_speed,
        chase_speed: stats.chase_speed,
        needs_line_of_sight: body.needs_line_of_sight,
        reaction_time: jitter.map_or(stats.reaction_time, |jitter| jitter.reaction_time(stats.reaction_time)),
    }
}

fn insert_spider_traits(
    commands: &mut Commands,
    entity: Entity,
    spider: &SpiderActorSetup,
    jitter: &SpiderJitter,
    melee_range: f32,
) {
    let mut entity = commands.entity(entity);
    entity.insert((
        spider.role,
        spider.tier,
        spider.biome,
        DistanceJitter { melee: jitter.melee_scale(melee_range), vision: jitter.vision_scale },
        PatrolTimeScale(jitter.patrol_scale),
    ));
    if spider.large_freeze {
        entity.insert(LargeFreezeTarget);
    }
}

/// The body sprite and the effects sprite. A split Void shows its face on the effects sprite, in
/// front of its ghosting body.
fn insert_graphics(
    commands: &mut Commands,
    entity: Entity,
    (gfx, effects_gfx): (Entity, Entity),
    transform: Transform,
    actor: &ResolvedActor,
) {
    let (archetype, body) = (&actor.archetype, &actor.body);
    let clip = first_clip(archetype, &body.animation_keys);
    commands.entity(effects_gfx).insert(StateDespawnMarker);
    match body.gfx {
        EnemyGfxKind::PixelPerfect => {
            let sprite = enemy_sprite(entity, transform, body.gfx_offset, body.pixel_aligned, clip);
            commands.entity(gfx).insert((sprite, pixel_perfect_layers()));
        },
        EnemyGfxKind::World => {
            let sprite = enemy_sprite(entity, transform, body.gfx_offset, body.pixel_aligned, clip);
            commands.entity(gfx).insert((sprite, RenderLayers::layer(0)));
        },
        EnemyGfxKind::VoidSplit => {
            let ghosting = GhostingSource {
                spawn_interval_ticks: VOID_GHOST_INTERVAL_TICKS,
                ghost_lifetime_ticks: VOID_GHOST_LIFETIME_TICKS,
                initial_alpha: 1.0,
                fade_curve: FadeCurve::Constant,
                color_mode: GhostColorMode::Original,
                scale_over_time: ScaleCurve::Axes { x: VOID_GHOST_MIN_SCALE, y: VOID_GHOST_MIN_SCALE },
                movement: GhostMovement::Static,
                render_layers: Some(pixel_perfect_layers()),
                ..default()
            };
            let face = Vec3::new(0.0, 0.0, VOID_FACE_Z_OFFSET);
            let face_transform = Transform::from_translation(transform.translation + face);
            let face_sprite = enemy_sprite(entity, face_transform, face, true, first_clip(archetype, &["Face"]));
            commands.entity(entity).insert(OrientToVelocity);
            commands.entity(gfx).insert((enemy_sprite(entity, transform, Vec3::ZERO, true, clip), ghosting, pixel_perfect_layers()));
            commands.entity(effects_gfx).insert((face_sprite, OrientToVelocity, pixel_perfect_layers()));
        },
    }
}

/// A sprite that follows the enemy's body.
fn enemy_sprite(
    gent: Entity,
    transform: Transform,
    offset: Vec3,
    pixel_aligned: bool,
    player: ScriptPlayer<SpriteAnimation>,
) -> impl Bundle {
    (
        EnemyGfx,
        TransformGfxFromGent { pixel_aligned, gent, offset: Some(offset) },
        Sprite { texture_atlas: Some(TextureAtlas::default()), ..default() },
        transform,
        GlobalTransform::default(),
        Visibility::Visible,
        InheritedVisibility::VISIBLE,
        ViewVisibility::default(),
        SpriteAnimationBundle { player },
        StateDespawnMarker,
    )
}

/// A playback of the first of `slots` the archetype binds.
fn first_clip(archetype: &EnemyArchetype, slots: &[impl AsRef<str>]) -> ScriptPlayer<SpriteAnimation> {
    let mut player = ScriptPlayer::default();
    if let Some(key) = slots.iter().find_map(|slot| archetype.anim.get(slot.as_ref())) {
        player.play_key(key);
    }
    player
}

/// Per-spider variation of patrol pace, vision, reaction, and (for small spiders) melee reach,
/// seeded by the entity and the level.
struct SpiderJitter {
    patrol_scale: f32,
    vision_scale: f32,
    reaction_scale: f32,
    melee_offset: f32,
}

impl SpiderJitter {
    fn new(entity: Entity, level_seed: u32, small: bool) -> Self {
        let bits = entity.to_bits();
        let mut state = ((bits ^ (bits >> 32)) as u32) ^ level_seed ^ 0x9E37_79B9;
        let mut next = move || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            state
        };
        // A magnitude from `least` to `least + spread`, with a random sign.
        let mut signed = |least: f32, spread: f32| {
            let magnitude = least + next() as f32 / u32::MAX as f32 * spread;
            let sign = (next() & 1) as f32 * 2.0 - 1.0;
            sign * magnitude
        };
        Self {
            patrol_scale: 1.0 + signed(0.05, 0.05),
            vision_scale: 1.0 + signed(0.05, 0.05),
            reaction_scale: 1.0 + signed(0.05, 0.05),
            melee_offset: if small { signed(0.0, 2.0) } else { 0.0 },
        }
    }

    fn melee_range(&self, base: f32) -> f32 {
        if base <= 0.0 { base } else { (base + self.melee_offset).max(1.0) }
    }

    fn melee_scale(&self, base: f32) -> f32 {
        if base <= 0.0 { 1.0 } else { self.melee_range(base) / base }
    }

    fn vision_range(&self, base: f32) -> f32 {
        if base <= 0.0 { base } else { base * self.vision_scale }
    }

    fn reaction_time(&self, base: u8) -> u8 {
        if base == 0 { 0 } else { ((base as f32 * self.reaction_scale).round() as i32).clamp(1, 32) as u8 }
    }
}
