use crate::game::player::foreground::{ActionMut, ActionTag};
use bevy::prelude::*;
use leafwing_input_manager::prelude::*;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::LinearVelocity;
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::{GameTickUpdate, GameTime};

use super::animation_hitbox;
use crate::game::combat::damage_source::Pushback;
use crate::game::combat::{DamageSource, Health, Stealthed};
use crate::game::gentstate::Facing;
use crate::game::player::player_anim::set_direction_slots;
use crate::game::player::skills::types::burning_dash_metadata;
use crate::game::player::states::BurningDashing;
use crate::game::player::weapon::Push;
use crate::game::player::{Player, PlayerAction, PlayerStateSet};

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
                .in_set(PlayerStateSet::Behavior)
                .in_set(crate::game::player::PlayerBehaviorSet::Execution),
        );
    }
}

// Burning Dash tuning
const BURNING_DASH_VELOCITY: f32 = 4.2; // ~500 px/s at 96 Hz
const HEALTH_COST_PER_TICK: u32 = 2; // Health reduced per tick
const MIN_HEALTH_THRESHOLD: u32 = 2; // Never go below this health
const DAMAGE_PER_HIT: f32 = 55.0;
const DAMAGE_INTERVAL_TICKS: u32 = 8; // Spawn damage collider every 8 ticks
const PUSHBACK: Push = Push { strength: 1.2, ticks: 14 }; // A little stronger than melee weapons

// On enter: initialize velocity and spawn sustained damage entity
fn burning_dashing_enter_system(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            ActionMut<BurningDashing>,
            &Facing,
            &mut LinearVelocity,
            &Gent,
            Has<crate::game::effects::stealthed::StealthEffect>,
        ),
        (With<Player>, Added<ActionTag<BurningDashing>>),
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
            burning_dashing.execution_mut().play(&mut script_player, burning_dash_metadata().animation_key);
        }

        // A sustained hitbox shaped by the animation; burning dash has its own impact sound.
        let damage_entity = commands
            .spawn((
                animation_hitbox(
                    entity,
                    gent,
                    DamageSource::new(u32::MAX, entity, DAMAGE_PER_HIT),
                    "BurningDashHit",
                ),
                Pushback(PUSHBACK.horizontal(facing.direction())),
            ))
            .insert_if(Stealthed, || is_stealthed)
            .id();
        burning_dashing.damage_entity = Some(damage_entity);
    }
}

// System that updates burning dashing state each tick
fn burning_dashing_update_system(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            ActionMut<BurningDashing>,
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
            burning_dash_still_pressed(&burning_dashing, action_state);

        // Exit conditions: button released or health too low
        if !still_pressed || health.current <= MIN_HEALTH_THRESHOLD {
            finish_burning_dash(
                &mut commands,
                entity,
                &mut velocity,
                burning_dashing.execution().id,
            );
            return;
        }

        burning_dashing.tick = burning_dashing.tick.saturating_add(1);
        apply_burning_dash_health_cost(&mut health);
        apply_burning_dash_velocity(&mut velocity, facing, &burning_dashing);
        refresh_burning_dash_damage_window(
            &mut damage_query,
            &burning_dashing,
        );

        if velocity.0.x.abs() < 0.01 {
            finish_burning_dash(
                &mut commands,
                entity,
                &mut velocity,
                burning_dashing.execution().id,
            );
        }
    }
}

fn burning_dash_still_pressed(
    burning_dashing: &BurningDashing,
    action_state: &ActionState<PlayerAction>,
) -> bool {
    burning_dashing
        .slot_action
        .is_some_and(|slot_action| action_state.pressed(&slot_action))
}

fn apply_burning_dash_health_cost(health: &mut Health) {
    if health.current <= MIN_HEALTH_THRESHOLD {
        return;
    }

    health.current = health
        .current
        .saturating_sub(HEALTH_COST_PER_TICK)
        .max(MIN_HEALTH_THRESHOLD);
}

fn apply_burning_dash_velocity(
    velocity: &mut LinearVelocity,
    facing: &Facing,
    burning_dashing: &BurningDashing,
) {
    let dir = facing.direction();
    velocity.0.x = dir * BURNING_DASH_VELOCITY * burning_dashing.speed_mod;
    velocity.0.y = 0.0;
}

fn refresh_burning_dash_damage_window(
    damage_query: &mut Query<&mut DamageSource>,
    burning_dashing: &BurningDashing,
) {
    if !burning_dashing.tick.is_multiple_of(DAMAGE_INTERVAL_TICKS) {
        return;
    }
    let Some(damage_entity) = burning_dashing.damage_entity else {
        return;
    };
    let Ok(mut damage_source) = damage_query.get_mut(damage_entity) else {
        return;
    };

    damage_source.damaged_set.clear();
}

fn finish_burning_dash(
    commands: &mut Commands,
    entity: Entity,
    velocity: &mut LinearVelocity,
    execution: u64,
) {
    velocity.0 = Vec2::ZERO;
    crate::game::player::foreground::finish_action(commands, entity, execution);
}
