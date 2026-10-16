//! Player spawn/despawn hooks.
//!
//! Spawns a player gent and its gfx entity with all required components and
//! default stats. Despawns both on death and signals game over.
use bevy::camera::visibility::RenderLayers;
use leafwing_input_manager::prelude::ActionState;
use theseeker_engine::gent::{Gent, GentPhysicsBundle, TransformGfxFromGent};
use theseeker_engine::physics::{
    Collider, CollisionGroups, Group, LinearVelocity,
    PLAYER,
};
use theseeker_engine::animation::SpriteAnimationBundle;

use crate::game::gentstate::Dead;
use crate::game::gentstate::Facing;
use crate::game::player::crits::Crits;
use crate::prelude::*;

use super::{
    AdaptationProgress, Alignment, EnemiesNearby, Essence,
    EssenceOrbCadence, FlickerAbility, Grounded, HitFreezeTime, Idle,
    JumpCount, MaterialInventory, Passives, Player, PlayerBlueprint,
    PlayerGentBundle, PlayerGfx, PlayerGfxBundle, PlayerStatMod, PlayerStats,
    Ready, SkillInventory, WallSlideTime, WhirlAbility,
};
use crate::game::combat::{Health, MAX_HEALTH};
use crate::game::player::weapon::WeaponInventory;
use crate::levelgen::foliage::FoliageInteractor;
use crate::game::world_objects::CapsuleDropPlatformState;

#[derive(Resource, Debug, Default, Clone)]
pub struct PersistedPlayerProgression {
    pub passives: Option<Passives>,
    pub materials: Option<MaterialInventory>,
    pub skills: Option<SkillInventory>,
    pub weapons: Option<WeaponInventory>,
    pub adaptation: Option<AdaptationProgress>,
    pub essence: Option<Essence>,
    pub(crate) essence_orb_cadence: Option<EssenceOrbCadence>,
    pub crafting_knowledge: Option<crate::game::capsule_commerce::CraftingKnowledge>,
}

#[expect(
    clippy::too_many_lines,
    reason = "This setup system assembles the full player entity hierarchy and initial gameplay components from one blueprint."
)]
pub fn setup_player(
    mut q: Query<(&mut Transform, Entity, &ChildOf), Added<PlayerBlueprint>>,
    parent_query: Query<Entity, With<Children>>,
    progression: Res<PersistedPlayerProgression>,
    mut commands: Commands,
) {
    for (mut xf_gent, e_gent, parent) in q.iter_mut() {
        // Z-ordering for gent and gfx
        xf_gent.translation.z = 15.0 * 0.000_001;
        let e_gfx = commands.spawn(()).id();
        let e_effects_gfx = commands.spawn(()).id();
        let passives = progression.passives.clone().unwrap_or_default();
        let materials = progression.materials.clone().unwrap_or_default();
        let skills = progression.skills.clone().unwrap_or_default();
        let weapons = progression.weapons.clone().unwrap_or_default();
        let adaptation = progression.adaptation.unwrap_or_default();
        let essence = progression.essence.unwrap_or_default();
        let essence_orb_cadence =
            progression.essence_orb_cadence.unwrap_or_default();
        let crafting_knowledge =
            progression.crafting_knowledge.clone().unwrap_or_default();
        let consumable_tracker =
            super::skills::consumables::SkillStockTracker::from_inventory(
                &materials,
            );

        let player_collider = Collider::cuboid(2.0, 5.0);
        let foliage_interactor =
            FoliageInteractor::player_body(&player_collider);
        commands.entity(e_gent).insert((
            Name::new("Player"),
            PlayerGentBundle {
                player: Player,
                marker: Gent {
                    e_gfx,
                    e_effects_gfx,
                },
                phys: GentPhysicsBundle {
                    collider: player_collider,
                    linear_velocity: LinearVelocity(Vec2::ZERO),
                },
            },
            Facing::Right,
            Health {
                current: MAX_HEALTH,
                max: MAX_HEALTH,
            },
            foliage_interactor,
            super::player_action::PlayerAction::input_map(),
            ActionState::<super::player_action::PlayerAction>::default(),
        ));

        // Initial states
        commands.entity(e_gent).insert((Idle, Grounded));
        super::foreground::transition_action(&mut commands, e_gent, Ready);

        // Stats and modifiers
        commands.entity(e_gent).insert((
            PlayerStats::default(),
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
            Crits::new(2.0),
            StateDespawnMarker,
            passives,
            materials,
            crafting_knowledge,
            skills,
        ));

        commands.entity(e_gent).insert((
            adaptation,
            essence,
            essence_orb_cadence,
            Alignment::default(),
            CollisionGroups::new(PLAYER, Group::all()),
        ));

        commands.entity(e_gent).insert(weapons);
        commands.entity(e_gent).insert(consumable_tracker);

        commands.entity(e_gent).insert((
            // Sensors
            super::sensors::GroundSensor::default(),
            super::sensors::WallSensor::default(),
            super::sensors::CeilingSensor::default(),
            super::sensors::EnemyProximitySensor::default(),
            super::sensors::LastGroundedPosition {
                position: xf_gent.translation.truncate(),
            },
            CapsuleDropPlatformState::default(),
            // Input buffer
            super::input_buffer::InputBuffer::default(),
            super::states::LastMoveDir::default(),
            super::states::HorizontalInputIntent::default(),
        ));

        // Detach from level parent to avoid inheriting level transforms
        if let Ok(parent_entity) = parent_query.get(parent.parent()) {
            commands.entity(parent_entity).detach_children(&[e_gent]);
        }

        commands.entity(e_gfx).insert((
            PlayerGfxBundle {
                marker: PlayerGfx { e_gent, base_offset: super::PLAYER_GFX_OFFSET },
                gent2gfx: TransformGfxFromGent {
                    pixel_aligned: true,
                    gent: e_gent,
                    offset: Some(super::PLAYER_GFX_OFFSET),
                },
                sprite: Sprite {
                    texture_atlas: Some(TextureAtlas::default()),
                    ..Default::default()
                },
                transform: *xf_gent,
                animation: SpriteAnimationBundle::default(),
            },
            RenderLayers::layer(2),
        ));
    }
}

/// Despawn player and trigger game over when player dies.
pub fn despawn_dead_player(
    query: Query<
        (
            Entity,
            &Gent,
            &Passives,
            &MaterialInventory,
            &SkillInventory,
            &WeaponInventory,
            &AdaptationProgress,
            &Essence,
            &EssenceOrbCadence,
            Option<&crate::game::capsule_commerce::CraftingKnowledge>,
        ),
        (With<Dead>, With<Player>),
    >,
    mut progression: ResMut<PersistedPlayerProgression>,
    mut commands: Commands,
) {
    for (
        entity,
        gent,
        passives,
        materials,
        skills,
        weapons,
        adaptation,
        essence,
        essence_orb_cadence,
        knowledge,
    ) in query.iter()
    {
        progression.passives = Some(passives.clone());
        progression.materials = Some(materials.storage_only());
        progression.skills = Some(skills.clone());
        progression.weapons = Some(weapons.clone());
        progression.adaptation = Some(*adaptation);
        progression.essence = Some(*essence);
        progression.essence_orb_cadence = Some(*essence_orb_cadence);
        progression.crafting_knowledge = knowledge.cloned();

        commands.entity(gent.e_gfx).despawn();
        commands.entity(entity).despawn();
        commands.insert_resource(super::super::game_over::GameOver);
    }
}
