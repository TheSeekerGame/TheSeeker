use bevy::ecs::hierarchy::ChildOf;
use bevy::prelude::*;
use leafwing_input_manager::prelude::*;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::{
    groups, AnimationCollider, Collider, LinearVelocity,
};
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::{GameTickUpdate, GameTime};

use crate::game::combat::damage_source::Pushback;
use crate::game::combat::sparks::WeaponHitSlot;
use crate::game::combat::{DamageSource, Health, SparkSource, Stealthed};
use crate::game::gentstate::Facing;
use crate::game::physics::Knockback;
use crate::game::player::skills::types::burning_dash_metadata;
use crate::game::player::states::{
    transition_action, BurningDashing, Ready,
};
use crate::game::player::{Player, PlayerAction, PlayerStateSet};
use crate::game::player::player_anim::set_direction_slots;

pub struct BurningDashingStatePlugin;

impl Plugin for BurningDashingStatePlugin {
    fn build(&self, app: &mut App) {
        // Core behavior (only when BurningDashing exists)
        app.add_systems(
            GameTickUpdate,
            (
                burning_dashing_enter_system,
                burning_dashing_update_system,
            )
                .chain()
                .run_if(any_with_component::<BurningDashing>)
                .in_set(PlayerStateSet::Behavior),
        );
        // Cleanup must always run to catch external removals
        app.add_systems(
            GameTickUpdate,
            burning_dashing_cleanup_on_remove.in_set(PlayerStateSet::Behavior),
        );
    }
}

// Burning Dash tuning
const BURNING_DASH_VELOCITY: f32 = 5.2; // ~500 px/s at 96 Hz
const HEALTH_COST_PER_TICK: u32 = 2; // Health reduced per tick
const MIN_HEALTH_THRESHOLD: u32 = 2; // Never go below this health
const DAMAGE_PER_HIT: f32 = 44.0;
const DAMAGE_INTERVAL_TICKS: u32 = 8; // Spawn damage collider every 8 ticks

/// Marker for the sustained damage entity spawned by Burning Dash
#[derive(Component)]
struct BurningDashDamage;

// On enter: initialize velocity and spawn sustained damage entity
fn burning_dashing_enter_system(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &mut BurningDashing,
            &Facing,
            &mut LinearVelocity,
            &Gent,
            Has<crate::game::effects::stealthed::StealthEffect>,
        ),
        (With<Player>, Added<BurningDashing>),
    >,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>>,
) {
    for (
        entity,
        mut burning_dashing,
        facing,
        mut velocity,
        gent,
        is_stealthed,
    ) in query.iter_mut()
    {
        // Set initial velocity based on facing direction
        let dir = facing.direction();
        velocity.0.x = dir * BURNING_DASH_VELOCITY * burning_dashing.speed_mod;
        velocity.0.y = 0.0;

        // Play burning dash animation
        if let Ok(mut script_player) = gfx_query.get_mut(gent.e_gfx) {
            // Ensure direction slots reflect current Facing before playing a key
            set_direction_slots(&mut script_player, facing);
            script_player.play_key(burning_dash_metadata().animation_key);
        }

        // Spawn persistent damage entity (like Whirling does)
        let damage_entity = commands
            .spawn((
                DamageSource::new(u32::MAX, entity, DAMAGE_PER_HIT), // Infinite lifetime, we'll manage it manually
                SparkSource::Default,
                Collider::cuboid(8.0, 8.0), // Placeholder - AnimationCollider will update from magenta pixels
                AnimationCollider(gent.e_gfx), // Reference to the graphics entity with the animation
                groups::player_attack(),
                Transform::from_translation(Vec3::ZERO), // Local position relative to player
                GlobalTransform::default(),
                Pushback(Knockback {
                    ticks: 0,
                    max_ticks: 14, // ~0.15s at 96Hz
                    strength: Vec2::new(facing.direction() * 1.2, 0.0), // Similar to melee weapons but slightly stronger
                }),
            ))
            .insert(BurningDashDamage)
            .insert(ChildOf(entity)) // Make it a child of the player
            .id();

        // Burning dash uses its own distinct spark-audio slot
        commands.entity(damage_entity).insert(WeaponHitSlot {
            slot_name: "BurningDashHit".to_string(),
        });

        // Mark damage entity as stealthed if player is stealthed
        if is_stealthed {
            commands.entity(damage_entity).insert(Stealthed);
        }

        burning_dashing.damage_entity = Some(damage_entity);
    }
}

// System that updates burning dashing state each tick
fn burning_dashing_update_system(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &mut BurningDashing,
            &mut LinearVelocity,
            &Facing,
            &mut Health,
            &ActionState<PlayerAction>,
        ),
        With<Player>,
    >,
    mut damage_query: Query<&mut DamageSource>,
    _time: Res<GameTime>,
) {
    for (
        entity,
        mut burning_dashing,
        mut velocity,
        facing,
        mut health,
        action_state,
    ) in query.iter_mut()
    {
        // Check if the skill button is still held down
        let still_pressed =
            if let Some(slot_action) = burning_dashing.slot_action {
                action_state.pressed(&slot_action)
            } else {
                false
            };

        // Exit conditions: button released or health too low
        if !still_pressed || health.current <= MIN_HEALTH_THRESHOLD {
            // Zero velocity on exit
            velocity.0 = Vec2::ZERO;
            // Despawn damage entity
            if let Some(damage_entity) = burning_dashing.damage_entity {
                commands.entity(damage_entity).despawn();
            }
            finish_burning_dash(&mut commands, entity);
            return;
        }

        // Advance tick counter
        burning_dashing.tick = burning_dashing.tick.saturating_add(1);

        // Apply health cost per tick
        if health.current > MIN_HEALTH_THRESHOLD {
            health.current = health
                .current
                .saturating_sub(HEALTH_COST_PER_TICK)
                .max(MIN_HEALTH_THRESHOLD);
        }

        // Maintain burning dash velocity
        let dir = facing.direction();
        velocity.0.x = dir * BURNING_DASH_VELOCITY * burning_dashing.speed_mod;
        velocity.0.y = 0.0;

        // Clear damage source's damaged_set periodically to allow repeated hits
        if burning_dashing.tick % DAMAGE_INTERVAL_TICKS == 0 {
            if let Some(damage_entity) = burning_dashing.damage_entity {
                if let Ok(mut damage_source) =
                    damage_query.get_mut(damage_entity)
                {
                    damage_source.damaged_set.clear();
                }
            }
        }

        // Cancel if horizontal movement is stopped by collision
        if velocity.0.x.abs() < 0.01 {
            velocity.0 = Vec2::ZERO;
            // Despawn damage entity
            if let Some(damage_entity) = burning_dashing.damage_entity {
                commands.entity(damage_entity).despawn();
            }
            finish_burning_dash(&mut commands, entity);
        }
    }
}

fn finish_burning_dash(commands: &mut Commands, entity: Entity) {
    transition_action(commands, entity, Ready);
}

/// Ensure sustained BurningDash damage collider is removed if state is removed externally
fn burning_dashing_cleanup_on_remove(
    mut removed: RemovedComponents<BurningDashing>,
    mut commands: Commands,
    query: Query<(Entity, &ChildOf), With<BurningDashDamage>>,
    mut cooldowns: ResMut<crate::game::player::skills::cooldowns::Cooldowns>,
    time: Res<GameTime>,
) {
    for player_entity in removed.read() {
        super::super::queue_state_cleanup(
            &mut commands,
            player_entity,
            super::super::cleanup_burning_dash,
        );
        for (damage_entity, child_of) in query.iter() {
            if child_of.parent() == player_entity {
                commands.entity(damage_entity).despawn();
            }
        }
        // Start delayed cooldown for Burning Dash (rate-based)
        let now_tick = time.tick() as u64;
        use crate::game::player::skills::types::SkillId;
        let spec = burning_dash_metadata().delayed_cooldown;
        cooldowns.start(
            player_entity,
            SkillId::BurningDash,
            spec,
            1.0,
            now_tick,
        );
    }
}
