//! NPC presentation. NPCs react to the player nearby but offer no interaction.
use bevy::ecs::hierarchy::ChildOf;
use theseeker_engine::{animation::SpriteAnimationBundle, gent::{Gent, TransformGfxFromGent}};
use theseeker_engine::physics::{Collider, CollisionGroups, PLAYER, SENSOR};
use crate::prelude::*;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum NpcKind { Vagrant, Collector, Monk, Exiled }

impl NpcKind {
    /// Name, idle animation key, and graphics offset.
    fn presentation(self) -> (&'static str, &'static str, Option<Vec3>) {
        let offset = (self != Self::Vagrant).then_some(Vec3::new(0.0, 3.0, 0.0));
        match self {
            Self::Vagrant => ("Vagrant", "anim.vagrant.Idle", offset),
            Self::Collector => ("Collector", "anim.collector.Idle", offset),
            Self::Monk => ("Monk", "anim.monk.Idle", offset),
            Self::Exiled => ("Exiled", "anim.exiled.Idle", offset),
        }
    }
}

pub(crate) fn setup_npcs(
    mut commands: Commands,
    mut npcs: Query<(Entity, &NpcKind, &mut Transform), Added<NpcKind>>,
) {
    for (entity, kind, mut transform) in &mut npcs {
        let (name, animation, offset) = kind.presentation();
        transform.translation.z = 13.0 * 0.000_001;
        let e_gfx = commands.spawn((
            SpriteAnimationBundle::new_play_key(animation),
            Sprite { texture_atlas: Some(TextureAtlas::default()), ..default() },
            *transform,
            Visibility::Visible,
            InheritedVisibility::VISIBLE,
            TransformGfxFromGent {
                pixel_aligned: false,
                gent: entity,
                offset,
            },
            StateDespawnMarker,
        )).id();
        let e_effects_gfx = commands.spawn_empty().id();
        commands.entity(entity).insert((
            Name::new(name),
            Gent { e_gfx, e_effects_gfx },
            Collider::cuboid(14.0, 14.0),
            CollisionGroups::new(SENSOR, PLAYER),
            StateDespawnMarker,
        )).remove::<ChildOf>();
    }
}
