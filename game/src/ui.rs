// use of EntityCommands only occurs in commented code; import removed after Bevy 0.16 migration

use crate::assets::UiAssets;
use crate::prelude::*;
use crate::ui::kill_counter::KillCounterPlugin;
use crate::ui::skill_toolbar::SkillToolbarPlugin;
use bevy::ecs::system::EntityCommands;

pub mod ability_widget;
mod controls_overlay;
mod kill_counter;
mod mainmenu;
mod passives;
mod passive_inventory;
pub mod popup;
mod skill_toolbar;

#[cfg(not(feature = "release"))]
mod console;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            controls_overlay::plugin,
            popup::plugin,
            passives::plugin,
            self::passive_inventory::PassiveInventoryPlugin,
            self::mainmenu::MainMenuPlugin,
            SkillToolbarPlugin,
            KillCounterPlugin,
        ));
        #[cfg(not(feature = "release"))]
        app.add_plugins((
            self::console::UiConsolePlugin,
        ));
    }
}

fn spawn_menuentry(
    commands: &mut Commands,
    uiassets: &UiAssets,
    text: &'static str,
) -> Entity {
    let color_text = Color::WHITE;

    let butt = commands
        .spawn((
            BackgroundColor(Color::NONE),
            Node {
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                padding: UiRect::all(Val::Px(4.0)),
                margin: UiRect::all(Val::Px(4.0)),
                ..Default::default()
            },
        ))
        .id();

    let text = commands
        .spawn((
            Text(text.into()),
            TextColor(color_text),
            TextFont {
                font: uiassets.font_regular.clone(),
                font_size: 32.0,
                ..Default::default()
            },
        ))
        .id();

    commands.entity(butt).add_children(&[text]);

    butt
}

// For use in sickle_ui contexts, use like:
// ```rust
// button(
//     row, // any  &mut UiBuilder<Entity> type
//     OnClick::new().system(new_game),
//     "YourButtonTextHere",
//     style.clone(),
// );
// ```

// -------------------------------------------------------------------------------------------------
// A very small helper trait to abstract over the `spawn` method shared by `Commands` and
// `ChildSpawnerCommands`. In Bevy 0.16 both types expose an inherent `spawn` method with the same
// signature, so the implementation here is just a thin delegation that keeps existing call-sites
// intact while avoiding lifetime pitfalls that arose during the migration.

pub(crate) trait Spawn {
    fn spawn<B: Bundle>(&mut self, bundle: B) -> EntityCommands;
}

impl<'w, 's> Spawn for Commands<'w, 's> {
    #[inline]
    fn spawn<B: Bundle>(&mut self, bundle: B) -> EntityCommands {
        // Call the inherent `spawn` on `Commands` directly.
        Commands::spawn(self, bundle)
    }
}

// Support spawning in Bevy 0.16 child builder API.
impl<'w> Spawn for bevy::ecs::hierarchy::ChildSpawnerCommands<'w> {
    #[inline]
    fn spawn<B: Bundle>(&mut self, bundle: B) -> EntityCommands {
        // Calling the inherent `spawn` method directly via the alias to bypass the trait call.
        bevy::ecs::hierarchy::ChildSpawnerCommands::spawn(self, bundle)
    }
}
