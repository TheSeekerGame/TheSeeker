use bevy::ecs::system::{EntityCommand, EntityCommands};
use bevy::ecs::world::EntityWorldMut;

use crate::graphics::ability_cooldown;
use crate::prelude::*;

#[derive(Component)]
pub struct AbilityWidget;


pub struct AbilityWidgetConfig<T: Component + Clone> {
    pub image_path: String,
    pub tracking_component: T,
    pub dir_up: bool,
}

impl<T: Component + Clone> AbilityWidgetConfig<T> {
    pub fn from(
        image_path: impl Into<String>,
        tracking_component: T,
        dir_up: bool,
    ) -> Self {
        Self {
            image_path: image_path.into(),
            tracking_component,
            dir_up,
        }
    }
}

/// Command to update the cooldown bar fill factor on a UI node's material.
struct SetFactor(f32);

impl EntityCommand for SetFactor {
    fn apply(self, mut entity: EntityWorldMut) {
        // Retrieve the progress material handle from this entity
        let Some(handle) =
            entity.get::<MaterialNode<ability_cooldown::Material>>()
        else {
            return;
        };
        let handle = handle.clone();

        // Update the underlying material asset in-place
        entity.world_scope(|world| {
            if let Some(mut assets) =
                world.get_resource_mut::<Assets<ability_cooldown::Material>>()
            {
                if let Some(material) = assets.get_mut(handle) {
                    material.factor = self.0;
                }
            }
        });
    }
}

pub trait AbilityWidgetCommands<'a> {
    fn factor(&'a mut self, factor: f32) -> &'a mut EntityCommands<'a>;
}

impl<'a> AbilityWidgetCommands<'a> for EntityCommands<'a> {
    fn factor(&'a mut self, factor: f32) -> &'a mut EntityCommands<'a> {
        self.queue(SetFactor(factor))
    }
}
