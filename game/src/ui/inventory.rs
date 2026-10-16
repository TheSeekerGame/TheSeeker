//! The player's inventory: equipped and stored skills, passives, and weapons, and the backpack.
//!
//! A keyboard-driven pixel panel in the capsule style. The inventory action opens and closes it;
//! WASD moves between tabs and wells, F equips or removes the selected item, and X closes it.
use bevy::ecs::query::QueryData;
use leafwing_input_manager::prelude::ActionState;

use super::pixel_canvas::{gain, rect, text_width, Art, ArtLibrary, Canvas, Raster};
use super::pixel_font::FONT_IMAGE_PATH;
use super::pixel_panel::{
    close_pressed, draw_frame, draw_key_hints, draw_tabs, draw_count, draw_well, horizontal_delta, panel_key_pressed,
    show_raster, vertical_delta, well_at, Accent, PanelScroll, UiPixelGrid, BAD, EMPTY_WELL, MUTED, PANEL_Z_INDEX, STRIDE, TEXT,
};
use crate::game::player::progression::UnlockedSkillPresentationState;
use crate::game::player::skills::types::SkillId;
use crate::game::player::weapon::{PlayerAmmo, PlayerMeleeWeapon, PlayerRangedWeapon, WeaponInventory};
use crate::game::player::{CraftMaterial, MaterialInventory, Passive, Passives, Player, PlayerAction, SkillInventory};
use crate::prelude::*;
use crate::ui::capsule_ui::CapsuleUiState;

const WIDTH: i32 = 226;
const HEIGHT: i32 = 118;
const MARGIN: i32 = 7;
const COLUMNS: usize = 6;
/// The detail column right of the grids.
const DETAIL_LEFT: i32 = 112;
const DETAIL_WIDTH: i32 = WIDTH - DETAIL_LEFT - MARGIN;
const SECTIONS_TOP: i32 = 18;
const FOOTER_RULE: i32 = 103;
const DETAIL_LINES: usize = 6;
const ACCENT: Accent = Accent { bright: Color::srgb(0.74, 0.68, 0.98), dim: Color::srgb(0.27, 0.23, 0.42) };

pub(crate) struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InventoryState>().add_systems(
            PreUpdate,
            handle_inventory_input.in_set(super::UiInputSet::Inventory).run_if(in_state(AppState::InGame)),
        );
        app.add_systems(Update, present_inventory.after(crate::camera::update_camera).after(super::sync_ui_scale).run_if(in_state(AppState::InGame)));
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum InventoryTab {
    #[default]
    Skills,
    Passives,
    Weapons,
    Items,
}

impl InventoryTab {
    const ALL: [Self; 4] = [Self::Skills, Self::Passives, Self::Weapons, Self::Items];
    const LABELS: [&'static str; 4] = ["SKILLS", "PASSIVES", "WEAPONS", "ITEMS"];
}

#[derive(Resource, Debug, Default)]
pub(crate) struct InventoryState {
    open: bool,
    tab: InventoryTab,
    tabs_focused: bool,
    row: usize,
    column: usize,
    /// Why the last action failed, cleared when the selection moves.
    failure: Option<&'static str>,
    scroll: PanelScroll,
}

impl InventoryState {
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    fn select(&mut self, row: usize, column: usize) {
        if (row, column) != (self.row, self.column) {
            (self.row, self.column, self.failure) = (row, column, None);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Item {
    Skill(SkillId),
    Passive(Passive),
    Melee(PlayerMeleeWeapon),
    Ranged(PlayerRangedWeapon),
    Ammo(PlayerAmmo),
    Material(CraftMaterial),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Cell {
    item: Item,
    equipped: bool,
}

/// A labeled grid of wells; an equipped row shows exactly its capacity.
struct Section {
    label: &'static str,
    visible_rows: usize,
    equipped_row: bool,
    cells: Vec<Option<Cell>>,
}

impl Section {
    fn row_count(&self) -> usize {
        if self.equipped_row { 1 } else { self.cells.len().div_ceil(COLUMNS * self.visible_rows).max(1) * self.visible_rows }
    }

    fn row_width(&self) -> usize {
        if self.equipped_row { self.cells.len() } else { COLUMNS }
    }
}

/// The player components the inventory shows and changes.
#[derive(QueryData)]
#[query_data(mutable)]
struct Loadout {
    passives: &'static mut Passives,
    skills: &'static mut SkillInventory,
    weapons: &'static mut WeaponInventory,
    materials: &'static MaterialInventory,
}

struct View<'a> {
    passives: &'a Passives,
    skills: &'a SkillInventory,
    weapons: &'a WeaponInventory,
    materials: &'a MaterialInventory,
    melee: PlayerMeleeWeapon,
}

impl View<'_> {
    fn sections(&self, tab: InventoryTab) -> Vec<Section> {
        let equipped = |cells: Vec<Option<Item>>| Section {
            label: "EQUIPPED",
            visible_rows: 1,
            equipped_row: true,
            cells: cells.into_iter().map(|item| item.map(|item| Cell { item, equipped: true })).collect(),
        };
        let stored = |label, visible_rows, mut items: Vec<Item>| {
            items.sort_by_key(|item| item.order());
            let cells = items.into_iter().map(|item| Some(Cell { item, equipped: false })).collect();
            Section { label, visible_rows, equipped_row: false, cells }
        };
        match tab {
            InventoryTab::Skills => {
                let capacity = SkillInventory::max_slots_for(self.passives);
                vec![
                    equipped(padded(self.skills.equipped.iter().map(|skill| Item::Skill(*skill)), capacity)),
                    stored("STORED", 3, self.skills.inventory.iter().map(|skill| Item::Skill(*skill)).collect()),
                ]
            },
            InventoryTab::Passives => vec![
                equipped(padded(self.passives.equipped.iter().map(|passive| Item::Passive(*passive)), Passives::MAX_EQUIPPED)),
                stored("STORED", 3, self.passives.inventory.iter().map(|passive| Item::Passive(*passive)).collect()),
            ],
            InventoryTab::Weapons => {
                let (ranged, ammo) = (self.weapons.equipped_ranged(), self.weapons.equipped_ammo());
                let melee = PlayerMeleeWeapon::ALL.into_iter().filter(|weapon| *weapon != self.melee).map(Item::Melee);
                let owned = self.weapons.owned_ranged().iter().copied().filter(|weapon| Some(*weapon) != ranged);
                let stocked = PlayerAmmo::ALL.into_iter().filter(|kind| Some(*kind) != ammo && self.weapons.count_ammo(*kind) > 0);
                vec![
                    equipped(vec![Some(Item::Melee(self.melee)), ranged.map(Item::Ranged), ammo.map(Item::Ammo)]),
                    stored("STORED", 3, melee.chain(owned.map(Item::Ranged)).chain(stocked.map(Item::Ammo)).collect()),
                ]
            },
            InventoryTab::Items => {
                let carried = CraftMaterial::STORAGE.into_iter().filter(|material| self.materials.backpack_count(*material) > 0);
                vec![stored("BACKPACK", 4, carried.map(Item::Material).collect())]
            },
        }
    }

    fn icon_path(&self, item: Item) -> &'static str {
        match item {
            Item::Skill(skill) => skill.icon_path_with_melee_weapon(self.melee),
            Item::Passive(passive) => passive.icon_path(),
            Item::Melee(weapon) => weapon.icon_path(),
            Item::Ranged(weapon) => weapon.icon_path(),
            Item::Ammo(ammo) => ammo.icon_path(),
            Item::Material(material) => material.icon_path(),
        }
    }

    /// The stock a stacked item shows.
    fn count(&self, item: Item) -> Option<u32> {
        match item {
            Item::Skill(SkillId::BowAttack) => Some(self.weapons.count_ammo(PlayerAmmo::Arrows)),
            Item::Skill(skill) => skill.stock_material().map(|material| self.materials.backpack_count(material)),
            Item::Ammo(ammo) => Some(self.weapons.count_ammo(ammo)),
            Item::Material(material) => Some(self.materials.backpack_count(material)),
            Item::Passive(_) | Item::Melee(_) | Item::Ranged(_) => None,
        }
    }

    /// Name, kind, and description; passive descriptions show their current rolled value.
    fn describe(&self, item: Item) -> (&'static str, &'static str, String) {
        match item {
            Item::Skill(skill) => (skill.name(), "SKILL", skill.description().to_owned()),
            Item::Passive(passive) => {
                (passive.name(), "PASSIVE", passive.describe(self.passives.passive_effective_value(passive)))
            },
            Item::Melee(weapon) => (weapon.name(), "MELEE", weapon.description().to_owned()),
            Item::Ranged(weapon) => (weapon.name(), "RANGED", weapon.description().to_owned()),
            Item::Ammo(ammo) => (ammo.name(), "AMMO", ammo.description().to_owned()),
            Item::Material(material) => (material.name(), "MATERIAL", material.description().to_owned()),
        }
    }
}

impl Item {
    /// Stored items keep a fixed order so equipping one never shuffles the rest.
    fn order(self) -> (u8, u16) {
        match self {
            Self::Skill(skill) => (0, skill as u16),
            Self::Passive(passive) => (1, passive as u16),
            Self::Melee(weapon) => (2, weapon as u16),
            Self::Ranged(weapon) => (3, weapon as u16),
            Self::Ammo(ammo) => (4, ammo as u16),
            Self::Material(material) => (5, material as u16),
        }
    }
}

fn padded(items: impl Iterator<Item = Item>, capacity: usize) -> Vec<Option<Item>> {
    let mut cells: Vec<_> = items.map(Some).collect();
    cells.resize(cells.len().max(capacity), None);
    cells
}

/// The flat list of navigable rows: (section, row within it).
fn rows(sections: &[Section]) -> Vec<(usize, usize)> {
    sections.iter().enumerate().flat_map(|(index, section)| (0..section.row_count()).map(move |row| (index, row))).collect()
}

fn selected_cell(sections: &[Section], row: usize, column: usize) -> Option<Cell> {
    let &(section, local) = rows(sections).get(row)?;
    sections[section].cells.get(local * COLUMNS + column).copied().flatten()
}

fn handle_inventory_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<InventoryState>,
    capsule: Res<CapsuleUiState>,
    menu: Res<super::mainmenu::MenuState>,
    mut player: Query<(&ActionState<PlayerAction>, Loadout), With<Player>>,
    mut melee: ResMut<PlayerMeleeWeapon>,
) {
    if menu.blocks_panel_input() { return; }
    let Ok((actions, mut loadout)) = player.single_mut() else { return };
    let toggled = actions.just_pressed(&PlayerAction::TogglePassiveInventory);
    if state.open && (toggled || close_pressed(&keys)) || capsule.is_open() {
        state.open = false;
        return;
    }
    if !state.open {
        state.open = toggled;
        return;
    }
    if !panel_key_pressed(&keys) {
        return;
    }
    let view = View {
        passives: &loadout.passives,
        skills: &loadout.skills,
        weapons: &loadout.weapons,
        materials: loadout.materials,
        melee: *melee,
    };
    let sections = view.sections(state.tab);
    navigate(&mut state, &sections, horizontal_delta(&keys), vertical_delta(&keys));
    let cell = selected_cell(&sections, state.row, state.column).filter(|_| !state.tabs_focused);
    if let Some(cell) = cell.filter(|_| keys.just_pressed(KeyCode::KeyF)) {
        state.failure = perform(cell, &mut loadout, &mut melee).err();
    }
}

fn navigate(state: &mut InventoryState, sections: &[Section], horizontal: i32, vertical: i32) {
    if state.tabs_focused {
        let tab = (state.tab as usize).saturating_add_signed(horizontal as isize).min(InventoryTab::ALL.len() - 1);
        if InventoryTab::ALL[tab] != state.tab {
            (state.tab, state.row, state.column, state.failure) = (InventoryTab::ALL[tab], 0, 0, None);
        }
        state.tabs_focused = vertical <= 0;
        return;
    }
    let rows = rows(sections);
    if vertical < 0 && state.row == 0 {
        state.tabs_focused = true;
        return;
    }
    let row = state.row.saturating_add_signed(vertical as isize).min(rows.len().saturating_sub(1));
    let width = sections[rows[row].0].row_width();
    let column = state.column.saturating_add_signed(horizontal as isize).min(width.saturating_sub(1));
    state.select(row, column);
}

/// Equips or removes the selected item; the error says why it could not.
fn perform(cell: Cell, loadout: &mut LoadoutItem, melee: &mut ResMut<PlayerMeleeWeapon>) -> Result<(), &'static str> {
    let capacity = SkillInventory::max_slots_for(&loadout.passives);
    let done = match (cell.item, cell.equipped) {
        (Item::Skill(skill), true) => loadout.skills.unequip_skill(skill),
        (Item::Skill(skill), false) => loadout.skills.equip_skill_with_capacity(skill, capacity),
        (Item::Passive(passive), true) => loadout.passives.unequip_passive(passive),
        (Item::Passive(passive), false) => loadout.passives.equip_passive(passive),
        (Item::Melee(weapon), false) => {
            **melee = weapon;
            true
        },
        (Item::Ranged(weapon), false) => loadout.weapons.equip_ranged(weapon),
        (Item::Ammo(ammo), false) => {
            loadout.weapons.equip_ammo(ammo);
            true
        },
        _ => true,
    };
    if done { Ok(()) } else { Err("NO FREE SLOT") }
}

/// What F does to a cell, if anything.
fn action_label(cell: Cell) -> Option<&'static str> {
    match (cell.item, cell.equipped) {
        (Item::Skill(_) | Item::Passive(_), true) => Some("REMOVE"),
        (Item::Material(_), _) | (_, true) => None,
        (_, false) => Some("EQUIP"),
    }
}

#[derive(Component)]
struct InventoryPanel;

#[expect(clippy::too_many_arguments, reason = "The panel draws player items with their art at the pixel grid.")]
fn present_inventory(
    mut commands: Commands,
    mut state: ResMut<InventoryState>,
    keys: Res<ButtonInput<KeyCode>>,
    player: Query<LoadoutReadOnly, With<Player>>,
    melee: Res<PlayerMeleeWeapon>,
    unlocks: Res<UnlockedSkillPresentationState>,
    mut library: ResMut<ArtLibrary>,
    server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut panel: Query<(Entity, &ImageNode, &mut Node), With<InventoryPanel>>,
    grid: UiPixelGrid,
) {
    let content = player.single().ok().filter(|_| state.open).and_then(|loadout| {
        let view = View {
            passives: loadout.passives,
            skills: loadout.skills,
            weapons: loadout.weapons,
            materials: loadout.materials,
            melee: *melee,
        };
        let sections = view.sections(state.tab);
        let navigable = rows(&sections);
        state.row = state.row.min(navigable.len().saturating_sub(1));
        state.column = state.column.min(sections[navigable[state.row].0].row_width().saturating_sub(1));
        let paths = sections.iter().flat_map(|section| section.cells.iter().flatten()).map(|cell| view.icon_path(cell.item));
        library.load(&server, std::iter::once(FONT_IMAGE_PATH).chain(paths));
        let available = grid.available_art()?;
        let compact = available.x < WIDTH as u32;
        let raster = if compact {
            draw_compact_inventory(&state, &view, &sections, &unlocks, &library.art(&images), available.y)
        } else { draw_inventory(&state, &view, &sections, &unlocks, &library.art(&images)) };
        let focus = inventory_focus(&state, &sections, compact.then_some(available.y));
        grid.scrollable(raster, &mut state.scroll, &keys, focus, Vec2::splat(0.5))
    });
    show_raster(&mut commands, &mut images, panel.single_mut().ok(), content, |image, node| {
        (Name::new("InventoryPanel"), InventoryPanel, image, node, ZIndex(PANEL_Z_INDEX), StateDespawnMarker)
    });
}

fn draw_inventory(
    state: &InventoryState,
    view: &View,
    sections: &[Section],
    unlocks: &UnlockedSkillPresentationState,
    art: &Art,
) -> Raster {
    let mut canvas = Canvas::new(UVec2::new(WIDTH as u32, HEIGHT as u32), art);
    draw_frame(&mut canvas, IVec2::new(WIDTH, HEIGHT), ACCENT, &[14, FOOTER_RULE]);
    canvas.fill(rect(DETAIL_LEFT - 6, 15, 1, FOOTER_RULE - 15), ACCENT.dim);
    draw_tabs(&mut canvas, IVec2::new(MARGIN, 4), &InventoryTab::LABELS, state.tab as usize, state.tabs_focused, ACCENT);
    let selection = rows(sections).get(state.row).copied().filter(|_| !state.tabs_focused);
    let mut top = SECTIONS_TOP;
    for (index, section) in sections.iter().enumerate() {
        let selected_row = selection.filter(|(section, _)| *section == index).map(|(_, row)| row);
        draw_section(&mut canvas, view, section, top, DETAIL_LEFT - 8, selected_row.map(|row| (row, state.column)), unlocks);
        top += 9 + section.visible_rows as i32 * STRIDE + 2;
    }
    let cell = selected_cell(sections, state.row, state.column).filter(|_| !state.tabs_focused);
    if let Some(cell) = cell {
        draw_detail(&mut canvas, view, cell, state.failure);
    }
    let action = cell.and_then(action_label).map(|label| (label, true));
    draw_key_hints(&mut canvas, FOOTER_RULE + 3, (MARGIN, WIDTH - MARGIN), action, "CLOSE", ACCENT);
    canvas.finish()
}

/// Narrow windows show the selected section above its details; navigation keeps the
/// same logical rows, so changing layout never changes which item is selected.
fn draw_compact_inventory(
    state: &InventoryState, view: &View, sections: &[Section], unlocks: &UnlockedSkillPresentationState, art: &Art, available_height: u32,
) -> Raster {
    const COMPACT_WIDTH: i32 = 106;
    const BODY_WIDTH: i32 = COMPACT_WIDTH - 2 * MARGIN;
    let visible_rows = ((available_height as i32 - 96) / STRIDE).clamp(1, 3) as usize;
    let detail_top = 31 + visible_rows as i32 * STRIDE;
    let cell = selected_cell(sections, state.row, state.column).filter(|_| !state.tabs_focused);
    let detail = cell.map(|cell| {
        let (name, kind, description) = view.describe(cell.item);
        let kind = match (view.count(cell.item), cell.equipped) {
            (Some(count), _) => format!("{kind} x{}", MaterialInventory::display_count(count)),
            (None, true) => format!("{kind} EQUIPPED"),
            (None, false) => kind.to_owned(),
        };
        (name.to_ascii_uppercase(), kind, state.failure.map_or(description, str::to_owned))
    });
    let detail_lines = detail.as_ref().map_or(2, |(name, kind, body)| {
        [name, kind, body].into_iter().map(|text| super::pixel_font::wrap_text(text, BODY_WIDTH as u32).len()).sum::<usize>()
    });
    let footer = detail_top + detail_lines as i32 * 10 + 7;
    let height = footer + 15;
    let mut canvas = Canvas::new(UVec2::new(COMPACT_WIDTH as u32, height as u32), art);
    draw_frame(&mut canvas, IVec2::new(COMPACT_WIDTH, height), ACCENT, &[14, detail_top - 3, footer]);
    draw_tabs(&mut canvas, IVec2::new(MARGIN, 4), &["SKL", "PASS", "WPN", "ITEM"], state.tab as usize, state.tabs_focused, ACCENT);
    let (index, local) = rows(sections).get(state.row).copied().unwrap_or((0, 0));
    let section = &sections[index];
    let selected = (!state.tabs_focused).then_some((local, state.column));
    let compact_section = Section {
        label: section.label, visible_rows: if section.equipped_row { 1 } else { visible_rows },
        equipped_row: section.equipped_row, cells: section.cells.clone(),
    };
    draw_section(&mut canvas, view, &compact_section, SECTIONS_TOP, COMPACT_WIDTH - MARGIN, selected, unlocks);
    if let Some((name, kind, body)) = detail {
        let mut y = detail_top;
        for (text, color) in [(&name, ACCENT.bright), (&kind, MUTED), (&body, if state.failure.is_some() { BAD } else { TEXT })] {
            y += canvas.paragraph(IVec2::new(MARGIN, y), BODY_WIDTH, 10, detail_lines, text, color) as i32 * 10;
        }
    } else {
        canvas.text(IVec2::new(MARGIN, detail_top), "EMPTY SLOT", MUTED);
    }
    draw_key_hints(&mut canvas, footer + 3, (MARGIN, COMPACT_WIDTH - MARGIN), cell.and_then(action_label).map(|label| (label, true)), "EXIT", ACCENT);
    canvas.finish()
}

fn inventory_focus(state: &InventoryState, sections: &[Section], compact_height: Option<u32>) -> Option<IRect> {
    if state.tabs_focused {
        let labels = if compact_height.is_some() { ["SKL", "PASS", "WPN", "ITEM"] } else { InventoryTab::LABELS };
        let selected = state.tab as usize;
        let left = MARGIN + labels[..selected].iter().map(|label| text_width(label) + 7).sum::<i32>();
        return Some(rect(left, 3, text_width(labels[selected]), 10));
    }
    let (index, local) = rows(sections).get(state.row).copied()?;
    let top = if compact_height.is_some() { SECTIONS_TOP } else {
        SECTIONS_TOP + sections[..index].iter().map(|section| 11 + section.visible_rows as i32 * STRIDE).sum::<i32>()
    };
    let visible_rows = compact_height.map_or(sections[index].visible_rows, |height| ((height as i32 - 96) / STRIDE).clamp(1, 3) as usize);
    let row = if sections[index].equipped_row { 0 } else { local % visible_rows };
    Some(well_at(IVec2::new(MARGIN, top + 9), row * COLUMNS + state.column, COLUMNS))
}

/// A section's label and its page of wells; `selected` is the selected (row, column) within it.
fn draw_section(
    canvas: &mut Canvas,
    view: &View,
    section: &Section,
    top: i32,
    right: i32,
    selected: Option<(usize, usize)>,
    unlocks: &UnlockedSkillPresentationState,
) {
    canvas.text(IVec2::new(MARGIN, top), section.label, MUTED);
    let page = selected.map_or(0, |(row, _)| row / section.visible_rows);
    let pages = section.row_count().div_ceil(section.visible_rows);
    if pages > 1 {
        let indicator = format!("{}/{pages}", page + 1);
        canvas.text(IVec2::new(right - text_width(&indicator), top), &indicator, MUTED);
    }
    let visible = if section.equipped_row { section.cells.len() } else { section.visible_rows * COLUMNS };
    let first = page * section.visible_rows * COLUMNS;
    let chosen = selected.map(|(row, column)| row * COLUMNS + column);
    for slot in 0..visible {
        let well = well_at(IVec2::new(MARGIN, top + 9), slot, COLUMNS);
        let outline = (chosen == Some(first + slot)).then_some(TEXT);
        let Some(cell) = section.cells.get(first + slot).copied().flatten() else {
            draw_empty_well(canvas, well, outline);
            continue;
        };
        draw_well(canvas, well, outline);
        let flash = unlock_gain(cell.item, unlocks);
        canvas.shaded_image(view.icon_path(cell.item), well, |texel| gain(texel, flash));
        if let Some(count) = view.count(cell.item) {
            draw_count(canvas, IRect::from_corners(well.min + 1, well.max - 1), count);
        }
    }
}

fn draw_empty_well(canvas: &mut Canvas, well: IRect, outline: Option<Color>) {
    draw_well(canvas, well, outline);
    canvas.fill(well, EMPTY_WELL);
}

/// Brightens a newly unlocked skill's icon.
fn unlock_gain(item: Item, unlocks: &UnlockedSkillPresentationState) -> f32 {
    let Item::Skill(skill) = item else { return 1.0 };
    unlocks.entry(skill).map_or(1.0, |entry| super::skill_toolbar::unlock_gain(entry.remaining_ticks))
}

fn draw_detail(canvas: &mut Canvas, view: &View, cell: Cell, failure: Option<&str>) {
    let (name, kind, description) = view.describe(cell.item);
    let at = IVec2::new(DETAIL_LEFT, SECTIONS_TOP);
    let lines = canvas.paragraph(at, DETAIL_WIDTH, 10, 2, &name.to_ascii_uppercase(), ACCENT.bright);
    let tag = at + IVec2::Y * 10 * lines as i32;
    let kind = match (view.count(cell.item), cell.equipped) {
        (Some(count), _) => format!("{kind}  x{}", MaterialInventory::display_count(count)),
        (None, true) => format!("{kind}  EQUIPPED"),
        (None, false) => kind.to_owned(),
    };
    canvas.text(tag, &kind, MUTED);
    let (body, color) = failure.map_or((description.as_str(), TEXT), |failure| (failure, BAD));
    canvas.paragraph(tag + IVec2::Y * 12, DETAIL_WIDTH, 10, DETAIL_LINES - lines, body, color);
}

#[cfg(test)]
mod tests;
