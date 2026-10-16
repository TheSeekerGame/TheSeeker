//! Health bars over wounded enemies. When an enemy's health changes, a spark flashes at the
//! bar's new edge, and a white trail follows the lost health down after a delay.
use std::time::Duration;

use bevy::reflect::TypePath;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use bevy::ui::UiScale;
use theseeker_engine::physics::Collider;

use crate::camera::MainCamera;
use crate::game::combat::{Health, Invulnerable};
use crate::game::enemy::Enemy;
use crate::game::gentstate::Dead;
use crate::prelude::*;

/// How long after a change the trail starts following the health.
const TRAIL_DELAY: Duration = Duration::from_millis(600);
/// The share of its distance to the health that the trail closes each frame.
const TRAIL_SPEED: f32 = 0.2;
const SPARK_SECONDS: f32 = 0.4;
/// The bar's size in UI pixels; the bar fills its middle half, leaving the spark room above
/// and below.
const BAR_SIZE: Vec2 = Vec2::new(80.0, 16.0);
/// How far above the enemy's collider the bar's top floats, in UI pixels.
const BAR_RISE: f32 = 55.0;

pub struct EnemyHpBarPlugin;

impl Plugin for EnemyHpBarPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiMaterialPlugin::<HpBarMaterial>::default())
            .add_systems(Update, (spawn_hp_bars, update_hp_bars));
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Copy, Debug)]
struct HpBarMaterial {
    /// The share of health left.
    #[uniform(0)]
    health: f32,
    /// Where the white trail of lost health ends, as a share of the bar.
    #[uniform(1)]
    trail: f32,
    /// The spark's opacity.
    #[uniform(2)]
    spark: f32,
    /// `1` while the enemy cannot take damage.
    #[uniform(3)]
    invulnerable: f32,
}

impl UiMaterial for HpBarMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/enemy_hp.wgsl".into()
    }
}

#[derive(Component)]
struct HpBar {
    enemy: Entity,
    /// When the enemy's health last changed, in presentation time.
    changed_at: Option<Duration>,
}

fn spawn_hp_bars(
    mut commands: Commands,
    enemies: Query<Entity, (Added<Health>, With<Enemy>, With<GlobalTransform>)>,
    mut materials: ResMut<Assets<HpBarMaterial>>,
) {
    for enemy in &enemies {
        let material = HpBarMaterial { health: 1.0, trail: 1.0, spark: 0.0, invulnerable: 0.0 };
        commands.spawn((
            Node { width: px(BAR_SIZE.x), height: px(BAR_SIZE.y), position_type: PositionType::Absolute, ..default() },
            MaterialNode(materials.add(material)),
            Visibility::Hidden,
            HpBar { enemy, changed_at: None },
            StateDespawnMarker,
        ));
    }
}

/// Floats each bar over its enemy while the enemy is wounded, animates it, and removes it once
/// the enemy dies or is gone.
fn update_hp_bars(
    mut commands: Commands,
    time: Res<PresentationTime>,
    ui_scale: Res<UiScale>,
    camera: Single<(&GlobalTransform, &Camera), With<MainCamera>>,
    enemies: Query<(&Health, Has<Invulnerable>, &GlobalTransform, Option<&Collider>), (With<Enemy>, Without<Dead>)>,
    mut bars: Query<(Entity, &mut HpBar, &MaterialNode<HpBarMaterial>, &mut Node, &mut Visibility)>,
    mut materials: ResMut<Assets<HpBarMaterial>>,
) {
    let (camera_transform, camera) = *camera;
    for (entity, mut bar, material, mut node, mut visibility) in &mut bars {
        let Ok((health, invulnerable, transform, collider)) = enemies.get(bar.enemy) else {
            commands.entity(entity).despawn();
            continue;
        };
        *visibility = if health.current < health.max { Visibility::Inherited } else { Visibility::Hidden };
        let above = transform.translation() + Vec3::Y * collider.map_or(0.0, |collider| collider.half_extents().y);
        if let Ok(screen) = camera.world_to_viewport(camera_transform, above) {
            let corner = screen / ui_scale.0.max(f32::EPSILON) - Vec2::new(BAR_SIZE.x / 2.0, BAR_RISE);
            node.left = px(corner.x);
            node.top = px(corner.y);
        }
        let Some(mut material) = materials.get_mut(&material.0) else {
            continue;
        };
        let share = health.current as f32 / health.max as f32;
        if (material.health - share).abs() > f32::EPSILON {
            bar.changed_at = Some(time.elapsed());
        }
        let since = bar.changed_at.map_or(Duration::MAX, |at| time.elapsed().saturating_sub(at));
        if since >= TRAIL_DELAY {
            material.trail = material.trail.lerp(share, TRAIL_SPEED);
        }
        material.spark = (1.0 - since.as_secs_f32() / SPARK_SECONDS).max(0.0);
        material.health = share;
        material.invulnerable = if invulnerable { 1.0 } else { 0.0 };
    }
}
