//! The Essence HUD: a plate whose frame shows planet immunity, and the Essence total, whose
//! changed digits animate in. A pixel canvas at the window's top-left corner.
use std::collections::VecDeque;

use super::pixel_canvas::{Art, ArtLibrary, Canvas, Raster};
use super::pixel_panel::UiPixelGrid;
use crate::game::combat::{KillCount, PlanetImmunityLevel};
use crate::game::player::progression::apply_essence_from_xp_orbs;
use crate::game::player::spawn::PersistedPlayerProgression;
use crate::game::player::{Essence, Player};
use crate::prelude::*;

/// Immunity levels side by side, one plate each.
const PLATE_PATH: &str = "ui/game/HUD1.png";
const PLATE: UVec2 = UVec2::new(55, 9);
/// Each digit's sheet holds its animation frames side by side.
const DIGIT_PATHS: [&str; 10] = [
    "animations/numbers/0__r1c4__3x10.png",
    "animations/numbers/1__r1c4__3x10.png",
    "animations/numbers/2__r1c4__3x10.png",
    "animations/numbers/3__r1c4__3x10.png",
    "animations/numbers/4__r1c4__3x10.png",
    "animations/numbers/5__r1c4__3x10.png",
    "animations/numbers/6__r1c4__3x10.png",
    "animations/numbers/7__r1c4__3x10.png",
    "animations/numbers/8__r1c4__3x10.png",
    "animations/numbers/9__r1c4__3x10.png",
];
const DIGIT: UVec2 = UVec2::new(3, 10);
const FIRST_DIGIT_X: i32 = 14;
const DIGIT_STEP: i32 = 4;
const DIGIT_SLOT_COUNT: usize = 3;
const ESSENCE_MINIMUM_HOLD_TICKS: u8 = 16;
const DIGIT_FRAME_COUNT: usize = 4;
const DIGIT_TICKS_PER_FRAME: u8 = 4;
const HUD_Z_INDEX: i32 = 25;

pub struct PlanetStatusHudPlugin;

impl Plugin for PlanetStatusHudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_planet_status_art)
            .add_systems(OnEnter(AppState::InGame), spawn_planet_status_hud)
            .add_systems(
                GameTickUpdate,
                update_essence_presentation.after(apply_essence_from_xp_orbs).run_if(in_state(AppState::InGame)),
            )
            .add_systems(Update, present_planet_status.run_if(in_state(AppState::InGame)));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EssenceDigitAnimation {
    frame: usize,
    ticks_on_frame: u8,
    playing: bool,
}

impl EssenceDigitAnimation {
    const fn new(playing: bool) -> Self {
        Self { frame: 0, ticks_on_frame: 0, playing }
    }

    fn tick(&mut self) -> Option<usize> {
        if !self.playing {
            return None;
        }
        self.ticks_on_frame += 1;
        if self.ticks_on_frame < DIGIT_TICKS_PER_FRAME {
            return None;
        }
        self.ticks_on_frame = 0;
        self.frame = (self.frame + 1).min(DIGIT_FRAME_COUNT - 1);
        self.playing = self.frame + 1 < DIGIT_FRAME_COUNT;
        Some(self.frame)
    }
}

/// Shows each new Essence total for a minimum time, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
struct EssencePresentation {
    displayed: u32,
    pending: VecDeque<u32>,
    hold_ticks: u8,
}

impl EssencePresentation {
    fn new(displayed: u32) -> Self {
        Self { displayed, pending: VecDeque::new(), hold_ticks: 0 }
    }

    fn enqueue(&mut self, total: u32) {
        let latest = self.pending.back().copied().unwrap_or(self.displayed);
        if total != latest {
            self.pending.push_back(total);
        }
    }

    fn tick(&mut self) -> Option<u32> {
        self.hold_ticks = self.hold_ticks.saturating_sub(1);
        if self.hold_ticks > 0 {
            return None;
        }
        let next = self.pending.pop_front()?;
        self.displayed = next;
        self.hold_ticks = ESSENCE_MINIMUM_HOLD_TICKS;
        Some(next)
    }
}

/// The left-aligned digits of an Essence total.
fn essence_digits(total: u32) -> [Option<u8>; DIGIT_SLOT_COUNT] {
    let text = total.min(Essence::MAX).to_string();
    std::array::from_fn(|slot| text.as_bytes().get(slot).map(|digit| digit - b'0'))
}

#[derive(Resource)]
struct EssenceHud {
    essence: EssencePresentation,
    digits: [Option<(u8, EssenceDigitAnimation)>; DIGIT_SLOT_COUNT],
}

impl EssenceHud {
    fn new(total: u32) -> Self {
        let digits = essence_digits(total).map(|digit| digit.map(|digit| (digit, EssenceDigitAnimation::new(true))));
        Self { essence: EssencePresentation::new(total), digits }
    }
}

#[derive(Component)]
struct PlanetStatusHud;

fn load_planet_status_art(mut library: ResMut<ArtLibrary>, server: Res<AssetServer>) {
    library.load(&server, DIGIT_PATHS.into_iter().chain([PLATE_PATH]));
}

/// Spawned with the game rather than on first draw, so the HUD is hidden from its first frame
/// whenever gameplay presentation is suspended.
fn spawn_planet_status_hud(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    persisted: Res<PersistedPlayerProgression>,
) {
    commands.insert_resource(EssenceHud::new(persisted.essence.unwrap_or_default().get()));
    let empty = Canvas::new(UVec2::ONE, &Art::default()).finish();
    commands.spawn((
        Name::new("PlanetStatusHud"),
        PlanetStatusHud,
        ImageNode::new(images.add(empty.into_image())),
        Node::default(),
        ZIndex(HUD_Z_INDEX),
        StateDespawnMarker,
        crate::ui::GameplayHud,
    ));
}

fn update_essence_presentation(essence: Query<Ref<Essence>, With<Player>>, hud: Option<ResMut<EssenceHud>>) {
    let (Ok(essence), Some(mut hud)) = (essence.single(), hud) else { return };
    for (_, animation) in hud.digits.iter_mut().flatten() {
        animation.tick();
    }
    if essence.is_changed() {
        hud.essence.enqueue(essence.get());
    }
    let Some(total) = hud.essence.tick() else { return };
    for (slot, digit) in essence_digits(total).into_iter().enumerate() {
        if hud.digits[slot].map(|(shown, _)| shown) != digit {
            hud.digits[slot] = digit.map(|digit| (digit, EssenceDigitAnimation::new(true)));
        }
    }
}

fn present_planet_status(
    hud: Option<Res<EssenceHud>>,
    kills: Res<KillCount>,
    library: Res<ArtLibrary>,
    mut images: ResMut<Assets<Image>>,
    mut shown: Query<(&ImageNode, &mut Node), With<PlanetStatusHud>>,
    grid: UiPixelGrid,
) {
    let (Some(hud), Ok((image, mut node))) = (hud, shown.single_mut()) else { return };
    let raster = draw_planet_status(&hud, PlanetImmunityLevel::from_kills(**kills), &library.art(&images));
    if let Some(layout) = grid.anchored(raster.size, Vec2::ZERO) {
        node.set_if_neq(layout);
    }
    raster.publish(&mut images, &image.image);
}

fn draw_planet_status(hud: &EssenceHud, immunity: PlanetImmunityLevel, art: &Art) -> Raster {
    let mut canvas = Canvas::new(PLATE, art);
    let plate = UVec2::X * PLATE.x * immunity.atlas_index() as u32;
    canvas.sprite(PLATE_PATH, URect::from_corners(plate, plate + PLATE), IVec2::ZERO, |texel| texel);
    for (slot, (digit, animation)) in hud.digits.iter().enumerate().filter_map(|(slot, digit)| Some((slot, (*digit)?))) {
        let frame = UVec2::X * DIGIT.x * animation.frame as u32;
        let at = IVec2::X * (FIRST_DIGIT_X + DIGIT_STEP * slot as i32);
        canvas.sprite(DIGIT_PATHS[usize::from(digit)], URect::from_corners(frame, frame + DIGIT), at, |texel| texel);
    }
    canvas.finish()
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;

    use super::*;

    #[test]
    fn essence_digits_are_left_aligned_and_limited_to_three_slots() {
        assert_eq!(essence_digits(0), [Some(0), None, None]);
        assert_eq!(essence_digits(42), [Some(4), Some(2), None]);
        assert_eq!(essence_digits(999), [Some(9), Some(9), Some(9)]);
        assert_eq!(essence_digits(u32::MAX), [Some(9), Some(9), Some(9)]);
    }

    #[test]
    fn essence_presentation_holds_each_queued_total_for_sixteen_ticks() {
        let mut presentation = EssencePresentation::new(0);
        presentation.enqueue(3);
        assert_eq!(presentation.tick(), Some(3));

        presentation.enqueue(6);
        for _ in 0..ESSENCE_MINIMUM_HOLD_TICKS - 1 {
            assert_eq!(presentation.tick(), None);
            assert_eq!(presentation.displayed, 3);
        }
        assert_eq!(presentation.tick(), Some(6));
        assert_eq!(presentation.displayed, 6);
    }

    #[test]
    fn essence_digit_animation_matches_four_tick_sheet_frames() {
        let mut animation = EssenceDigitAnimation::new(true);
        let expected_frames = [None, None, None, Some(1), None, None, None, Some(2), None, None, None, Some(3)];
        for expected_frame in expected_frames {
            assert_eq!(animation.tick(), expected_frame);
        }
        assert_eq!(animation.tick(), None);
        assert!(!animation.playing);
    }

    #[test]
    fn only_changed_digits_restart_their_animation() {
        let mut world = World::new();
        let mut hud = EssenceHud::new(12);
        for (_, animation) in hud.digits.iter_mut().flatten() {
            animation.playing = false;
        }
        world.insert_resource(hud);
        let mut essence = Essence::default();
        essence.grant(15);
        world.spawn((Player, essence));
        world.run_system_once(update_essence_presentation).unwrap();
        let digits = world.resource::<EssenceHud>().digits;
        assert!(digits[0].is_some_and(|(digit, animation)| digit == 1 && !animation.playing));
        assert!(digits[1].is_some_and(|(digit, animation)| digit == 5 && animation.playing));
    }
}
