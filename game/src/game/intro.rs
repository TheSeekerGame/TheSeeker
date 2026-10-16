//! Opening cinematic of a new expedition: the escape pod falls through the dusk sky and
//! crashes into the spawn crater, and the player takes over inside the capsule.
//!
//! Effect art comes from `tools/surface_art/generate.py`; the sky it falls past is
//! `levelgen::surface_backdrop`. The timeline counts game ticks, so it pauses with the game.
//! Gameplay input skips to the reveal; menu input leaves the cinematic paused.

use bevy::sprite::Anchor;
use leafwing_input_manager::prelude::ActionState;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::script::ScriptPlayer;

use crate::camera::{CameraShake, ScriptedCameraFocus, SnapCameraNext};
use crate::game::player::spawns::tether_node::{CapsuleInfluenceGrowth, TetherRangeArtwork};
use crate::game::player::{Player, PlayerAction, PlayerStateSet};
use crate::game::world_objects::{
    Capsule, CapsuleOccupant, HoldSpawnOccupancy, PendingSpawnOccupancy, CAPSULE_ART_FADE_TICKS,
};
use crate::graphics::pixel_perfect::pixel_perfect_layers;
use crate::levelgen::impact_crater::{ImpactFormed, ImpactIntact};
use crate::levelgen::pipeline::{BuildPhase, GenerationBuild};
use crate::levelgen::surface_backdrop::SurfaceBackdropImages;
use crate::prelude::*;
use crate::ui::GameplayHud;

/// Camera height above the resting capsule view when the cinematic opens.
const START_ALTITUDE: f32 = 1500.0;
const FADE_IN: TickSpan = TickSpan::new(0, 72);
/// The distant pod drops into view, then holds just above the frame center.
const EMBER_ENTER: TickSpan = TickSpan::new(48, 144);
const DESCENT: TickSpan = TickSpan::new(96, 509);
/// The camera outruns the pod, which slips out of the top of the frame.
const EMBER_EXIT: TickSpan = TickSpan::new(374, 432);
const TRAIL_PUFF_INTERVAL: u32 = 4;
/// Last ticks before impact: the pod streaks into the crater.
const PLUNGE: TickSpan = TickSpan::new(556, 576);
const IMPACT_TICK: u32 = PLUNGE.end;
const FLASH: TickSpan = TickSpan::new(IMPACT_TICK, IMPACT_TICK + 30);
/// The dust has cleared enough to reveal the capsule.
const REVEAL_TICK: u32 = IMPACT_TICK + 246;
/// Ticks after the capsule opens before the player appears inside, as the interior starts
/// showing through the closed shell's fade.
const PLAYER_REVEAL_DELAY_TICKS: u32 = CAPSULE_ART_FADE_TICKS / 4;
/// Presses this early still belong to the menu that started the game.
const SKIP_GRACE_TICKS: u32 = 24;

/// Screen offsets of the falling pod above the frame center.
const EMBER_TOP_OFFSET: f32 = 125.0;
const EMBER_CRUISE_OFFSET: f32 = 38.0;
const EMBER_EXIT_OFFSET: f32 = 140.0;
const PLUNGE_START_OFFSET: f32 = 175.0;

/// Positions relative to the capsule anchor.
const POD_CENTER_OFFSET: Vec2 = Vec2::new(0.0, 73.0);
/// The dust's base sits below the crater floor, where the crater walls hide it.
const DUST_BASE_OFFSET: Vec2 = Vec2::new(0.0, 12.0);
const BURST_BASE_OFFSET: Vec2 = Vec2::new(0.0, 24.0);

/// The pod, burst, and dust draw in front of the capsule (z -2e-6) but behind the terrain
/// (z 0), so the crater's near wall hides whatever sinks into the bowl.
const EMBER_Z: f32 = -0.5e-6;
const BURST_Z: f32 = -0.8e-6;
const DUST_Z: f32 = -1.0e-6;
const TRAIL_Z: f32 = -1.2e-6;
/// Debris and sparks fly out over the rim.
const DEBRIS_Z: f32 = 0.6;
const SPARK_Z: f32 = 0.7;
/// Screen overlays sit just under the screen frame (z 24).
const OVERLAY_Z_INDEX: i32 = 23;

const DEBRIS_IMAGE: &str = "animations/intro/Debris__r1c8__8x8.png";
const DEBRIS_COUNT: usize = 18;
const SPARK_COUNT: usize = 44;
/// Spark colors from ignition to burnout; sparks blend through them and fade out.
const SPARK_COLORS: [Color; 4] = [
    Color::srgb(1.0, 0.99, 0.96),
    Color::srgb(1.0, 0.79, 0.47),
    Color::srgb(0.97, 0.58, 0.32),
    Color::srgb(0.49, 0.29, 0.40),
];

pub struct IntroPlugin;

impl Plugin for IntroPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), prepare_intro);
        app.add_systems(
            Update,
            (begin_intro, request_intro_skip)
                .run_if(resource_exists::<Intro>)
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            GameTickUpdate,
            (
                suppress_intro_input,
                advance_intro_clock,
                direct_intro_camera,
                animate_falling_pod,
                trigger_impact,
                simulate_intro_particles,
                expire_intro_effects,
                fade_intro_overlays,
                finish_intro,
            )
                .chain()
                .before(PlayerStateSet::Input)
                .run_if(resource_exists::<Intro>)
                .run_if(in_state(GameState::Playing))
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(OnExit(AppState::InGame), discard_intro);
    }
}

/// Plays the landing cinematic once the next world has finished generating.
#[derive(Resource)]
pub(crate) struct IntroRequested;

#[derive(Resource)]
struct Intro {
    stage: IntroStage,
    tick: u32,
    /// Tick at which the player became the capsule's occupant.
    occupied_at: Option<u32>,
    skip: bool,
    rng: StdRng,
    scene: Option<IntroScene>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum IntroStage {
    /// The world is still generating behind a black cover.
    Waiting,
    Playing,
    /// Occupancy is resolving; the player appears once they are inside the capsule.
    Revealing,
}

/// World entities the cinematic stages.
#[derive(Clone, Copy)]
struct IntroScene {
    capsule: Entity,
    capsule_anchor: Vec2,
    capsule_gfx: Entity,
    player_gfx: Entity,
}

#[derive(Clone, Copy)]
struct TickSpan {
    start: u32,
    end: u32,
}

impl TickSpan {
    const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    fn contains(self, tick: u32) -> bool {
        (self.start..self.end).contains(&tick)
    }

    /// 0 before the span, 1 after it.
    fn progress(self, tick: u32) -> f32 {
        ((tick as f32 - self.start as f32) / (self.end - self.start) as f32).clamp(0.0, 1.0)
    }
}

/// Terrain and backdrop art that differs before and after the crater forms.
type CraterStateQuery<'w, 's> =
    Query<'w, 's, (Entity, Has<ImpactFormed>), Or<(With<ImpactFormed>, With<ImpactIntact>)>>;

fn show_crater(formed: bool, crater_q: &CraterStateQuery, visibility_q: &mut Query<&mut Visibility>) {
    for (entity, is_formed) in crater_q.iter() {
        let Ok(mut visibility) = visibility_q.get_mut(entity) else { continue };
        *visibility = if is_formed == formed { Visibility::Inherited } else { Visibility::Hidden };
    }
}

/// Everything spawned for the cinematic; despawned when it ends.
#[derive(Component)]
struct IntroEntity;

/// Effect with a fixed lifetime in intro ticks.
#[derive(Component)]
struct IntroEffect {
    despawn_tick: u32,
}

#[derive(Component)]
struct FallingEmber;

#[derive(Component)]
struct PlungingPod;

#[derive(Component)]
struct IntroCover;

#[derive(Component)]
struct IntroFlash;

#[derive(Component)]
struct IntroParticle {
    velocity: Vec2,
    gravity: f32,
    drag: f32,
    look: ParticleLook,
}

#[derive(Clone, Copy)]
enum ParticleLook {
    Spark { birth: u32, lifetime: u32 },
    Debris { variant: usize, spin_period: u32 },
}

fn prepare_intro(mut commands: Commands, requested: Option<Res<IntroRequested>>) {
    if requested.is_none() {
        return;
    }
    commands.remove_resource::<IntroRequested>();
    commands.insert_resource(HoldSpawnOccupancy);
    commands.insert_resource(Intro {
        stage: IntroStage::Waiting,
        tick: 0,
        occupied_at: None,
        skip: false,
        rng: StdRng::seed_from_u64(0x1A4D_17F0),
        scene: None,
    });
    commands.spawn((
        Name::new("IntroCover"),
        IntroEntity,
        IntroCover,
        full_screen_node(),
        BackgroundColor(Color::BLACK),
        ZIndex(OVERLAY_Z_INDEX),
        StateDespawnMarker,
    ));
}

fn full_screen_node() -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Px(0.0),
        right: Val::Px(0.0),
        top: Val::Px(0.0),
        bottom: Val::Px(0.0),
        ..default()
    }
}

#[expect(clippy::too_many_arguments, reason = "Starting the cinematic waits on the world, backdrop, and staged entities.")]
fn begin_intro(
    mut commands: Commands,
    mut intro: ResMut<Intro>,
    build: Option<Res<GenerationBuild>>,
    backdrop: Option<Res<SurfaceBackdropImages>>,
    asset_server: Res<AssetServer>,
    capsule_q: Query<(Entity, &Transform, &Gent), With<Capsule>>,
    player_q: Query<(&Transform, &Gent), With<Player>>,
    crater_q: CraterStateQuery,
    mut visibility_q: Query<&mut Visibility>,
) {
    if intro.stage != IntroStage::Waiting {
        return;
    }
    if !build.is_some_and(|build| build.phase == BuildPhase::Ready) {
        return;
    }
    let Some(backdrop) = backdrop else { return };
    if !backdrop.0.iter().all(|image| asset_server.is_loaded_with_dependencies(image)) {
        return;
    }
    let (Ok((capsule_entity, capsule, capsule_gent)), Ok((player, player_gent))) = (capsule_q.single(), player_q.single()) else {
        return;
    };

    let scene = IntroScene {
        capsule: capsule_entity,
        capsule_anchor: capsule.translation.truncate(),
        capsule_gfx: capsule_gent.e_gfx,
        player_gfx: player_gent.e_gfx,
    };
    for entity in [scene.capsule_gfx, scene.player_gfx] {
        if let Ok(mut visibility) = visibility_q.get_mut(entity) {
            *visibility = Visibility::Hidden;
        }
    }
    // The ground is still intact; the impact forms the crater.
    show_crater(false, &crater_q, &mut visibility_q);
    let rest = Vec2::new(scene.capsule_anchor.x, player.translation.y);
    commands.insert_resource(ScriptedCameraFocus(rest + Vec2::Y * START_ALTITUDE));
    intro.scene = Some(scene);
    intro.stage = IntroStage::Playing;
    intro.tick = 0;
}

fn request_intro_skip(
    mut intro: ResMut<Intro>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    gamepads: Query<&Gamepad>,
    panels: crate::ui::OpenPanels,
    pause: Res<crate::gamestate::PauseRequests>,
) {
    if panels.any() || panels.menu_input_consumed() || pause.is_paused() { return; }
    if intro.stage != IntroStage::Playing || intro.tick < SKIP_GRACE_TICKS {
        return;
    }
    let pressed = keys.get_just_pressed().any(|key| !matches!(key, KeyCode::KeyP | KeyCode::Escape | KeyCode::KeyM))
        || mouse.get_just_pressed().next().is_some()
        || gamepads.iter().any(|gamepad| gamepad.get_just_pressed().next().is_some());
    if pressed {
        intro.skip = true;
    }
}

fn advance_intro_clock(
    mut commands: Commands,
    mut intro: ResMut<Intro>,
    effects_q: Query<Entity, With<IntroEntity>>,
    crater_q: CraterStateQuery,
    mut visibility_q: Query<&mut Visibility>,
) {
    if intro.stage == IntroStage::Waiting {
        return;
    }
    intro.tick += 1;
    if intro.stage == IntroStage::Revealing || (!intro.skip && intro.tick < REVEAL_TICK) {
        return;
    }
    // Skipping drops whatever is on screen and lands the capsule immediately.
    if intro.skip {
        for entity in effects_q.iter() {
            commands.entity(entity).despawn();
        }
    }
    if let Some(scene) = intro.scene {
        if let Ok(mut visibility) = visibility_q.get_mut(scene.capsule_gfx) {
            *visibility = Visibility::Visible;
        }
    }
    show_crater(true, &crater_q, &mut visibility_q);
    commands.remove_resource::<HoldSpawnOccupancy>();
    intro.stage = IntroStage::Revealing;
}

fn direct_intro_camera(
    intro: Res<Intro>,
    mut focus: Option<ResMut<ScriptedCameraFocus>>,
    player_q: Query<&Transform, With<Player>>,
) {
    let (Some(scene), Some(focus), Ok(player)) = (intro.scene, focus.as_mut(), player_q.single()) else {
        return;
    };
    let rest = Vec2::new(scene.capsule_anchor.x, player.translation.y);
    focus.0 = rest + Vec2::Y * camera_altitude(intro.tick, intro.stage);
}

fn camera_altitude(tick: u32, stage: IntroStage) -> f32 {
    if stage != IntroStage::Playing {
        return 0.0;
    }
    START_ALTITUDE * (1.0 - smootherstep(DESCENT.progress(tick)))
}

fn smootherstep(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

/// Height of the distant pod above the frame center, while it is on screen.
fn ember_screen_offset(tick: u32) -> Option<f32> {
    if EMBER_ENTER.contains(tick) {
        let t = ease_out_cubic(EMBER_ENTER.progress(tick));
        return Some(EMBER_TOP_OFFSET + (EMBER_CRUISE_OFFSET - EMBER_TOP_OFFSET) * t);
    }
    if (EMBER_ENTER.end..EMBER_EXIT.start).contains(&tick) {
        return Some(EMBER_CRUISE_OFFSET + 2.0 * (tick as f32 * 0.11).sin());
    }
    if EMBER_EXIT.contains(tick) {
        let t = EMBER_EXIT.progress(tick).powi(2);
        return Some(EMBER_CRUISE_OFFSET + (EMBER_EXIT_OFFSET - EMBER_CRUISE_OFFSET) * t);
    }
    None
}

fn animate_falling_pod(
    mut commands: Commands,
    mut intro: ResMut<Intro>,
    focus: Option<Res<ScriptedCameraFocus>>,
    mut ember_q: Query<(Entity, &mut Transform), (With<FallingEmber>, Without<PlungingPod>)>,
    mut plunge_q: Query<(Entity, &mut Transform), (With<PlungingPod>, Without<FallingEmber>)>,
) {
    let (Some(scene), Some(focus)) = (intro.scene, focus) else { return };
    if intro.stage != IntroStage::Playing {
        return;
    }
    let tick = intro.tick;
    let ember = ember_screen_offset(tick).map(|offset| focus.0 + Vec2::Y * offset);
    match (ember, ember_q.single_mut()) {
        (Some(position), Ok((_, mut transform))) => {
            transform.translation = position.extend(EMBER_Z);
        },
        (Some(position), Err(_)) => {
            spawn_animated(&mut commands, "anim.intro.PodEmber", position, EMBER_Z, Anchor::BOTTOM_CENTER)
                .insert(FallingEmber);
        },
        (None, Ok((entity, _))) => commands.entity(entity).despawn(),
        (None, Err(_)) => {},
    }
    if let Some(position) = ember.filter(|_| tick.is_multiple_of(TRAIL_PUFF_INTERVAL)) {
        let jitter = Vec2::new(intro.rng.random_range(-1.0..=1.0), 3.0);
        spawn_animated(&mut commands, "anim.intro.TrailPuff", position + jitter, TRAIL_Z, Anchor::CENTER)
            .insert(IntroEffect { despawn_tick: tick + 36 });
    }

    let pod = scene.capsule_anchor + POD_CENTER_OFFSET;
    let plunge = PLUNGE.contains(tick).then(|| {
        pod + Vec2::Y * PLUNGE_START_OFFSET * (1.0 - PLUNGE.progress(tick))
    });
    match (plunge, plunge_q.single_mut()) {
        (Some(position), Ok((_, mut transform))) => {
            transform.translation = position.extend(EMBER_Z);
        },
        (Some(position), Err(_)) => {
            // The sphere sits 60 px below the sprite center.
            let anchor = Anchor(Vec2::new(0.0, -60.0 / 180.0));
            spawn_animated(&mut commands, "anim.intro.PodStreak", position, EMBER_Z, anchor)
                .insert(PlungingPod);
        },
        (None, Ok((entity, _))) => commands.entity(entity).despawn(),
        (None, Err(_)) => {},
    }
}

fn spawn_animated<'a>(
    commands: &'a mut Commands,
    key: &str,
    position: Vec2,
    z: f32,
    anchor: Anchor,
) -> EntityCommands<'a> {
    let mut animation = ScriptPlayer::<SpriteAnimation>::default();
    animation.play_key(key);
    commands.spawn((
        Name::new("IntroEffect"),
        IntroEntity,
        Sprite {
            texture_atlas: Some(TextureAtlas::default()),
            ..default()
        },
        anchor,
        animation,
        Transform::from_translation(position.extend(z)),
        pixel_perfect_layers(),
        StateDespawnMarker,
    ))
}

fn trigger_impact(
    mut commands: Commands,
    mut intro: ResMut<Intro>,
    asset_server: Res<AssetServer>,
    mut atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    crater_q: CraterStateQuery,
    mut visibility_q: Query<&mut Visibility>,
) {
    if intro.stage != IntroStage::Playing || intro.tick != IMPACT_TICK {
        return;
    }
    let Some(scene) = intro.scene else { return };
    if let Ok(mut visibility) = visibility_q.get_mut(scene.capsule_gfx) {
        *visibility = Visibility::Visible;
    }
    // The flash hides the switch from intact ground to the fresh crater.
    show_crater(true, &crater_q, &mut visibility_q);
    commands.insert_resource(CameraShake::new(6.0, 0.7, 16.0));
    commands.spawn((
        Name::new("IntroFlash"),
        IntroEntity,
        IntroFlash,
        full_screen_node(),
        BackgroundColor(Color::WHITE),
        ZIndex(OVERLAY_Z_INDEX),
        StateDespawnMarker,
    ));
    let tick = intro.tick;
    spawn_animated(&mut commands, "anim.intro.ImpactBurst", scene.capsule_anchor + BURST_BASE_OFFSET, BURST_Z, Anchor::BOTTOM_CENTER)
        .insert(IntroEffect { despawn_tick: tick + 28 });
    spawn_animated(&mut commands, "anim.intro.DustCloud", scene.capsule_anchor + DUST_BASE_OFFSET, DUST_Z, Anchor::BOTTOM_CENTER)
        .insert(IntroEffect { despawn_tick: tick + 240 });

    let origin = scene.capsule_anchor + POD_CENTER_OFFSET - Vec2::Y * 20.0;
    let debris_layout = atlas_layouts.add(TextureAtlasLayout::from_grid(UVec2::splat(8), 8, 1, None, None));
    let debris_image = asset_server.load(DEBRIS_IMAGE);
    for _ in 0..DEBRIS_COUNT {
        let rng = &mut intro.rng;
        let velocity = Vec2::new(rng.random_range(0.8..3.4) * rng.random_range(-1.0f32..1.0).signum(), rng.random_range(2.4..5.2));
        let look = ParticleLook::Debris { variant: rng.random_range(0..2), spin_period: rng.random_range(3..7) };
        commands.spawn((
            Name::new("IntroDebris"),
            IntroEntity,
            IntroEffect { despawn_tick: tick + 170 },
            IntroParticle { velocity, gravity: 0.11, drag: 0.995, look },
            Sprite::from_atlas_image(debris_image.clone(), TextureAtlas { layout: debris_layout.clone(), index: 0 }),
            Transform::from_translation((origin + Vec2::new(rng.random_range(-14.0..14.0), 0.0)).extend(DEBRIS_Z)),
            pixel_perfect_layers(),
            StateDespawnMarker,
        ));
    }
    for _ in 0..SPARK_COUNT {
        let rng = &mut intro.rng;
        let angle = rng.random_range(0.1..std::f32::consts::PI - 0.1);
        let speed = rng.random_range(1.6..6.0);
        let lifetime = rng.random_range(26..72);
        commands.spawn((
            Name::new("IntroSpark"),
            IntroEntity,
            IntroEffect { despawn_tick: tick + lifetime },
            IntroParticle {
                velocity: Vec2::from_angle(angle) * speed,
                gravity: 0.07,
                drag: 0.965,
                look: ParticleLook::Spark { birth: tick, lifetime },
            },
            Sprite::from_color(SPARK_COLORS[0], Vec2::new(if rng.random_bool(0.4) { 2.0 } else { 1.0 }, 1.0)),
            Transform::from_translation(origin.extend(SPARK_Z)),
            pixel_perfect_layers(),
            StateDespawnMarker,
        ));
    }
}

fn simulate_intro_particles(
    intro: Res<Intro>,
    mut particles: Query<(&mut IntroParticle, &mut Transform, &mut Sprite)>,
) {
    for (mut particle, mut transform, mut sprite) in &mut particles {
        let gravity = particle.gravity;
        let drag = particle.drag;
        particle.velocity.y -= gravity;
        particle.velocity *= drag;
        transform.translation += particle.velocity.extend(0.0);
        style_particle(particle.look, intro.tick, &mut sprite);
    }
}

fn style_particle(look: ParticleLook, tick: u32, sprite: &mut Sprite) {
    match look {
        ParticleLook::Spark { birth, lifetime } => {
            let age = ((tick - birth) as f32 / lifetime as f32).clamp(0.0, 1.0);
            let position = age * (SPARK_COLORS.len() - 1) as f32;
            let index = (position as usize).min(SPARK_COLORS.len() - 2);
            let color = SPARK_COLORS[index].mix(&SPARK_COLORS[index + 1], position - index as f32);
            sprite.color = color.with_alpha(1.0 - age * age);
        },
        ParticleLook::Debris { variant, spin_period } => {
            let Some(atlas) = sprite.texture_atlas.as_mut() else { return };
            atlas.index = variant * 4 + (tick / spin_period) as usize % 4;
        },
    }
}

fn expire_intro_effects(
    mut commands: Commands,
    intro: Res<Intro>,
    effects: Query<(Entity, &IntroEffect)>,
) {
    for (entity, effect) in &effects {
        if intro.tick >= effect.despawn_tick {
            commands.entity(entity).despawn();
        }
    }
}

fn fade_intro_overlays(
    mut commands: Commands,
    intro: Res<Intro>,
    mut cover_q: Query<(Entity, &mut BackgroundColor), (With<IntroCover>, Without<IntroFlash>)>,
    mut flash_q: Query<(Entity, &mut BackgroundColor), (With<IntroFlash>, Without<IntroCover>)>,
) {
    if intro.stage == IntroStage::Waiting {
        return;
    }
    for (entity, mut color) in &mut cover_q {
        let opacity = 1.0 - smootherstep(FADE_IN.progress(intro.tick));
        if opacity <= 0.0 {
            commands.entity(entity).despawn();
        } else {
            color.0 = Color::BLACK.with_alpha(opacity);
        }
    }
    for (entity, mut color) in &mut flash_q {
        // Hold full white briefly, then ease out.
        let opacity = (1.0 - FLASH.progress(intro.tick.saturating_sub(3))).powi(2);
        if opacity <= 0.0 {
            commands.entity(entity).despawn();
        } else {
            color.0 = Color::WHITE.with_alpha(opacity);
        }
    }
}

fn finish_intro(
    mut commands: Commands,
    mut intro: ResMut<Intro>,
    player_q: Query<(Has<CapsuleOccupant>, Has<PendingSpawnOccupancy>), With<Player>>,
    leftovers_q: Query<Entity, With<IntroEntity>>,
    mut visibility_q: Query<&mut Visibility>,
    tether_art_q: Query<Entity, With<TetherRangeArtwork>>,
    hud_q: Query<Entity, With<GameplayHud>>,
) {
    if intro.stage != IntroStage::Revealing {
        return;
    }
    let Ok((occupant, pending)) = player_q.single() else { return };
    // Wait until occupancy is resolved and the closed shell fades to the interior.
    if pending && !occupant {
        return;
    }
    let tick = intro.tick;
    let occupied_at = *intro.occupied_at.get_or_insert(tick);
    // The player appears once the fading shell is about half transparent.
    let player_shown = !occupant || tick >= occupied_at + PLAYER_REVEAL_DELAY_TICKS;
    if let (true, Some(scene)) = (player_shown, intro.scene) {
        if let Ok(mut visibility) = visibility_q.get_mut(scene.player_gfx) {
            *visibility = Visibility::Visible;
        }
    }
    let finish_tick = occupied_at + CAPSULE_ART_FADE_TICKS;
    // The capsule's range spreads out from where it landed; it starts growing a tick before
    // the ring is shown so it never appears at full size.
    if let (true, Some(scene)) = (tick + 1 == finish_tick, intro.scene) {
        commands.entity(scene.capsule).insert(CapsuleInfluenceGrowth::default());
    }
    if occupant && tick < finish_tick {
        return;
    }
    for entity in tether_art_q.iter().chain(hud_q.iter()) {
        if let Ok(mut visibility) = visibility_q.get_mut(entity) {
            *visibility = Visibility::Inherited;
        }
    }
    for entity in &leftovers_q {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<ScriptedCameraFocus>();
    commands.insert_resource(SnapCameraNext);
    commands.remove_resource::<Intro>();
}

/// Holds gameplay presentation that would give the landing away.
fn suppress_intro_input(
    mut players: Query<&mut ActionState<PlayerAction>, With<Player>>,
    mut hidden_q: Query<&mut Visibility, Or<(With<TetherRangeArtwork>, With<GameplayHud>)>>,
) {
    for mut actions in &mut players {
        actions.reset_all();
    }
    for mut visibility in &mut hidden_q {
        *visibility = Visibility::Hidden;
    }
}

fn discard_intro(mut commands: Commands) {
    commands.remove_resource::<Intro>();
    commands.remove_resource::<ScriptedCameraFocus>();
    commands.remove_resource::<HoldSpawnOccupancy>();
}
