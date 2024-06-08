use crate::graphics::hp_bar::HpBarUiMaterial;
use crate::prelude::*;
use bevy::ecs::system::{EntityCommand, EntityCommands};
use sickle_ui::ui_builder::{UiBuilder, UiRoot};
use sickle_ui::ui_style::*;
use sickle_ui::widgets::prelude::{UiContainerExt, UiRowExt};

#[derive(Component)]
struct AbilityWidget;

pub trait UiAbilityWidgetExt<'w, 's> {
    fn ability_widget<'a>(
        &'a mut self,
        config: AbilityWidgetConfig,
    ) -> UiBuilder<'w, 's, 'a, Entity>;
}

impl<'w, 's> UiAbilityWidgetExt<'w, 's> for UiBuilder<'w, 's, '_, Entity> {
    fn ability_widget<'a>(
        &'a mut self,
        config: AbilityWidgetConfig,
    ) -> UiBuilder<'w, 's, 'a, Entity> {
        self.container(
            (
                MaterialNodeBundle::<HpBarUiMaterial>::default(), /*{
                                                                      style: Style {
                                                                          width: Val::Percent(100.0),
                                                                          height: Val::Percent(100.0),
                                                                          align_self: AlignSelf::Center,
                                                                          ..default()
                                                                      },
                                                                      //material: handle,
                                                                      ..default()
                                                                  },*/
                AbilityWidget,
            ),
            |ability_card| {
                let entity = ability_card.context().clone();
                // Someone tell me theres a better way then this. I mean it works at least
                ability_card.commands().add(move |w: &mut World| {
                    let Some(mut ui_materials) = w.get_resource_mut::<Assets<HpBarUiMaterial>>()
                    else {
                        return;
                    };
                    let handle = ui_materials.add(HpBarUiMaterial {
                        factor: 1.0,
                        background_color: Color::rgb(0.15, 0.15, 0.15).into(),
                        filled_color: Color::rgb(0.8, 0.2, 0.2).into(),
                    });
                    w.entity_mut(entity).insert(handle);
                });
                ability_card.named("ability");
                ability_card
                    .style()
                    .position_type(PositionType::Relative)
                    .width(Val::Px(96.0))
                    .height(Val::Px(96.0));
            },
        );
        self.row(|children| {
            children.container(
                (ImageBundle::default(), AbilityWidget),
                |ability_card| {
                    ability_card.named("ability");
                    ability_card
                        .style()
                        .position_type(PositionType::Relative)
                        .width(Val::Px(96.0))
                        .height(Val::Px(96.0))
                        .image(config.image_path);
                },
            );
        })
    }
}

pub struct AbilityWidgetConfig {
    pub image_path: String,
}

impl AbilityWidgetConfig {
    pub fn from(image_path: impl Into<String>) -> Self {
        Self {
            image_path: image_path.into(),
        }
    }
}

struct SetFactor(f32);

impl EntityCommand for SetFactor {
    fn apply(self, entity: Entity, world: &mut World) {
        let Some(handle) = world.entity(entity).get::<Handle<HpBarUiMaterial>>() else {
            return;
        };
        let handle = handle.clone();
        let Some(mut assets) = world.get_resource_mut::<Assets<HpBarUiMaterial>>() else {
            return;
        };
        let Some(mut material) = assets.get_mut(handle) else {
            return;
        };
        material.factor = self.0;
    }
}

pub trait AbilityWidgetCommands<'a> {
    fn factor(&'a mut self, factor: f32) -> &mut EntityCommands<'a>;
}

impl<'a> AbilityWidgetCommands<'a> for EntityCommands<'a> {
    fn factor(&'a mut self, factor: f32) -> &mut EntityCommands<'a> {
        self.add(SetFactor(factor))
    }
}

// Examples for oneshot updates, and dynamic updates of widgets
/*
struct SetFont(String, f32, Color);

impl EntityCommand for SetFont {
    fn apply(self, entity: Entity, world: &mut World) {
        let asset_server = world.resource::<AssetServer>();
        let font = asset_server.load(&self.0);

        if let Some(mut text) = world.entity_mut(entity).get_mut::<Text>() {
            for text_section in &mut text.sections {
                text_section.style.font = font.clone();
                text_section.style.font_size = self.1;
                text_section.style.color = self.2;
            }
        }
    }
}

pub trait AbilityWidgetCommands<'a> {
    fn font(
        &'a mut self,
        font: impl Into<String>,
        size: f32,
        color: Color,
    ) -> &mut EntityCommands<'a>;
}

impl<'a> AbilityWidgetCommands<'a> for EntityCommands<'a> {
    fn font(
        &'a mut self,
        font: impl Into<String>,
        size: f32,
        color: Color,
    ) -> &mut EntityCommands<'a> {
        self.add(SetFont(font.into(), size, color))
    }
}

fn update_fps(
    mut commands: Commands,
    diagnostics: Res<DiagnosticsStore>,
    label: Query<Entity, With<FpsText>>,
    asset_server: Res<AssetServer>,
) {
    for label in &label {
        let Some(fps_diagnostic) = diagnostics.get(&FrameTimeDiagnosticsPlugin::FPS) else {
            continue;
        };

        let Some(smoothed_fps) = fps_diagnostic.smoothed() else {
            continue;
        };

        // Target frame rate for 60 Hz monitors is actually slightly less than 60,
        // so we round down slightly to avoid flickering under happy circumstances.
        let text_color = if smoothed_fps > 59.5 {
            Color::GREEN
        } else if smoothed_fps > 30.0 {
            Color::YELLOW
        } else {
            Color::RED
        };

        let text_style = TextStyle {
            font: asset_server.load("FiraSans-Bold.ttf"),
            font_size: 60.0,
            color: text_color,
        };

        commands
            .entity(label)
            .set_text(format!("FPS: {:3.0}", smoothed_fps), text_style.into());
    }
}*/
