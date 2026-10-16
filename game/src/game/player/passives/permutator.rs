use bevy::prelude::*;


/// Tracks the last used skill and whether the last two differ.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct PermutatorState {
    pub last: Option<crate::game::player::skills::types::SkillId>,
    pub active: bool,
}

/// Listens for SkillUsed events to update the PermutatorState per player.
pub fn permutator_skill_tracker(
    mut events: MessageReader<super::PassiveEvent>,
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
        let PassiveEvent::SkillUsed { owner, skill } = *evt else {
            continue;
        };
        let Ok((entity, passives, maybe_state)) = players.get_mut(owner) else {
            continue;
        };
        if !passives.contains(&Passive::Permutator) {
            continue;
        }

        let mut state = maybe_state.as_deref().copied().unwrap_or_default();
        // Active only if we have a previous skill and it differs.
        state.active = state.last.map(|prev| prev != skill).unwrap_or(false);
        state.last = Some(skill);

        match maybe_state {
            Some(mut existing) => {
                *existing = state;
            },
            None => {
                commands.entity(entity).insert(state);
            },
        }
    }
}
