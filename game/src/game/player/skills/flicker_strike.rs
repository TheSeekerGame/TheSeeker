use crate::game::player::foreground::{ActionMut, ActionTag};
use bevy::prelude::*;

use super::types::FlickerStrikeMetadata;
use crate::game::player::passives::frenzied_attack::EnergyRegenDeltas;
use crate::game::player::states::{
    transition_action, FlickerPhase, FlickerStriking,
};
use crate::game::player::{
    FlickerAbility, Player, PlayerAction, PlayerStatMod,
};
use theseeker_engine::time::GameTime;

pub(crate) const FLICKER_STRIKE_METADATA: FlickerStrikeMetadata =
    FlickerStrikeMetadata {
        max_energy: 3.0,
        chunk_cost: 0.25,
        regen_per_second: 0.075,
        range: 150.0,
    };

pub(super) fn try_start_flicker_strike_slot(
    ctx: &mut super::registry::SkillActivationContext,
    world: &mut super::activation::ActivationWorld,
) -> bool {
    let requested = ctx.action_state.just_pressed(&ctx.slot)
        || ctx.buffer.check_buffered(ctx.slot, ctx.current_tick).is_some();
    // Needs energy for one hit and an enemy in range.
    if !requested || ctx.flicker_ability.energy < FLICKER_STRIKE_METADATA.chunk_cost {
        return false;
    }
    let player_pos = ctx.transform.translation.truncate();
    let enemy_in_range = world.enemies.iter().any(|(_, enemy)| {
        player_pos.distance(enemy.translation.truncate()) <= FLICKER_STRIKE_METADATA.range
    });
    if !enemy_in_range {
        return false;
    }

    ctx.buffer.clear_action(ctx.slot);
    let mut flickering = FlickerStriking::new(*ctx.melee_weapon);
    flickering.slot_action = Some(ctx.slot);
    transition_action(&mut world.instant.commands, ctx.entity, flickering);
    true
}

/// Regenerate flicker energy while not flickering
pub fn flicker_energy_regen(
    mut query: Query<
        (
            Entity,
            &mut FlickerAbility,
            Option<&PlayerStatMod>,
        ),
        (With<Player>, Without<ActionTag<FlickerStriking>>),
    >,
    mut energy_deltas: ResMut<EnergyRegenDeltas>,
    time: Res<GameTime>,
) {
    // Clear previous tick values once per tick
    energy_deltas.flicker.clear();
    for (entity, mut flicker_ability, stat_mod) in query.iter_mut() {
        let cdr = stat_mod.map(|s| s.cdr).unwrap_or(1.0);
        let regen_per_tick =
            FLICKER_STRIKE_METADATA.regen_per_second * cdr / time.hz as f32;
        let capacity = (FLICKER_STRIKE_METADATA.max_energy
            - flicker_ability.energy)
            .max(0.0);
        let applied = regen_per_tick.min(capacity);
        if applied > 0.0 {
            flicker_ability.energy += applied;
            energy_deltas.flicker.insert(entity, applied);
        }
    }
}

/// Ends the flicker strike once its button is released; the outro always plays out.
pub fn flicker_active_tick(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            ActionMut<FlickerStriking>,
            &leafwing_input_manager::action_state::ActionState<PlayerAction>,
        ),
        With<Player>,
    >,
) {
    for (entity, mut flickering, input) in &mut query {
        let held = flickering.slot_action.is_some_and(|slot| input.pressed(&slot));
        if flickering.phase != FlickerPhase::Outro && !held {
            request_flicker_outro(entity, &mut commands, &mut flickering);
        }
    }
}

fn request_flicker_outro(
    entity: Entity,
    commands: &mut Commands,
    flickering: &mut FlickerStriking,
) {
    // Do not cancel during dash or intro; defer outro until safe.
    if matches!(
        flickering.phase,
        FlickerPhase::Dashing | FlickerPhase::Intro
    ) {
        flickering.pending_outro = true;
        return;
    }

    // Not mid-dash/intro: safe to transition immediately.
    commands.entity(entity).insert(FlickerOutroTransition);
}

// Marker for transitioning to outro
#[derive(Component)]
pub struct FlickerOutroTransition;
