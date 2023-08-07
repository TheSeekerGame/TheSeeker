use crate::assets::UiAssets;
use crate::locale::L10nKey;
use crate::prelude::*;

mod console;
mod mainmenu;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(iyes_ui::UiExtrasPlugin);
        app.add_plugins((
            self::console::UiConsolePlugin,
            self::mainmenu::MainMenuPlugin,
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
