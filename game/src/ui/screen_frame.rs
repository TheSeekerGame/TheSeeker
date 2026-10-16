use bevy::shader::ShaderRef;
use crate::assets::GameUiAssets;
use crate::game::player::{Alignment, Player, PlayerGfx};
use crate::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::*;
use theseeker_engine::gent::Gent;

pub struct ScreenFramePlugin;

const FRAME_THICKNESS_GAME_PIXELS: f32 = 9.0;
const FRAME_CORNER_SIZE_GAME_PIXELS: f32 = 16.0;
const FRAME_Z_INDEX: i32 = 24;
const FRAME_UI_BASE_DIVISOR: f32 = 5.0;
const LOW_ALIGNMENT_PULSE_THRESHOLD: f32 = 0.25;
const LOW_ALIGNMENT_FRAME_PULSE_HZ: f32 = 0.75;
const DEPLETED_ALIGNMENT_FRAME_PULSE_HZ: f32 =
    LOW_ALIGNMENT_FRAME_PULSE_HZ * 2.0;
pub(crate) const ALIGNMENT_PULSE_LINEAR_GRAY: f32 = 0.025;
const ALIGNMENT_PULSE_COLOR: Color = Color::linear_rgb(
    ALIGNMENT_PULSE_LINEAR_GRAY,
    ALIGNMENT_PULSE_LINEAR_GRAY,
    ALIGNMENT_PULSE_LINEAR_GRAY,
);
const PLAYER_ALIGNMENT_PULSE_DIM_STRENGTH: f32 = 0.55;

impl Plugin for ScreenFramePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiMaterialPlugin::<AlignmentPulseMaterial>::default());
        app.init_resource::<PlayerAlignmentPulseState>();
        app.init_resource::<AlignmentPulseSample>();
        app.add_systems(OnEnter(AppState::InGame), spawn_screen_frame);
        app.add_systems(
            GameTickPre,
            restore_player_alignment_pulse
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            Update,
            pulse_alignment_frame_and_toolbar
                .in_set(GameTickSet::Post)
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            PostUpdate,
            pulse_player_alignment.run_if(in_state(AppState::InGame)),
        );
    }
}

#[derive(Clone, Copy)]
struct PlayerAlignmentPulseColors {
    base: Color,
    applied: Color,
}

#[derive(Resource, Default)]
struct PlayerAlignmentPulseState {
    sprites: HashMap<Entity, PlayerAlignmentPulseColors>,
}

/// How strongly low Alignment pulses the frame, the toolbar, and the player this frame.
#[derive(Resource, Default, Clone, Copy)]
pub(crate) struct AlignmentPulseSample {
    active: bool,
    pub(crate) intensity: f32,
}

#[derive(Component)]
struct AlignmentPulseFill;

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub(crate) struct AlignmentPulseMaterial {
    #[uniform(0)]
    intensity: f32,
    #[uniform(1)]
    pulse_color: LinearRgba,
    #[uniform(2)]
    flip: Vec4,
    #[texture(3)]
    #[sampler(4)]
    texture: Handle<Image>,
}

impl AlignmentPulseMaterial {
    pub(crate) fn new(
        texture: Handle<Image>,
        flip_x: bool,
        flip_y: bool,
    ) -> Self {
        Self {
            intensity: 0.0,
            pulse_color: ALIGNMENT_PULSE_COLOR.to_linear(),
            flip: Vec4::new(
                f32::from(flip_x),
                f32::from(flip_y),
                0.0,
                0.0,
            ),
            texture,
        }
    }
}

impl UiMaterial for AlignmentPulseMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/alignment_pulse_ui.wgsl".into()
    }
}

fn spawn_screen_frame(
    mut commands: Commands,
    game_ui_assets: Res<GameUiAssets>,
    mut pulse_materials: ResMut<Assets<AlignmentPulseMaterial>>,
) {
    let thickness = px(FRAME_THICKNESS_GAME_PIXELS * FRAME_UI_BASE_DIVISOR);
    let corner = px(FRAME_CORNER_SIZE_GAME_PIXELS * FRAME_UI_BASE_DIVISOR);
    let full = percent(100.0);
    // Each piece is pinned to the left or right and the top or bottom edge.
    let pinned = |right: bool, bottom: bool, width: Val, height: Val| Node {
        position_type: PositionType::Absolute,
        left: if right { Val::Auto } else { px(0.0) },
        right: if right { px(0.0) } else { Val::Auto },
        top: if bottom { Val::Auto } else { px(0.0) },
        bottom: if bottom { px(0.0) } else { Val::Auto },
        width,
        height,
        ..default()
    };
    let bands = [(false, false, full, thickness), (false, true, full, thickness), (false, false, thickness, full), (true, false, thickness, full)];
    let bands = bands.map(|(right, bottom, width, height)| {
        (Name::new("screen_frame_band"), pinned(right, bottom, width, height), BackgroundColor(Color::BLACK), AlignmentPulseFill)
    });
    // The corner art is the bottom-left piece, mirrored into the other corners.
    let corners = [(false, true), (true, true), (false, false), (true, false)].map(|(right, bottom)| {
        let material = AlignmentPulseMaterial::new(game_ui_assets.frame_corner_bottom_left.clone(), right, !bottom);
        (Name::new("screen_frame_corner"), pinned(right, bottom, corner, corner), MaterialNode(pulse_materials.add(material)), ZIndex(1))
    });
    commands.spawn((
        Node { position_type: PositionType::Absolute, width: full, height: full, ..default() },
        ZIndex(FRAME_Z_INDEX),
        StateDespawnMarker,
        Name::new("screen_frame_root"),
        Children::spawn((SpawnIter(bands.into_iter()), SpawnIter(corners.into_iter()))),
    ));
}

fn pulse_alignment_frame_and_toolbar(
    alignment_q: Query<&Alignment, With<Player>>,
    game_time: Res<GameTime>,
    mut pulse_sample: ResMut<AlignmentPulseSample>,
    material_q: Query<&MaterialNode<AlignmentPulseMaterial>>,
    mut pulse_materials: ResMut<Assets<AlignmentPulseMaterial>>,
    mut fill_q: Query<&mut BackgroundColor, With<AlignmentPulseFill>>,
) {
    let alignment_factor =
        alignment_q.single().map(Alignment::factor).unwrap_or(1.0);
    pulse_sample.active = alignment_factor < LOW_ALIGNMENT_PULSE_THRESHOLD;
    pulse_sample.intensity = alignment_frame_intensity(
        alignment_factor,
        game_time.time_in_seconds() as f32,
    );
    let intensity = pulse_sample.intensity;

    for material_node in &material_q {
        if let Some(mut material) = pulse_materials.get_mut(&material_node.0) {
            material.intensity = intensity;
        }
    }
    for mut fill in &mut fill_q {
        fill.0 = alignment_frame_fill_color(intensity);
    }
}

fn restore_player_alignment_pulse(
    mut pulse_state: ResMut<PlayerAlignmentPulseState>,
    mut gfx_q: Query<&mut Sprite, With<PlayerGfx>>,
) {
    pulse_state.sprites.retain(|entity, colors| {
        let Ok(mut sprite) = gfx_q.get_mut(*entity) else {
            return false;
        };
        if sprite.color == colors.applied {
            sprite.color = colors.base;
        } else {
            colors.base = sprite.color;
        }
        colors.applied = sprite.color;
        true
    });
}

fn pulse_player_alignment(
    player_q: Query<&Gent, With<Player>>,
    mut gfx_q: Query<(Entity, &mut Sprite), With<PlayerGfx>>,
    pulse_sample: Res<AlignmentPulseSample>,
    mut pulse_state: ResMut<PlayerAlignmentPulseState>,
) {
    pulse_state
        .sprites
        .retain(|entity, _| gfx_q.contains(*entity));
    let Ok(gent) = player_q.single() else {
        return;
    };
    let Ok((entity, mut sprite)) = gfx_q.get_mut(gent.e_gfx) else {
        return;
    };
    let colors = pulse_state.sprites.entry(entity).or_insert(
        PlayerAlignmentPulseColors {
            base: sprite.color,
            applied: sprite.color,
        },
    );
    if sprite.color != colors.applied {
        colors.base = sprite.color;
    }

    let applied = alignment_player_pulse_color(
        colors.base,
        pulse_sample.intensity,
        pulse_sample.active,
    );
    sprite.color = applied;
    colors.applied = applied;
}

fn alignment_frame_intensity(
    alignment_factor: f32,
    elapsed_secs: f32,
) -> f32 {
    let frequency_hz = if alignment_factor <= 0.0 {
        DEPLETED_ALIGNMENT_FRAME_PULSE_HZ
    } else if alignment_factor < LOW_ALIGNMENT_PULSE_THRESHOLD {
        LOW_ALIGNMENT_FRAME_PULSE_HZ
    } else {
        return 0.0;
    };

    (0.5
        + 0.5
            * (elapsed_secs
                * frequency_hz
                * std::f32::consts::TAU)
                .sin())
    .clamp(0.0, 1.0)
}

fn alignment_frame_fill_color(intensity: f32) -> Color {
    let target = ALIGNMENT_PULSE_COLOR.to_linear();
    let intensity = intensity.clamp(0.0, 1.0);
    Color::linear_rgba(
        target.red * intensity,
        target.green * intensity,
        target.blue * intensity,
        1.0,
    )
}

fn alignment_player_pulse_color(
    base: Color,
    intensity: f32,
    active: bool,
) -> Color {
    if !active || intensity >= 1.0 {
        return base;
    }
    let base = base.to_linear();
    let brightness = 1.0
        - PLAYER_ALIGNMENT_PULSE_DIM_STRENGTH
            * (1.0 - intensity.clamp(0.0, 1.0));
    Color::linear_rgba(
        base.red * brightness,
        base.green * brightness,
        base.blue * brightness,
        base.alpha,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_alignment_updates_material_and_fill_together() {
        let mut app = App::new();
        app.init_resource::<GameTime>();
        app.init_resource::<AlignmentPulseSample>();
        app.init_resource::<Assets<AlignmentPulseMaterial>>();
        app.add_systems(Update, pulse_alignment_frame_and_toolbar);

        let mut alignment = Alignment::default();
        alignment.tick(false, 25.0);
        app.world_mut().spawn((Player, alignment));

        let material = app
            .world_mut()
            .resource_mut::<Assets<AlignmentPulseMaterial>>()
            .add(AlignmentPulseMaterial::new(
                Handle::default(),
                false,
                false,
            ));
        app.world_mut().spawn(MaterialNode(material.clone()));
        let fill = app
            .world_mut()
            .spawn((AlignmentPulseFill, BackgroundColor(Color::BLACK)))
            .id();

        app.update();

        let sample = app.world().resource::<AlignmentPulseSample>();
        assert!(sample.active);
        assert!((sample.intensity - 0.5).abs() < f32::EPSILON);

        let material_intensity = app
            .world()
            .resource::<Assets<AlignmentPulseMaterial>>()
            .get(&material)
            .expect("pulse material should remain loaded")
            .intensity;
        assert!((material_intensity - sample.intensity).abs() < f32::EPSILON);

        let fill_red = app
            .world()
            .get::<BackgroundColor>(fill)
            .expect("pulse fill should retain its background color")
            .0
            .to_linear()
            .red;
        assert!(fill_red > 0.0);
    }

    #[test]
    fn frame_does_not_pulse_at_or_above_warning_threshold() {
        assert_eq!(
            alignment_frame_intensity(LOW_ALIGNMENT_PULSE_THRESHOLD, 0.25),
            0.0,
        );
        assert_eq!(alignment_frame_intensity(1.0, 0.25), 0.0);
    }

    #[test]
    fn low_alignment_pulse_reaches_both_intensity_extremes() {
        let maximum_time = 0.25 / LOW_ALIGNMENT_FRAME_PULSE_HZ;
        let minimum_time = 0.75 / LOW_ALIGNMENT_FRAME_PULSE_HZ;

        assert!(
            (alignment_frame_intensity(0.24, maximum_time) - 1.0).abs()
                < f32::EPSILON
        );
        assert!(
            alignment_frame_intensity(0.24, minimum_time).abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn depleted_alignment_pulse_runs_at_double_frequency() {
        assert_eq!(
            DEPLETED_ALIGNMENT_FRAME_PULSE_HZ,
            LOW_ALIGNMENT_FRAME_PULSE_HZ * 2.0,
        );

        let first_peak_time =
            0.25 / DEPLETED_ALIGNMENT_FRAME_PULSE_HZ;
        assert!(
            (alignment_frame_intensity(0.0, first_peak_time) - 1.0).abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn frame_fill_stays_opaque_throughout_the_pulse() {
        for intensity in [0.0, 0.5, 1.0] {
            assert_eq!(
                alignment_frame_fill_color(intensity).to_linear().alpha,
                1.0,
            );
        }
    }

    #[test]
    fn player_alignment_pulse_matches_frame_polarity_and_preserves_alpha() {
        let base = Color::srgba(0.8, 0.6, 0.4, 0.7);
        assert_eq!(alignment_player_pulse_color(base, 0.5, false), base);
        assert_eq!(alignment_player_pulse_color(base, 1.0, true), base);

        let pulsed = alignment_player_pulse_color(base, 0.0, true).to_linear();
        let linear_base = base.to_linear();
        assert!(pulsed.red < linear_base.red);
        assert!(pulsed.green < linear_base.green);
        assert!(pulsed.blue < linear_base.blue);
        assert!(
            (pulsed.red
                - linear_base.red
                    * (1.0 - PLAYER_ALIGNMENT_PULSE_DIM_STRENGTH))
                .abs()
                < f32::EPSILON
        );
        assert_eq!(pulsed.alpha, linear_base.alpha);
    }
}
