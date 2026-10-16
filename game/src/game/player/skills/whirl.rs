use crate::game::player::foreground::{ActionRef, ActionTag};
use bevy::prelude::*;

use crate::game::player::passives::frenzied_attack::EnergyRegenDeltas;
use crate::game::player::states::{transition_action, Whirling};
use crate::game::player::{Player, PlayerAction, PlayerStatMod, WhirlAbility};
use theseeker_engine::time::GameTime;

use super::types::WhirlMetadata;

pub(crate) const WHIRL_METADATA: WhirlMetadata = WhirlMetadata {
    max_energy: 2.0,
    min_energy_to_start: 0.5,
    drain_per_second: 1.0,
    regen_per_second: 0.2,
};

pub(super) fn try_start_whirl_slot(
    ctx: &mut super::registry::SkillActivationContext,
    world: &mut super::activation::ActivationWorld,
) -> bool {
    let commands = &mut world.instant.commands;
    // Start condition: just pressed or buffered
    let requested = ctx.action_state.just_pressed(&ctx.slot)
        || ctx.buffer.check_buffered(ctx.slot, ctx.current_tick).is_some();
    if !requested {
        return false;
    }

    // Energy gate
    if ctx.whirl_ability.energy < WHIRL_METADATA.min_energy_to_start {
        return false;
    }

    ctx.buffer.clear_action(ctx.slot);
    // Store which slot this whirl is on so we can check the right button during sustain
    let mut whirling = Whirling::new(*ctx.melee_weapon);
    whirling.slot_action = Some(ctx.slot);
    transition_action(commands, ctx.entity, whirling);
    true
}

/// Regenerate whirl energy while not whirling (scaled by CDR).
pub fn whirl_energy_regen(
    mut query: Query<
        (
            Entity,
            &mut WhirlAbility,
            Option<&PlayerStatMod>,
        ),
        (With<Player>, Without<ActionTag<Whirling>>),
    >,
    mut energy_deltas: ResMut<EnergyRegenDeltas>,
    time: Res<GameTime>,
) {
    // Clear previous tick values once per tick
    energy_deltas.whirl.clear();
    for (entity, mut whirl_ability, stat_mod) in query.iter_mut() {
        let cdr = stat_mod.map(|s| s.cdr).unwrap_or(1.0);
        let regen_per_tick =
            WHIRL_METADATA.regen_per_second * cdr / time.hz as f32;
        let capacity =
            (WHIRL_METADATA.max_energy - whirl_ability.energy).max(0.0);
        let applied = regen_per_tick.min(capacity);
        if applied > 0.0 {
            whirl_ability.energy += applied;
            energy_deltas.whirl.insert(entity, applied);
        }
    }
}

/// While whirling, drain energy and end when released past minimum or on energy exhaustion.
pub fn whirl_active_tick(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            ActionRef<Whirling>,
            &mut WhirlAbility,
            &leafwing_input_manager::action_state::ActionState<PlayerAction>,
            Option<&PlayerStatMod>,
        ),
        (With<Player>, With<ActionTag<Whirling>>),
    >,
    time: Res<GameTime>,
) {
    const MIN_TICKS: u32 = 48; // ~0.5s at 96 Hz
    for (entity, whirling, mut whirl_ability, input, stat_mod) in
        query.iter_mut()
    {
        // Compute drain per tick (reverse-cooldown model)
        let cdr = stat_mod.map(|s| s.cdr).unwrap_or(1.0).max(0.001);
        let energy_cost_per_tick =
            (WHIRL_METADATA.drain_per_second / time.hz as f32) / cdr;

        // Continue if held or not yet past minimum duration and we still have energy
        // Check the slot action that was used to start this whirl
        let button_held = whirling.slot_action.is_some_and(|action| input.pressed(&action));
        let past_minimum = whirling.tick >= MIN_TICKS;
        let has_energy = whirl_ability.energy > 0.0;

        if (button_held || !past_minimum) && has_energy {
            whirl_ability.energy =
                (whirl_ability.energy - energy_cost_per_tick).max(0.0);
            continue;
        }

        whirling.finish(&mut commands, entity);
    }
}
