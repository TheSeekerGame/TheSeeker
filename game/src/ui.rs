use bevy::ecs::system::EntityCommands;

use crate::assets::UiAssets;
use crate::locale::L10nKey;
use crate::prelude::*;
use crate::ui::kill_counter::KillCounterPlugin;
use crate::ui::skill_toolbar::SkillToolbarPlugin;

pub mod ability_widget;
mod controls_overlay;
mod kill_counter;
mod mainmenu;
mod passives;
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
            L10nKey(text.to_owned()),
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

/// For use in sickle_ui contexts, use like:
/// ```rust
/// button(
///     row, // any  &mut UiBuilder<Entity> type
///     OnClick::new().system(new_game),
///     "YourButtonTextHere",
///     style.clone(),
/// );
/// ```
// pub fn button<'w, 's, 'a>(
//     parent: &'a mut UiBuilder<'w, 's, '_, Entity>,
//     behavior: OnClick,
//     text: &'static str,
//     style: TextStyle,
// ) -> UiBuilder<'w, 's, 'a, Entity> {
//     parent.container(
//         (
//             behavior,
//             ButtonBundle {
//                 background_color: BackgroundColor(Color::NONE),
//                 style: Style {
//                     justify_content: JustifyContent::Center,
//                     align_items: AlignItems::Center,
//                     padding: UiRect::all(Val::Px(4.0)),
//                     margin: UiRect::all(Val::Px(4.0)),
//                     ..Default::default()
//                 },
//                 ..Default::default()
//             },
//         ),
//         |button| {
//             button.spawn((
//                 L10nKey(text.to_owned()),
//                 TextBundle {
//                     text: Text::from_section(text, style),
//                     ..Default::default()
//                 },
//             ));
//         },
//     )
// }

trait Spawn {
    fn spawn<B: Bundle>(&mut self, bundle: B) -> EntityCommands;
}

impl Spawn for Commands<'_, '_> {
    fn spawn<B: Bundle>(&mut self, bundle: B) -> EntityCommands {
        Commands::spawn(self, bundle)
    }
}

impl Spawn for ChildBuilder<'_> {
    fn spawn<B: Bundle>(&mut self, bundle: B) -> EntityCommands {
        theseeker_engine::prelude::ChildBuild::spawn(self, bundle)
    }
}
