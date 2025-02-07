use leafwing_input_manager::prelude::ActionState;
use rapier2d::prelude::InteractionGroups;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::{Gent, TransformGfxFromGent};
use theseeker_engine::physics::{PhysicsWorld, PLAYER, SENSOR};
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::{animation::SpriteAnimationBundle, physics::Collider};

use crate::assets::DialogAssets;
use crate::camera::MainCamera;
use crate::prelude::*;
use crate::ui::popup::PopupUi;

use super::player::{Idle, Player, PlayerAction};

pub struct MerchantPlugin;

impl Plugin for MerchantPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<MerchantDialogueInteractionEvent>();

        app.add_systems(Startup, load_assets);

        app.add_systems(
            OnEnter(AppState::InGame),
            initialize_resources,
        );

        app.add_systems(
            GameTickUpdate,
            (
                setup_merchant
                    .run_if(any_matching::<Added<MerchantBlueprint>>()),
                merchant_proximity_to_player.run_if(any_matching::<(
                    With<Player>,
                    Without<Idle>,
                )>()),
                (
                    player_enters_merchant_range.run_if(
                        any_added_component::<MerchantInPlayerRange>.and_then(
                            not(any_with_component::<MerchantNonInteractable>),
                        ),
                    ),
                    player_in_merchant_range
                        .after(player_enters_merchant_range)
                        .run_if(
                            any_with_component::<MerchantInPlayerRange>
                                .and_then(not(any_with_component::<
                                    MerchantNonInteractable,
                                >)),
                        ),
                    player_leaves_merchant_range.run_if(
                        |removed: RemovedComponents<MerchantInPlayerRange>| {
                            !removed.is_empty()
                        },
                    ),
                )
                    .after(merchant_proximity_to_player),
                (
                    spawn_merchant_dialog_ui
                        .after(merchant_proximity_to_player)
                        .run_if(not(any_with_component::<
                            MerchantDialogueBox,
                        >)),
                    spawn_merchant_dialog_text
                        .after(spawn_merchant_dialog_ui)
                        .run_if(any_added_component::<MerchantDialogueBox>),
                    update_dialog_background.before(advance_dialog),
                    advance_dialog.after(spawn_merchant_dialog_text).run_if(
                        any_with_component::<MerchantDialogueBox>
                            .and_then(
                                any_with_component::<MerchantDialogueText>,
                            )
                            .and_then(
                                any_with_component::<MerchantInPlayerRange>,
                            ),
                    ),
                    handle_finished_dialogue_stage.after(advance_dialog),
                )
                    .after(player_in_merchant_range)
                    .run_if(on_event::<
                        MerchantDialogueInteractionEvent,
                    >()),
            )
                .run_if(
                    in_state(GameState::Playing)
                        .and_then(in_state(AppState::InGame)),
                ),
        );
    }
}

#[derive(Component, Default)]
pub struct MerchantBlueprint;

#[derive(Bundle, LdtkEntity, Default)]
pub struct MerchantBlueprintBundle {
    marker: MerchantBlueprint,
}

#[derive(Component)]
pub struct MerchantGfx {
    pub e_gent: Entity,
}

#[derive(Bundle)]
pub struct MerchantGfxBundle {
    marker: MerchantGfx,
    gent2gfx: TransformGfxFromGent,
    sprite: SpriteSheetBundle,
    animation: SpriteAnimationBundle,
}

#[derive(Component)]
struct MerchantTalkHint;

#[derive(Component)]
struct MerchantNonInteractable;

#[derive(Component)]
struct MerchantInPlayerRange;

#[derive(Resource, Default)]
enum MerchantDialogueStage {
    #[default]
    First,
    Second,
    Third,
}

impl MerchantDialogueStage {
    fn initial_step(&self) -> u8 {
        match self {
            Self::First => 0,
            Self::Second => 7,
            Self::Third => 13,
        }
    }

    fn last_step(&self) -> u8 {
        match self {
            Self::First => 6,
            Self::Second => 12,
            Self::Third => 19,
        }
    }
}

const MR_SNAFFLES_DIALOGS: [u8; 3] = [5, 9, 19];

#[derive(Resource, Default)]
struct MerchantDialogueCurrentStep(u8);

#[derive(Component)]
struct MerchantDialogueBox;

#[derive(Component)]
struct MerchantDialogueText;

#[derive(Event)]
struct MerchantDialogueInteractionEvent;

pub fn setup_merchant(
    mut q: Query<(&mut Transform, Entity), Added<MerchantBlueprint>>,
    mut commands: Commands,
) {
    for (mut xf_gent, e_gent) in q.iter_mut() {
        println!("added merchant");
        xf_gent.translation.z = 13.0 * 0.000001;
        println!("{:?}", xf_gent);
        let e_gfx = commands.spawn(()).id();
        let e_effects_gfx = commands.spawn_empty().id();
        commands
            .entity(e_gent)
            .insert((
                Name::new("Merchant"),
                Gent {
                    e_gfx,
                    e_effects_gfx,
                },
                Collider::cuboid(
                    40.0,
                    40.0,
                    InteractionGroups {
                        memberships: SENSOR,
                        filter: PLAYER,
                    },
                ),
            ))
            .remove_parent();
        let mut player = ScriptPlayer::<SpriteAnimation>::default();
        player.play_key("anim.merchant.Idle");
        commands.entity(e_gfx).insert((
            MerchantGfxBundle {
                marker: MerchantGfx { e_gent },
                gent2gfx: TransformGfxFromGent {
                    pixel_aligned: false,
                    gent: e_gent,
                },
                sprite: SpriteSheetBundle {
                    transform: *xf_gent,
                    ..Default::default()
                },
                animation: SpriteAnimationBundle { player },
            },
            StateDespawnMarker,
        ));
    }
}

fn merchant_proximity_to_player(
    mut commands: Commands,
    mut animation_query: Query<
        &mut ScriptPlayer<SpriteAnimation>,
        With<MerchantGfx>,
    >,
    merchant_query: Query<
        (
            Entity,
            &Gent,
            &GlobalTransform,
            &Collider,
        ),
        With<MerchantBlueprint>,
    >,
    spatial_query: Res<PhysicsWorld>,
) {
    let Ok((entity, gent, transform, collider)) = merchant_query.get_single()
    else {
        return;
    };
    let intersections = spatial_query.intersect(
        transform.translation().xy(),
        collider.0.shape(),
        collider.0.collision_groups(),
        Some(entity),
    );
    let is_player_nearby = !intersections.is_empty();

    if is_player_nearby {
        commands.entity(entity).insert(MerchantInPlayerRange);
    } else {
        commands.entity(entity).remove::<MerchantInPlayerRange>();
    }

    if let Ok(mut animation) = animation_query.get_mut(gent.e_gfx) {
        animation.set_slot("PlayerNearby", is_player_nearby);
    }
}

fn player_enters_merchant_range(
    mut commands: Commands,
    talk_hint: Query<Entity, With<MerchantTalkHint>>,
) {
    if talk_hint.is_empty() {
        commands
            .popup()
            .insert(MerchantTalkHint)
            .with_children(|popup| {
                popup.row().with_children(|row| {
                    row.text("Press ");
                    row.control_icon("F");
                    row.text(" to talk");
                });
            });
    }
}

fn player_in_merchant_range(
    mut event_writer: EventWriter<MerchantDialogueInteractionEvent>,
    player_query: Query<&ActionState<PlayerAction>, With<Player>>,
) {
    if let Ok(player_action) = player_query.get_single() {
        if player_action.just_pressed(&PlayerAction::Interact) {
            event_writer.send(MerchantDialogueInteractionEvent);
        }
    }
}

fn player_leaves_merchant_range(
    mut commands: Commands,
    mut dialogue_current_step: ResMut<MerchantDialogueCurrentStep>,
    query: Query<
        Entity,
        Or<(
            With<MerchantTalkHint>,
            With<MerchantDialogueBox>,
            With<MerchantDialogueText>,
        )>,
    >,
    dialogue_stage: Res<MerchantDialogueStage>,
) {
    for entity in &query {
        commands.entity(entity).despawn_recursive();
    }
    dialogue_current_step.0 = dialogue_stage.initial_step();
}

fn spawn_merchant_dialog_ui(
    mut commands: Commands,
    camera_query: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    dialog_assets: Res<DialogAssets>,
    images: Res<Assets<Image>>,
) {
    let Ok((camera, camera_transform)) = camera_query.get_single() else {
        return;
    };
    let viewport_size = camera.logical_viewport_size().unwrap_or_default();
    let image_size = images
        .get(dialog_assets.vagrant_background.clone_weak())
        .map(|image| image.size_f32())
        .unwrap_or_default();
    let viewport_position = Vec2::new(
        viewport_size.x / 2.0,
        image_size.y * 1.2,
    );
    let dialog_position = camera
        .viewport_to_world_2d(camera_transform, viewport_position)
        .unwrap_or_default();

    commands.spawn((
        MerchantDialogueBox,
        SpriteSheetBundle {
            sprite: Sprite {
                custom_size: Some(image_size * 0.5),
                ..Default::default()
            },
            texture: dialog_assets.vagrant_background.clone(),
            transform: Transform::from_translation(
                dialog_position.extend(500.0),
            ),
            ..Default::default()
        },
        StateDespawnMarker,
    ));
}

fn spawn_merchant_dialog_text(
    mut commands: Commands,
    camera_query: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    dialogue_current_step: Res<MerchantDialogueCurrentStep>,
    dialogue_asset_handles: Res<DialogueAssetHandles>,
    images: Res<Assets<Image>>,
) {
    let Ok((camera, camera_transform)) = camera_query.get_single() else {
        return;
    };
    let viewport_size = camera.logical_viewport_size().unwrap_or_default();
    let image_size = images
        .get(
            dialogue_asset_handles
                .get_handle(&dialogue_current_step.0)
                .unwrap_or(&Handle::default()),
        )
        .map(|image| image.size_f32())
        .unwrap_or_default();
    let viewport_position = Vec2::new(
        viewport_size.x / 2.0,
        image_size.y * 1.2,
    );
    let text_position = camera
        .viewport_to_world_2d(camera_transform, viewport_position)
        .unwrap_or_default();

    commands.spawn((
        MerchantDialogueText,
        SpriteSheetBundle {
            sprite: Sprite {
                custom_size: Some(image_size * 0.5),
                ..Default::default()
            },
            transform: Transform::from_translation(text_position.extend(501.0)),
            ..Default::default()
        },
    ));
}

fn advance_dialog(
    mut text_query: Query<
        (&mut Sprite, &mut Handle<Image>),
        With<MerchantDialogueText>,
    >,
    mut dialogue_current_step: ResMut<MerchantDialogueCurrentStep>,
    dialogue_asset_handles: Res<DialogueAssetHandles>,
    dialogue_stage: Res<MerchantDialogueStage>,
    images: Res<Assets<Image>>,
) {
    if dialogue_current_step.0 <= dialogue_stage.last_step() {
        for (mut sprite, mut image) in &mut text_query {
            let default_handle = Handle::default();
            let image_handle = dialogue_asset_handles
                .get_handle(&dialogue_current_step.0)
                .unwrap_or(&default_handle);
            let image_size = images
                .get(image_handle)
                .map(|image| image.size_f32())
                .unwrap_or_default();

            sprite.custom_size = Some(image_size * 0.5);
            *image = image_handle.clone();
        }
        dialogue_current_step.0 += 1;
    }
}

fn handle_finished_dialogue_stage(
    mut commands: Commands,
    merchant_query: Query<Entity, With<MerchantBlueprint>>,
    query: Query<
        Entity,
        Or<(
            With<MerchantTalkHint>,
            With<MerchantDialogueBox>,
            With<MerchantDialogueText>,
        )>,
    >,
    dialogue_current_step: Res<MerchantDialogueCurrentStep>,
    dialogue_stage: Res<MerchantDialogueStage>,
) {
    if dialogue_current_step.0 > dialogue_stage.last_step() {
        if let Ok(entity) = merchant_query.get_single() {
            commands.entity(entity).insert(MerchantNonInteractable);
            for entity in &query {
                commands.entity(entity).despawn_recursive();
            }
        }
    }
}

fn update_dialog_background(
    mut query: Query<&mut Handle<Image>, With<MerchantDialogueBox>>,
    dialogue_current_step: Res<MerchantDialogueCurrentStep>,
    dialog_assets: Res<DialogAssets>,
) {
    if let Ok(mut image) = query.get_single_mut() {
        *image = if MR_SNAFFLES_DIALOGS.contains(&dialogue_current_step.0) {
            dialog_assets.mr_snaffles_background.clone()
        } else {
            dialog_assets.vagrant_background.clone()
        }
    }
}

fn initialize_resources(mut commands: Commands) {
    commands.insert_resource(MerchantDialogueStage::default());
    commands.insert_resource(MerchantDialogueCurrentStep::default());
}

#[derive(Resource, Deref)]
pub struct DialogueAssetHandles(HashMap<u8, Handle<Image>>);

impl DialogueAssetHandles {
    pub fn get_handle(&self, value: &u8) -> Option<&Handle<Image>> {
        self.get(value)
    }
}

pub fn load_assets(assets: Res<AssetServer>, mut commands: Commands) {
    let asset_mappings: Vec<(u8, &str)> = vec![
        (0, "animations/dialogue/000_1f.png"),
        (1, "animations/dialogue/001_1f.png"),
        (2, "animations/dialogue/002_1f.png"),
        (3, "animations/dialogue/003_1f.png"),
        (4, "animations/dialogue/004_1f.png"),
        (5, "animations/dialogue/005_1f.png"),
        (6, "animations/dialogue/006_1f.png"),
        (7, "animations/dialogue/007_1f.png"),
        (8, "animations/dialogue/008_1f.png"),
        (9, "animations/dialogue/009_1f.png"),
        (10, "animations/dialogue/010_1f.png"),
        (11, "animations/dialogue/011_1f.png"),
        (12, "animations/dialogue/012_1f.png"),
        (13, "animations/dialogue/013_1f.png"),
        (14, "animations/dialogue/014_1f.png"),
        (15, "animations/dialogue/015_1f.png"),
        (16, "animations/dialogue/016_1f.png"),
        (17, "animations/dialogue/017_1f.png"),
        (18, "animations/dialogue/018_1f.png"),
        (19, "animations/dialogue/019_1f.png"),
    ];

    commands.insert_resource(DialogueAssetHandles(
        HashMap::from_iter(
            asset_mappings
                .iter()
                .map(|(x, y)| (*x, assets.load(*y)))
                .collect::<Vec<_>>(),
        ),
    ));
}
