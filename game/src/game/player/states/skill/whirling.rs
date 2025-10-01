use bevy::prelude::*;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::{groups, AnimationCollider, Collider};
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::GameTickUpdate;

use crate::game::combat::sparks::WeaponHitSlot;
use crate::game::combat::DamageSource;
use crate::game::combat::Stealthed;
use crate::game::gentstate::Facing;
use crate::game::effects::stealthed::StealthEffect;
use crate::game::player::skills::types::{whirl_animation_key, SkillWeaponKind};
use crate::game::player::states::{WeaponType, Whirling};
use crate::game::player::weapon::{CurrentWeapon, PlayerMeleeWeapon};
use crate::game::player::{Player, PlayerStatMod, PlayerStateSet};
use crate::game::player::player_anim::set_direction_slots;

pub struct WhirlingStatePlugin;

impl Plugin for WhirlingStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                whirling_enter_system,
                whirling_update_system,
                whirling_cleanup_on_remove,
            )
                .chain()
                .in_set(PlayerStateSet::Behavior),
        );
    }
}

/// Marker attached to the sustained damage entity spawned by Whirling
#[derive(Component)]
struct WhirlingDamage;

// On enter: play animation and spawn sustained damage entity
fn whirling_enter_system(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &mut Whirling,
            &Gent,
            &Facing,
            Has<StealthEffect>,
            Option<&PlayerStatMod>,
        ),
        (With<Player>, Added<Whirling>),
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>>,
    _weapon: CurrentWeapon,
) {
    for (entity, mut whirling, gent, facing, is_stealthed, stat_mod) in
        query.iter_mut()
    {
        // Play whirl animation
        if let Ok(mut script_player) = gfx_query.get_mut(gent.e_gfx) {
            // Ensure direction slots reflect current Facing before playing a key
            set_direction_slots(&mut script_player, facing);
            let weapon_kind: SkillWeaponKind = whirling.weapon_type.into();
            script_player.play_key(whirl_animation_key(weapon_kind));
        }

        // Calculate damage based on weapon type and stat modifiers
        let base_damage = match whirling.weapon_type {
            WeaponType::Sword => PlayerMeleeWeapon::Sword.base_damage(),
            WeaponType::Hammer => PlayerMeleeWeapon::Hammer.base_damage(),
            WeaponType::Bow => {
                // Shouldn't happen, but provide fallback
                PlayerMeleeWeapon::Sword.base_damage()
            },
        };
        let damage = base_damage * stat_mod.map(|s| s.damage).unwrap_or(1.0);

        // Spawn sustained damage entity
        let damage_entity = commands
            .spawn((
                DamageSource::new(u32::MAX, entity, damage),
                Collider::cuboid(8.0, 8.0), // Placeholder – updated by AnimationCollider
                AnimationCollider(gent.e_gfx),
                groups::player_attack(),
                Transform::from_translation(Vec3::ZERO),
                GlobalTransform::default(),
                WhirlingDamage,
            ))
            .insert(ChildOf(entity))
            .id();

        // Mark weapon hit slot so sparks can play correct impact SFX
        let weapon_slot = match whirling.weapon_type {
            WeaponType::Sword => "SwordHit",
            WeaponType::Hammer => "HammerHit",
            WeaponType::Bow => "BowHit",
        };
        commands.entity(damage_entity).insert(WeaponHitSlot {
            slot_name: weapon_slot.to_string(),
        });

        // Add stealth if player is stealthed
        if is_stealthed {
            commands.entity(damage_entity).insert(Stealthed);
        }

        // Store damage entity reference
        whirling.damage_entity = Some(damage_entity);
    }
}

// Per-tick: manage animation and damage rhythm
pub fn whirling_update_system(
    mut commands: Commands,
    mut query: Query<(&mut Whirling, Has<StealthEffect>), With<Player>>,
    mut damage_query: Query<&mut DamageSource>,
) {
    for (mut whirling, is_stealthed) in query.iter_mut() {
        // Increment tick counter
        whirling.tick = whirling.tick.saturating_add(1);

        // Calculate frame number based on ticks (8 ticks per frame)
        let raw_frame = (whirling.tick - 1) / 8 + 1;

        // Handle animation looping based on weapon type
        let frame = match whirling.weapon_type {
            WeaponType::Sword => {
                // Sword: frames 1-6, loop 2-5
                if raw_frame == 1 {
                    1
                } else {
                    ((raw_frame - 2) % 4) + 2
                }
            },
            WeaponType::Hammer => {
                // Hammer: frames 1-10, loop 3-8
                if raw_frame <= 2 {
                    raw_frame
                } else {
                    ((raw_frame - 3) % 6) + 3
                }
            },
            _ => raw_frame, // Fallback
        };

        // Check if we're on a damage frame and clear damaged_set to allow repeated hits per loop
        if let Some(damage_entity) = whirling.damage_entity {
            if let Ok(mut damage_source) = damage_query.get_mut(damage_entity) {
                match whirling.weapon_type {
                    WeaponType::Sword => {
                        // Sword damage frames: 2 and 4
                        if frame == 2 || frame == 4 {
                            if whirling.last_damage_frame != frame {
                                damage_source.damaged_set.clear();
                                whirling.last_damage_frame = frame;
                            }
                        }
                    },
                    WeaponType::Hammer => {
                        // Hammer damage frames: 4 and 7
                        if frame == 4 || frame == 7 {
                            if whirling.last_damage_frame != frame {
                                damage_source.damaged_set.clear();
                                whirling.last_damage_frame = frame;
                            }
                        }
                    },
                    _ => {},
                }
            }
        }

        // Synchronize Stealthed on damage entity with player's state
        if let Some(damage_entity) = whirling.damage_entity {
            if is_stealthed {
                // Player is stealthed, ensure damage entity has Stealthed
                commands.entity(damage_entity).insert(Stealthed);
            } else {
                // Player is not stealthed, ensure damage entity doesn't have Stealthed
                commands.entity(damage_entity).remove::<Stealthed>();
            }
        }
    }
}

/// Ensure sustained damage entity is removed when Whirling ends
fn whirling_cleanup_on_remove(
    mut removed: RemovedComponents<crate::game::player::states::Whirling>,
    mut commands: Commands,
    query: Query<(Entity, &DamageSource), With<WhirlingDamage>>,
) {
    for player_entity in removed.read() {
        super::super::queue_state_cleanup(
            &mut commands,
            player_entity,
            super::super::cleanup_whirling,
        );
        for (damage_entity, damage_source) in query.iter() {
            if damage_source.owner == player_entity {
                commands.entity(damage_entity).despawn();
            }
        }
    }
}
