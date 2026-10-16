use crate::game::combat::DamageSource;
use crate::game::enemy::{Enemy, EnemyStateSet};
use crate::game::gentstate::{Dead, Facing};
use theseeker_engine::physics::{
    Collider, CollisionGroups, Group, PhysicsSet, PhysicsWorld, ShapeCast, TileCollisionWorld, ENEMY,
    ENEMY_ATTACK, PLAYER, PLAYER_ATTACK,
};
use crate::game::player::Player;
use crate::graphics::pixel_perfect::pixel_perfect_layers;
use crate::levelgen::foliage::FoliageInteractor;
use crate::prelude::*;
use theseeker_engine::animation::PlaybackState;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::assets::projectile::{
    ProjectileDescriptor, ProjectileDescriptorKind,
};
use theseeker_engine::script::ScriptPlayer;

#[derive(Component)]
struct DirectSpriteProjectile {
    velocity_px_per_second: Vec2,
}

#[derive(Component)]
struct SpritePopProjectile {
    owner: Entity,
    damage: f32,
    pop_anim_key: String,
    pop_clip: std::sync::Arc<SpriteAnimation>,
    trigger_radius: f32,
    ignore_owner: bool,
    damage_ignore_arches: Vec<String>,
    age_ticks: u32,
    lifetime_ticks: u32,
}

#[derive(Component)]
struct PoppedSpriteProjectile;

pub(super) struct EnemySpriteProjectileSpawn<'a> {
    pub(super) clips: &'a crate::game::actors::action::ProjectileClips,
    pub(super) owner: Entity,
    pub(super) start: Vec2,
    pub(super) target: Option<Vec2>,
    pub(super) facing: Facing,
    pub(super) descriptor: &'a ProjectileDescriptor,
    pub(super) damage: f32,
}

pub(super) fn can_spawn_sprite_pop_projectile(
    descriptor: &ProjectileDescriptor,
) -> bool {
    descriptor.kind == ProjectileDescriptorKind::DirectSpritePop
        && descriptor.direct.is_some()
        && descriptor.sprite.is_some()
        && descriptor.pop.is_some()
}

pub(super) fn spawn_enemy_sprite_pop_projectile(
    commands: &mut Commands,
    spec: EnemySpriteProjectileSpawn<'_>,
) {
    let Some(direct) = spec.descriptor.direct else {
        return;
    };
    let Some(sprite) = spec.descriptor.sprite.as_ref() else {
        return;
    };
    let Some(pop) = spec.descriptor.pop.as_ref() else {
        return;
    };

    let direction = direct_projectile_direction(
        spec.start,
        spec.target,
        spec.facing,
        direct.horizontal,
    );
    if direction.length_squared() <= f32::EPSILON {
        return;
    }

    let mut player = ScriptPlayer::<SpriteAnimation>::default();
    player.play_snapshot(&sprite.idle_anim_key, spec.clips.idle.clone(), None);

    commands.spawn((
        SpritePopProjectile {
            owner: spec.owner,
            damage: spec.damage,
            pop_anim_key: sprite.pop_anim_key.clone(),
            pop_clip: spec.clips.pop.clone(),
            trigger_radius: pop.trigger_radius,
            ignore_owner: pop.ignore_owner,
            damage_ignore_arches: pop.damage_ignore_arches.clone(),
            age_ticks: 0,
            lifetime_ticks: spec.descriptor.lifetime_ticks,
        },
        DirectSpriteProjectile {
            velocity_px_per_second: direction * direct.speed,
        },
        FoliageInteractor::projectile(),
        Collider::cuboid(
            spec.descriptor.collider.half_width,
            spec.descriptor.collider.half_height,
        ),
        CollisionGroups::new(Group::NONE, Group::NONE),
        Transform::from_translation(spec.start.extend(sprite.z.unwrap_or(1.0))),
        GlobalTransform::default(),
        Visibility::Visible,
        InheritedVisibility::VISIBLE,
        ViewVisibility::default(),
        Sprite {
            texture_atlas: Some(TextureAtlas::default()),
            ..Default::default()
        },
        player,
        pixel_perfect_layers(),
        StateDespawnMarker,
    ));
}

fn direct_projectile_direction(
    start: Vec2,
    target: Option<Vec2>,
    facing: Facing,
    horizontal: bool,
) -> Vec2 {
    let fallback_x = facing.direction();
    let delta = target.unwrap_or(start + Vec2::new(fallback_x, 0.0)) - start;
    if horizontal {
        let x = if delta.x.abs() > f32::EPSILON {
            delta.x.signum()
        } else {
            fallback_x
        };
        return Vec2::new(x, 0.0);
    }
    delta.normalize_or_zero()
}

fn sprite_pop_projectile_motion(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &mut Transform,
            &Collider,
            &DirectSpriteProjectile,
            &mut SpritePopProjectile,
            &mut ScriptPlayer<SpriteAnimation>,
        ),
        Without<PoppedSpriteProjectile>,
    >,
    target_query: Query<(), (Or<(With<Player>, With<Enemy>)>, Without<Dead>)>,
    actors: Query<&crate::game::actors::ActorDefinition>,
    physics: PhysicsWorld,
    collision_world: Option<Res<TileCollisionWorld>>,
    time: Res<GameTime>,
) {
    let collision_world =
        collision_world.as_deref().filter(|world| world.is_ready());

    for (
        entity,
        mut transform,
        collider,
        direct,
        mut projectile,
        mut anim,
    ) in query.iter_mut()
    {
        if projectile.age_ticks >= projectile.lifetime_ticks {
            pop_sprite_projectile(
                &mut commands,
                entity,
                None,
                &projectile,
                &mut anim,
                &actors,
            );
            continue;
        }
        projectile.age_ticks = projectile.age_ticks.saturating_add(1);

        let delta = direct.velocity_px_per_second / time.hz as f32;
        if delta.length_squared() <= f32::EPSILON {
            continue;
        }

        let origin = transform.translation.xy();
        let direction = delta.normalize();
        let distance = delta.length();
        let hit = first_sprite_projectile_hit(
            entity,
            projectile.owner,
            origin,
            direction,
            distance,
            collider,
            projectile.trigger_radius,
            projectile.ignore_owner,
            &physics,
            collision_world,
            &target_query,
        );

        if let Some(hit) = hit {
            transform.translation =
                (origin + direction * hit.distance.min(distance))
                    .extend(transform.translation.z);
            pop_sprite_projectile(
                &mut commands,
                entity,
                hit.target,
                &projectile,
                &mut anim,
                &actors,
            );
            continue;
        }

        transform.translation = (origin + delta).extend(transform.translation.z);
    }
}

struct SpriteProjectileHit {
    distance: f32,
    target: Option<Entity>,
}

#[expect(
    clippy::too_many_arguments,
    reason = "Projectile hit tests need owner filtering, collision shape, sweep geometry, physics worlds, and live target filters together."
)]
fn first_sprite_projectile_hit(
    projectile_entity: Entity,
    owner: Entity,
    origin: Vec2,
    direction: Vec2,
    distance: f32,
    collider: &Collider,
    trigger_radius: f32,
    ignore_owner: bool,
    physics: &PhysicsWorld<'_>,
    collision_world: Option<&TileCollisionWorld>,
    target_query: &Query<(), (Or<(With<Player>, With<Enemy>)>, Without<Dead>)>,
) -> Option<SpriteProjectileHit> {
    let half_extents = collider.half_extents();
    let inflated = Collider::cuboid(
        half_extents.x + trigger_radius,
        half_extents.y + trigger_radius,
    );
    let interaction = CollisionGroups::new(
        PLAYER_ATTACK | ENEMY_ATTACK,
        PLAYER | ENEMY,
    );

    let entity_hit = physics.cast_shape(
        ShapeCast {
            origin,
            dir: direction,
            shape: &inflated,
            max_toi: distance,
            interaction,
            exclude: Some(projectile_entity),
        },
        |entity| {
            (!ignore_owner || entity != owner)
                && target_query.get(entity).is_ok()
        },
    );

    let wall_hit = collision_world.and_then(|world| {
        let forward_extent = if direction.x.abs() >= direction.y.abs() {
            half_extents.x
        } else {
            half_extents.y
        };
        world
            .ray_cast_solid(
                origin,
                direction,
                distance + forward_extent + trigger_radius,
            )
            .map(|hit| {
                (hit.time_of_impact - forward_extent - trigger_radius)
                    .max(0.0)
            })
    });

    let entity_distance = entity_hit
        .as_ref()
        .map(|(_, hit)| hit.time_of_impact)
        .unwrap_or(f32::MAX);
    let wall_distance = wall_hit.unwrap_or(f32::MAX);

    if entity_distance == f32::MAX && wall_distance == f32::MAX {
        return None;
    }

    if entity_distance <= wall_distance {
        return entity_hit.map(|(target, hit)| SpriteProjectileHit {
            distance: hit.time_of_impact,
            target: Some(target),
        });
    }

    Some(SpriteProjectileHit {
        distance: wall_distance,
        target: None,
    })
}

fn pop_sprite_projectile(
    commands: &mut Commands,
    entity: Entity,
    target: Option<Entity>,
    projectile: &SpritePopProjectile,
    anim: &mut ScriptPlayer<SpriteAnimation>,
    actors: &Query<&crate::game::actors::ActorDefinition>,
) {
    anim.play_snapshot(&projectile.pop_anim_key, projectile.pop_clip.clone(), None);

    let mut damage_source =
        DamageSource::new(u32::MAX, projectile.owner, projectile.damage)
            .with_max_targets(1);
    // Some projectiles pass through actors of named archetypes.
    let ignored = |target| {
        actors.get(target).is_ok_and(|actor| projectile.damage_ignore_arches.contains(&actor.archetype.id))
    };
    if let Some(target) = target.filter(|&target| !ignored(target)) {
        damage_source.target_set.insert(target);
    }

    commands
        .entity(entity)
        .remove::<DirectSpriteProjectile>()
        .insert((PoppedSpriteProjectile, damage_source));
}

fn despawn_finished_sprite_pop_projectiles(
    mut commands: Commands,
    query: Query<
        (Entity, &PlaybackState),
        With<PoppedSpriteProjectile>,
    >,
) {
    for (entity, signals) in query.iter() {
        if signals.finished {
            commands.entity(entity).try_despawn();
        }
    }
}

pub(super) struct EnemyProjectileRuntimePlugin;

impl Plugin for EnemyProjectileRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            sprite_pop_projectile_motion
                .after(PhysicsSet)
                .before(crate::game::combat::damage_source::determine_damage_targets)
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            GameTickUpdate,
            despawn_finished_sprite_pop_projectiles
                .in_set(EnemyStateSet::Animation)
                .run_if(in_state(AppState::InGame)),
        );
    }
}
