use crate::assets::UiAssets;
use crate::prelude::*;

pub struct UiConsolePlugin;

impl Plugin for UiConsolePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (toggle_console, console_text_input),
        );
    }
}

#[derive(Component)]
struct UiConsole;
#[derive(Component)]
struct UiConsolePrompt(Entity);

fn toggle_console(
    mut commands: Commands,
    kbd: Res<Input<KeyCode>>,
    query_existing: Query<Entity, With<UiConsole>>,
    ui_assets: Option<Res<UiAssets>>,
    // mut input_switch: ResMut<InputSwitch>,
    // appstate: Res<State<AppState>>,
) {
    if kbd.just_pressed(KeyCode::Grave) {
        if query_existing.is_empty() {
            // spawn console
            let console = commands
                .spawn((
                    UiConsole,
                    NodeBundle {
                        style: Style {
                            position_type: PositionType::Absolute,
                            bottom: Val::Percent(5.0),
                            left: Val::Percent(5.0),
                            top: Val::Auto,
                            right: Val::Auto,
                            padding: UiRect::all(Val::Px(8.0)),
                            align_items: AlignItems::Center,
                            ..Default::default()
                        },
                        background_color: BackgroundColor(Color::BEIGE),
                        ..Default::default()
                    },
                ))
                .id();
            let prompt_style = if let Some(ui_assets) = &ui_assets {
                TextStyle {
                    font: ui_assets.font_bold.clone(),
                    font_size: 24.0,
                    color: Color::RED,
                }
            } else {
                TextStyle::default()
            };
            let input_style = if let Some(ui_assets) = &ui_assets {
                TextStyle {
                    font: ui_assets.font_regular.clone(),
                    font_size: 16.0,
                    color: Color::BLACK,
                }
            } else {
                TextStyle::default()
            };
            let prompt = commands
                .spawn((
                    UiConsolePrompt(console),
                    TextBundle {
                        text: Text::from_sections([
                            TextSection::new("~ ", prompt_style),
                            TextSection::new("", input_style),
                        ]),
                        ..Default::default()
                    },
                ))
                .id();
            commands.entity(console).push_children(&[prompt]);
            debug!("Console spawned.");
        } else {
            // despawn console
            for e in &query_existing {
                commands.entity(e).despawn_recursive();
            }
            debug!("Console despawned.");
        }
    }
}

/// Implement a simple "console" to type commands in
fn console_text_input(
    mut commands: Commands,
    mut evr_char: EventReader<ReceivedCharacter>,
    kbd: Res<Input<KeyCode>>,
    mut query: Query<(&mut Text, &UiConsolePrompt)>,
) {
    if kbd.just_pressed(KeyCode::Escape) {
        for (_, prompt) in &query {
            commands.entity(prompt.0).despawn_recursive();
        }
        evr_char.clear();
        return;
    }
    if kbd.just_pressed(KeyCode::Return) {
        for (text, prompt) in &query {
            commands.run_clicommand(&text.sections[1].value);
            commands.entity(prompt.0).despawn_recursive();
        }
        evr_char.clear();
        return;
    }
    if kbd.just_pressed(KeyCode::Back) {
        for (mut text, prompt) in &mut query {
            if text.sections[1].value.is_empty() {
                commands.entity(prompt.0).despawn_recursive();
            }
            text.sections[1].value.pop();
        }
        evr_char.clear();
        return;
    }
    for ev in evr_char.iter() {
        for (mut text, _) in &mut query {
            text.sections[1].value.push(ev.char);
        }
    }
}
