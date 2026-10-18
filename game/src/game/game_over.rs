//! The death menu: a pixel panel over the fading screen that offers a new expedition from
//! the capsule or abandoning the planet for the main menu.
use bevy::prelude::UiTargetCamera as TargetCamera;
use theseeker_engine::prelude::{in_state, resource_added, resource_exists, Color, GameTickUpdate, GameTime, Resource};

use crate::camera::{MainCamera, SnapCameraNext};
use crate::game::combat::KillCount;
use crate::game::player::PlayerStateSet;
use crate::gamestate::GameState;
use crate::prelude::*;
use crate::ui::mainmenu::MenuState;
use crate::ui::pixel_canvas::{rect, text_width, Art, ArtLibrary, Canvas, Raster};
use crate::ui::pixel_font::FONT_IMAGE_PATH;
use crate::ui::pixel_panel::{draw_frame, key_hint, show_raster, vertical_delta, Accent, PanelScroll, UiPixelGrid, MUTED, TEXT, WELL_FILL};
use crate::ui::UiInputSet;

const WIDTH: i32 = 142;
const MARGIN: i32 = 7;
const FIRST_STAT: i32 = 19;
const STAT_ROW: i32 = 11;
const FIRST_BUTTON: i32 = 46;
const BUTTON_HEIGHT: i32 = 17;
const HINT_TOP: i32 = FIRST_BUTTON + GameOverAction::ALL.len() as i32 * BUTTON_HEIGHT + 9;
const HEIGHT: i32 = HINT_TOP + 13;
const ACCENT: Accent = Accent { bright: Color::srgb(0.86, 0.53, 0.50), dim: Color::srgb(0.34, 0.17, 0.19) };
/// The screen behind the menu darkens to this opacity over two seconds.
const FADE_OPACITY: f32 = 0.77;
const FADE_SECONDS: f32 = 2.0;

/// A plugin that handles when the player has a game over
pub struct GameOverPlugin;

impl Plugin for GameOverPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            on_game_over
                .run_if(in_state(GameState::Playing))
                .after(PlayerStateSet::Animation)
                .run_if(in_state(AppState::InGame))
                .run_if(resource_added::<GameOver>),
        );
        app.add_systems(
            PreUpdate,
            handle_game_over_input
                .after(UiInputSet::Menu)
                .run_if(in_state(AppState::InGame).and_then(resource_exists::<GameOver>).and_then(resource_exists::<GameOverMenu>)),
        );
        app.add_systems(
            Update,
            (
                update_fade_in,
                present_game_over_menu
                    .after(crate::ui::sync_ui_scale)
                    .run_if(in_state(AppState::InGame).and_then(resource_exists::<GameOver>).and_then(resource_exists::<GameOverMenu>)),
            ),
        );
    }
}

/// Fades the screen to a target opacity using master clock seconds.
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct FadeIn {
    progress: f32,
}

pub fn update_fade_in(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut query: Query<(
        Entity,
        &mut BackgroundColor,
        &mut FadeIn,
    )>,
) {
    for (entity, mut bg_color, mut fade_in) in query.iter_mut() {
        fade_in.progress += time.delta_secs() / FADE_SECONDS;
        fade_in.progress = fade_in.progress.clamp(0.0, 1.0);
        bg_color.0.set_alpha(fade_in.progress * FADE_OPACITY);
        if fade_in.progress >= 1.0 {
            commands.entity(entity).remove::<FadeIn>();
        }
    }
}

/// Inserted on player despawn
#[derive(Resource)]
pub struct GameOver;

/// The death menu's selection and the results of the run that ended.
#[derive(Resource, Debug, Default)]
struct GameOverMenu {
    selected: usize,
    kills: u32,
    score: u32,
    scroll: PanelScroll,
}

/// Marker for entities belonging to the game over UI so they can be cleaned.
#[derive(Component, Default)]
struct GameOverUi;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum GameOverAction {
    NewExpedition,
    AbandonPlanet,
}

impl GameOverAction {
    const ALL: [Self; 2] = [Self::NewExpedition, Self::AbandonPlanet];

    fn label(self) -> &'static str {
        match self {
            Self::NewExpedition => "NEW EXPEDITION",
            Self::AbandonPlanet => "ABANDON PLANET",
        }
    }
}

pub fn on_game_over(
    q_cam: Query<Entity, With<MainCamera>>,
    mut commands: Commands,
    mut kill_count: ResMut<KillCount>,
    time: Res<GameTime>,
) {
    let Ok(cam_e) = q_cam.single() else {
        return;
    };

    commands.spawn((
        Name::new("GameOverFade"),
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(Color::srgba_u8(0, 0, 0, 0)),
        // This ensures we draw the ui above all other uis
        GlobalZIndex(i32::MAX - 1000),
        FadeIn { progress: 0.0 },
        TargetCamera(cam_e),
        StateDespawnMarker,
        GameOverUi,
    ));

    let score = (f64::from(kill_count.0) / time.time_in_seconds().max(f64::EPSILON)) * 100.0;
    commands.insert_resource(GameOverMenu { kills: kill_count.0, score: score as u32, ..default() });
    kill_count.0 = 0;
}

#[expect(clippy::too_many_arguments, reason = "The death menu translates frame input into app lifecycle commands.")]
fn handle_game_over_input(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    pause_menu: Res<MenuState>,
    mut menu: ResMut<GameOverMenu>,
    grid: UiPixelGrid,
    q_ui: Query<Entity, With<GameOverUi>>,
    mut next_app_state: ResMut<NextState<AppState>>,
    mut next_game_state: ResMut<NextState<GameState>>,
) {
    if pause_menu.blocks_panel_input() {
        return;
    }
    let last = GameOverAction::ALL.len() - 1;
    menu.selected = menu.selected.saturating_add_signed(vertical_delta(&keys) as isize).min(last);
    let clicked = game_over_pointer(&grid, &menu).filter(|_| mouse.just_pressed(MouseButton::Left));
    let confirmed = keys.any_just_pressed([KeyCode::KeyF, KeyCode::Enter, KeyCode::Space]).then_some(menu.selected);
    let Some(index) = clicked.or(confirmed) else { return };

    for entity in &q_ui {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<GameOver>();
    commands.remove_resource::<GameOverMenu>();
    match GameOverAction::ALL[index] {
        GameOverAction::NewExpedition => {
            commands.insert_resource(SnapCameraNext);
            next_game_state.set(GameState::Playing);
        },
        GameOverAction::AbandonPlanet => next_app_state.set(AppState::MainMenu),
    }
}

/// The button under the cursor, if any.
fn game_over_pointer(grid: &UiPixelGrid, menu: &GameOverMenu) -> Option<usize> {
    let cursor = grid.cursor_art()?;
    let available = grid.available_art()?;
    let shown = UVec2::new(WIDTH as u32, HEIGHT as u32).min(available);
    let origin = ((available - shown) / 2).as_vec2();
    let point = cursor - origin + menu.scroll.offset.as_vec2();
    if point.x < MARGIN as f32 || point.x >= (WIDTH - MARGIN) as f32 || point.y < FIRST_BUTTON as f32 {
        return None;
    }
    let index = (point.y as i32 - FIRST_BUTTON) / BUTTON_HEIGHT;
    (index < GameOverAction::ALL.len() as i32).then_some(index as usize)
}

#[expect(clippy::too_many_arguments, reason = "The death menu publishes its raster on the current UI grid.")]
fn present_game_over_menu(
    mut commands: Commands,
    mut menu: ResMut<GameOverMenu>,
    keys: Res<ButtonInput<KeyCode>>,
    mut library: ResMut<ArtLibrary>,
    server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut panel: Query<(Entity, &ImageNode, &mut Node), With<GameOverUi>>,
    grid: UiPixelGrid,
) {
    library.load(&server, [FONT_IMAGE_PATH]);
    let raster = draw_game_over(&menu, &library.art(&images));
    let focus = button_rect(menu.selected);
    let content = grid.scrollable(raster, &mut menu.scroll, &keys, Some(focus), Vec2::splat(0.5));
    show_raster(&mut commands, &mut images, panel.single_mut().ok(), content, |image, node| {
        (Name::new("GameOverMenu"), GameOverUi, image, node, GlobalZIndex(i32::MAX - 999), StateDespawnMarker)
    });
}

fn button_rect(index: usize) -> IRect {
    rect(MARGIN, FIRST_BUTTON + index as i32 * BUTTON_HEIGHT, WIDTH - 2 * MARGIN, BUTTON_HEIGHT - 2)
}

fn draw_game_over(menu: &GameOverMenu, art: &Art) -> Raster {
    let mut canvas = Canvas::new(UVec2::new(WIDTH as u32, HEIGHT as u32), art);
    draw_frame(&mut canvas, IVec2::new(WIDTH, HEIGHT), ACCENT, &[14, FIRST_BUTTON - 5, HINT_TOP - 5]);
    let title = "GAME OVER";
    canvas.text(IVec2::new((WIDTH - text_width(title)) / 2, 4), title, ACCENT.bright);
    for (index, (label, value)) in [("KILLS", menu.kills), ("SCORE", menu.score)].into_iter().enumerate() {
        let y = FIRST_STAT + index as i32 * STAT_ROW;
        canvas.text(IVec2::new(MARGIN, y), label, MUTED);
        let value = value.to_string();
        canvas.text(IVec2::new(WIDTH - MARGIN - text_width(&value), y), &value, TEXT);
    }
    for (index, action) in GameOverAction::ALL.into_iter().enumerate() {
        let button = button_rect(index);
        let selected = index == menu.selected;
        canvas.plate(button, 1, if selected { ACCENT.dim } else { WELL_FILL }, ACCENT.dim);
        let label = action.label();
        canvas.text(IVec2::new((WIDTH - text_width(label)) / 2, button.min.y + 4), label, if selected { TEXT } else { MUTED });
    }
    key_hint(&mut canvas, IVec2::new(MARGIN, HINT_TOP), "F", "SELECT", ACCENT.bright);
    canvas.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn death_menu_systems_have_compatible_parameters() {
        let mut schedule = Schedule::default();
        schedule.add_systems((on_game_over, handle_game_over_input, present_game_over_menu));
        schedule.initialize(&mut World::new()).expect("death menu systems initialize");
    }

    fn confirm(selected: usize) -> World {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::KeyF);
        world.insert_resource(keys);
        world.init_resource::<ButtonInput<MouseButton>>();
        world.init_resource::<MenuState>();
        world.init_resource::<crate::camera::GameplayPixelScale>();
        world.init_resource::<bevy::ui::UiScale>();
        world.init_resource::<NextState<AppState>>();
        world.init_resource::<NextState<GameState>>();
        world.insert_resource(GameOver);
        world.insert_resource(GameOverMenu { selected, ..default() });
        world.spawn(GameOverUi);
        world.run_system_once(handle_game_over_input).unwrap();
        world
    }

    #[test]
    fn a_new_expedition_resumes_play_and_closes_the_menu() {
        let mut world = confirm(0);
        assert!(!world.contains_resource::<GameOver>());
        assert!(!world.contains_resource::<GameOverMenu>());
        assert!(world.contains_resource::<SnapCameraNext>());
        assert!(matches!(world.resource::<NextState<GameState>>(), NextState::Pending(GameState::Playing)));
        assert!(matches!(world.resource::<NextState<AppState>>(), NextState::Unchanged));
        assert_eq!(world.query_filtered::<(), With<GameOverUi>>().iter(&world).count(), 0);
    }

    #[test]
    fn abandoning_the_planet_returns_to_the_main_menu() {
        let world = confirm(1);
        assert!(!world.contains_resource::<GameOver>());
        assert!(matches!(world.resource::<NextState<AppState>>(), NextState::Pending(AppState::MainMenu)));
    }

    #[test]
    fn death_menu_snapshots() {
        use crate::ui::pixel_canvas::tests::{load_art, save_snapshot, snapshot_directory};
        let Some(directory) = snapshot_directory() else { return };
        let (path, font) = load_art(FONT_IMAGE_PATH);
        let art = Art::from_iter([(path, &font)]);
        let menu = GameOverMenu { kills: 23, score: 41, ..default() };
        save_snapshot(&directory, "death-menu", draw_game_over(&menu, &art));
        let abandon = GameOverMenu { selected: 1, ..menu };
        save_snapshot(&directory, "death-menu-abandon", draw_game_over(&abandon, &art));
    }
}
