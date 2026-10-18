//! The capsule's terminal and storage: panels projected beside their hotspots.
//!
//! Each frame redraws the open panel from game state as a pixel canvas, anchored
//! to its hotspot's composited screen position on the gameplay pixel grid. The
//! station in reach is offered through the toolbar's interact prompt.
use bevy::ecs::query::QueryData;
use bevy::ecs::system::SystemParam;

use crate::camera::{update_camera, MainCamera};
use crate::game::capsule_commerce::{
    craft_recipe, discoverable_recipes, purchase_tether, recipe, research_recipes, CapsuleActionResult,
    CapsuleRecipe, CapsuleRecipeId, CraftingKnowledge, CAPSULE_ACTION_ESSENCE_COST, CAPSULE_RECIPES,
};
use crate::game::enemy::{CorpseLoot, Decay, OpiTombstone};
use crate::game::player::weapon::WeaponInventory;
use crate::game::player::{CraftMaterial, Essence, MaterialInventory, Passives, Player, SkillInventory};
use crate::game::world_objects::{Capsule, CapsuleOccupant};
use crate::graphics::pixel_view::{update_pixel_view, PixelView};
use crate::levelgen::harvestable_materials::HarvestableResource;
use crate::prelude::*;
use crate::ui::inventory::InventoryState;
use crate::ui::pixel_canvas::ArtLibrary;
use crate::ui::pixel_font::FONT_IMAGE_PATH;
use crate::ui::pixel_panel::{
    absolute_node, close_pressed, horizontal_delta, panel_key_pressed, show_raster, vertical_delta, UiPixelGrid,
    PanelScroll, PANEL_Z_INDEX,
};
use crate::ui::skill_toolbar::StationPrompt;
use panels::draw_panel;

const HOTSPOT_RADIUS: f32 = 7.0;
const PRIORITY_INTERACTION_RADIUS: f32 = 22.0;
const COLUMNS: usize = 6;
/// Rows of terminal entries; every crafting entry stays visible on one page.
const TERMINAL_ROWS: usize = 4;
const TERMINAL_PAGE: usize = TERMINAL_ROWS * COLUMNS;
const STORAGE_PAGE: usize = COLUMNS;

pub(crate) struct CapsuleUiPlugin;

impl Plugin for CapsuleUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CapsuleUiState>()
            .add_systems(Startup, load_capsule_art)
            .add_systems(PreUpdate, handle_capsule_ui_input.in_set(super::UiInputSet::Panels)
                .run_if(in_state(AppState::InGame)))
            .add_systems(Update, (select_capsule_prompt, present_capsule_panel.after(update_pixel_view))
                .chain().after(update_camera).after(super::sync_ui_scale).run_if(in_state(AppState::InGame)));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Station {
    Terminal,
    Storage,
}

impl Station {
    fn offset(self) -> Vec2 {
        match self {
            Self::Terminal => Vec2::new(-15.0, 59.0),
            Self::Storage => Vec2::new(9.0, 85.0),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Terminal => "TERMINAL",
            Self::Storage => "STORAGE",
        }
    }

    /// The panel's top-left corner relative to the projected hotspot, in art pixels.
    fn panel_offset(self, size: Vec2) -> Vec2 {
        match self {
            Self::Terminal => Vec2::new(-size.x - 16.0, -48.0),
            Self::Storage => Vec2::new(28.0, -84.0),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum TerminalTab {
    #[default]
    Shop,
    Crafting,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum TerminalFocus {
    Tabs,
    #[default]
    Items,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum StoragePane {
    #[default]
    Backpack,
    Storage,
}

impl StoragePane {
    fn count(self, materials: &MaterialInventory, material: CraftMaterial) -> u32 {
        match self {
            Self::Backpack => materials.backpack_count(material),
            Self::Storage => materials.storage_count(material),
        }
    }
}

/// The result of the last action, cleared when the selection moves.
type Feedback = Result<&'static str, &'static str>;

#[derive(Resource, Debug, Default)]
pub(crate) struct CapsuleUiState {
    open: Option<(Station, Entity)>,
    terminal_tab: TerminalTab,
    terminal_focus: TerminalFocus,
    terminal_selection: usize,
    storage_pane: StoragePane,
    storage_selection: usize,
    feedback: Option<Feedback>,
    scroll: PanelScroll,
}

impl CapsuleUiState {
    pub(crate) fn is_open(&self) -> bool {
        self.open.is_some()
    }

    fn open(&mut self, station: Station, capsule: Entity) {
        self.open = Some((station, capsule));
        self.scroll = default();
        self.feedback = None;
        self.terminal_focus = TerminalFocus::Items;
        self.terminal_selection = 0;
        self.storage_selection = 0;
    }

    fn close(&mut self) {
        self.open = None;
        self.feedback = None;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalEntry {
    ShopTether,
    Research,
    Recipe(CapsuleRecipeId),
}

impl TerminalEntry {
    /// The recipe whose output the entry provides; the shop sells tether nodes for essence.
    fn recipe(self) -> Option<&'static CapsuleRecipe> {
        match self {
            Self::ShopTether => Some(recipe(CapsuleRecipeId::TetherNode)),
            Self::Research => None,
            Self::Recipe(id) => Some(recipe(id)),
        }
    }

    fn title(self) -> &'static str {
        self.recipe().map_or("Research Kit", |recipe| recipe.title)
    }

    fn description(self) -> &'static str {
        self.recipe().map_or("Discovers recipes from carried materials", |recipe| recipe.description)
    }

    /// The action's key-hint verb and its success message.
    fn action(self) -> (&'static str, &'static str) {
        match self {
            Self::ShopTether => ("BUY", "PURCHASED"),
            Self::Research => ("SCAN", "NEW RECIPES FOUND"),
            Self::Recipe(_) => ("CRAFT", "CRAFTED"),
        }
    }

    fn feedback(self, result: CapsuleActionResult) -> Feedback {
        match result {
            CapsuleActionResult::Completed => Ok(self.action().1),
            CapsuleActionResult::MissingEssence => Err("INSUFFICIENT ESSENCE"),
            CapsuleActionResult::MissingIngredients => Err("MISSING INGREDIENTS"),
            CapsuleActionResult::NotDiscovered => Err("RECIPE UNKNOWN"),
            CapsuleActionResult::AlreadyOwned => Err("ALREADY OWNED"),
            CapsuleActionResult::NothingToDiscover => Err("NOTHING TO DISCOVER"),
        }
    }

    fn availability(self, ledger: &LedgerItem) -> Availability {
        let can_pay = ledger.essence.can_spend(CAPSULE_ACTION_ESSENCE_COST);
        let (owned, ready) = match self {
            Self::ShopTether => (false, can_pay),
            Self::Research => (false, can_pay && !discoverable_recipes(ledger.knowledge, ledger.materials).is_empty()),
            Self::Recipe(id) => (
                recipe(id).output.is_owned(ledger.passives, ledger.weapons),
                recipe(id).ingredients.iter().all(|cost| ledger.materials.has_total(cost.material, cost.amount)),
            ),
        };
        match (owned, ready) {
            (true, _) => Availability::Owned,
            (false, true) => Availability::Ready,
            (false, false) => Availability::Blocked,
        }
    }

}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Availability {
    Ready,
    Blocked,
    Owned,
}

fn terminal_entries(tab: TerminalTab, knowledge: &CraftingKnowledge) -> Vec<TerminalEntry> {
    match tab {
        TerminalTab::Shop => vec![TerminalEntry::ShopTether],
        TerminalTab::Crafting => std::iter::once(TerminalEntry::Research)
            .chain(
                CapsuleRecipeId::ALL
                    .into_iter()
                    .filter(|id| knowledge.is_discovered(*id))
                    .map(TerminalEntry::Recipe),
            )
            .collect(),
    }
}

fn storage_stacks(materials: &MaterialInventory, pane: StoragePane) -> Vec<CraftMaterial> {
    CraftMaterial::STORAGE.into_iter().filter(|material| pane.count(materials, *material) > 0).collect()
}

/// The player components that capsule actions read and update.
#[derive(QueryData)]
#[query_data(mutable)]
struct Commerce {
    essence: &'static mut Essence,
    materials: &'static mut MaterialInventory,
    knowledge: &'static mut CraftingKnowledge,
    skills: &'static mut SkillInventory,
    passives: &'static mut Passives,
    weapons: &'static mut WeaponInventory,
}

impl CommerceItem<'_, '_> {
    fn perform(&mut self, entry: TerminalEntry) -> CapsuleActionResult {
        match entry {
            TerminalEntry::ShopTether => purchase_tether(
                &mut self.essence, &mut self.materials, &mut self.skills, &mut self.passives, &mut self.weapons,
            ),
            TerminalEntry::Research => research_recipes(&mut self.knowledge, &self.materials, &mut self.essence),
            TerminalEntry::Recipe(id) => craft_recipe(
                id, &self.knowledge, &mut self.materials, &mut self.skills, &mut self.passives, &mut self.weapons,
            ),
        }
    }
}

/// The player components that capsule panels present.
#[derive(QueryData)]
struct Ledger {
    essence: &'static Essence,
    materials: &'static MaterialInventory,
    knowledge: &'static CraftingKnowledge,
    passives: &'static Passives,
    weapons: &'static WeaponInventory,
}

/// Harvesting and corpse looting take priority over nearby capsule hotspots.
#[derive(SystemParam)]
struct NearbyInteractions<'w, 's> {
    harvestables: Query<'w, 's, &'static GlobalTransform, With<HarvestableResource>>,
    corpses: Query<'w, 's, &'static CorpseLoot, Or<(With<Decay>, With<OpiTombstone>)>>,
    transforms: Query<'w, 's, &'static GlobalTransform>,
}

impl NearbyInteractions<'_, '_> {
    fn station(&self, player: Vec2, capsule: Vec2) -> Option<Station> {
        let near = |transform: &GlobalTransform| {
            transform.translation().truncate().distance_squared(player)
                <= PRIORITY_INTERACTION_RADIUS * PRIORITY_INTERACTION_RADIUS
        };
        let harvest = self.harvestables.iter().any(near);
        let corpse = self.corpses.iter().any(|corpse| self.transforms.get(corpse.visual_entity).is_ok_and(near));
        if harvest || corpse { None } else { nearest_station(player, capsule) }
    }
}

fn nearest_station(player: Vec2, capsule: Vec2) -> Option<Station> {
    [Station::Terminal, Station::Storage]
        .into_iter()
        .map(|station| (station, player.distance_squared(capsule + station.offset())))
        .filter(|(_, distance)| *distance <= HOTSPOT_RADIUS * HOTSPOT_RADIUS)
        .min_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(station, _)| station)
}

fn load_capsule_art(mut library: ResMut<ArtLibrary>, server: Res<AssetServer>) {
    let recipes = CAPSULE_RECIPES.iter().map(|recipe| recipe.icon_path);
    let materials = CraftMaterial::STORAGE.map(CraftMaterial::icon_path);
    library.load(&server, std::iter::once(FONT_IMAGE_PATH).chain(recipes).chain(materials));
}

fn handle_capsule_ui_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<CapsuleUiState>,
    mut player: Query<(&GlobalTransform, &CapsuleOccupant, Commerce), With<Player>>,
    capsules: Query<&GlobalTransform, With<Capsule>>,
    nearby: NearbyInteractions,
    inventory: Res<InventoryState>,
    menu: Res<super::mainmenu::MenuState>,
) {
    if menu.blocks_panel_input() { return; }
    let Ok((transform, occupant, mut commerce)) = player.single_mut() else {
        state.close();
        return;
    };
    let Ok(capsule) = capsules.get(occupant.shelter) else {
        state.close();
        return;
    };
    if state.open.is_some_and(|(_, open)| open != occupant.shelter) {
        state.close();
    }
    match state.open.map(|(station, _)| station) {
        Some(_) if close_pressed(&keys) => state.close(),
        Some(_) if !panel_key_pressed(&keys) => {},
        Some(Station::Terminal) => handle_terminal_input(&keys, &mut state, &mut commerce),
        Some(Station::Storage) => handle_storage_input(&keys, &mut state, &mut commerce.materials),
        None if keys.just_pressed(KeyCode::KeyF) && !inventory.is_open() => {
            let player = transform.translation().truncate();
            let Some(station) = nearby.station(player, capsule.translation().truncate()) else { return };
            state.open(station, occupant.shelter);
        },
        None => {},
    }
}

fn handle_terminal_input(keys: &ButtonInput<KeyCode>, state: &mut CapsuleUiState, commerce: &mut CommerceItem) {
    navigate_terminal(state, &commerce.knowledge, horizontal_delta(keys), vertical_delta(keys));
    if !keys.just_pressed(KeyCode::KeyF) {
        return;
    }
    if state.terminal_focus == TerminalFocus::Tabs {
        state.terminal_focus = TerminalFocus::Items;
        return;
    }
    let entries = terminal_entries(state.terminal_tab, &commerce.knowledge);
    let Some(entry) = entries.get(state.terminal_selection).copied() else { return };
    state.feedback = Some(entry.feedback(commerce.perform(entry)));
}

fn navigate_terminal(state: &mut CapsuleUiState, knowledge: &CraftingKnowledge, horizontal: i32, vertical: i32) {
    let count = terminal_entries(state.terminal_tab, knowledge).len().div_ceil(TERMINAL_PAGE).max(1) * TERMINAL_PAGE;
    state.terminal_selection = state.terminal_selection.min(count.saturating_sub(1));
    match state.terminal_focus {
        TerminalFocus::Tabs => navigate_terminal_tabs(state, horizontal, vertical),
        TerminalFocus::Items => navigate_terminal_grid(state, count, horizontal, vertical),
    }
}

fn navigate_terminal_tabs(state: &mut CapsuleUiState, horizontal: i32, vertical: i32) {
    let tab = match horizontal.signum() {
        -1 => TerminalTab::Shop,
        1 => TerminalTab::Crafting,
        _ => state.terminal_tab,
    };
    if tab != state.terminal_tab {
        state.terminal_tab = tab;
        state.terminal_selection = 0;
        state.feedback = None;
    }
    if vertical > 0 {
        state.terminal_focus = TerminalFocus::Items;
    }
}

/// Moves within the grid's rows; moving up from the first row focuses the tabs.
fn navigate_terminal_grid(state: &mut CapsuleUiState, count: usize, horizontal: i32, vertical: i32) {
    let previous = state.terminal_selection;
    let column = previous % COLUMNS;
    let moved = match horizontal.signum() {
        -1 if column > 0 => previous - 1,
        1 if column + 1 < COLUMNS && previous + 1 < count => previous + 1,
        _ => previous,
    };
    if vertical < 0 && moved < COLUMNS {
        state.terminal_focus = TerminalFocus::Tabs;
    }
    state.terminal_selection = match vertical.signum() {
        -1 if moved >= COLUMNS => moved - COLUMNS,
        1 if moved + COLUMNS < count => moved + COLUMNS,
        _ => moved,
    };
    if state.terminal_selection != previous {
        state.feedback = None;
    }
}

/// Moves the selected stack wholly into the other location.
fn handle_storage_input(keys: &ButtonInput<KeyCode>, state: &mut CapsuleUiState, materials: &mut MaterialInventory) {
    navigate_storage(state, materials, horizontal_delta(keys), vertical_delta(keys));
    if !keys.just_pressed(KeyCode::KeyF) {
        return;
    }
    let Some(material) = storage_stacks(materials, state.storage_pane).get(state.storage_selection).copied() else {
        return;
    };
    let moved = match state.storage_pane {
        StoragePane::Backpack => materials.move_backpack_to_storage(material, materials.backpack_count(material)),
        StoragePane::Storage => materials.move_storage_to_backpack(material, materials.storage_count(material)),
    };
    state.feedback = Some(if moved { Ok("STACK MOVED") } else { Err("EMPTY STACK") });

}

/// Vertical input selects a pane, keeping the column; horizontal input moves along its stacks.
fn navigate_storage(state: &mut CapsuleUiState, materials: &MaterialInventory, horizontal: i32, vertical: i32) {
    let previous = (state.storage_pane, state.storage_selection);
    state.storage_pane = match vertical.signum() {
        -1 => StoragePane::Backpack,
        1 => StoragePane::Storage,
        _ => state.storage_pane,
    };
    let selection = if state.storage_pane == previous.0 { previous.1 } else { previous.1 % STORAGE_PAGE };
    let count = storage_stacks(materials, state.storage_pane).len().max(selection + 1);
    let last = count.div_ceil(STORAGE_PAGE).max(1) * STORAGE_PAGE - 1;
    state.storage_selection = match horizontal.signum() {
        -1 => selection.saturating_sub(1),
        1 if selection < last => selection + 1,
        _ => selection,
    }
    .min(last);
    if (state.storage_pane, state.storage_selection) != previous {
        state.feedback = None;
    }
}

/// Places capsule panels beside their hotspots as the composited camera shows them.
#[derive(SystemParam)]
struct HotspotProjection<'w, 's> {
    view: Res<'w, PixelView>,
    camera: Query<'w, 's, (&'static Camera, &'static GlobalTransform), With<MainCamera>>,
    capsules: Query<'w, 's, &'static GlobalTransform, With<Capsule>>,
    grid: UiPixelGrid<'w, 's>,
}

impl HotspotProjection<'_, '_> {
    fn available_art(&self) -> Option<UVec2> {
        let available = self.grid.available_art()?;
        Some(UVec2::new(available.x, available.y.saturating_sub(super::skill_toolbar::FRAME_HEIGHT as u32 + 3).max(1)))
    }

    /// A node of `size` art pixels next to the station, on the pixel grid and above the toolbar.
    fn panel_node(&self, capsule: Entity, station: Station, size: UVec2) -> Option<Node> {
        let (camera, camera_transform) = self.camera.single().ok()?;
        let window = self.grid.window.single().ok()?;
        let unit = self.grid.unit()?;
        let displayed = self.view.0?.composite_transform(camera_transform);
        let hotspot = self.capsules.get(capsule).ok()?.translation() + station.offset().extend(0.0);
        let viewport = camera.world_to_viewport(&displayed, hotspot).ok()?;
        let origin = camera.logical_viewport_rect()?.min;
        let anchor = snap_to_physical_pixel(viewport - origin, window.scale_factor()) / self.grid.ui_scale();
        let desired = anchor + station.panel_offset(size.as_vec2()) * unit;
        let size = size.as_vec2() * unit;
        let minimum = Vec2::splat(unit);
        let available = self.available_art()?.as_vec2() * unit;
        let maximum = ((available - size) / unit).floor() * unit;
        Some(absolute_node(desired.clamp(minimum, maximum.max(minimum)), size))
    }
}

fn snap_to_physical_pixel(logical_position: Vec2, scale_factor: f32) -> Vec2 {
    let scale_factor = scale_factor.max(f32::EPSILON);
    (logical_position * scale_factor).round() / scale_factor
}

#[derive(Component)]
struct CapsulePanel;

#[expect(clippy::too_many_arguments, reason = "The capsule panel publishes its raster next to the composited station hotspot.")]
fn present_capsule_panel(
    mut commands: Commands,
    mut state: ResMut<CapsuleUiState>,
    keys: Res<ButtonInput<KeyCode>>,
    player: Query<Ledger, With<Player>>,
    library: Res<ArtLibrary>,
    mut images: ResMut<Assets<Image>>,
    mut panel: Query<(Entity, &ImageNode, &mut Node), With<CapsulePanel>>,
    projection: HotspotProjection,
) {
    let content = state.open.and_then(|(station, capsule)| {
        let raster = draw_panel(station, &state, &player.single().ok()?, &library.art(&images));
        let focus = panels::selection_area(station, &state);
        let raster = state.scroll.view(raster, projection.available_art()?, &keys, focus);
        Some((projection.panel_node(capsule, station, raster.size)?, raster))
    });
    show_raster(&mut commands, &mut images, panel.single_mut().ok(), content, |image, node| {
        (Name::new("CapsulePanel"), CapsulePanel, image, node, ZIndex(PANEL_Z_INDEX), StateDespawnMarker)
    });
}

/// Offers the station in reach, while no panel is open, as the interact prompt above the toolbar.
fn select_capsule_prompt(
    state: Res<CapsuleUiState>,
    player: Query<(&GlobalTransform, &CapsuleOccupant), With<Player>>,
    capsules: Query<&GlobalTransform, With<Capsule>>,
    nearby: NearbyInteractions,
    mut prompt: ResMut<StationPrompt>,
) {
    let station = player.single().ok().filter(|_| !state.is_open()).and_then(|(player, occupant)| {
        let capsule = capsules.get(occupant.shelter).ok()?;
        nearby.station(player.translation().truncate(), capsule.translation().truncate())
    });
    prompt.0 = station.map(|station| (station.name(), station.accent().bright));
}

mod panels;

#[cfg(test)]
mod tests;
