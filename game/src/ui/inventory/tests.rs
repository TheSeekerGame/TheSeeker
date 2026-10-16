use bevy::ecs::system::RunSystemOnce;
use strum::IntoEnumIterator;

use super::*;
use crate::ui::pixel_canvas::tests::{load_art, save_snapshot, snapshot_directory};
use crate::ui::pixel_font::wrap_text;

/// The player components the inventory shows.
struct Kit {
    passives: Passives,
    skills: SkillInventory,
    weapons: WeaponInventory,
    materials: MaterialInventory,
}

impl Kit {
    fn starting() -> Self {
        Self {
            passives: Passives::default(),
            skills: SkillInventory::default(),
            weapons: WeaponInventory::default(),
            materials: MaterialInventory::empty(),
        }
    }

    /// Everything unlocked, a full loadout, and a mixed backpack.
    fn stocked() -> Self {
        let mut kit = Self::starting();
        kit.passives.unlock_all_max();
        for passive in [Passive::BatteryPack, Passive::Bloodstone, Passive::FireCrystal, Passive::Prism] {
            kit.passives.equip_passive(passive);
        }
        kit.skills.unlock_all_skills(SkillInventory::max_slots_for(&kit.passives));
        kit.weapons.grant_bow_bundle(24);
        for (index, material) in CraftMaterial::STORAGE.into_iter().enumerate() {
            kit.materials.add_to_backpack(material, index as u32 * 7 % 12);
        }
        kit
    }

    fn view(&self, melee: PlayerMeleeWeapon) -> View<'_> {
        View {
            passives: &self.passives,
            skills: &self.skills,
            weapons: &self.weapons,
            materials: &self.materials,
            melee,
        }
    }
}

fn all_items() -> impl Iterator<Item = Item> {
    SkillInventory::DEBUG_UNLOCK_ORDER.map(Item::Skill).into_iter()
        .chain(Passive::iter().map(Item::Passive))
        .chain(PlayerMeleeWeapon::ALL.map(Item::Melee))
        .chain([Item::Ranged(PlayerRangedWeapon::Bow)])
        .chain(PlayerAmmo::ALL.map(Item::Ammo))
        .chain(CraftMaterial::STORAGE.map(Item::Material))
}

/// A world holding `kit` on the player, with the inventory in `state`.
fn world_with(kit: Kit, state: InventoryState) -> World {
    let mut world = World::new();
    world.init_resource::<ButtonInput<KeyCode>>();
    world.init_resource::<CapsuleUiState>();
    world.init_resource::<super::super::mainmenu::MenuState>();
    world.insert_resource(state);
    world.insert_resource(PlayerMeleeWeapon::Sword);
    world.spawn((Player, ActionState::<PlayerAction>::default(), kit.passives, kit.skills, kit.weapons, kit.materials));
    world
}

/// Presses `key` for one run of the input system.
fn tap(world: &mut World, key: KeyCode) {
    let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
    keys.reset_all();
    keys.press(key);
    world.run_system_once(handle_inventory_input).unwrap();
}

fn skills(world: &mut World) -> SkillInventory {
    world.query::<&SkillInventory>().single(world).unwrap().clone()
}

#[test]
fn inventory_systems_have_compatible_parameters() {
    let mut schedule = Schedule::default();
    schedule.add_systems((handle_inventory_input, present_inventory));
    schedule.initialize(&mut World::new()).expect("inventory systems initialize");
}

#[test]
fn navigation_climbs_into_the_tabs_and_switching_tabs_resets_the_selection() {
    let kit = Kit::stocked();
    let view = kit.view(PlayerMeleeWeapon::Sword);
    let mut state = InventoryState { open: true, ..default() };
    let sections = view.sections(state.tab);

    navigate(&mut state, &sections, 0, 1);
    for _ in 0..COLUMNS {
        navigate(&mut state, &sections, 1, 0);
    }
    assert_eq!((state.row, state.column), (1, COLUMNS - 1), "stored rows span every column");
    navigate(&mut state, &sections, 0, -1);
    assert_eq!((state.row, state.column), (0, 4), "the equipped row is as wide as its five slots");
    navigate(&mut state, &sections, 0, -1);
    assert!(state.tabs_focused);
    navigate(&mut state, &sections, 1, 0);
    assert_eq!((state.tab, state.row, state.column), (InventoryTab::Passives, 0, 0));
    navigate(&mut state, &sections, 0, 1);
    assert!(!state.tabs_focused);
}

#[test]
fn f_equips_stored_items_and_removes_equipped_ones() {
    let mut kit = Kit::starting();
    kit.skills.unlock_skill(SkillId::Dash);
    let mut world = world_with(kit, InventoryState { open: true, ..default() });

    tap(&mut world, KeyCode::KeyS);
    tap(&mut world, KeyCode::KeyF);
    assert_eq!(skills(&mut world).equipped, [SkillId::Attack, SkillId::TetherNode, SkillId::Dash]);
    tap(&mut world, KeyCode::KeyW);
    tap(&mut world, KeyCode::KeyD);
    tap(&mut world, KeyCode::KeyF);
    let skills = skills(&mut world);
    assert_eq!((skills.equipped, skills.inventory), (vec![SkillId::Attack, SkillId::Dash], vec![SkillId::TetherNode]));
}

#[test]
fn a_full_loadout_explains_the_refusal_until_the_selection_moves() {
    let mut kit = Kit::starting();
    for skill in [SkillId::Dash, SkillId::Whirl] {
        kit.skills.unlock_skill_with_capacity(skill, SkillInventory::MAX_EQUIPPED);
    }
    kit.skills.unlock_skill(SkillId::Stealth);
    let mut world = world_with(kit, InventoryState { open: true, ..default() });

    tap(&mut world, KeyCode::KeyS);
    tap(&mut world, KeyCode::KeyF);
    assert_eq!(world.resource::<InventoryState>().failure, Some("NO FREE SLOT"));
    assert_eq!(skills(&mut world).inventory, [SkillId::Stealth]);
    tap(&mut world, KeyCode::KeyD);
    assert_eq!(world.resource::<InventoryState>().failure, None);
}

#[test]
fn equipping_a_stored_melee_weapon_swaps_the_wielded_one() {
    let mut world = world_with(Kit::starting(), InventoryState { open: true, tab: InventoryTab::Weapons, ..default() });
    tap(&mut world, KeyCode::KeyS);
    tap(&mut world, KeyCode::KeyF);
    assert_eq!(*world.resource::<PlayerMeleeWeapon>(), PlayerMeleeWeapon::Hammer);
}

#[test]
fn x_closes_the_inventory() {
    let mut world = world_with(Kit::starting(), InventoryState { open: true, ..default() });
    tap(&mut world, KeyCode::KeyX);
    assert!(!world.resource::<InventoryState>().is_open());
}

#[test]
fn every_item_fits_the_detail_column() {
    let kit = Kit::stocked();
    let view = kit.view(PlayerMeleeWeapon::Sword);
    for item in all_items() {
        let (name, _, description) = view.describe(item);
        let title = wrap_text(&name.to_ascii_uppercase(), DETAIL_WIDTH as u32).len();
        assert!(title <= 2, "{name} fits two lines");
        let lines = wrap_text(&description, DETAIL_WIDTH as u32).len();
        assert!(lines <= DETAIL_LINES - title, "{name}: {description:?} takes {lines} lines");
    }
}

/// Writes representative panels as PNG files when `SEEKER_UI_SNAPSHOTS` names a directory.
#[test]
fn inventory_panel_snapshots() {
    let Some(directory) = snapshot_directory() else { return };
    let kit = Kit::stocked();
    let view = kit.view(PlayerMeleeWeapon::Sword);
    let paths = all_items().map(|item| view.icon_path(item)).collect::<std::collections::BTreeSet<_>>();
    let art = std::iter::once(FONT_IMAGE_PATH).chain(paths).map(load_art).collect::<Vec<_>>();
    let art = Art::from_iter(art.iter().map(|(path, image)| (*path, image)));
    let mut unlocks = UnlockedSkillPresentationState::default();
    unlocks.trigger(SkillId::Dash);
    let open = InventoryState { open: true, ..default() };
    let panels = [
        ("inventory-skills", InventoryState { row: 1, column: 2, ..open }),
        ("inventory-tabs", InventoryState { tabs_focused: true, tab: InventoryTab::Passives, ..open }),
        ("inventory-passives", InventoryState { tab: InventoryTab::Passives, row: 4, column: 1, ..open }),
        ("inventory-full", InventoryState { tab: InventoryTab::Passives, row: 1, failure: Some("NO FREE SLOT"), ..open }),
        ("inventory-weapons", InventoryState { tab: InventoryTab::Weapons, column: 1, ..open }),
        ("inventory-items", InventoryState { tab: InventoryTab::Items, column: 3, ..open }),
    ];
    for (name, state) in panels {
        let sections = view.sections(state.tab);
        save_snapshot(&directory, name, draw_inventory(&state, &view, &sections, &unlocks, &art));
    }
}

#[test]
fn empty_visible_inventory_rows_are_navigable_and_have_no_item_action() {
    let kit = Kit::starting();
    let view = kit.view(PlayerMeleeWeapon::Sword);
    let sections = view.sections(InventoryTab::Skills);
    let mut state = InventoryState { open: true, ..default() };
    for _ in 0..3 { navigate(&mut state, &sections, 0, 1); }
    for _ in 0..5 { navigate(&mut state, &sections, 1, 0); }
    assert_eq!((state.row, state.column), (3, 5));
    assert_eq!(selected_cell(&sections, state.row, state.column), None);
}

#[test]
fn compact_inventory_snapshot() {
    let Some(directory) = snapshot_directory() else { return };
    let kit = Kit::stocked();
    let view = kit.view(PlayerMeleeWeapon::Sword);
    let paths = all_items().map(|item| view.icon_path(item)).collect::<std::collections::BTreeSet<_>>();
    let art = std::iter::once(FONT_IMAGE_PATH).chain(paths).map(load_art).collect::<Vec<_>>();
    let art = Art::from_iter(art.iter().map(|(path, image)| (*path, image)));
    let state = InventoryState { open: true, row: 2, column: 1, ..default() };
    let sections = view.sections(state.tab);
    save_snapshot(&directory, "inventory-compact", draw_compact_inventory(&state, &view, &sections, &default(), &art, 120));
    let empty = InventoryState { open: true, row: 3, column: 5, ..default() };
    let kit = Kit::starting();
    let view = kit.view(PlayerMeleeWeapon::Sword);
    let sections = view.sections(empty.tab);
    save_snapshot(&directory, "inventory-compact-empty", draw_compact_inventory(&empty, &view, &sections, &default(), &art, 120));
}

/// Runs the production input-owner sets and silent master clock without graphics or a window.
#[test]
fn frame_inventory_input_pauses_before_tick_admission_and_closes_while_paused() {
    use theseeker_engine::audio::{AudioEngineConfig, AudioPlugin};
    use theseeker_engine::input::InputManagerPlugin;
    use theseeker_engine::time::GameTimePlugin;
    let mut app = App::new();
    app.insert_resource(AudioEngineConfig { output_enabled: false, ..default() });
    app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default(), bevy::state::app::StatesPlugin, bevy::input::InputPlugin));
    app.insert_state(AppState::InGame);
    app.add_plugins((AudioPlugin, GameTimePlugin, InputManagerPlugin::<PlayerAction>::default(), crate::gamestate::GameStatePlugin));
    app.init_resource::<CapsuleUiState>().init_resource::<super::super::mainmenu::MenuState>().init_resource::<InventoryState>();
    app.insert_resource(PlayerMeleeWeapon::Sword).init_resource::<DeliveredJumpInput>();
    super::super::configure_input_ownership(&mut app);
    app.add_systems(PreUpdate, handle_inventory_input.in_set(super::super::UiInputSet::Inventory));
    app.add_systems(GameTickUpdate, record_jump_input.after(super::super::suppress_gameplay_input).run_if(in_state(GameState::Playing)));
    let kit = Kit::starting();
    app.world_mut().spawn((Player, ActionState::<PlayerAction>::default(), PlayerAction::input_map(), crate::game::player::InputBuffer::default(),
        kit.passives, kit.skills, kit.weapons, kit.materials));
    app.update();
    let before = app.world().resource::<GameTime>().tick();
    keyboard_event(&mut app, KeyCode::KeyM, bevy::input::ButtonState::Pressed);
    app.update();
    assert!(app.world().resource::<InventoryState>().is_open());
    assert_eq!(*app.world().resource::<State<GameState>>().get(), GameState::Paused);
    assert!(app.world().resource::<GameTransport>().paused);
    assert_eq!(app.world().resource::<GameTime>().tick(), before, "opening the panel admits no gameplay ticks");

    keyboard_event(&mut app, KeyCode::KeyS, bevy::input::ButtonState::Pressed);
    keyboard_event(&mut app, KeyCode::Space, bevy::input::ButtonState::Pressed);
    app.update();
    assert_eq!(app.world().resource::<InventoryState>().row, 1, "frame navigation remains live while ticks are frozen");
    assert_eq!(app.world().resource::<GameTime>().tick(), before);
    keyboard_event(&mut app, KeyCode::KeyM, bevy::input::ButtonState::Released);
    app.update();
    keyboard_event(&mut app, KeyCode::KeyM, bevy::input::ButtonState::Pressed);
    // Increasing the scheduling lead forces several resume ticks without a wall-clock sleep.
    app.world_mut().resource_mut::<GameTransport>().lead_seconds += 4.0 / 96.0;
    app.world_mut().resource_mut::<DeliveredJumpInput>().0.clear();
    app.update();
    assert!(!app.world().resource::<InventoryState>().is_open());
    assert_eq!(*app.world().resource::<State<GameState>>().get(), GameState::Playing);
    assert!(!app.world().resource::<GameTransport>().paused);
    let delivered = &app.world().resource::<DeliveredJumpInput>().0;
    assert!(delivered.len() >= 2, "the resumed frame exercises multiple fixed input-bank reads");
    assert!(delivered.iter().all(|&(pressed, edge)| !pressed && !edge), "panel input does not replay during catch-up");
}

#[derive(Resource, Default)]
struct DeliveredJumpInput(Vec<(bool, bool)>);

fn record_jump_input(actions: Query<&ActionState<PlayerAction>, With<Player>>, mut delivered: ResMut<DeliveredJumpInput>) {
    let actions = actions.single().unwrap();
    delivered.0.push((actions.pressed(&PlayerAction::Jump), actions.just_pressed(&PlayerAction::Jump)));
}

fn keyboard_event(app: &mut App, key_code: KeyCode, state: bevy::input::ButtonState) {
    use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};
    app.world_mut().write_message(KeyboardInput {
        key_code, logical_key: Key::Unidentified(NativeKey::Unidentified), state,
        text: None, repeat: false, window: Entity::PLACEHOLDER,
    });
}
