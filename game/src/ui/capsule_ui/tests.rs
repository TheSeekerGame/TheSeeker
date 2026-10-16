use super::panels::{ingredient_label, INGREDIENT_GAP, INGREDIENT_ICON, MARGIN, RIGHT};
use super::*;
use crate::graphics::pixel_view::PixelViewMapping;
use crate::ui::pixel_canvas::tests::{load_art, save_snapshot, snapshot_directory};
use crate::ui::pixel_canvas::{text_width, Art};
use crate::ui::pixel_panel::draw_prompt;
use crate::ui::pixel_font::wrap_text;

struct Stock {
    essence: Essence,
    materials: MaterialInventory,
    knowledge: CraftingKnowledge,
    passives: Passives,
    weapons: WeaponInventory,
}

impl Stock {
    fn stocked() -> Self {
        let mut player = Self {
            essence: Essence::default(),
            materials: MaterialInventory::empty(),
            knowledge: CraftingKnowledge::default(),
            passives: Passives::default(),
            weapons: WeaponInventory::default(),
        };
        player.essence.grant(120);
        player.knowledge.discovered.extend(CapsuleRecipeId::ALL);
        for (index, material) in CraftMaterial::STORAGE.into_iter().enumerate() {
            player.materials.add_to_backpack(material, index as u32 % 4);
            player.materials.add_to_storage(material, index as u32 % 3 * 7);
        }
        player
    }

    fn ledger(&self) -> LedgerItem<'_, '_> {
        LedgerItem {
            essence: &self.essence,
            materials: &self.materials,
            knowledge: &self.knowledge,
            passives: &self.passives,
            weapons: &self.weapons,
        }
    }
}

fn all_art() -> Vec<(&'static str, Image)> {
    let recipes = CAPSULE_RECIPES.iter().map(|recipe| recipe.icon_path);
    let materials = CraftMaterial::STORAGE.map(CraftMaterial::icon_path);
    std::iter::once(FONT_IMAGE_PATH).chain(recipes).chain(materials).map(load_art).collect()
}

#[test]
fn capsule_systems_have_compatible_parameters() {
    let mut schedule = Schedule::default();
    schedule.add_systems((
        handle_capsule_ui_input,
        select_capsule_prompt,
        present_capsule_panel,
    ));
    schedule.initialize(&mut World::new()).expect("capsule UI systems initialize");
}

#[test]
fn hotspot_requires_exact_seven_unit_radius() {
    let terminal = Station::Terminal.offset();
    assert_eq!(nearest_station(terminal + Vec2::X * 7.0, Vec2::ZERO), Some(Station::Terminal));
    assert_eq!(nearest_station(terminal + Vec2::X * 7.01, Vec2::ZERO), None);
}

#[test]
fn terminal_navigation_matches_the_visible_grid_and_tabs() {
    let mut knowledge = CraftingKnowledge::default();
    knowledge.discovered.extend(CapsuleRecipeId::ALL);
    let mut state = CapsuleUiState { terminal_tab: TerminalTab::Crafting, ..default() };

    navigate_terminal(&mut state, &knowledge, 1, 0);
    assert_eq!(state.terminal_selection, 1);
    navigate_terminal(&mut state, &knowledge, 0, 1);
    assert_eq!(state.terminal_selection, 7);
    navigate_terminal(&mut state, &knowledge, 0, 1);
    assert_eq!(state.terminal_selection, 13);
    navigate_terminal(&mut state, &knowledge, 0, -1);
    assert_eq!(state.terminal_selection, 7);
    navigate_terminal(&mut state, &knowledge, 0, -1);
    assert_eq!(state.terminal_selection, 1);
    navigate_terminal(&mut state, &knowledge, 0, -1);
    assert_eq!(state.terminal_focus, TerminalFocus::Tabs);
    navigate_terminal(&mut state, &knowledge, -1, 0);
    assert_eq!(state.terminal_tab, TerminalTab::Shop);
    navigate_terminal(&mut state, &knowledge, 0, 1);
    assert_eq!(state.terminal_focus, TerminalFocus::Items);
}

#[test]
fn every_terminal_entry_fits_the_detail_area() {
    let player = Stock::stocked();
    let width = RIGHT - MARGIN;
    let results = [
        CapsuleActionResult::Completed,
        CapsuleActionResult::MissingEssence,
        CapsuleActionResult::MissingIngredients,
        CapsuleActionResult::NotDiscovered,
        CapsuleActionResult::AlreadyOwned,
        CapsuleActionResult::NothingToDiscover,
    ];
    let entries = terminal_entries(TerminalTab::Crafting, &player.knowledge);
    assert!(entries.len() <= TERMINAL_PAGE, "every entry is visible on one page");
    for entry in entries.into_iter().chain([TerminalEntry::ShopTether]) {
        assert!(text_width(&entry.title().to_ascii_uppercase()) <= width, "{entry:?} title fits one line");
        let messages = results.map(|result| entry.feedback(result).unwrap_or_else(|message| message));
        for message in messages.into_iter().chain([entry.description()]) {
            assert!(wrap_text(message, width as u32).len() <= 2, "{entry:?}: {message:?} fits two lines");
        }
        let chips = entry.recipe().map_or(0, |recipe| recipe.ingredients.iter().map(|cost| {
            INGREDIENT_ICON + text_width(&ingredient_label(u32::MAX, cost.amount)) + INGREDIENT_GAP
        }).sum());
        assert!(chips - INGREDIENT_GAP <= width, "{entry:?} ingredients fit one row");
    }
}

#[test]
fn unchanged_state_redraws_identical_pixels() {
    let art = all_art();
    let art = Art::from_iter(art.iter().map(|(path, image)| (*path, image)));
    let player = Stock::stocked();
    let mut state = CapsuleUiState { terminal_tab: TerminalTab::Crafting, ..default() };
    let first = draw_panel(Station::Terminal, &state, &player.ledger(), &art);
    assert_eq!(first, draw_panel(Station::Terminal, &state, &player.ledger(), &art));
    state.terminal_selection = 1;
    assert_ne!(first, draw_panel(Station::Terminal, &state, &player.ledger(), &art));
}

/// Writes representative panels as PNG files when `SEEKER_UI_SNAPSHOTS` names a directory.
#[test]
fn capsule_panel_snapshots() {
    let Some(directory) = snapshot_directory() else { return };
    let art = all_art();
    let art = Art::from_iter(art.iter().map(|(path, image)| (*path, image)));
    let player = Stock::stocked();
    let crafting = CapsuleUiState { terminal_tab: TerminalTab::Crafting, terminal_selection: 3, ..default() };
    let feedback = CapsuleUiState { feedback: Some(Err("MISSING INGREDIENTS")), terminal_selection: 12, ..crafting };
    let tabs = CapsuleUiState { terminal_focus: TerminalFocus::Tabs, ..default() };
    let storage = CapsuleUiState { storage_pane: StoragePane::Storage, storage_selection: 1, ..default() };
    let moved = CapsuleUiState { feedback: Some(Ok("STACK MOVED")), storage_selection: 7, ..default() };
    let panels = [
        ("terminal-crafting", Station::Terminal, crafting),
        ("terminal-feedback", Station::Terminal, feedback),
        ("terminal-tabs", Station::Terminal, tabs),
        ("storage", Station::Storage, storage),
        ("storage-moved", Station::Storage, moved),
    ];
    for (name, station, state) in panels {
        save_snapshot(&directory, name, draw_panel(station, &state, &player.ledger(), &art));
    }
    let empty = CapsuleUiState { terminal_selection: 17, storage_selection: 5, ..default() };
    let stock = Stock { materials: MaterialInventory::empty(), ..Stock::stocked() };
    save_snapshot(&directory, "terminal-empty", draw_panel(Station::Terminal, &empty, &stock.ledger(), &art));
    save_snapshot(&directory, "storage-empty", draw_panel(Station::Storage, &empty, &stock.ledger(), &art));
    for station in [Station::Terminal, Station::Storage] {
        let prompt = draw_prompt(station.name(), station.accent().bright, &art);
        save_snapshot(&directory, &format!("prompt-{}", station.name().to_lowercase()), prompt);
    }
}

#[test]
fn projection_snaps_to_physical_pixels() {
    assert_eq!(snap_to_physical_pixel(Vec2::new(12.4, 18.1), 2.0), Vec2::new(12.5, 18.0));
}

#[test]
fn projected_anchor_uses_the_composited_camera_grid() {
    let camera = GlobalTransform::from(Transform::from_xyz(10.18, -3.18, 100.0));
    let displayed = PixelViewMapping::new(camera.translation(), 5, UVec2::ZERO).composite_transform(&camera);
    assert!(displayed.translation().truncate().distance(Vec2::new(10.2, -3.2)) < f32::EPSILON);
}

#[test]
fn x_closes_capsule_interfaces() {
    let mut keys = ButtonInput::default();
    keys.press(KeyCode::KeyX);
    assert!(close_pressed(&keys));
}

#[test]
fn storage_stack_order_is_deterministic() {
    let mut materials = MaterialInventory::empty();
    materials.add_to_storage(CraftMaterial::Ore2, 1);
    materials.add_to_storage(CraftMaterial::BatWing, 1);
    assert_eq!(storage_stacks(&materials, StoragePane::Storage), vec![CraftMaterial::BatWing, CraftMaterial::Ore2]);
}

#[test]
fn storage_navigation_follows_rows_and_columns() {
    let mut materials = MaterialInventory::empty();
    materials.add_to_backpack(CraftMaterial::BatWing, 1);
    materials.add_to_backpack(CraftMaterial::IceShard, 1);
    materials.add_to_storage(CraftMaterial::Ore2, 1);
    let mut state = CapsuleUiState::default();

    navigate_storage(&mut state, &materials, 1, 0);
    assert_eq!(state.storage_selection, 1);
    navigate_storage(&mut state, &materials, 0, 1);
    assert_eq!(state.storage_pane, StoragePane::Storage);
    assert_eq!(state.storage_selection, 1, "empty destination columns keep the selection");
    navigate_storage(&mut state, &materials, 0, -1);
    assert_eq!(state.storage_pane, StoragePane::Backpack);
}

#[test]
fn empty_terminal_and_storage_wells_remain_selectable() {
    let knowledge = CraftingKnowledge::default();
    let mut state = CapsuleUiState::default();
    for _ in 0..5 { navigate_terminal(&mut state, &knowledge, 1, 0); }
    for _ in 0..2 { navigate_terminal(&mut state, &knowledge, 0, 1); }
    assert_eq!(state.terminal_selection, 17);
    assert!(terminal_entries(state.terminal_tab, &knowledge).get(state.terminal_selection).is_none());
    let materials = MaterialInventory::empty();
    for _ in 0..5 { navigate_storage(&mut state, &materials, 1, 0); }
    navigate_storage(&mut state, &materials, 0, 1);
    assert_eq!((state.storage_pane, state.storage_selection), (StoragePane::Storage, 5));
}

#[test]
fn moving_the_last_stack_on_a_storage_page_keeps_its_empty_well_selectable() {
    let mut materials = MaterialInventory::empty();
    for material in CraftMaterial::STORAGE.into_iter().take(STORAGE_PAGE + 1) {
        materials.add_to_backpack(material, 1);
    }
    let moved = storage_stacks(&materials, StoragePane::Backpack)[STORAGE_PAGE];
    let mut state = CapsuleUiState { storage_selection: STORAGE_PAGE, ..default() };
    let mut keys = ButtonInput::default();
    keys.press(KeyCode::KeyF);
    handle_storage_input(&keys, &mut state, &mut materials);
    assert_eq!(materials.backpack_count(moved), 0);
    assert_eq!(materials.storage_count(moved), 1);
    assert_eq!(state.storage_selection, STORAGE_PAGE);
    navigate_storage(&mut state, &materials, 1, 0);
    assert_eq!(state.storage_selection, STORAGE_PAGE + 1, "the remaining empty wells on that page stay navigable");
}
