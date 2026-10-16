use bevy::prelude::*;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::{groups, AnimationCollider, Collider};

use crate::game::combat::sparks::WeaponHitSlot;
use crate::game::combat::DamageSource;

pub mod attacking;
pub mod burning_dashing;
pub mod dash_striking;
pub mod dashing;
pub mod flicker_striking;
pub mod gathering;
pub mod whirling;

/// A player hitbox that follows its owner and takes its shape each frame from the
/// magenta pixels of the owner's animation.
fn animation_hitbox(
    owner: Entity,
    gent: &Gent,
    damage: DamageSource,
    hit_slot: &str,
) -> impl Bundle {
    (
        damage,
        // Replaced by the shape of the first animation frame.
        Collider::cuboid(8.0, 8.0),
        AnimationCollider(gent.e_gfx),
        groups::player_attack(),
        Transform::default(),
        GlobalTransform::default(),
        WeaponHitSlot { slot_name: hit_slot.to_owned() },
        ChildOf(owner),
    )
}
