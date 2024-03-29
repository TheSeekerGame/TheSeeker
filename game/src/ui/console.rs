use crate::assets::UiAssets;
use crate::prelude::*;

pub struct UiConsolePlugin;

impl Plugin for UiConsolePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ConsoleCommandHistory>();
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
#[derive(Component)]
struct UiConsolePromptHistoryEntry(Option<usize>);

#[derive(Resource, Default)]
struct ConsoleCommandHistory(Vec<String>);

fn toggle_console(
    mut commands: Commands,
    kbd: Res<ButtonInput<KeyCode>>,
    query_existing: Query<Entity, With<UiConsole>>,
    ui_assets: Option<Res<UiAssets>>,
    // mut input_switch: ResMut<InputSwitch>,
    // appstate: Res<State<AppState>>,
) {
    if kbd.just_pressed(KeyCode::Backquote) {
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
                    UiConsolePromptHistoryEntry(None),
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
    kbd: Res<ButtonInput<KeyCode>>,
    mut query: Query<(
        &mut Text,
        &mut UiConsolePromptHistoryEntry,
        &UiConsolePrompt,
    )>,
    mut history: ResMut<ConsoleCommandHistory>,
) {
    if kbd.just_pressed(KeyCode::Escape) {
        for (_, _, prompt) in &query {
            commands.entity(prompt.0).despawn_recursive();
        }
        evr_char.clear();
        return;
    }
    if kbd.just_pressed(KeyCode::Enter) {
        for (text, _, prompt) in &query {
            history.0.push(text.sections[1].value.clone());
            commands.run_clicommand(&text.sections[1].value);
            commands.entity(prompt.0).despawn_recursive();
        }
        evr_char.clear();
        return;
    }
    if kbd.just_pressed(KeyCode::Backspace) {
        for (mut text, mut hisentry, prompt) in &mut query {
            if text.sections[1].value.is_empty() {
                commands.entity(prompt.0).despawn_recursive();
            }
            text.sections[1].value.pop();
            hisentry.0 = None;
        }
        evr_char.clear();
        return;
    }
    if kbd.just_pressed(KeyCode::ArrowUp) {
        for (mut text, mut hisentry, _) in &mut query {
            if let Some(i) = hisentry.0.as_mut() {
                if *i > 0 {
                    *i -= 1;
                }
                text.sections[1].value = history.0[*i].clone();
            } else {
                let i = history.0.len() - 1;
                hisentry.0 = Some(i);
                text.sections[1].value = history.0[i].clone();
            }
        }
        evr_char.clear();
        return;
    }
    if kbd.just_pressed(KeyCode::ArrowDown) {
        for (mut text, mut hisentry, _) in &mut query {
            if let Some(i) = hisentry.0.as_mut() {
                if *i < history.0.len() - 1 {
                    *i += 1;
                }
                text.sections[1].value = history.0[*i].clone();
            }
        }
        evr_char.clear();
        return;
    }
    for ev in evr_char.read() {
        for (mut text, mut hisentry, _) in &mut query {
            text.sections[1].value.push(ev.char.chars().next().unwrap());
            hisentry.0 = None;
        }
    }
}
