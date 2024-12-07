use sickle_ui::ui_builder::UiBuilder;
use sickle_ui::widgets::prelude::UiContainerExt;

use crate::assets::UiAssets;
use crate::locale::L10nKey;
use crate::prelude::*;
use crate::ui::kill_counter::KillCounterPlugin;
use crate::ui::skill_toolbar::SkillToolbarPlugin;

pub mod ability_widget;
mod console;
mod kill_counter;
mod mainmenu;
mod skill_toolbar;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(iyes_ui::UiExtrasPlugin);
        app.add_plugins((
            self::console::UiConsolePlugin,
            self::mainmenu::MainMenuPlugin,
            SkillToolbarPlugin,
            KillCounterPlugin,
        ));
    }
}

fn spawn_menuentry(
    commands: &mut Commands,
    uiassets: &UiAssets,
    behavior: OnClick,
    text: &'static str,
) -> Entity {
    let color_text = Color::WHITE;

    let butt = commands
        .spawn((
            behavior,
            ButtonBundle {
                background_color: BackgroundColor(Color::NONE),
                style: Style {
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    padding: UiRect::all(Val::Px(4.0)),
                    margin: UiRect::all(Val::Px(4.0)),
                    ..Default::default()
                },
                ..Default::default()
            },
        ))
        .id();

    let text = commands
        .spawn((
            L10nKey(text.to_owned()),
            TextBundle {
                text: Text::from_section(
                    text,
                    TextStyle {
                        color: color_text,
                        font_size: 32.0,
                        font: uiassets.font_regular.clone(),
                    },
                ),
                ..Default::default()
            },
        ))
        .id();

    commands.entity(butt).push_children(&[text]);

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
pub fn button<'w, 's, 'a>(
    parent: &'a mut UiBuilder<'w, 's, '_, Entity>,
    behavior: OnClick,
    text: &'static str,
    style: TextStyle,
) -> UiBuilder<'w, 's, 'a, Entity> {
    parent.container(
        (
            behavior,
            ButtonBundle {
                background_color: BackgroundColor(Color::NONE),
                style: Style {
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    padding: UiRect::all(Val::Px(4.0)),
                    margin: UiRect::all(Val::Px(4.0)),
                    ..Default::default()
                },
                ..Default::default()
            },
        ),
        |button| {
            button.spawn((
                L10nKey(text.to_owned()),
                TextBundle {
                    text: Text::from_section(text, style),
                    ..Default::default()
                },
            ));
        },
    )
}
