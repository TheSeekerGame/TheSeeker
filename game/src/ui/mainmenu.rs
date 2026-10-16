//! Game start orchestration and the keyboard/mouse pixel pause menu.
use crate::levelgen::mapgen;
use crate::levelgen::{self, LevelBiomeMask, LevelChunkColorMask, LevelChunkOccupancyMask, LevelPngConfig, LevelPngHandle};
use crate::prelude::*;
use bevy::app::AppExit;
use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParam;
use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
use std::path::{Path, PathBuf};

use super::pixel_canvas::{rect, text_width, Art, ArtLibrary, Canvas, Raster};
use super::pixel_font::FONT_IMAGE_PATH;
use super::pixel_panel::{draw_frame, key_hint, show_raster, vertical_delta, Accent, PanelScroll, UiPixelGrid, MUTED, TEXT};
use super::UiInputSet;

const WIDTH: i32 = 142;
const HEIGHT: i32 = 134;
const FIRST_BUTTON: i32 = 21;
const BUTTON_HEIGHT: i32 = 17;
const ACCENT: Accent = Accent { bright: Color::srgb(0.74, 0.68, 0.98), dim: Color::srgb(0.27, 0.23, 0.42) };

pub struct MainMenuPlugin;

impl Plugin for MainMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuState>();
        app.add_systems(OnEnter(AppState::MainMenu), spawn_menu_camera);
        app.add_systems(PreUpdate, handle_menu_input.in_set(UiInputSet::Menu)
            .run_if(in_state(AppState::InGame).or_else(in_state(AppState::MainMenu))));
        app.add_systems(Update, present_menu.after(crate::camera::update_camera).after(super::sync_ui_scale)
            .run_if(in_state(AppState::InGame).or_else(in_state(AppState::MainMenu))));
    }
}

#[derive(Resource, Default)]
pub(crate) struct MenuState {
    open: bool,
    selected: usize,
    scroll: PanelScroll,
    consumed_input: bool,
}

impl MenuState {
    pub(crate) fn is_open(&self) -> bool { self.open }
    pub(crate) fn blocks_panel_input(&self) -> bool { self.open || self.consumed_input }
}

#[derive(Component)]
struct MenuPanel;

#[derive(Clone, Copy)]
enum MenuAction { Resume, NewGame, LoadCustom, Quit }

impl MenuAction {
    const ALL: [Self; 4] = [Self::Resume, Self::NewGame, Self::LoadCustom, Self::Quit];
    fn label(self) -> &'static str {
        match self {
            Self::Resume => "RESUME",
            Self::NewGame => "NEW GAME",
            Self::LoadCustom => "LOAD CUSTOM LEVEL",
            Self::Quit => "QUIT",
        }
    }
}

fn spawn_menu_camera(mut commands: Commands) {
    commands.spawn((Camera2d, StateDespawnMarker));
}

#[derive(SystemParam)]
pub(crate) struct WorldStarter<'w> {
    config: ResMut<'w, LevelPngConfig>,
    images: ResMut<'w, Assets<Image>>,
    handle: Option<Res<'w, LevelPngHandle>>,
    chunks: ResMut<'w, mapgen::ChunkLibrary>,
    maps: ResMut<'w, mapgen::MapLibrary>,
}

impl WorldStarter<'_> {
    /// Builds the request before switching screens, so a failed request leaves the current world intact.
    pub(crate) fn generated(&mut self, commands: &mut Commands, next: &mut NextState<AppState>, current: AppState) {
        let request = levelgen::pipeline::GenerationRequest::random(self.config.clone());
        let result = match mapgen::generate_map(&mut self.chunks, &mut self.maps, &request) {
            Ok(result) => result,
            Err(err) => { error!("Failed to generate chunk map: {err}"); return; },
        };
        self.clear_previous_asset();
        levelgen::diagnostics::begin(commands);
        levelgen::reset_level_runtime(commands);
        reset_expedition_progress(commands);
        commands.remove_resource::<crate::game::intro::IntroRequested>();
        if !levelgen::diagnostics::enabled() && std::env::var_os("SEEKER_SKIP_INTRO").is_none() {
            commands.insert_resource(crate::game::intro::IntroRequested);
        }
        self.config.path.clone_from(&result.synthetic_path);
        insert_generated_map_resources(commands, &result);
        info!("generation_fixture: seed={} map={} templates={:?}", result.seed, result.map_name, result.selected_templates);
        commands.insert_resource(request);
        commands.insert_resource(result.compiled);
        next.set(start_state(current));
    }

    fn custom(&mut self, commands: &mut Commands, next: &mut NextState<AppState>, current: AppState) {
        let Some(path) = pick_custom_level_file() else { return };
        let image = match load_custom_level_image(&path, &mut self.images) {
            Ok(image) => image,
            Err(err) => { error!("Failed to load custom level: {err}"); return; },
        };
        self.clear_previous_asset();
        levelgen::reset_level_runtime(commands);
        commands.remove_resource::<crate::game::GameOver>();
        commands.remove_resource::<crate::game::intro::IntroRequested>();
        reset_player_runtime(commands);
        commands.insert_resource(LevelPngHandle(image));
        self.config.path = path.to_string_lossy().into_owned();
        commands.insert_resource(levelgen::pipeline::GenerationRequest::random(self.config.clone()));
        next.set(start_state(current));
    }

    fn clear_previous_asset(&mut self) {
        let handle = self.handle.as_ref().map(|handle| handle.0.clone());
        cleanup_previous_level_asset(&mut self.images, handle, &self.config.path);
    }
}

/// A newly generated expedition starts with a fresh player loadout and no owner-scoped runtime state.
fn reset_expedition_progress(commands: &mut Commands) {
    use crate::game::player::{spawn::PersistedPlayerProgression, weapon::PlayerMeleeWeapon};
    commands.insert_resource(PersistedPlayerProgression::default());
    commands.insert_resource(PlayerMeleeWeapon::default());
    commands.insert_resource(crate::game::combat::KillCount::default());
    reset_player_runtime(commands);
    commands.remove_resource::<crate::game::GameOver>();
}

fn reset_player_runtime(commands: &mut Commands) {
    use crate::game::player::progression::UnlockedSkillPresentationState;
    use crate::game::player::passives::frenzied_attack::{EnergyRegenDeltas, LifeDebt};
    commands.insert_resource(crate::game::player::skills::cooldowns::Cooldowns::default());
    commands.insert_resource(EnergyRegenDeltas::default());
    commands.insert_resource(LifeDebt::default());
    commands.insert_resource(UnlockedSkillPresentationState::default());
    commands.insert_resource(crate::game::player::skills::tether_node::TetherConnectionStatus::default());
}

fn start_state(current: AppState) -> AppState {
    if current == AppState::InGame { AppState::Restart } else { AppState::InGame }
}

fn insert_generated_map_resources(commands: &mut Commands, result: &mapgen::GeneratedMap) {
    commands.insert_resource(LevelBiomeMask {
        width: result.width, height: result.height, cells: result.biomes.clone(), tundra_inner: Vec::new(),
    });
    if let Some(pre_impact) = result.pre_impact.clone() { commands.insert_resource(pre_impact); }
    commands.insert_resource(LevelChunkColorMask {
        chunk_size: mapgen::CHUNK_IMAGE_SIZE, chunks_w: result.chunks_w, chunks_h: result.chunks_h, colors: result.chunk_colors.clone(),
    });
    commands.insert_resource(LevelChunkOccupancyMask {
        chunks_w: result.chunks_w, chunks_h: result.chunks_h, occupied: result.chunk_occupied.clone(),
    });
}

#[expect(clippy::too_many_arguments, reason = "The menu translates frame input into app lifecycle commands and file selection.")]
fn handle_menu_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    app_state: Res<State<AppState>>,
    mut menu: ResMut<MenuState>,
    panels: (Res<super::inventory::InventoryState>, Res<super::capsule_ui::CapsuleUiState>),
    grid: UiPixelGrid,
    mut starter: WorldStarter,
    mut commands: Commands,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
    mut pause: ResMut<crate::gamestate::PauseRequests>,
) {
    menu.consumed_input = false;
    let standalone = *app_state.get() == AppState::MainMenu;
    if keys.just_pressed(KeyCode::Escape) && !panels.0.is_open() && !panels.1.is_open() {
        menu.consumed_input = true;
        menu.open = !menu.open;
        menu.selected = 0;
        menu.scroll = default();
        return;
    }
    if !menu.open && !standalone { return; }
    let first = usize::from(standalone);
    menu.selected = menu.selected.saturating_add_signed(vertical_delta(&keys) as isize).clamp(first, 3);
    let clicked = menu_pointer(&grid, &menu).filter(|_| mouse.just_pressed(MouseButton::Left));
    let Some(index) = clicked.or_else(|| keys.any_just_pressed([KeyCode::KeyF, KeyCode::Enter, KeyCode::Space]).then_some(menu.selected)) else { return };
    menu.consumed_input = true;
    menu.selected = index.max(first);
    match MenuAction::ALL[menu.selected] {
        MenuAction::Resume => { menu.open = false; pause.resume_manual(); },
        MenuAction::NewGame => starter.generated(&mut commands, &mut next, *app_state.get()),
        MenuAction::LoadCustom => starter.custom(&mut commands, &mut next, *app_state.get()),
        MenuAction::Quit => { exit.write(AppExit::Success); },
    }
}

fn menu_pointer(grid: &UiPixelGrid, menu: &MenuState) -> Option<usize> {
    let cursor = grid.cursor_art()?;
    let available = grid.available_art()?;
    let shown = UVec2::new(WIDTH as u32, HEIGHT as u32).min(available);
    let origin = ((available - shown) / 2).as_vec2();
    let point = cursor - origin + menu.scroll.offset.as_vec2();
    if point.x < 7.0 || point.x >= (WIDTH - 7) as f32 || point.y < FIRST_BUTTON as f32 { return None; }
    let index = (point.y as i32 - FIRST_BUTTON) / BUTTON_HEIGHT;
    (index < 4).then_some(index as usize)
}

#[expect(clippy::too_many_arguments, reason = "The pixel menu publishes its raster on the current UI grid.")]
fn present_menu(
    mut commands: Commands,
    mut state: ResMut<MenuState>,
    app_state: Res<State<AppState>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut library: ResMut<ArtLibrary>,
    server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut panel: Query<(Entity, &ImageNode, &mut Node), With<MenuPanel>>,
    grid: UiPixelGrid,
) {
    let standalone = *app_state.get() == AppState::MainMenu;
    let content = (state.open || standalone).then(|| {
        library.load(&server, [FONT_IMAGE_PATH]);
        let raster = draw_menu(&state, standalone, &library.art(&images));
        let focus = menu_focus(state.selected);
        grid.scrollable(raster, &mut state.scroll, &keys, Some(focus), Vec2::splat(0.5))
    }).flatten();
    show_raster(&mut commands, &mut images, panel.single_mut().ok(), content, |image, node| {
        (Name::new("PauseMenu"), MenuPanel, image, node, GlobalZIndex(i32::MAX - 900), StateDespawnMarker)
    });
}

fn menu_focus(selected: usize) -> IRect {
    let width = text_width(MenuAction::ALL[selected].label());
    rect((WIDTH - width) / 2 - 3, FIRST_BUTTON + selected as i32 * BUTTON_HEIGHT + 2, width + 6, 12)
}

fn draw_menu(state: &MenuState, standalone: bool, art: &Art) -> Raster {
    let mut canvas = Canvas::new(UVec2::new(WIDTH as u32, HEIGHT as u32), art);
    draw_frame(&mut canvas, IVec2::new(WIDTH, HEIGHT), ACCENT, &[14, 93]);
    let title = if standalone { "THE SEEKER" } else { "PAUSED" };
    canvas.text(IVec2::new((WIDTH - text_width(title)) / 2, 4), title, ACCENT.bright);
    for (index, action) in MenuAction::ALL.into_iter().enumerate().skip(usize::from(standalone)) {
        let top = FIRST_BUTTON + index as i32 * BUTTON_HEIGHT;
        let selected = index == state.selected;
        canvas.plate(rect(7, top, WIDTH - 14, 15), 1, if selected { ACCENT.dim } else { super::pixel_panel::WELL_FILL }, ACCENT.dim);
        let label = action.label();
        canvas.text(IVec2::new((WIDTH - text_width(label)) / 2, top + 4), label, if selected { TEXT } else { MUTED });
    }
    key_hint(&mut canvas, IVec2::new(7, 98), "F", "SELECT", ACCENT.bright);
    key_hint(&mut canvas, IVec2::new(7, 110), "ESC", "RESUME", MUTED);
    key_hint(&mut canvas, IVec2::new(7, 122), "PGUP/DN", "SCROLL", MUTED);
    canvas.finish()
}

fn load_custom_level_image(
    path: &Path,
    images: &mut Assets<Image>,
) -> Result<Handle<Image>, String> {
    let bytes = std::fs::read(path).map_err(|err| {
        format!(
            "could not read {}: {err}",
            path.display()
        )
    })?;
    let image = Image::from_buffer(
        &bytes,
        ImageType::Extension("png"),
        CompressedImageFormats::default(),
        true,
        ImageSampler::Default,
        RenderAssetUsages::default(),
    )
    .map_err(|err| {
        format!(
            "could not decode {}: {err}",
            path.display()
        )
    })?;
    Ok(images.add(image))
}

fn default_levels_directory() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    dir.push("assets");
    dir.push("levels");
    Some(dir)
}

fn cleanup_previous_level_asset(
    images: &mut Assets<Image>,
    previous_handle: Option<Handle<Image>>,
    previous_path: &str,
) {
    let is_custom_file = Path::new(previous_path).is_absolute();
    let is_generated =
        previous_path.starts_with(mapgen::GENERATED_LEVEL_PREFIX);
    if !(is_custom_file || is_generated) {
        return;
    }

    if let Some(handle) = previous_handle {
        images.remove(handle.id());
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn pick_custom_level_file() -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new();
    dialog = dialog.add_filter("PNG Level", &["png"]);
    if let Some(default_dir) = default_levels_directory() {
        dialog = dialog.set_directory(default_dir);
    }
    dialog.pick_file()
}

#[cfg(target_arch = "wasm32")]
fn pick_custom_level_file() -> Option<PathBuf> {
    warn!("Custom level selection is not supported on wasm builds");
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starting_a_world_always_enters_a_fresh_game_lifetime() {
        assert_eq!(start_state(AppState::InGame), AppState::Restart);
        assert_eq!(start_state(AppState::AssetsLoading), AppState::InGame);
        assert_eq!(start_state(AppState::MainMenu), AppState::InGame);
    }

    #[test]
    fn new_expedition_clears_persisted_loadout_storage_and_run_counters() {
        use bevy::ecs::system::RunSystemOnce;
        use crate::game::player::{CraftMaterial, MaterialInventory, SkillInventory};
        use crate::game::player::spawn::PersistedPlayerProgression;
        use crate::game::player::skills::types::SkillId;
        use crate::game::player::weapon::PlayerMeleeWeapon;
        let mut world = World::new();
        let mut materials = MaterialInventory::empty();
        materials.add_to_storage(CraftMaterial::BatWing, 7);
        let mut skills = SkillInventory::default();
        skills.unlock_skill(SkillId::Dash);
        world.insert_resource(PersistedPlayerProgression { materials: Some(materials), skills: Some(skills), ..default() });
        world.insert_resource(PlayerMeleeWeapon::Hammer);
        world.insert_resource(crate::game::combat::KillCount(17));
        world.insert_resource(crate::game::GameOver);
        world.run_system_once(|mut commands: Commands| reset_expedition_progress(&mut commands)).unwrap();
        let progress = world.resource::<PersistedPlayerProgression>();
        assert!(progress.materials.is_none(), "a new expedition cannot inherit protected storage");
        assert!(progress.skills.is_none(), "unlocked skills belong to the preceding expedition");
        assert_eq!(*world.resource::<PlayerMeleeWeapon>(), PlayerMeleeWeapon::Sword);
        assert_eq!(world.resource::<crate::game::combat::KillCount>().0, 0);
        assert!(!world.contains_resource::<crate::game::GameOver>());
    }

    #[test]
    fn pause_menu_snapshot() {
        use crate::ui::pixel_canvas::tests::{load_art, save_snapshot, snapshot_directory};
        let Some(directory) = snapshot_directory() else { return };
        let (path, font) = load_art(FONT_IMAGE_PATH);
        let art = Art::from_iter([(path, &font)]);
        save_snapshot(&directory, "pause-menu", draw_menu(&MenuState::default(), false, &art));
        let state = MenuState { selected: 3, ..default() };
        let focus = menu_focus(3);
        let compact = PanelScroll::default().view(draw_menu(&state, false, &art), UVec2::new(104, 62), &default(), Some(focus));
        save_snapshot(&directory, "pause-menu-small-quit", compact);
    }
}
