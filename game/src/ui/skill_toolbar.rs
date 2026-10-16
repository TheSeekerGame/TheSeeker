//! The skill toolbar, the interact prompt above it, and level-up captions: pixel canvases
//! redrawn every frame.
//!
//! The toolbar frame holds the equipped skills with their cooldowns and stock, and shows health
//! and Alignment through the bar windows cut into its art.
use std::collections::HashSet;
use std::f32::consts::TAU;

use bevy::ecs::query::QueryData;
use bevy::ecs::system::SystemParam;

use super::pixel_canvas::{gain, lift, rect, text_width, Art, ArtLibrary, Canvas, Raster};
use super::pixel_font::FONT_IMAGE_PATH;
use super::pixel_panel::{draw_prompt, show_raster, UiPixelGrid, PANEL_Z_INDEX, TEXT};
use super::screen_frame::{AlignmentPulseSample, ALIGNMENT_PULSE_LINEAR_GRAY};
use crate::game::combat::Health;
use crate::game::effects::stealthed::StealthEffect;
use crate::game::interaction::InteractionFocus;
use crate::game::player::passives::PassiveEvent;
use crate::game::player::progression::{AdaptationLevelUpEvent, UnlockedSkillPresentationState};
use crate::game::player::skills::channeled::ChanneledSkill;
use crate::game::player::skills::cooldowns::Cooldowns;
use crate::game::player::skills::tether_node::TetherConnectionStatus;
use crate::game::player::skills::types::{flicker_strike_metadata, whirl_metadata, SkillId};
use crate::game::player::weapon::{PlayerAmmo, PlayerMeleeWeapon, WeaponInventory};
use crate::game::player::{
    Alignment, FlickerAbility, MaterialInventory, Passive, Passives, Player, PlayerStateSet, SkillInventory,
    WhirlAbility,
};
use crate::prelude::*;

const TOOLBAR_Z_INDEX: i32 = 27;
const CAPTION_Z_INDEX: i32 = 28;
const ICON: i32 = 12;
/// The frame art's top rows are empty and left out of the toolbar.
const FRAME_TOP: i32 = 6;
pub(super) const FRAME_HEIGHT: i32 = 30 - FRAME_TOP;
/// Where the first slot icon and the bar windows sit in the cropped frame.
const FIRST_SLOT: IVec2 = IVec2::new(9, 9 - FRAME_TOP);
const SLOT_STRIDE: i32 = 14;
const HEALTH_BAR: IVec2 = IVec2::new(9, 23 - FRAME_TOP);
const ALIGNMENT_BAR: IVec2 = IVec2::new(9, 26 - FRAME_TOP);

const HEALTH_TRACK: Color = Color::srgb(0.231, 0.125, 0.125);
const HEALTH_FILL: Color = Color::srgb(0.635, 0.196, 0.306);
const ALIGNMENT_TRACK: Color = Color::srgb(0.176, 0.153, 0.227);
const ALIGNMENT_FILL: Color = Color::srgb(0.247, 0.314, 0.431);
const ALIGNMENT_WARNING: Color = Color::srgb(0.50, 0.62, 0.80);
const ALIGNMENT_DEPLETED: Color = Color::srgb(0.38, 0.47, 0.62);
const ALIGNMENT_LOW_THRESHOLD: f32 = 0.2;
const ALIGNMENT_LOW_PULSE_HZ: f32 = 1.5;
const ALIGNMENT_DEPLETED_PULSE_HZ: f32 = 0.75;

/// An icon stays pressed for this long after its skill is used, and while it channels.
const PRESS_TICKS: u32 = 12;
const UNAVAILABLE_GAIN: f32 = 0.75;
const READY_FLASH_TICKS: u32 = 16;
const READY_FLASH_GAIN: f32 = 1.5;
const UNLOCK_FLASH_GAIN: f32 = 2.4;
const COOLDOWN_SHADE: Color = Color::srgba(0.0, 0.0, 0.0, 0.6);
/// Skills that recharge through energy or channel rather than cool down.
const CHANNELED_SKILLS: [SkillId; 3] = [SkillId::Whirl, SkillId::BurningDash, SkillId::FlickerStrike];

const CAPTION_COLOR: Color = Color::srgb(0.88, 0.94, 1.0);
const CAPTION_LIMIT: usize = 3;
const CAPTION_DELAY_SECS: f32 = 0.24;
const CAPTION_HOLD_SECS: f32 = 0.95;
const CAPTION_TOTAL_SECS: f32 = 1.6;
const CAPTION_TOP: u32 = 4;

pub struct SkillToolbarPlugin;

impl Plugin for SkillToolbarPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ToolbarIcons>()
            .init_resource::<StationPrompt>()
            .init_resource::<Captions>()
            .add_systems(Startup, load_toolbar_art)
            .add_systems(OnEnter(AppState::InGame), (reset_toolbar_presentation, spawn_toolbar).chain())
            .add_systems(
                GameTickUpdate,
                advance_toolbar_icons.after(PlayerStateSet::Behavior).run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (present_toolbar, present_prompt, (queue_level_up_captions, present_captions).chain())
                    .after(super::sync_ui_scale)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

fn reset_toolbar_presentation(mut icons: ResMut<ToolbarIcons>, mut captions: ResMut<Captions>, mut prompt: ResMut<StationPrompt>) {
    *icons = default();
    *captions = default();
    *prompt = default();
}

/// The toolbar image; the capsule prompt is placed above it.
#[derive(Component)]
pub struct ToolbarFrame;

fn frame_path(slots: usize) -> &'static str {
    if slots > SkillInventory::MAX_EQUIPPED { "ui/game/ToolbarFrame5Slots.png" } else { "ui/game/ToolbarFrame.png" }
}

fn load_toolbar_art(mut library: ResMut<ArtLibrary>, server: Res<AssetServer>) {
    let skills = SkillInventory::DEBUG_UNLOCK_ORDER.into_iter();
    let icons = skills.flat_map(|skill| PlayerMeleeWeapon::ALL.map(|weapon| skill.icon_path_with_melee_weapon(weapon)));
    let frames = [SkillInventory::MAX_EQUIPPED, SkillInventory::MAX_EQUIPPED + 1].map(frame_path);
    library.load(&server, icons.chain(frames).chain([FONT_IMAGE_PATH]));
}

/// A slot's stock count, set in smooth text because pixel digits would cover the icon.
#[derive(Component)]
struct StockLabel(usize);

/// Spawned with the game rather than on first draw, so the HUD is hidden from its first frame
/// whenever gameplay presentation is suspended.
fn spawn_toolbar(mut commands: Commands, mut images: ResMut<Assets<Image>>, server: Res<AssetServer>) {
    let empty = Canvas::new(UVec2::ONE, &Art::default()).finish();
    let font = TextFont { font: server.load("font/Tektur-Medium.ttf").into(), font_size: FontSize::Px(12.0), ..default() };
    let labels = (0..=SkillInventory::MAX_EQUIPPED).map(move |slot| {
        (StockLabel(slot), Text::default(), font.clone(), Node { position_type: PositionType::Absolute, ..default() })
    });
    commands.spawn((
        Name::new("SkillToolbar"),
        ToolbarFrame,
        ImageNode::new(images.add(empty.into_image())),
        Node::default(),
        ZIndex(TOOLBAR_Z_INDEX),
        StateDespawnMarker,
        crate::ui::GameplayHud,
        Children::spawn(SpawnIter(labels)),
    ));
}

/// How an equipped skill's icon reacts to use, cooldowns, and recharge.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct IconState {
    press_ticks: u32,
    channeling: bool,
    unavailable: bool,
    flash_ticks: u32,
    energy_full: Option<bool>,
}

impl IconState {
    fn pressed(self) -> bool {
        self.channeling || self.press_ticks > 0
    }

    /// Advances one tick. `ready_flash` is whether becoming usable again flashes the icon, and
    /// `energy_full` whether the skill's energy is full, for skills that recharge through energy.
    fn advance(&mut self, used: bool, channeling: bool, unavailable: bool, ready_flash: bool, energy_full: Option<bool>) {
        if !self.channeling {
            self.press_ticks = self.press_ticks.saturating_sub(1);
        }
        self.flash_ticks = self.flash_ticks.saturating_sub(1);
        if used {
            self.press_ticks = PRESS_TICKS;
        }
        if self.channeling && !channeling {
            self.press_ticks = 0;
        }
        self.channeling = channeling;
        if unavailable {
            self.flash_ticks = 0;
        } else if self.unavailable && ready_flash {
            self.flash_ticks = READY_FLASH_TICKS;
        }
        self.unavailable = unavailable;
        if self.energy_full == Some(false) && energy_full == Some(true) {
            self.flash_ticks = READY_FLASH_TICKS;
        }
        self.energy_full = energy_full;
    }

    /// The icon's brightness: dimmed while unusable, flashing when it becomes usable.
    fn gain(self) -> f32 {
        if self.unavailable {
            return UNAVAILABLE_GAIN;
        }
        1.0 + (READY_FLASH_GAIN - 1.0) * self.flash_ticks as f32 / READY_FLASH_TICKS as f32
    }
}

/// Brightness of a newly unlocked skill's icon while it flashes.
pub(crate) fn unlock_gain(remaining_ticks: u32) -> f32 {
    let remaining = remaining_ticks as f32 / UnlockedSkillPresentationState::FLASH_TICKS as f32;
    1.0 + (UNLOCK_FLASH_GAIN - 1.0) * remaining.sqrt()
}

#[derive(Resource, Default)]
struct ToolbarIcons {
    states: HashMap<SkillId, IconState>,
    /// Channeled skills seen so far; their icons are not dimmed by cooldowns.
    channeled: HashSet<SkillId>,
}

/// The player state the toolbar shows.
#[derive(QueryData)]
struct ToolbarPlayer {
    entity: Entity,
    skills: &'static SkillInventory,
    passives: &'static Passives,
    materials: &'static MaterialInventory,
    weapons: &'static WeaponInventory,
    whirl: &'static WhirlAbility,
    flicker: &'static FlickerAbility,
    health: &'static Health,
    alignment: &'static Alignment,
    stealthed: Has<StealthEffect>,
}

impl ToolbarPlayerItem<'_, '_> {
    /// Missing ammunition, stock, or a tether connection, which no cooldown restores.
    fn lacks_means(&self, skill: SkillId, tether_connected: bool) -> bool {
        match skill {
            SkillId::BowAttack => !self.weapons.can_fire_bow(),
            SkillId::TetherNode if !tether_connected => true,
            _ => skill.stock_material().is_some_and(|material| self.materials.backpack_count(material) == 0),
        }
    }

    fn energy_full(&self, skill: SkillId) -> bool {
        match skill {
            SkillId::Whirl => self.whirl.energy >= whirl_metadata().max_energy,
            SkillId::FlickerStrike => self.flicker.energy >= flicker_strike_metadata().max_energy,
            _ => false,
        }
    }

    /// How much of the icon is still shaded: the remaining cooldown, or missing energy.
    fn recharge(&self, skill: SkillId, cooldowns: &Cooldowns) -> f32 {
        match skill {
            SkillId::Whirl => 1.0 - self.whirl.energy / whirl_metadata().max_energy,
            SkillId::FlickerStrike => 1.0 - self.flicker.energy / flicker_strike_metadata().max_energy,
            _ => cooldowns.get(self.entity, skill).map_or(0.0, |entry| entry.remaining / entry.initial.max(1.0)),
        }
        .clamp(0.0, 1.0)
    }

    /// The stock a skill spends, if it counts one.
    fn stock(&self, skill: SkillId) -> Option<u32> {
        match skill {
            SkillId::BowAttack => {
                let arrows = self.weapons.equipped_ammo().filter(|ammo| *ammo == PlayerAmmo::Arrows);
                Some(arrows.map_or(0, |ammo| self.weapons.count_ammo(ammo)))
            },
            _ => skill.stock_material().map(|material| self.materials.backpack_count(material)),
        }
    }
}

fn advance_toolbar_icons(
    player: Query<ToolbarPlayer, With<Player>>,
    channeled: Query<&ChanneledSkill, With<Player>>,
    mut events: MessageReader<PassiveEvent>,
    cooldowns: Res<Cooldowns>,
    tether: Res<TetherConnectionStatus>,
    time: Res<GameTime>,
    mut icons: ResMut<ToolbarIcons>,
) {
    let Ok(player) = player.single() else { return };
    let used: HashSet<SkillId> = events
        .read()
        .filter_map(|event| match *event {
            PassiveEvent::SkillUsed { owner, skill } if owner == player.entity => Some(skill),
            _ => None,
        })
        .collect();
    let channeling: HashSet<SkillId> = channeled.iter().map(|channeled| channeled.skill).collect();
    let ToolbarIcons { states, channeled } = &mut *icons;
    channeled.extend(CHANNELED_SKILLS.into_iter().chain(channeling.iter().copied()));
    states.retain(|skill, _| player.skills.is_equipped(skill));
    for &skill in &player.skills.equipped {
        let recharges = channeled.contains(&skill);
        let lacks_means = player.lacks_means(skill, tether.available);
        let cooling = !cooldowns.is_ready(player.entity, skill, time.tick()) || skill == SkillId::Stealth && player.stealthed;
        let unavailable = lacks_means || !recharges && cooling;
        let energy_full = recharges.then(|| player.energy_full(skill));
        let state = states.entry(skill).or_default();
        state.advance(used.contains(&skill), channeling.contains(&skill), unavailable, !recharges, energy_full);
    }
}

/// Everything besides the player that the toolbar shows.
#[derive(SystemParam)]
struct ToolbarSources<'w> {
    icons: Res<'w, ToolbarIcons>,
    cooldowns: Res<'w, Cooldowns>,
    melee: Res<'w, PlayerMeleeWeapon>,
    unlocks: Res<'w, UnlockedSkillPresentationState>,
    pulse: Res<'w, AlignmentPulseSample>,
    time: Res<'w, Time<Real>>,
}

/// An equipped skill as the toolbar draws it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Slot {
    icon: &'static str,
    pressed: bool,
    gain: f32,
    /// Shaded rows of the icon; energy fills from the bottom and cooldowns clear from the top.
    shade: i32,
    energy: bool,
    /// Shown by the slot's text label.
    stock: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
struct Toolbar {
    slots: usize,
    skills: Vec<Slot>,
    health: f32,
    alignment: f32,
    elapsed_secs: f32,
    frame_pulse: f32,
}

fn present_toolbar(
    player: Query<ToolbarPlayer, With<Player>>,
    sources: ToolbarSources,
    library: Res<ArtLibrary>,
    mut images: ResMut<Assets<Image>>,
    mut toolbar: Query<(&ImageNode, &mut Node), With<ToolbarFrame>>,
    mut labels: Query<(&StockLabel, &mut Text, &mut Node), Without<ToolbarFrame>>,
    grid: UiPixelGrid,
) {
    let (Ok(player), Ok((image, mut node))) = (player.single(), toolbar.single_mut()) else { return };
    let skills = player.skills.equipped.iter().map(|&skill| {
        let state = sources.icons.states.get(&skill).copied().unwrap_or_default();
        let flash = sources.unlocks.entry(skill).map(|entry| unlock_gain(entry.remaining_ticks));
        Slot {
            icon: skill.icon_path_with_melee_weapon(*sources.melee),
            pressed: state.pressed(),
            gain: flash.unwrap_or(state.gain()),
            shade: (player.recharge(skill, &sources.cooldowns) * ICON as f32).ceil() as i32,
            energy: matches!(skill, SkillId::Whirl | SkillId::FlickerStrike),
            stock: player.stock(skill),
        }
    });
    let toolbar = Toolbar {
        slots: if player.passives.contains(&Passive::BatteryPack) { 5 } else { 4 },
        skills: skills.collect(),
        health: player.health.current as f32 / player.health.max.max(1) as f32,
        alignment: player.alignment.factor(),
        elapsed_secs: sources.time.elapsed_secs(),
        frame_pulse: sources.pulse.intensity,
    };
    let raster = draw_toolbar(&toolbar, &library.art(&images));
    let Some((layout, unit)) = grid.anchored(raster.size, Vec2::new(0.5, 1.0)).zip(grid.unit()) else { return };
    node.set_if_neq(layout);
    for (label, mut text, mut node) in &mut labels {
        let stock = toolbar.skills.get(label.0).filter(|_| label.0 < toolbar.slots).and_then(|slot| slot.stock);
        let hidden = Node { display: Display::None, position_type: PositionType::Absolute, ..default() };
        let (count, placement) = stock.map_or((String::new(), hidden), |stock| {
            (MaterialInventory::display_count(stock), stock_label_node(label.0, raster.size, unit))
        });
        if text.0 != count {
            text.0 = count;
        }
        node.set_if_neq(placement);
    }
    raster.publish(&mut images, &image.image);
}

/// Pins a slot's count just inside its icon's bottom-right corner.
fn stock_label_node(slot: usize, toolbar: UVec2, unit: f32) -> Node {
    let icon_end = FIRST_SLOT + IVec2::new(SLOT_STRIDE * slot as i32 + ICON, ICON);
    let inset = (toolbar.as_ivec2() - icon_end).as_vec2() * unit;
    Node {
        position_type: PositionType::Absolute,
        right: px(inset.x + 0.8 * unit),
        bottom: px(inset.y + 0.4 * unit),
        ..default()
    }
}

fn draw_toolbar(toolbar: &Toolbar, art: &Art) -> Raster {
    let slots = toolbar.slots as i32;
    let bar_width = slots * SLOT_STRIDE - 2;
    let mut canvas = Canvas::new(UVec2::new((slots * SLOT_STRIDE + 16) as u32, FRAME_HEIGHT as u32), art);
    draw_bar(&mut canvas, HEALTH_BAR, IVec2::new(bar_width, 2), toolbar.health, (HEALTH_TRACK, HEALTH_FILL));
    let alignment = alignment_colors(toolbar.alignment, toolbar.elapsed_secs);
    draw_bar(&mut canvas, ALIGNMENT_BAR, IVec2::new(bar_width, 1), toolbar.alignment, alignment);
    let frame = frame_path(toolbar.slots);
    let pulse = ALIGNMENT_PULSE_LINEAR_GRAY * toolbar.frame_pulse;
    let size = UVec2::new((slots * SLOT_STRIDE + 16) as u32, 30);
    canvas.sprite(frame, URect::from_corners(UVec2::Y * FRAME_TOP as u32, size), IVec2::ZERO, |texel| lift(texel, pulse));
    for (index, slot) in toolbar.skills.iter().take(toolbar.slots).enumerate() {
        draw_slot(&mut canvas, FIRST_SLOT + IVec2::X * SLOT_STRIDE * index as i32, slot);
    }
    canvas.finish()
}

fn draw_bar(canvas: &mut Canvas, at: IVec2, size: IVec2, fraction: f32, (track, fill): (Color, Color)) {
    let filled = (fraction.clamp(0.0, 1.0) * size.x as f32).ceil() as i32;
    canvas.fill(rect(at.x, at.y, size.x, size.y), track);
    canvas.fill(rect(at.x, at.y, filled, size.y), fill);
}

/// A pressed icon sinks into its slot: its outer ring gives way to the frame's.
fn draw_slot(canvas: &mut Canvas, at: IVec2, slot: &Slot) {
    let inset = u32::from(slot.pressed);
    let source = URect::new(inset, inset, ICON as u32 - inset, ICON as u32 - inset);
    canvas.sprite(slot.icon, source, at + inset as i32, |texel| gain(texel, slot.gain));
    let shade = slot.shade.clamp(0, ICON);
    let top = if slot.energy { at.y } else { at.y + ICON - shade };
    canvas.fill(rect(at.x, top, ICON, shade), COOLDOWN_SHADE);
}

/// Track and fill colors; low Alignment pulses toward a warning color, and depletion pulses the track.
fn alignment_colors(fraction: f32, elapsed_secs: f32) -> (Color, Color) {
    let pulse = |hz: f32| 0.5 + 0.5 * (elapsed_secs * hz * TAU).sin();
    let fraction = fraction.clamp(0.0, 1.0);
    if fraction > ALIGNMENT_LOW_THRESHOLD {
        return (ALIGNMENT_TRACK, ALIGNMENT_FILL);
    }
    if fraction <= 0.0 {
        let strength = 0.18 + pulse(ALIGNMENT_DEPLETED_PULSE_HZ) * 0.22;
        return (mix(ALIGNMENT_TRACK, ALIGNMENT_DEPLETED, strength), ALIGNMENT_FILL);
    }
    let urgency = 0.2 + 0.8 * (1.0 - fraction / ALIGNMENT_LOW_THRESHOLD);
    let strength = urgency * pulse(ALIGNMENT_LOW_PULSE_HZ);
    (mix(ALIGNMENT_TRACK, ALIGNMENT_WARNING, strength * 0.12), mix(ALIGNMENT_FILL, ALIGNMENT_WARNING, strength * 0.32))
}

fn mix(from: Color, to: Color, amount: f32) -> Color {
    from.to_linear().mix(&to.to_linear(), amount).into()
}

/// The capsule station in reach and its accent, set by the capsule UI.
#[derive(Resource, Default)]
pub(crate) struct StationPrompt(pub(crate) Option<(&'static str, Color)>);

/// What the interact key does nearby: use a capsule station or enter a shelter.
#[derive(SystemParam)]
struct InteractPrompt<'w> {
    station: Res<'w, StationPrompt>,
    shelter: Res<'w, InteractionFocus>,
}

impl InteractPrompt<'_> {
    fn label(&self) -> Option<(&'static str, Color)> {
        self.station.0.or_else(|| self.shelter.has_target().then_some(("ENTER", TEXT)))
    }
}

#[derive(Component)]
struct PromptPlate;

/// Rests the interact prompt on top of the toolbar.
fn present_prompt(
    mut commands: Commands,
    prompt: InteractPrompt,
    library: Res<ArtLibrary>,
    mut images: ResMut<Assets<Image>>,
    mut shown: Query<(Entity, &ImageNode, &mut Node), With<PromptPlate>>,
    toolbar: Query<Entity, With<ToolbarFrame>>,
    grid: UiPixelGrid,
) {
    let Ok(toolbar) = toolbar.single() else { return };
    let content = prompt.label().zip(grid.unit()).map(|((label, color), unit)| {
        let raster = draw_prompt(label, color, &library.art(&images));
        let size = raster.size.as_vec2() * unit;
        let node = Node {
            position_type: PositionType::Absolute,
            left: percent(50.0),
            top: px(-size.y),
            margin: UiRect::left(px(-size.x / 2.0)),
            width: px(size.x),
            height: px(size.y),
            ..default()
        };
        (node, raster)
    });
    show_raster(&mut commands, &mut images, shown.single_mut().ok(), content, |image, node| {
        (Name::new("InteractPrompt"), PromptPlate, image, node, ZIndex(PANEL_Z_INDEX), ChildOf(toolbar))
    });
}

/// A level-up announcement; `age` counts from when it was queued.
struct Caption {
    lines: Vec<String>,
    age: f32,
}

impl Caption {
    /// Fully visible after a short delay, then fading out; `None` before it appears.
    fn opacity(&self) -> Option<f32> {
        let shown = self.age - CAPTION_DELAY_SECS;
        (shown >= 0.0).then(|| (1.0 - (shown - CAPTION_HOLD_SECS).max(0.0) / (CAPTION_TOTAL_SECS - CAPTION_HOLD_SECS)).max(0.0))
    }
}

#[derive(Resource, Default)]
struct Captions(Vec<Caption>);

fn queue_level_up_captions(
    mut events: MessageReader<AdaptationLevelUpEvent>,
    mut captions: ResMut<Captions>,
    time: Res<Time<Real>>,
) {
    for caption in &mut captions.0 {
        caption.age += time.delta_secs();
    }
    captions.0.retain(|caption| caption.age < CAPTION_DELAY_SECS + CAPTION_TOTAL_SECS);
    for event in events.read() {
        let unlocked = event.unlocked_skill.map(|skill| format!("NEW ABILITY UNLOCKED: {}", skill.name().to_uppercase()));
        let lines = std::iter::once("ADAPTATION INCREASED".to_owned()).chain(unlocked).collect();
        if captions.0.len() < CAPTION_LIMIT {
            captions.0.push(Caption { lines, age: 0.0 });
        }
    }
}

#[derive(Component)]
struct CaptionStack;

fn present_captions(
    mut commands: Commands,
    captions: Res<Captions>,
    library: Res<ArtLibrary>,
    mut images: ResMut<Assets<Image>>,
    mut shown: Query<(Entity, &ImageNode, &mut Node), With<CaptionStack>>,
    grid: UiPixelGrid,
) {
    let visible: Vec<_> = captions.0.iter().filter_map(|caption| Some((&caption.lines[..], caption.opacity()?))).collect();
    let content = (!visible.is_empty()).then(|| draw_captions(&visible, &library.art(&images))).and_then(|raster| {
        let mut node = grid.anchored(raster.size, Vec2::new(0.5, 0.0))?;
        node.top = px(CAPTION_TOP as f32 * grid.unit()?);
        Some((node, raster))
    });
    show_raster(&mut commands, &mut images, shown.single_mut().ok(), content, |image, node| {
        (Name::new("LevelUpCaptions"), CaptionStack, image, node, ZIndex(CAPTION_Z_INDEX), StateDespawnMarker)
    });
}

/// Stacked plates, each fading with its caption's `opacity`.
fn draw_captions(captions: &[(&[String], f32)], art: &Art) -> Raster {
    let widest = captions.iter().flat_map(|(lines, _)| lines.iter()).map(|line| text_width(line)).max().unwrap_or(0);
    let width = widest + 12;
    let heights: Vec<i32> = captions.iter().map(|(lines, _)| lines.len() as i32 * 10 + 5).collect();
    let height = heights.iter().map(|height| height + 1).sum::<i32>() - 1;
    let mut canvas = Canvas::new(UVec2::new(width as u32, height.max(1) as u32), art);
    let mut top = 0;
    for ((lines, opacity), plate_height) in captions.iter().zip(heights) {
        let fill = Color::srgba(0.02, 0.03, 0.05, 0.72 * opacity);
        canvas.plate(rect(0, top, width, plate_height), 1, fill, Color::WHITE.with_alpha(0.28 * opacity));
        for (index, line) in lines.iter().enumerate() {
            let at = IVec2::new((width - text_width(line)) / 2, top + 4 + 10 * index as i32);
            canvas.text(at, line, CAPTION_COLOR.with_alpha(*opacity));
        }
        top += plate_height + 1;
    }
    canvas.finish()
}

#[cfg(test)]
mod tests;
