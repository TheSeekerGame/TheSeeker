use crate::game::player::{Passives, Player};
use crate::prelude::*;

use crate::game::pickups::PickupAssetHandles;

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
    pickup_assets: Res<PickupAssetHandles>,
    passives: Query<&Passives, With<Player>>,
    passives_ui_node: Query<Entity, With<PassivesUiNode>>,
    children_q: Query<&Children>,
) {
    // Remove the display since passives now show in the inventory window
    if let Ok(entity) = passives_ui_node.single() {
        if let Ok(children) = children_q.get(entity) {
            for child_entity in children.iter() {
                commands.entity(child_entity).despawn();
            }
        }
    }
}
