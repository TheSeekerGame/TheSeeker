use bevy::prelude::*;

use super::PassiveContext;
use crate::game::player::PlayerStatMod;

/// Tracks the last used skill and whether the last two differ.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct PermutatorState {
    pub last: Option<crate::game::player::skills::types::SkillId>,
    pub active: bool,
}

/// Listens for SkillUsed events to update the PermutatorState per player.
pub fn permutator_skill_tracker(
    mut events: EventReader<super::PassiveEvent>,
    mut players: Query<
        (
            Entity,
            &super::super::Passives,
            Option<&mut PermutatorState>,
        ),
        With<super::super::Player>,
    >,
    mut commands: Commands,
) {
    use super::PassiveEvent;
    use crate::game::player::Passive;

    for evt in events.read() {
        if let PassiveEvent::SkillUsed { owner, skill } = *evt {
            if let Ok((entity, passives, maybe_state)) = players.get_mut(owner)
            {
                if !passives.contains(&Passive::Permutator) {
                    continue;
                }
                let mut state =
                    maybe_state.as_deref().copied().unwrap_or_default();
                // Active only if we have a previous skill and it differs
                state.active = match state.last {
                    Some(prev) => prev != skill,
                    None => false,
                };
                state.last = Some(skill);
                match maybe_state {
                    Some(mut s) => {
                        *s = state;
                    },
                    None => {
                        commands.entity(entity).insert(state);
                    },
                }
            }
        }
    }
}

// Repeats for channeled skills are emitted in skills::channeled

/// Permutator: when rotation_active is true, double CDR (halves cooldowns, affects energy model).
pub struct PermutatorEffect;

impl super::PassiveEffect for PermutatorEffect {
    fn modify_stats(
        &self,
        stats: &mut PlayerStatMod,
        context: &PassiveContext,
    ) {
        if context.rotation_active {
            stats.cdr *= 2.0;
        }
    }

    fn priority(&self) -> i32 {
        5
    }
    fn name(&self) -> &'static str {
        "Permutator"
    }
}
