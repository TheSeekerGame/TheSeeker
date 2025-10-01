use crate::game::player::{Passives, Player};
use crate::prelude::*;

// Legacy simple passives list; UI is now provided by the inventory window.

pub(super) fn plugin(app: &mut App) {
    app.add_systems(
        OnEnter(AppState::InGame),
        spawn_passive_ui_container,
    );
    app.add_systems(
        GameTickUpdate,
        display_passives.run_if(in_state(AppState::InGame)),
    );
}

#[derive(Component)]
struct PassivesUiNode;

fn spawn_passive_ui_container(mut commands: Commands) {
    commands.spawn((
        Name::new("PassivesUi"),
        PassivesUiNode,
        Node {
            padding: UiRect::all(Val::Px(8.0)),
            row_gap: Val::Px(2.0),
            ..Default::default()
        },
        StateDespawnMarker,
    ));
}

fn display_passives(
    mut commands: Commands,
    _passives: Query<&Passives, With<Player>>,
    passives_ui_node: Query<Entity, With<PassivesUiNode>>,
    children_q: Query<&Children>,
) {
    // Clear legacy list entries; the inventory window is authoritative
    if let Ok(entity) = passives_ui_node.single() {
        if let Ok(children) = children_q.get(entity) {
            for child_entity in children.iter() {
                commands.entity(child_entity).despawn();
            }
        }
    }
}
