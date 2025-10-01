use bevy::prelude::*;

use crate::game::player::skills::types::SkillId;
use crate::game::player::Player;

/// Marker that a skill is channeled for as long as this component is present on the player.
/// Holds the logical skill id and a per-channel tick counter.
#[derive(Component, Debug, Clone, Copy)]
pub struct ChanneledSkill {
    pub skill: SkillId,
    pub held_ticks: u32,
}

impl ChanneledSkill {
    pub fn new(skill: SkillId) -> Self {
        Self {
            skill,
            held_ticks: 0,
        }
    }
}

/// Increments held ticks for all channeled skills and emits synthetic `SkillUsed` events
/// every 48 ticks after the first 48, so long channels count as repeats for rotation logic.
pub fn channeled_skill_tick_and_emit_repeats(
    mut query: Query<(Entity, &mut ChanneledSkill), With<Player>>,
    mut passive_events: EventWriter<
        crate::game::player::passives::PassiveEvent,
    >,
) {
    const THRESHOLD: u32 = 48;
    for (entity, mut ch) in query.iter_mut() {
        ch.held_ticks = ch.held_ticks.saturating_add(1);
        if ch.held_ticks > THRESHOLD && ch.held_ticks % THRESHOLD == 1 {
            passive_events.write(
                crate::game::player::passives::PassiveEvent::SkillUsed {
                    owner: entity,
                    skill: ch.skill,
                },
            );
        }
    }
}
