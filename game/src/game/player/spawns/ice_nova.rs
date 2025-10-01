use bevy::prelude::*;
use bevy::render::view::RenderLayers;

use theseeker_engine::animation::SpriteAnimationBundle;
use theseeker_engine::physics::{groups, Collider};

use crate::game::combat::damage_source::DamageSource;
use crate::game::combat::Stealthed;
use crate::game::effects::frozen::{Frozen, FrozenSize};
use crate::game::enemy::{Enemy, EnemyVariant, MovementState};
use crate::game::effects::stealthed::StealthEffect;
use crate::prelude::*;

const ICE_NOVA_RADIUS: f32 = 60.0;
const ICE_NOVA_DAMAGE: f32 = 27.0;
const ICE_NOVA_FREEZE_TICKS: u32 = 300;
const ICE_NOVA_VISUAL_LIFETIME: u32 = 32;
const ICE_NOVA_BURST_TICK: u32 = 9;
const ICE_NOVA_Z: f32 = 30.0 * 0.000001;

#[derive(Component)]
pub struct IceNova {
    owner: Entity,
    center: Vec2,
    elapsed_ticks: u32,
    visual: Entity,
    applied_burst: bool,
}

#[derive(Component)]
struct IceNovaVisual;

pub struct IceNovaPlugin;

impl Plugin for IceNovaPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            tick_ice_nova.run_if(in_state(AppState::InGame)).before(
                crate::game::combat::damage_source::determine_damage_targets,
            ),
        );
    }
}

pub fn spawn_ice_nova(commands: &mut Commands, owner: Entity, center: Vec2) {
    let visual = commands
        .spawn((
            IceNovaVisual,
            SpriteAnimationBundle::new_play_key("anim.player.IceNova"),
            Sprite {
                texture_atlas: Some(TextureAtlas::default()),
                ..Default::default()
            },
            Transform::from_translation(center.extend(ICE_NOVA_Z)),
            GlobalTransform::default(),
            Visibility::Visible,
            InheritedVisibility::default(),
            ViewVisibility::default(),
            RenderLayers::layer(2),
            StateDespawnMarker,
        ))
        .id();

    commands.spawn((
        IceNova {
            owner,
            center,
            elapsed_ticks: 0,
            visual,
            applied_burst: false,
        },
        Transform::from_translation(center.extend(0.0)),
        GlobalTransform::default(),
        StateDespawnMarker,
    ));
}

pub(crate) fn tick_ice_nova(
    mut commands: Commands,
    mut query: Query<(Entity, &mut IceNova)>,
    enemy_query: Query<
        (
            Entity,
            &GlobalTransform,
            Option<&MovementState>,
        ),
        (
            With<Enemy>,
            Without<crate::game::gentstate::Dead>,
        ),
    >,
    mut frozen_query: Query<&mut Frozen>,
    owner_stealth: Query<(), With<StealthEffect>>,
) {
    for (entity, mut nova) in query.iter_mut() {
        nova.elapsed_ticks = nova.elapsed_ticks.saturating_add(1);

        if nova.elapsed_ticks == ICE_NOVA_BURST_TICK && !nova.applied_burst {
            apply_ice_nova_burst(
                &mut commands,
                &mut frozen_query,
                &enemy_query,
                &owner_stealth,
                nova.owner,
                nova.center,
            );
            nova.applied_burst = true;
        }

        if nova.elapsed_ticks >= ICE_NOVA_VISUAL_LIFETIME {
            commands.entity(nova.visual).despawn();
            commands.entity(entity).despawn();
        }
    }
}

fn apply_ice_nova_burst(
    commands: &mut Commands,
    frozen_query: &mut Query<&mut Frozen>,
    enemy_query: &Query<
        (
            Entity,
            &GlobalTransform,
            Option<&MovementState>,
        ),
        (
            With<Enemy>,
            Without<crate::game::gentstate::Dead>,
        ),
    >,
    owner_stealth: &Query<(), With<StealthEffect>>,
    owner: Entity,
    center: Vec2,
) {
    let mut affected_enemies: Vec<(Entity, FrozenSize)> = Vec::new();

    for (enemy_entity, enemy_tf, movement_state) in enemy_query.iter() {
        let enemy_pos = enemy_tf.translation().truncate();
        if enemy_pos.distance(center) <= ICE_NOVA_RADIUS {
            let variant = movement_state
                .map(|s| s.enemy_variant)
                .unwrap_or(EnemyVariant::Default);
            let size = match variant {
                EnemyVariant::BigSpider => FrozenSize::Large,
                _ => FrozenSize::Small,
            };
            affected_enemies.push((enemy_entity, size));
        }
    }

    for (enemy_entity, size) in affected_enemies {
        if let Ok(mut frozen) = frozen_query.get_mut(enemy_entity) {
            frozen.refresh(ICE_NOVA_FREEZE_TICKS);
            frozen.size = size;
            frozen.inflicted_by = Some(owner);
        } else {
            let mut component = Frozen::new(ICE_NOVA_FREEZE_TICKS, size);
            component.inflicted_by = Some(owner);
            commands.entity(enemy_entity).insert(component);
        }
    }

    let mut damage_entity = commands.spawn((
        DamageSource::new(1, owner, ICE_NOVA_DAMAGE).with_max_targets(64),
        Collider::ball(ICE_NOVA_RADIUS),
        groups::player_attack(),
        Transform::from_translation(center.extend(0.0)),
        GlobalTransform::from_translation(center.extend(0.0)),
        StateDespawnMarker,
    ));

    if owner_stealth.get(owner).is_ok() {
        damage_entity.insert(Stealthed);
    }
}
