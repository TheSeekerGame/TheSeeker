use super::DamageInfo;
use crate::graphics::NoDamageNumbers;
use crate::prelude::*;
// Bevy types are available via crate prelude
use std::collections::HashMap;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::gent::TransformGfxFromGent;
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::GameTickUpdate;

/// Identifies the type of spark to spawn when damage is dealt
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SparkSource {
    Default,
    #[allow(dead_code)] // Reserved for future explosive spark effects
    Exploding,
    // Add more spark types here as needed
}

/// Optional component to specify which weapon slot should be set for spark animations
#[derive(Component, Debug, Clone)]
pub struct WeaponHitSlot {
    pub slot_name: String,
}

/// Internal: configuration for slot setup on a spawned spark
#[derive(Component, Debug, Clone)]
struct SparkSlotConfig {
    /// Optional weapon slot name to drive audio (e.g., "SwordHit")
    weapon_slot: Option<String>,
    /// Chosen spark visual variation [1..=6]
    picked_spark: u8,
}

/// Configuration for a spark type
#[derive(Debug, Clone)]
pub struct SparkSpec {
    /// Animation asset key to play
    pub animation_key: &'static str,
    /// Duration in game ticks (96Hz)
    pub lifetime_ticks: u32,
    /// Position offset relative to target
    pub offset: Vec3,
    /// If true, prevents overlapping sparks of the same type on the same enemy
    pub prevent_overlap: bool,
}

/// Registry of spark configurations
fn get_spark_spec(spark_type: SparkSource) -> SparkSpec {
    match spark_type {
        SparkSource::Default => SparkSpec {
            animation_key: "anim.spider.Sparks",
            lifetime_ticks: 16, // ~0.167s at 96Hz
            offset: Vec3::ZERO,
            prevent_overlap: false, // Allow multiple default sparks
        },
        SparkSource::Exploding => SparkSpec {
            animation_key: "anim.player.ExplodingSparks",
            lifetime_ticks: 24,
            offset: Vec3::new(0.0, 0.0, 0.01), // Slightly above default sparks
            prevent_overlap: true, // Prevent overlapping exploding sparks
        },
    }
}

/// Component marking a spark entity
#[derive(Component)]
pub struct DamageSpark {
    /// Entity that received the damage (for transform syncing)
    pub target_entity: Entity,
    /// Type of spark (used for overlap prevention)
    pub spark_type: SparkSource,
    /// Remaining lifetime in ticks
    pub lifetime_ticks: u32,
}

/// Resource tracking Z-index counters for each damaged entity
#[derive(Resource, Default)]
pub struct SparkZCounters {
    counters: HashMap<Entity, f32>,
}

impl SparkZCounters {
    fn get_next_z(&mut self, entity: Entity) -> f32 {
        let counter = self.counters.entry(entity).or_insert(0.0);
        let z = *counter;
        *counter += 0.001; // Small increment for proper layering
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
                spawn_damage_sparks.after(super::damage_source::apply_damage),
                // Configure slots; will retry until runtime is initialized
                configure_spark_animation_slots.after(spawn_damage_sparks),
                tick_damage_sparks,
                cleanup_dead_entity_spark_counters,
            )
                .chain(),
        );
    }
}

/// Spawns damage sparks when enemies take damage
fn spawn_damage_sparks(
    mut commands: Commands,
    mut damage_events: EventReader<DamageInfo>,
    mut z_counters: ResMut<SparkZCounters>,
    target_query: Query<&GlobalTransform, With<Gent>>,
    source_query: Query<(
        Option<&SparkSource>,
        Option<&WeaponHitSlot>,
    )>,
    spark_query: Query<&DamageSpark>,
    suppress_query: Query<(), With<NoDamageNumbers>>,
) {
    for damage_info in damage_events.read() {
        // Suppress sparks on targets that opt-out (e.g., Bell)
        if suppress_query.get(damage_info.target).is_ok() {
            continue;
        }
        // Get target position
        let Ok(target_transform) = target_query.get(damage_info.target) else {
            continue;
        };

        // Get spark type and weapon slot from damage source
        let (spark_type, weapon_slot) = source_query
            .get(damage_info.source)
            .ok()
            .map(|(spark_src, weapon_slot)| {
                (
                    spark_src.copied().unwrap_or(SparkSource::Default),
                    weapon_slot,
                )
            })
            .unwrap_or((SparkSource::Default, None));

        let spec = get_spark_spec(spark_type);

        // Overlap prevention
        if spec.prevent_overlap {
            // Check if there's already an active spark of the same type on this enemy
            let has_active_spark = spark_query.iter().any(|spark| {
                spark.target_entity == damage_info.target
                    && spark.spark_type == spark_type
            });

            if has_active_spark {
                continue; // Skip spawning this spark
            }
        }

        // Get next Z index for this target
        let z_offset = z_counters.get_next_z(damage_info.target);

        // Spawn spark entity
        let spark_entity = commands
            .spawn((
                DamageSpark {
                    target_entity: damage_info.target,
                    spark_type,
                    lifetime_ticks: spec.lifetime_ticks,
                },
                TransformGfxFromGent {
                    pixel_aligned: false,
                    gent: damage_info.target,
                    offset: Some(spec.offset + Vec3::new(0.0, 0.0, z_offset)),
                },
                Sprite {
                    texture_atlas: Some(TextureAtlas::default()),
                    ..Default::default()
                },
                Transform::from_translation(
                    target_transform.translation()
                        + spec.offset
                        + Vec3::new(0.0, 0.0, z_offset),
                ),
                GlobalTransform::default(),
                Visibility::default(),
                ViewVisibility::default(),
                StateDespawnMarker,
            ))
            .id();

        // Add animation
        let mut animation = ScriptPlayer::<SpriteAnimation>::default();
        animation.play_key(spec.animation_key);

        // Defer slot setup until ScriptPlayer is attached so slot-enable triggers fire
        if spark_type == SparkSource::Default {
            let mut rng = rand::rng();
            let picked_spark = rng.random_range(1..=6) as u8;
            commands.entity(spark_entity).insert(SparkSlotConfig {
                weapon_slot: weapon_slot.map(|w| w.slot_name.clone()),
                picked_spark,
            });
        }

        commands.entity(spark_entity).insert(animation);
    }
}

/// After spark entities are created and their ScriptPlayer is attached, set slots so slot-enable triggers fire
fn configure_spark_animation_slots(
    mut query: Query<(
        Entity,
        &DamageSpark,
        &SparkSlotConfig,
        &mut ScriptPlayer<SpriteAnimation>,
    )>,
    mut commands: Commands,
) {
    for (entity, spark, slot_cfg, mut anim) in query.iter_mut() {
        // Configure only default sparks
        if spark.spark_type == SparkSource::Default {
            let spark_slot = format!("Spark{}", slot_cfg.picked_spark);
            anim.set_slot(&spark_slot, true);
            let mut applied = anim.has_slot(&spark_slot);
            if let Some(ref weapon) = slot_cfg.weapon_slot {
                anim.set_slot(weapon, true);
                anim.set_slot("AttackHit", true);
                applied &= anim.has_slot(weapon);
            }
            // Remove config only after slots are observable to handle uninitialized runtimes
            if applied {
                commands.entity(entity).remove::<SparkSlotConfig>();
            }
        }
    }
}

/// Updates spark lifetimes and despawns expired sparks
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

/// Cleans up Z counters for dead entities
fn cleanup_dead_entity_spark_counters(
    mut z_counters: ResMut<SparkZCounters>,
    mut removed: RemovedComponents<Gent>,
) {
    for entity in removed.read() {
        z_counters.cleanup(entity);
    }
}
