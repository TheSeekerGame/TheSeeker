//! Player spawn/despawn hooks.
//!
//! Spawns a player gent and its gfx entity with all required components and
//! default stats. Despawns both on death and signals game over.
use bevy::math::Dir2;
use bevy::prelude::*;
use bevy::render::view::RenderLayers;
use leafwing_input_manager::prelude::ActionState;
use theseeker_engine::gent::{Gent, GentPhysicsBundle, TransformGfxFromGent};
use theseeker_engine::physics::{
    Collider, CollisionGroups as InteractionGroups, Group, LinearVelocity,
    ShapeCaster, GROUND, PLAYER,
};
// use theseeker_engine::animation::SpriteAnimationBundle; // not used directly here

use crate::game::gentstate::Dead;
use crate::game::gentstate::Facing;
use crate::game::player::crits::Crits;
use crate::prelude::*;
use theseeker_engine::physics::ColliderShapeAccess;

use super::{
    EnemiesNearby, FlickerAbility, Grounded, HitFreezeTime, Idle, JumpCount,
    Passives, Player, PlayerBlueprint, PlayerGentBundle, PlayerGfx,
    PlayerGfxBundle, PlayerStatMod, PlayerStats, Ready, SkillInventory,
    WallSlideTime, WhirlAbility,
};
use crate::game::combat::{Health, MAX_HEALTH};

pub fn setup_player(
    mut q: Query<(&mut Transform, Entity, &ChildOf), Added<PlayerBlueprint>>,
    parent_query: Query<Entity, With<Children>>,
    mut commands: Commands,
) {
    for (mut xf_gent, e_gent, parent) in q.iter_mut() {
        // Z-ordering for gent and gfx
        xf_gent.translation.z = 15.0 * 0.000001;
        let e_gfx = commands.spawn(()).id();
        let e_effects_gfx = commands.spawn(()).id();
        let passives = Passives::default();

        commands.entity(e_gent).insert((
            Name::new("Player"),
            PlayerGentBundle {
                player: Player,
                marker: Gent {
                    e_gfx,
                    e_effects_gfx,
                },
                phys: GentPhysicsBundle {
                    collider: Collider::cuboid(2.0, 5.0),
                    shapecast: ShapeCaster {
                        shape: Collider::cuboid(2.0, 5.0)
                            .shared_shape()
                            .clone(),
                        origin: Vec2::new(0.0, 0.0),
                        max_toi: 50.0,
                        direction: Dir2::NEG_Y,
                        interaction: InteractionGroups::new(PLAYER, GROUND),
                    },
                    linear_velocity: LinearVelocity(Vec2::ZERO),
                },
                coyote_time: Default::default(),
            },
            Facing::Right,
            Health {
                current: MAX_HEALTH,
                max: MAX_HEALTH,
            },
            super::player_action::PlayerAction::input_map(),
            ActionState::<super::player_action::PlayerAction>::default(),
        ));

        // Initial states
        commands.entity(e_gent).insert((Idle, Ready, Grounded));

        // Stats and modifiers
        commands.entity(e_gent).insert((
            PlayerStats::init_from_config(),
            PlayerStatMod::new(),
            EnemiesNearby(0),
        ));

        // Other components - split into two inserts to avoid tuple size limit
        commands.entity(e_gent).insert((
            WallSlideTime(f32::MAX),
            HitFreezeTime(u32::MAX, None),
            JumpCount(0),
            WhirlAbility {
                energy: super::skills::types::whirl_metadata().max_energy,
            },
            FlickerAbility {
                energy: super::skills::types::flicker_strike_metadata()
                    .max_energy,
            },
            super::skills::explosive_mine::ExplosiveMineAbility {
                energy: super::skills::types::explosive_mine_metadata()
                    .max_energy,
            },
            Crits::new(2.0),
            StateDespawnMarker,
            passives,
            SkillInventory::default(),
            super::BuffTick::default(),
            InteractionGroups::new(PLAYER, Group::all()),
        ));

        commands.entity(e_gent).insert((
            // Sensors
            super::sensors::GroundSensor::default(),
            super::sensors::WallSensor::default(),
            super::sensors::CeilingSensor::default(),
            super::sensors::EnemyProximitySensor::default(),
            super::sensors::LastGroundedPosition {
                position: xf_gent.translation.truncate(),
            },
            // Input buffer
            super::input_buffer::InputBuffer::default(),
        ));

        // Detach from level parent to avoid inheriting level transforms
        if let Ok(parent_entity) = parent_query.get(parent.parent()) {
            commands.entity(parent_entity).remove_children(&[e_gent]);
        }

        commands.entity(e_gfx).insert((
            PlayerGfxBundle {
                marker: PlayerGfx { e_gent },
                gent2gfx: TransformGfxFromGent {
                    pixel_aligned: false,
                    gent: e_gent,
                    offset: None,
                },
                sprite: Sprite {
                    texture_atlas: Some(TextureAtlas::default()),
                    ..Default::default()
                },
                transform: *xf_gent,
                animation: Default::default(),
            },
            RenderLayers::layer(2),
        ));
    }
}

/// Despawn player and trigger game over when player dies.
pub fn despawn_dead_player(
    query: Query<(Entity, &Gent), (With<Dead>, With<Player>)>,
    mut commands: Commands,
) {
    for (entity, gent) in query.iter() {
        commands.entity(gent.e_gfx).despawn();
        commands.entity(entity).despawn();
        commands.insert_resource(super::super::game_over::GameOver);
    }
}
