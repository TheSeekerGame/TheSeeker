//! Capsule panels drawn from state: the terminal, storage, and hotspot prompt.
use super::*;
use crate::ui::pixel_canvas::{rect, text_width, Art, Canvas, Raster};
use crate::ui::pixel_panel::{
    draw_frame, draw_key_hints, draw_tabs, draw_well, well_at, Accent, BAD, DISABLED,
    EMPTY_WELL, GOOD, MUTED, STRIDE, TEXT, UNAVAILABLE_ICON, WELL,
};

// Layout in art pixels. Text positions name the top of the capital letters.
const WIDTH: i32 = 106;
pub(super) const MARGIN: i32 = 6;
pub(super) const RIGHT: i32 = WIDTH - MARGIN;
pub(super) const INGREDIENT_ICON: i32 = 13;
pub(super) const INGREDIENT_GAP: i32 = 3;
const TERMINAL_GRID_TOP: i32 = 18;
/// The rule between the terminal's entry grid and the selected entry's detail.
const TERMINAL_DETAIL: i32 = TERMINAL_GRID_TOP + TERMINAL_ROWS as i32 * STRIDE + 1;
const TERMINAL_HEIGHT: i32 = TERMINAL_DETAIL + 65;
const STORAGE_HEIGHT: i32 = 108;

const AMBER: Accent = Accent { bright: Color::srgb(0.96, 0.61, 0.12), dim: Color::srgb(0.42, 0.25, 0.07) };
const CYAN: Accent = Accent { bright: Color::srgb(0.31, 0.91, 0.91), dim: Color::srgb(0.10, 0.36, 0.39) };

const ESSENCE_MASK: &[&str] = &[".###.", "#####", "#####", "#####", ".###."];
const RESEARCH_MASK: &[&str] = &[
    "..#####..", "....#....", "....#....", "...###...", "...#.#...", "..#...#..", ".#.....#.", ".#######.",
];
const OWNED_MASK: &[&str] = &["....#", "...#.", "#.#..", ".#..."];

impl Station {
    pub(super) fn accent(self) -> Accent {
        match self {
            Self::Terminal => AMBER,
            Self::Storage => CYAN,
        }
    }
}

pub(super) fn draw_panel(station: Station, state: &CapsuleUiState, ledger: &LedgerItem, art: &Art) -> Raster {
    let height = match station {
        Station::Terminal => TERMINAL_HEIGHT,
        Station::Storage => STORAGE_HEIGHT,
    };
    let mut canvas = Canvas::new(UVec2::new(WIDTH as u32, height as u32), art);
    match station {
        Station::Terminal => draw_terminal(&mut canvas, state, ledger),
        Station::Storage => draw_storage(&mut canvas, state, ledger.materials),
    }
    canvas.finish()
}

/// Tabs and essence, the entry grid, the selected entry's detail, and key hints.
fn draw_terminal(canvas: &mut Canvas, state: &CapsuleUiState, ledger: &LedgerItem) {
    let detail = TERMINAL_DETAIL;
    draw_frame(canvas, IVec2::new(WIDTH, TERMINAL_HEIGHT), AMBER, &[14, detail, detail + 50]);
    let tabs_focused = state.terminal_focus == TerminalFocus::Tabs;
    draw_tabs(canvas, IVec2::new(MARGIN, 4), &["SHOP", "CRAFT"], state.terminal_tab as usize, tabs_focused, AMBER);
    let balance = ledger.essence.get().to_string();
    let balance_x = RIGHT - text_width(&balance);
    canvas.text(IVec2::new(balance_x, 4), &balance, TEXT);
    draw_essence_orb(canvas, IVec2::new(balance_x - 7, 4));

    let entries = terminal_entries(state.terminal_tab, ledger.knowledge);
    let page = state.terminal_selection / TERMINAL_PAGE * TERMINAL_PAGE;
    let focused = state.terminal_focus == TerminalFocus::Items;
    for slot in 0..TERMINAL_PAGE {
        let index = page + slot;
        let well = well_rect(slot, TERMINAL_GRID_TOP);
        let selected = (index == state.terminal_selection).then_some(if focused { TEXT } else { AMBER.dim });
        draw_well(canvas, well, selected);
        let Some(entry) = entries.get(index).copied() else {
            canvas.fill(well, EMPTY_WELL);
            continue;
        };
        let availability = entry.availability(ledger);
        draw_entry_icon(canvas, entry, well, availability);
        if availability == Availability::Owned {
            canvas.mask(IVec2::new(well.max.x - 6, well.max.y - 5), OWNED_MASK, GOOD);
        }
    }

    let Some(entry) = entries.get(state.terminal_selection).copied() else {
        draw_key_hints(canvas, detail + 54, (MARGIN, RIGHT), None, "EXIT", AMBER);
        return;
    };
    canvas.text(IVec2::new(MARGIN, detail + 4), &entry.title().to_ascii_uppercase(), AMBER.bright);
    let (message, color) = match state.feedback {
        Some(Ok(message)) => (message, GOOD),
        Some(Err(message)) => (message, BAD),
        None => (entry.description(), MUTED),
    };
    canvas.paragraph(IVec2::new(MARGIN, detail + 14), RIGHT - MARGIN, 10, 2, message, color);
    match entry {
        TerminalEntry::Recipe(id) => draw_ingredients(canvas, id, ledger.materials, detail + 35),
        TerminalEntry::ShopTether => draw_essence_cost(canvas, ledger.essence, detail + 35, None),
        TerminalEntry::Research => {
            let discoveries = discoverable_recipes(ledger.knowledge, ledger.materials).len();
            draw_essence_cost(canvas, ledger.essence, detail + 35, Some(discoveries));
        },
    }
    let ready = entry.availability(ledger) == Availability::Ready;
    draw_key_hints(canvas, detail + 54, (MARGIN, RIGHT), Some((entry.action().0, ready)), "EXIT", AMBER);
}

/// Ingredient icons with owned/required counts; owned counts show at most 99 so three fit one row.
fn draw_ingredients(canvas: &mut Canvas, id: CapsuleRecipeId, materials: &MaterialInventory, y: i32) {
    let mut x = MARGIN;
    for cost in recipe(id).ingredients {
        let owned = materials.total_count(cost.material);
        let color = if owned >= cost.amount { GOOD } else { BAD };
        canvas.image(cost.material.icon_path(), rect(x, y, 12, 12), Color::WHITE);
        x += INGREDIENT_ICON + canvas.text(IVec2::new(x + INGREDIENT_ICON, y + 3), &ingredient_label(owned, cost.amount), color);
        x += INGREDIENT_GAP;
    }
}

pub(super) fn ingredient_label(owned: u32, required: u32) -> String {
    format!("{}/{required}", owned.min(99))
}

fn draw_essence_orb(canvas: &mut Canvas, at: IVec2) {
    canvas.mask(at, ESSENCE_MASK, AMBER.bright);
    canvas.fill(rect(at.x + 1, at.y + 1, 1, 1), TEXT);
}

fn draw_essence_cost(canvas: &mut Canvas, essence: &Essence, y: i32, discoveries: Option<usize>) {
    let color = if essence.can_spend(CAPSULE_ACTION_ESSENCE_COST) { GOOD } else { BAD };
    draw_essence_orb(canvas, IVec2::new(MARGIN, y + 3));
    let cost = canvas.text(IVec2::new(MARGIN + 8, y + 3), &CAPSULE_ACTION_ESSENCE_COST.to_string(), color);
    if let Some(discoveries) = discoveries {
        canvas.text(IVec2::new(MARGIN + 8 + cost + 8, y + 3), &format!("{discoveries} TO DISCOVER"), MUTED);
    }
}

/// Backpack and storage rows with stack counts, the selected stack's name, and key hints.
fn draw_storage(canvas: &mut Canvas, state: &CapsuleUiState, materials: &MaterialInventory) {
    draw_frame(canvas, IVec2::new(WIDTH, STORAGE_HEIGHT), CYAN, &[40, 80]);
    draw_storage_pane(canvas, state, materials, StoragePane::Backpack, 0);
    draw_storage_pane(canvas, state, materials, StoragePane::Storage, 40);
    let selected = storage_stacks(materials, state.storage_pane).get(state.storage_selection).copied();
    let (status, color) = match (state.feedback, selected) {
        (Some(Ok(message)), _) => (message.to_owned(), GOOD),
        (Some(Err(message)), _) => (message.to_owned(), BAD),
        (None, Some(material)) => (material.name().to_ascii_uppercase(), TEXT),
        (None, None) => ("NOTHING HERE".to_owned(), MUTED),
    };
    canvas.text(IVec2::new(MARGIN, 84), &status, color);
    let verb = match state.storage_pane {
        StoragePane::Backpack => "STORE",
        StoragePane::Storage => "TAKE",
    };
    draw_key_hints(canvas, 95, (MARGIN, RIGHT), Some((verb, selected.is_some())), "EXIT", CYAN);
}

fn draw_storage_pane(canvas: &mut Canvas, state: &CapsuleUiState, materials: &MaterialInventory, pane: StoragePane, top: i32) {
    let stacks = storage_stacks(materials, pane);
    let active = state.storage_pane == pane;
    let selection = if active { state.storage_selection } else { 0 };
    let page = selection / STORAGE_PAGE;
    let label = match pane {
        StoragePane::Backpack => "BACKPACK",
        StoragePane::Storage => "STORAGE",
    };
    canvas.text(IVec2::new(MARGIN, top + 4), label, if active { CYAN.bright } else { MUTED });
    let pages = stacks.len().max(selection + 1).div_ceil(STORAGE_PAGE);
    if pages > 1 {
        let indicator = format!("{}/{pages}", page + 1);
        canvas.text(IVec2::new(RIGHT - text_width(&indicator), top + 4), &indicator, MUTED);
    }
    for slot in 0..STORAGE_PAGE {
        let well = well_rect(slot, top + 14);
        draw_well(canvas, well, (active && page * STORAGE_PAGE + slot == selection).then_some(TEXT));
        let Some(material) = stacks.get(page * STORAGE_PAGE + slot).copied() else {
            canvas.fill(well, EMPTY_WELL);
            continue;
        };
        canvas.image(material.icon_path(), well, Color::WHITE);
        let count = MaterialInventory::display_count(pane.count(materials, material));
        canvas.text(IVec2::new(well.min.x + (WELL + 1 - text_width(&count)) / 2, top + 30), &count, TEXT);
    }
}

fn well_rect(slot: usize, top: i32) -> IRect {
    well_at(IVec2::new(MARGIN, top), slot, COLUMNS)
}

fn draw_entry_icon(canvas: &mut Canvas, entry: TerminalEntry, area: IRect, availability: Availability) {
    let ready = availability == Availability::Ready;
    let (tint, research) = if ready { (Color::WHITE, AMBER.bright) } else { (UNAVAILABLE_ICON, DISABLED) };
    match entry.recipe() {
        Some(recipe) => canvas.image(recipe.icon_path, area, tint),
        None => canvas.mask(area.min + (area.size() - IVec2::new(9, 8)) / 2, RESEARCH_MASK, research),
    }
}

/// The active well or tabs guide a scroll viewport in unusually small windows.
pub(super) fn selection_area(station: Station, state: &CapsuleUiState) -> Option<IRect> {
    let well = match station {
        Station::Terminal if state.terminal_focus == TerminalFocus::Tabs => terminal_tab_area(state.terminal_tab),
        Station::Terminal => well_rect(state.terminal_selection % TERMINAL_PAGE, TERMINAL_GRID_TOP),
        Station::Storage => {
            let top = if state.storage_pane == StoragePane::Backpack { 14 } else { 54 };
            well_rect(state.storage_selection % STORAGE_PAGE, top)
        },
    };
    Some(well)
}

fn terminal_tab_area(tab: TerminalTab) -> IRect {
    let (left, label) = match tab {
        TerminalTab::Shop => (MARGIN, "SHOP"),
        TerminalTab::Crafting => (MARGIN + text_width("SHOP") + 7, "CRAFT"),
    };
    rect(left, 3, text_width(label), 10)
}
