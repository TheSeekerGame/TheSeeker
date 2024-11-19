use crate::graphics::hp_bar::HpBarUiMaterial;
use crate::prelude::*;
use bevy::ecs::system::{EntityCommand, EntityCommands};
use sickle_ui::ui_builder::UiBuilder;
use sickle_ui::ui_style::*;
use sickle_ui::widgets::column::UiColumnExt;
use sickle_ui::widgets::prelude::UiContainerExt;
use std::f32::consts::PI;

#[derive(Component)]
pub struct AbilityWidget;

pub trait UiAbilityWidgetExt<'w, 's> {
    fn ability_widget<'a, T: Component + Clone>(
        &'a mut self,
        config: AbilityWidgetConfig<T>,
    ) -> UiBuilder<'w, 's, 'a, Entity>;
}

impl<'w, 's> UiAbilityWidgetExt<'w, 's> for UiBuilder<'w, 's, '_, Entity> {
    /// Draws a 96.0x96.0 tile with a progress bar overlayed.
    /// modify
    fn ability_widget<'a, T: Component + Clone>(
        &'a mut self,
        config: AbilityWidgetConfig<T>,
    ) -> UiBuilder<'w, 's, 'a, Entity> {
        self.column(|column| {
            column.style().justify_content(JustifyContent::Center);
            column.container(
                (
                    ImageBundle::default(),
                    AbilityWidget,
                    config.tracking_component.clone(),
                ),
                |ability_card| {
                    ability_card.named("ability");
                    ability_card
                        .style()
                        .width(Val::Px(60.0))
                        .height(Val::Px(60.0))
                        .image(config.image_path);
                },
            );
            column.container(
                (
                    MaterialNodeBundle::<HpBarUiMaterial>::default(),
                    config.tracking_component,
                ),
                |ability_card| {
                    let entity = ability_card.context().clone();
                    // Adds the progress bar material to the ui node
                    // Someone tell me theres a better way then this. I mean it works at least
                    ability_card.commands().add(move |w: &mut World| {
                        let Some(mut ui_materials) =
                            w.get_resource_mut::<Assets<HpBarUiMaterial>>()
                        else {
                            return;
                        };
                        let handle = ui_materials.add(HpBarUiMaterial {
                            factor: 0.3,
                            background_color: Color::rgba(0.0, 0.0, 0.0, 0.0).into(),
                            filled_color: Color::rgba(0.0, 0.0, 0.0, 0.6).into(),
                        });
                        w.entity_mut(entity).insert(handle);
                        // Make the bar go from bottom to top
                        w.entity_mut(entity).insert(Transform::from_rotation(
                            Quat::from_axis_angle(
                                Vec3::Z,
                                if config.dir_up { -1.0 } else { 1.0 } * PI * 0.5,
                            ),
                        ));
                    });
                    ability_card.named("ability");
                    ability_card
                        .style()
                        .position_type(PositionType::Absolute)
                        .width(Val::Px(60.0))
                        .height(Val::Px(60.0));
                },
            );
        })
    }
}

pub struct AbilityWidgetConfig<T: Component + Clone> {
    pub image_path: String,
    pub tracking_component: T,
    pub dir_up: bool,
}

impl<T: Component + Clone> AbilityWidgetConfig<T> {
    pub fn from(image_path: impl Into<String>, tracking_component: T, dir_up: bool) -> Self {
        Self {
            image_path: image_path.into(),
            tracking_component,
            dir_up,
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
        let Some(material) = assets.get_mut(handle) else {
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
