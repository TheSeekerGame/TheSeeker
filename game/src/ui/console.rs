use bevy::color::palettes;
use bevy::input::keyboard::{Key, KeyboardInput};

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
                    BackgroundColor(palettes::css::BEIGE.into()),
                    Node {
                        position_type: PositionType::Absolute,
                        bottom: Val::Percent(5.0),
                        left: Val::Percent(5.0),
                        top: Val::Auto,
                        right: Val::Auto,
                        padding: UiRect::all(Val::Px(8.0)),
                        align_items: AlignItems::Center,
                        ..Default::default()
                    },
                ))
                .id();
            let prompt_prefix = commands
                .spawn((
                    Text("~ ".into()),
                    TextFont {
                        font: ui_assets.as_ref().unwrap().font_bold.clone(),
                        font_size: 24.0,
                        ..Default::default()
                    },
                    TextColor(palettes::css::RED.into()),
                ))
                .id();
            let prompt = commands
                .spawn((
                    UiConsolePrompt(console),
                    UiConsolePromptHistoryEntry(None),
                    Text("".into()),
                    TextFont {
                        font: ui_assets.as_ref().unwrap().font_regular.clone(),
                        font_size: 16.0,
                        ..Default::default()
                    },
                    TextColor(Color::BLACK),
                ))
                .id();
            commands
                .entity(console)
                .add_children(&[prompt_prefix, prompt]);
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
    mut evr_char: EventReader<KeyboardInput>,
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
            history.0.push(text.0.clone());
            commands.run_cli(&text.0);
            commands.entity(prompt.0).despawn_recursive();
        }
        evr_char.clear();
        return;
    }
    if kbd.just_pressed(KeyCode::Backspace) {
        for (mut text, mut hisentry, prompt) in &mut query {
            if text.0.is_empty() {
                commands.entity(prompt.0).despawn_recursive();
            }
            text.0.pop();
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
                text.0 = history.0[*i].clone();
            } else {
                let i = history.0.len() - 1;
                hisentry.0 = Some(i);
                text.0 = history.0[i].clone();
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
                text.0 = history.0[*i].clone();
            }
        }
        evr_char.clear();
        return;
    }
    for ev in evr_char.read() {
        for (mut text, mut hisentry, _) in &mut query {
            match &ev.logical_key {
                Key::Character(character) => {
                    text.0.push(character.chars().next().unwrap());
                },
                Key::Space => {
                    text.0.push(' ');
                },
                _ => {},
            }
            hisentry.0 = None;
        }
    }
}
