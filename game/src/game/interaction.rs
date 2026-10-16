//! Proximity for NPCs and world objects, and entering shelters with the interact action.
use leafwing_input_manager::prelude::ActionState;
use theseeker_engine::{assets::animation::SpriteAnimation, gent::Gent, script::ScriptPlayer};
use theseeker_engine::physics::{Collider, PhysicsSet};
use crate::prelude::*;
use super::{
    npc::{NpcKind, setup_npcs},
    player::{Player, PlayerAction, skills::portable_tent::PendingTentEntry},
    world_objects::WorldObjectKind,
};

#[derive(Component)]
struct InInteractionRange;

/// The nearest shelter in range.
#[derive(Resource, Default)]
pub(crate) struct InteractionFocus(Option<(Entity, WorldObjectKind)>);

impl InteractionFocus {
    pub(crate) fn has_target(&self) -> bool {
        self.0.is_some()
    }
}

pub(crate) struct InteractionPlugin;
impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InteractionFocus>();
        app.add_systems(GameTickUpdate, (
            setup_npcs,
            (update_proximity, select_focus, activate_interaction).chain(),
        ).after(PhysicsSet)
            .run_if(in_state(GameState::Playing).and_then(in_state(AppState::InGame))));
    }
}

fn update_proximity(
    mut commands: Commands,
    players: Query<(&GlobalTransform, &Collider), With<Player>>,
    objects: Query<(Entity, &GlobalTransform, &Collider, Option<&NpcKind>, Option<&Gent>),
        Or<(With<NpcKind>, With<WorldObjectKind>)>>,
    mut animations: Query<&mut ScriptPlayer<SpriteAnimation>>,
) {
    let player = players.single().ok();
    for (entity, transform, collider, npc, gent) in &objects {
        let nearby = player.is_some_and(|(player_transform, player_collider)| {
            colliders_in_range(player_transform.translation().truncate(), player_collider,
                transform.translation().truncate(), collider)
        });
        if nearby {
            commands.entity(entity).insert(InInteractionRange);
        } else {
            commands.entity(entity).remove::<InInteractionRange>();
        }
        if npc != Some(&NpcKind::Vagrant) { continue; }
        let Some(gent) = gent else { continue; };
        let Ok(mut animation) = animations.get_mut(gent.e_gfx) else { continue; };
        animation.set_slot("PlayerNearby", nearby);
    }
}

fn colliders_in_range(player: Vec2, player_collider: &Collider, object: Vec2, collider: &Collider) -> bool {
    let half_extents = |collider: &Collider| collider.cuboid_half_extents()
        .unwrap_or_else(|| collider.half_extents());
    let delta = (player - object).abs();
    let extent = half_extents(player_collider) + half_extents(collider);
    delta.x <= extent.x && delta.y <= extent.y
}

fn select_focus(
    players: Query<&GlobalTransform, With<Player>>,
    objects: Query<(Entity, &GlobalTransform, &WorldObjectKind), With<InInteractionRange>>,
    mut focus: ResMut<InteractionFocus>,
) {
    let player = players.single().ok().map(|player| player.translation().truncate());
    focus.0 = player.and_then(|player| {
        objects
            .iter()
            .filter(|(_, _, kind)| kind.interior_kind().is_some())
            .map(|(entity, transform, kind)| (entity, *kind, transform.translation().truncate().distance_squared(player)))
            .min_by(|(.., a), (.., b)| a.total_cmp(b))
            .map(|(entity, kind, _)| (entity, kind))
    });
}

fn activate_interaction(
    mut commands: Commands,
    players: Query<(Entity, &ActionState<PlayerAction>, &GlobalTransform),
        (With<Player>, Without<crate::game::spikes::SpikeRespawnPending>)>,
    focus: Res<InteractionFocus>,
) {
    let Ok((player, actions, transform)) = players.single() else { return; };
    let Some((shelter, kind)) = focus.0.filter(|_| actions.just_pressed(&PlayerAction::Interact)) else { return; };
    let Some(kind) = kind.interior_kind() else { return; };
    commands.entity(player).insert(PendingTentEntry { shelter, kind, return_position: transform.translation().truncate() });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proximity_is_inclusive_rectangular_overlap() {
        let player = Collider::cuboid(2.0, 3.0);
        let npc = Collider::cuboid(14.0, 14.0);
        assert!(colliders_in_range(Vec2::ZERO, &player, Vec2::new(16.0, 17.0), &npc));
        assert!(!colliders_in_range(Vec2::ZERO, &player, Vec2::new(16.01, 17.0), &npc));
    }
}
