use super::DamageInfo;
use crate::game::physics::projectile::Projectile;
use crate::graphics::NoDamageNumbers;
use crate::levelgen::breakable_wall::BreakableWall;
use crate::prelude::*;
use std::collections::HashMap;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::{
    Gent, TransformGfxFromGent,
};
use theseeker_engine::physics::LinearVelocity;
use theseeker_engine::presentation::PresentationAlignment;
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::GameTickUpdate;

const DIRECTION_EPSILON_SQUARED: f32 = 0.0001;
const SPARK_DIRECTION_COUNT: i32 = 16;
const SPARK_DIRECTION_SLOTS: [&str; SPARK_DIRECTION_COUNT as usize] = [
    "SparkDirection0",
    "SparkDirection1",
    "SparkDirection2",
    "SparkDirection3",
    "SparkDirection4",
    "SparkDirection5",
    "SparkDirection6",
    "SparkDirection7",
    "SparkDirection8",
    "SparkDirection9",
    "SparkDirection10",
    "SparkDirection11",
    "SparkDirection12",
    "SparkDirection13",
    "SparkDirection14",
    "SparkDirection15",
];
const SWORD_SPARK_ANIMATIONS: &[&str] = &["anim.player.SwordHitSpark"];
const HAMMER_SPARK_ANIMATIONS: &[&str] = &["anim.player.HammerHitSpark"];
const PROJECTILE_SPARK_ANIMATIONS: &[&str] =
    &["anim.player.ProjectileHitSpark"];
const EXPLODING_SPARK_ANIMATIONS: &[&str] =
    &["anim.player.ExplodingSparks"];

/// Selects the impact visual used by a damage source.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SparkSource {
    Sword,
    Hammer,
    Projectile,
    Exploding,
}

/// Optional component specifying the weapon audio slot for an impact.
#[derive(Component, Debug, Clone)]
pub struct WeaponHitSlot {
    pub slot_name: String,
}

/// Slot setup deferred until the animation runtime is initialized.
#[derive(Component, Debug, Clone)]
struct SparkSlotConfig {
    weapon_slot: Option<String>,
    direction_slot: &'static str,
}

#[derive(Component)]
struct SparkVisual;

/// Configuration for a spark type.
#[derive(Debug, Clone)]
pub struct SparkSpec {
    /// Animation variants eligible for this spark type.
    pub animation_keys: &'static [&'static str],
    /// Duration in game ticks (96 Hz).
    pub lifetime_ticks: u32,
    /// Position offset relative to the target.
    pub offset: Vec3,
    /// If true, prevents overlapping sparks of the same type on one enemy.
    pub prevent_overlap: bool,
}

impl SparkSpec {
    fn animation_key(&self) -> &'static str {
        self.animation_keys[0]
    }
}

fn get_spark_spec(spark_type: SparkSource) -> SparkSpec {
    match spark_type {
        SparkSource::Sword => SparkSpec {
            animation_keys: SWORD_SPARK_ANIMATIONS,
            lifetime_ticks: 12,
            offset: Vec3::ZERO,
            prevent_overlap: false,
        },
        SparkSource::Hammer => SparkSpec {
            animation_keys: HAMMER_SPARK_ANIMATIONS,
            lifetime_ticks: 12,
            offset: Vec3::ZERO,
            prevent_overlap: false,
        },
        SparkSource::Projectile => SparkSpec {
            animation_keys: PROJECTILE_SPARK_ANIMATIONS,
            lifetime_ticks: 12,
            offset: Vec3::ZERO,
            prevent_overlap: false,
        },
        SparkSource::Exploding => SparkSpec {
            animation_keys: EXPLODING_SPARK_ANIMATIONS,
            lifetime_ticks: 24,
            offset: Vec3::new(0.0, 0.0, 0.01),
            prevent_overlap: true,
        },
    }
}

fn classify_spark_source(
    explicit: Option<SparkSource>,
    weapon_slot: Option<&str>,
) -> SparkSource {
    explicit.unwrap_or(match weapon_slot {
        Some("SwordHit") => SparkSource::Sword,
        Some("HammerHit") => SparkSource::Hammer,
        _ => SparkSource::Projectile,
    })
}

fn usable_direction(direction: Vec2) -> Option<Vec2> {
    (direction.is_finite()
        && direction.length_squared() > DIRECTION_EPSILON_SQUARED)
        .then(|| direction.normalize())
}

fn resolve_impact_direction(
    projectile_velocity: Option<Vec2>,
    linear_velocity: Option<Vec2>,
    source_position: Option<Vec2>,
    owner_position: Option<Vec2>,
    target_position: Vec2,
) -> Vec2 {
    projectile_velocity
        .and_then(usable_direction)
        .or_else(|| linear_velocity.and_then(usable_direction))
        .or_else(|| {
            source_position
                .and_then(|source| usable_direction(target_position - source))
        })
        .or_else(|| {
            owner_position
                .and_then(|owner| usable_direction(target_position - owner))
        })
        .unwrap_or(Vec2::X)
}

fn impact_angle(direction: Vec2) -> f32 {
    direction.y.atan2(direction.x)
}

fn impact_direction_index(direction: Vec2) -> u8 {
    let scaled = impact_angle(direction) * SPARK_DIRECTION_COUNT as f32
        / std::f32::consts::TAU;
    (scaled.round() as i32).rem_euclid(SPARK_DIRECTION_COUNT) as u8
}


#[derive(Default)]
struct SparkHitSource {
    explicit_spark: Option<SparkSource>,
    weapon_slot: Option<String>,
    position: Option<Vec2>,
    projectile_velocity: Option<Vec2>,
    linear_velocity: Option<Vec2>,
}

fn read_spark_hit_source(
    source: Entity,
    source_query: &Query<(
        Option<&SparkSource>,
        Option<&WeaponHitSlot>,
        Option<&GlobalTransform>,
        Option<&Projectile>,
        Option<&LinearVelocity>,
    )>,
) -> SparkHitSource {
    source_query
        .get(source)
        .map(
            |(
                spark_source,
                weapon_slot,
                transform,
                projectile,
                velocity,
            )| SparkHitSource {
                explicit_spark: spark_source.copied(),
                weapon_slot: weapon_slot.map(|slot| slot.slot_name.clone()),
                position: transform.map(|tf| tf.translation().truncate()),
                projectile_velocity: projectile.map(|value| value.vel.0),
                linear_velocity: velocity.map(|value| value.0),
            },
        )
        .unwrap_or_default()
}

struct SparkSpawn {
    target_entity: Entity,
    target_translation: Vec3,
    spark_type: SparkSource,
    direction_index: u8,
    z_offset: f32,
    weapon_slot: Option<String>,
}

fn spawn_spark_visual(
    commands: &mut Commands,
    spec: &SparkSpec,
    spawn: SparkSpawn,
) {
    let offset = spec.offset + Vec3::new(0.0, 0.0, spawn.z_offset);
    let follower = commands
        .spawn((
            DamageSpark {
                target_entity: spawn.target_entity,
                spark_type: spawn.spark_type,
                lifetime_ticks: spec.lifetime_ticks,
            },
            TransformGfxFromGent {
                pixel_aligned: false,
                gent: spawn.target_entity,
                offset: Some(offset),
            },
            Transform::from_translation(spawn.target_translation + offset),
            GlobalTransform::default(),
            Visibility::Inherited,
            InheritedVisibility::default(),
            StateDespawnMarker,
        ))
        .id();

    let mut animation = ScriptPlayer::<SpriteAnimation>::default();
    animation.play_key(spec.animation_key());
    let needs_direction = spawn.spark_type != SparkSource::Exploding;
    let visual = commands
        .spawn((
            SparkVisual,
            PresentationAlignment::UprightCameraPixels,
            Sprite {
                texture_atlas: Some(TextureAtlas::default()),
                ..Default::default()
            },
            Transform::default(),
            if needs_direction {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            },
            crate::graphics::pixel_view::RenderBand::ActorOverlay.layers(),
            animation,
            ChildOf(follower),
        ))
        .id();

    if needs_direction {
        commands.entity(visual).insert(SparkSlotConfig {
            weapon_slot: spawn.weapon_slot,
            direction_slot: SPARK_DIRECTION_SLOTS
                [usize::from(spawn.direction_index)],
        });
    }
}

/// Component marking a spark follower entity.
#[derive(Component)]
pub struct DamageSpark {
    /// Entity that received the damage.
    pub target_entity: Entity,
    /// Type of spark, used for overlap prevention.
    pub spark_type: SparkSource,
    /// Remaining lifetime in ticks.
    pub lifetime_ticks: u32,
}

/// Tracks small per-target Z offsets for simultaneous impacts.
#[derive(Resource, Default)]
pub struct SparkZCounters {
    counters: HashMap<Entity, f32>,
}

impl SparkZCounters {
    fn get_next_z(&mut self, entity: Entity) -> f32 {
        let counter = self.counters.entry(entity).or_insert(0.0);
        let z = *counter;
        *counter += 0.001;
        z
    }

    fn cleanup(&mut self, entity: Entity) {
        self.counters.remove(&entity);
    }
}

pub struct DamageSparksPlugin;

impl Plugin for DamageSparksPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SparkZCounters>();
        app.add_systems(
            GameTickUpdate,
            (
                spawn_damage_sparks
                    .after(super::damage_source::apply_damage)
                    .before(super::damage_source::damage_source_cleanup),
                configure_spark_animation_slots.after(spawn_damage_sparks),
                tick_damage_sparks,
                cleanup_dead_entity_spark_counters,
            )
                .chain(),
        );

    }
}

/// Spawns damage sparks when entities take damage.
#[expect(
    clippy::too_many_arguments,
    reason = "Bevy system dependencies are expressed as explicit system parameters."
)]
fn spawn_damage_sparks(
    mut commands: Commands,
    mut damage_events: MessageReader<DamageInfo>,
    mut z_counters: ResMut<SparkZCounters>,
    target_query: Query<&GlobalTransform, With<Gent>>,
    wall_query: Query<&GlobalTransform, With<BreakableWall>>,
    source_query: Query<(
        Option<&SparkSource>,
        Option<&WeaponHitSlot>,
        Option<&GlobalTransform>,
        Option<&Projectile>,
        Option<&LinearVelocity>,
    )>,
    spark_query: Query<&DamageSpark>,
    suppress_query: Query<(), With<NoDamageNumbers>>,
) {
    for damage_info in damage_events.read() {
        if suppress_query.get(damage_info.target).is_ok() {
            continue;
        }

        let target_transform = target_query
            .get(damage_info.target)
            .ok()
            .or_else(|| wall_query.get(damage_info.target).ok());
        let Some(target_transform) = target_transform else {
            continue;
        };
        let target_position = target_transform.translation().truncate();

        let source = read_spark_hit_source(damage_info.source, &source_query);
        let spark_type = classify_spark_source(
            source.explicit_spark,
            source.weapon_slot.as_deref(),
        );
        let spec = get_spark_spec(spark_type);
        if spec.prevent_overlap
            && active_spark_exists(
                damage_info.target,
                spark_type,
                &spark_query,
            )
        {
            continue;
        }

        let owner_position = target_query
            .get(damage_info.owner)
            .ok()
            .map(|tf| tf.translation().truncate());
        let direction = resolve_impact_direction(
            source.projectile_velocity,
            source.linear_velocity,
            source.position,
            owner_position,
            target_position,
        );
        spawn_spark_visual(
            &mut commands,
            &spec,
            SparkSpawn {
                target_entity: damage_info.target,
                target_translation: target_transform.translation(),
                spark_type,
                direction_index: impact_direction_index(direction),
                z_offset: z_counters.get_next_z(damage_info.target),
                weapon_slot: source.weapon_slot,
            },
        );
    }
}

fn active_spark_exists(
    target: Entity,
    spark_type: SparkSource,
    spark_query: &Query<&DamageSpark>,
) -> bool {
    spark_query.iter().any(|spark| {
        spark.target_entity == target && spark.spark_type == spark_type
    })
}

fn configure_spark_animation_slots(
    mut query: Query<(
        Entity,
        &SparkSlotConfig,
        &mut ScriptPlayer<SpriteAnimation>,
        &mut Visibility,
    )>,
    mut commands: Commands,
) {
    for (entity, slot_config, mut animation, mut visibility) in query.iter_mut()
    {
        if apply_spark_slots(&mut animation, slot_config) {
            *visibility = Visibility::Inherited;
            commands.entity(entity).remove::<SparkSlotConfig>();
        }
    }
}

fn apply_spark_slots(
    animation: &mut ScriptPlayer<SpriteAnimation>,
    slot_config: &SparkSlotConfig,
) -> bool {
    animation.set_slot(slot_config.direction_slot, true);
    let mut applied = animation.has_slot(slot_config.direction_slot);
    if let Some(weapon_slot) = &slot_config.weapon_slot {
        animation.set_slot(weapon_slot, true);
        animation.set_slot("AttackHit", true);
        applied &= animation.has_slot(weapon_slot);
    }
    applied
}



fn tick_damage_sparks(
    mut commands: Commands,
    mut query: Query<(Entity, &mut DamageSpark)>,
) {
    for (entity, mut spark) in query.iter_mut() {
        if spark.lifetime_ticks > 0 {
            spark.lifetime_ticks -= 1;
        } else {
            commands.entity(entity).despawn();
        }
    }
}

fn cleanup_dead_entity_spark_counters(
    mut z_counters: ResMut<SparkZCounters>,
    mut removed: RemovedComponents<Gent>,
) {
    for entity in removed.read() {
        z_counters.cleanup(entity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_vec2_close(actual: Vec2, expected: Vec2) {
        assert!(
            actual.distance(expected) < 0.0001,
            "expected {expected:?}, got {actual:?}"
        );
    }

    #[test]
    fn classifies_weapon_slots_and_projectile_fallback() {
        assert_eq!(
            classify_spark_source(None, Some("SwordHit")),
            SparkSource::Sword
        );
        assert_eq!(
            classify_spark_source(None, Some("HammerHit")),
            SparkSource::Hammer
        );
        assert_eq!(
            classify_spark_source(None, Some("BowHit")),
            SparkSource::Projectile
        );
        assert_eq!(
            classify_spark_source(None, None),
            SparkSource::Projectile
        );
        assert_eq!(
            classify_spark_source(
                Some(SparkSource::Exploding),
                Some("SwordHit")
            ),
            SparkSource::Exploding
        );
    }

    #[test]
    fn velocity_takes_priority_for_impact_direction() {
        let direction = resolve_impact_direction(
            Some(Vec2::new(0.0, -4.0)),
            Some(Vec2::X),
            Some(Vec2::ZERO),
            Some(Vec2::ZERO),
            Vec2::new(10.0, 0.0),
        );

        assert_vec2_close(direction, Vec2::NEG_Y);
    }

    #[test]
    fn impact_direction_falls_back_through_source_and_owner() {
        let source_direction = resolve_impact_direction(
            None,
            None,
            Some(Vec2::new(3.0, 4.0)),
            Some(Vec2::new(-10.0, 0.0)),
            Vec2::new(3.0, 9.0),
        );
        assert_vec2_close(source_direction, Vec2::Y);

        let owner_direction = resolve_impact_direction(
            None,
            None,
            Some(Vec2::new(3.0, 9.0)),
            Some(Vec2::new(8.0, 9.0)),
            Vec2::new(3.0, 9.0),
        );
        assert_vec2_close(owner_direction, Vec2::NEG_X);

        assert_vec2_close(
            resolve_impact_direction(None, None, None, None, Vec2::ZERO),
            Vec2::X,
        );
    }

    #[test]
    fn impact_directions_select_the_nearest_prebaked_row() {
        assert_eq!(impact_direction_index(Vec2::X), 0);
        assert_eq!(impact_direction_index(Vec2::new(1.0, 1.0)), 2);
        assert_eq!(impact_direction_index(Vec2::Y), 4);
        assert_eq!(impact_direction_index(Vec2::NEG_X), 8);
        assert_eq!(impact_direction_index(Vec2::NEG_Y), 12);
    }

    #[test]
    fn overlay_position_snaps_relative_to_the_camera() {
        let camera = Vec2::new(1.07, -2.11);
        let snapped =
            crate::graphics::pixel_view::PixelViewMapping::new(camera.extend(0.0), 6, UVec2::ZERO).align_overlay(Vec2::new(10.18, -3.18));
        let physical_screen_position = (snapped - camera) * 6.0;

        assert_vec2_close(
            physical_screen_position,
            physical_screen_position.round(),
        );
    }

    #[test]
    fn spark_proxy_keeps_its_grid_and_expires_at_the_presented_lifetime() {
        use theseeker_engine::presentation::{PresentationPlugin, PresentationSource, PresentationProxy};
        use crate::camera::{GameplayPixelScale, MainCamera};
        use crate::graphics::pixel_view::PixelViewPlugin;
        use theseeker_engine::time::{run_gametickupdate_schedule, GameTickPre, MasterClock};

        let mut app = App::new();
        for schedule in [Schedule::new(GameTickPre), Schedule::new(GameTickUpdate), Schedule::new(GameTickPost)] {
            app.add_schedule(schedule);
        }
        app.init_resource::<GameTime>().init_resource::<GameplayPixelScale>();
        app.insert_resource(RenderInterpolation::at(0.0)).add_plugins(PresentationPlugin);
        app.add_systems(GameTickUpdate, tick_damage_sparks);
        app.add_plugins(PixelViewPlugin);
        let camera = Vec2::new(1.07, -2.11);
        app.world_mut().spawn((MainCamera, Transform::from_translation(camera.extend(0.0))));
        let follower = app.world_mut().spawn((
            DamageSpark { target_entity: Entity::PLACEHOLDER, spark_type: SparkSource::Sword, lifetime_ticks: 12 },
            Transform::from_xyz(10.18, -3.18, 0.0), Visibility::Inherited,
        )).id();
        let source = app.world_mut().spawn((SparkVisual, PresentationAlignment::UprightCameraPixels, PresentationSource, Sprite::default(),
            Transform::default(), Visibility::Inherited, ChildOf(follower))).id();
        // Several simulation ticks may execute before a rendered frame. The proxy
        // must acquire its render policy before the logical source can disappear.
        for tick in 1..=13 {
            app.world_mut().resource_mut::<GameTime>().update_from_master(&MasterClock { tick, ..Default::default() });
            run_gametickupdate_schedule(app.world_mut());
        }
        assert!(app.world().get_entity(source).is_err());
        let proxy = app.world_mut().query_filtered::<Entity, With<PresentationProxy>>().single(app.world()).unwrap();
        *app.world_mut().resource_mut::<RenderInterpolation>() = RenderInterpolation::at(11.9);
        app.update();
        let transform = app.world().get::<Transform>(proxy).unwrap();
        let divisor = app.world().resource::<GameplayPixelScale>().divisor() as f32;
        let screen = (transform.translation.truncate() - camera) * divisor;
        assert_vec2_close(screen, screen.round());
        assert_eq!(app.world().get::<Visibility>(proxy), Some(&Visibility::Visible));
        *app.world_mut().resource_mut::<RenderInterpolation>() = RenderInterpolation::at(12.0);
        app.update();
        assert!(app.world().get_entity(proxy).is_err());
    }

    #[test]
    fn each_new_spark_type_has_an_independent_variant_registry() {
        assert_eq!(
            get_spark_spec(SparkSource::Sword).animation_keys,
            SWORD_SPARK_ANIMATIONS
        );
        assert_eq!(
            get_spark_spec(SparkSource::Hammer).animation_keys,
            HAMMER_SPARK_ANIMATIONS
        );
        assert_eq!(
            get_spark_spec(SparkSource::Projectile).animation_keys,
            PROJECTILE_SPARK_ANIMATIONS
        );
    }
}
