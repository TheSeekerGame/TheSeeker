use bevy::{prelude::*, reflect::TypePath, asset::Asset, utils::HashMap};
use serde::Deserialize;
use super::{
    EnemyGentBundle, EnemyGfxBundle, EnemyEffectsGfxBundle, Enemy, EnemyGfx,
    EnemyEffectGfx,
};
use crate::game::enemy::components::{
    Movement, Patrol, RangedAttack, TargetSensor, GroundSensor, RangeSensor,
};
use crate::game::gentstate::{Patrolling, AddQueue, TransitionQueue};
use crate::game::gentstate::Idle; // although initial not used maybe
use crate::game::gentstate::GentState;
use crate::game::gentstate::GenericState;
use crate::game::gentstate::Transitionable;
use crate::prelude::*;
use theseeker_engine::gent::TransformGfxFromGent;
use theseeker_engine::physics::{Collider, ShapeCaster, LinearVelocity, ENEMY, GROUND};
use rapier2d::prelude::{InteractionGroups, Group};
use rapier2d::geometry::SharedShape;
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::{Gent, GentPhysicsBundle};
use crate::game::attack::Health;
use crate::StateDespawnMarker;

/// Placeholder brain component – for Phase 2 we just need a marker so that
/// future systems can find a `Brain` on the entity. The real implementation
/// (transition table, triggers, etc.) will come in Phase 4.
#[derive(Component, Debug, Default, Clone)]
pub struct Brain;

/// System that consumes `EnemyArchetypeHandle` and builds out the full enemy
/// entity based on the archetype data.
///
/// Runs both on `Startup` (to catch entities placed in the scene) and whenever
/// an entity with `EnemyArchetypeHandle` is spawned during play.
pub fn apply_enemy_archetype(
    mut commands: Commands,
    assets: Res<Assets<EnemyArchetypeAsset>>,
    query: Query<(Entity, &EnemyArchetypeHandle, &Transform), (With<EnemyArchetypeHandle>, Without<Enemy>)>,
) {
    for (entity, handle, transform) in &query {
        // If the asset isn't ready yet, skip – we'll catch it on the next
        // frame when Bevy marks the component as `Changed` (still Added=false)
        // but for our purposes Added is enough; in worst-case the enemy will
        // appear a tick later once the asset is loaded.
        let Some(archetype) = assets.get(handle.id()) else { continue };

        // Child gfx entities first, so we can store their IDs in `Gent`.
        let e_gfx = commands.spawn_empty().id();
        let e_effects_gfx = commands.spawn_empty().id();

        // -----------------------------------------------------------------
        // Legacy compatibility: spiders were historically spawned 2 px higher
        // so their feet sit flush on the ground.  Replicate that offset here
        // to avoid them looking "buried" in tiles.  This matches the logic
        // in the old `setup_enemy` path (`xf_gent.translation.y += 2.0`).
        // -----------------------------------------------------------------
        let mut base_transform = *transform;
        base_transform.translation.y += 2.0;

        commands.entity(entity)
            .insert(Name::new("Enemy"))
            .insert(EnemyGentBundle {
                enemy: Enemy,
                marker: Gent {
                    e_gfx,
                    e_effects_gfx,
                },
                phys: GentPhysicsBundle {
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
                        direction: crate::prelude::Dir2::NEG_Y,
                        origin: Vec2::new(0.0, -2.0),
                        max_toi: 0.0,
                        interaction: InteractionGroups {
                            memberships: ENEMY,
                            filter: GROUND,
                        },
                    },
                    linear_velocity: LinearVelocity(Vec2::ZERO),
                },
            })
            .insert((
                // Behaviour / data components
                Movement { walk_speed: archetype.movement_speed },
                Patrol {
                    idle: archetype.idle_time_ticks,
                    walk_min: archetype.walk_time_min,
                    walk_max: archetype.walk_time_max,
                },
                RangedAttack {
                    damage: archetype.projectile_damage,
                    range: archetype.ranged_range,
                },
                TargetSensor { vision: archetype.vision_range, ..Default::default() },
                GroundSensor::default(),
                RangeSensor::default(),
                Health { current: archetype.health, max: archetype.health },
                crate::game::gentstate::Facing::Right,
                Patrolling,
                Idle,
                Brain::default(),
                AddQueue::default(),
                TransitionQueue::default(),
                StateDespawnMarker,
            ));

        // Ensure transform from spawner is preserved (with +2 px Y offset).
        commands
            .entity(entity)
            .insert(TransformBundle::from(base_transform));

        // --- graphics entity ---
        let mut gfx_player = ScriptPlayer::<SpriteAnimation>::default();
        if let Some(key) = archetype.anim.get("Idle") {
            gfx_player.play_key(key);
        }

        commands.entity(e_gfx).insert((
            EnemyGfxBundle {
                marker: EnemyGfx { e_gent: entity },
                gent2gfx: TransformGfxFromGent {
                    pixel_aligned: false,
                    gent: entity,
                },
                sprite: SpriteBundle {
                    sprite: Sprite {
                        texture_atlas: Some(TextureAtlas::default()),
                        ..Default::default()
                    },
                    transform: base_transform,
                    ..Default::default()
                },
                animation: theseeker_engine::animation::SpriteAnimationBundle {
                    player: gfx_player,
                },
            },
            StateDespawnMarker,
        ));

        // --- effect gfx entity ---
        let mut anim_player = ScriptPlayer::<SpriteAnimation>::default();
        anim_player.play_key("anim.spider.Sparks");
        commands.entity(e_effects_gfx).insert((
            EnemyEffectsGfxBundle {
                marker: EnemyEffectGfx { e_gent: entity },
                gent2gfx: TransformGfxFromGent {
                    pixel_aligned: false,
                    gent: entity,
                },
                sprite: SpriteBundle {
                    sprite: Sprite {
                        texture_atlas: Some(TextureAtlas::default()),
                        ..Default::default()
                    },
                    transform: base_transform.with_translation(Vec3::new(0., 0., 1.)),
                    ..Default::default()
                },
                animation: theseeker_engine::animation::SpriteAnimationBundle {
                    player: anim_player,
                },
            },
            StateDespawnMarker,
        ));

        // Clean up remnants of the old pipeline if present.
        commands.entity(entity).remove::<super::EnemyBlueprint>();
    }
}

#[derive(Asset, Deserialize, TypePath, Debug, Clone)]
pub struct EnemyArchetypeAsset {
    pub health:            u32,
    pub movement_speed:    f32,
    pub vision_range:      f32,
    pub ranged_range:      f32,
    pub projectile_damage: f32,
    pub idle_time_ticks:   u32,
    pub walk_time_min:     u32,
    pub walk_time_max:     u32,
    pub state_init: String,
    pub anim: HashMap<String, String>,
}

#[derive(Component, Debug, Deref, DerefMut, Clone)]
#[deref(forward)]
pub struct EnemyArchetypeHandle(pub Handle<EnemyArchetypeAsset>);

impl EnemyArchetypeHandle {
    /// Convenience helper that resolves an archetype name to its dedicated
    /// TOML file under `assets/enemies/archetypes/`.
    pub fn key(key: &str, asset_server: &AssetServer) -> Self {
        let path = format!("enemies/archetypes/{}.arch.toml", key);
        EnemyArchetypeHandle(asset_server.load(path))
    }
}
