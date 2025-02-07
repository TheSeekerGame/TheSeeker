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
        display_passives.run_if(in_state(AppState::InGame).and_then(
            any_matching::<(Changed<Passives>, With<Player>)>(),
        )),
    );
}

#[derive(Component)]
struct PassivesUiNode;

fn spawn_passive_ui_container(mut commands: Commands) {
    commands.spawn((
        Name::new("PassivesUi"),
        PassivesUiNode,
        NodeBundle {
            style: Style {
                padding: UiRect::all(Val::Px(8.0)),
                row_gap: Val::Px(2.0),
                ..Default::default()
            },
            ..Default::default()
        },
        StateDespawnMarker,
    ));
}

// TODO: Improve this system by making it event driven
fn display_passives(
    mut commands: Commands,
    pickup_assets: Res<PickupAssetHandles>,
    passives: Query<&Passives, With<Player>>,
    passives_ui_node: Query<Entity, With<PassivesUiNode>>,
) {
    let Ok(passives) = passives.get_single() else {
        return;
    };

    if let Ok(entity) = passives_ui_node.get_single() {
        commands.entity(entity).despawn_descendants().with_children(
            |builder| {
                passives.iter().for_each(|passive| {
                    if let Some(handle) =
                        pickup_assets.get_passive_handle(passive)
                    {
                        builder.spawn(ImageBundle {
                            image: UiImage::new(handle.clone()),
                            style: Style {
                                width: Val::Px(64.0),
                                height: Val::Px(64.0),
                                ..Default::default()
                            },
                            ..Default::default()
                        });
                    }
                });
            },
        );
    }
}
