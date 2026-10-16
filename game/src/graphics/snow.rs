//! Falling snow: layers of soft flakes in front of and behind the world, parallaxed by depth.
//! The flakes live in a field around the camera and wrap around it, so the snow never runs out.
use std::f32::consts::TAU;

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use rand::seq::IndexedRandom;
use rand::{rng, Rng};
use theseeker_engine::audio::transport::TICK_RATE_HZ;

use crate::camera::{GameplayPixelScale, MainCamera};
use crate::graphics::background_blur::{apply_background_blur, BackgroundBlurSettings};
use crate::parallax::{Parallax, ParallaxOrigin};
use crate::prelude::*;

pub struct SnowPlugin;

impl Plugin for SnowPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (spawn_snow_field.run_if(not(resource_exists::<SnowField>)), update_snow_field)
                .chain()
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(OnExit(AppState::InGame), |mut commands: Commands| {
            commands.remove_resource::<SnowField>();
        });
    }
}

/// The field around the camera that the flakes fill: the view scaled up, then padded, so
/// flakes enter the view unseen.
const FIELD_SCALE: f32 = 1.3;
const FIELD_PADDING: Vec2 = Vec2::splat(32.0);
/// How many distinct flake images each layer draws its flakes from.
const IMAGES_PER_LAYER: usize = 10;
/// How often the field scatters its flakes around the camera after it appears, while the
/// camera settles, and how far apart in seconds.
const SETTLING_SCATTERS: u32 = 3;
const SCATTER_INTERVAL: f32 = 0.15;
/// How long a twinkle pauses before each fade, 100 to 500 game ticks, and how long the fade
/// lasts, in seconds.
const TWINKLE_PAUSE: Vec2 = Vec2::new(100.0 / TICK_RATE_HZ as f32, 500.0 / TICK_RATE_HZ as f32);
const TWINKLE_FADE: Vec2 = Vec2::new(2.0, 8.0);

/// One layer of snow. Each range is drawn from uniformly, once per flake.
struct SnowLayer {
    /// The layer's parallax depth: below 1 in front of the world, above 1 behind it.
    depth: f32,
    count: usize,
    z: f32,
    /// The background blur group whose blur softens the layer's flakes.
    blur_group: usize,
    size: Vec2,
    /// Skews flake sizes toward the range's top (up to 1) or its bottom (down to -1).
    size_bias: f32,
    fall_speed: Vec2,
    /// Every flake drifts right, a little faster or slower.
    wind: Vec2,
    sway_amplitude: Vec2,
    sway_frequency: Vec2,
    /// The alpha a flake twinkles up to.
    alpha: Vec2,
}

const SNOW_LAYERS: [SnowLayer; 7] = [
    front_layer(0.8),
    front_layer(0.9),
    front_layer(1.0),
    front_layer(1.1),
    front_layer(1.2),
    SnowLayer {
        depth: 1.0 / 0.85,
        count: 55,
        z: -0.5,
        blur_group: 0,
        size: Vec2::new(4.0, 7.5),
        size_bias: 0.0,
        fall_speed: Vec2::new(15.0, 20.0),
        wind: Vec2::new(2.0, 2.8),
        sway_amplitude: Vec2::new(0.35, 0.9),
        sway_frequency: Vec2::new(0.7, 1.3),
        alpha: Vec2::new(0.05, 0.15),
    },
    SnowLayer {
        depth: 1.0 / 0.75,
        count: 45,
        z: -2.5,
        blur_group: 1,
        size: Vec2::new(3.0, 6.0),
        size_bias: 0.0,
        fall_speed: Vec2::new(10.0, 15.0),
        wind: Vec2::new(1.5, 2.1),
        sway_amplitude: Vec2::new(0.25, 0.75),
        sway_frequency: Vec2::new(0.55, 1.05),
        alpha: Vec2::new(0.05, 0.15),
    },
];

/// A layer in front of the world, `distance` times the base front depth away. Nearer layers
/// fall faster, the farthest (1.2) at the base speed, and favour larger flakes.
const fn front_layer(distance: f32) -> SnowLayer {
    let speed_up = 1.2 / distance;
    SnowLayer {
        depth: 0.8 * distance,
        count: 14,
        z: 0.1,
        blur_group: 2,
        size: Vec2::new(5.0, 7.5),
        size_bias: 5.0 * (1.0 - distance),
        fall_speed: Vec2::new(20.0 * speed_up, 30.0 * speed_up),
        wind: Vec2::new(2.5, 3.5),
        sway_amplitude: Vec2::new(0.4, 1.0),
        sway_frequency: Vec2::new(0.8, 1.6),
        alpha: Vec2::new(0.15, 0.65),
    }
}

impl SnowLayer {
    fn flake_size(&self, rng: &mut impl Rng) -> f32 {
        let r: f32 = rng.random();
        let t = if self.size_bias > 0.0 {
            1.0 - r.powf(1.0 + 2.0 * self.size_bias)
        } else {
            r.powf(1.0 - 2.0 * self.size_bias)
        };
        self.size.x + (self.size.y - self.size.x) * t
    }
}

/// The snow while the game runs: each layer's flake images, and the scatters left while the
/// camera settles.
#[derive(Resource)]
struct SnowField {
    images: Vec<Vec<Handle<Image>>>,
    scatters_left: u32,
    last_scatter_at: f32,
}

impl SnowField {
    /// Whether the flakes scatter around the camera at `now`.
    fn scatter_due(&mut self, now: f32) -> bool {
        let due = self.scatters_left > 0 && now - self.last_scatter_at > SCATTER_INTERVAL;
        if due {
            self.scatters_left -= 1;
            self.last_scatter_at = now;
        }
        due
    }
}

#[derive(Component)]
struct SnowFlake {
    /// The flake's index in `SNOW_LAYERS`.
    layer: usize,
    fall_speed: f32,
    wind: f32,
    sway_amplitude: f32,
    sway_frequency: f32,
    sway_phase: f32,
    twinkle: Twinkle,
}

impl SnowFlake {
    fn layer(&self) -> &'static SnowLayer {
        &SNOW_LAYERS[self.layer]
    }

    /// How fast the flake moves across the view at `time`.
    fn velocity(&self, time: f32) -> Vec2 {
        let sway = (self.sway_phase + time * self.sway_frequency).sin() * self.sway_amplitude;
        Vec2::new(self.wind + sway, -self.fall_speed)
    }
}

/// A flake's slow twinkle: it pauses, fades between invisible and its peak alpha, and repeats
/// the other way.
struct Twinkle {
    peak: f32,
    /// Whether the current fade brightens the flake.
    brightening: bool,
    /// Seconds into the current fade; negative while the pause before it lasts.
    clock: f32,
    duration: f32,
}

impl Twinkle {
    fn new(peak: f32, brightening: bool, rng: &mut impl Rng) -> Self {
        Self { peak, brightening, clock: -pick(rng, TWINKLE_PAUSE), duration: pick(rng, TWINKLE_FADE) }
    }

    fn alpha(&self) -> f32 {
        let t = (self.clock / self.duration).clamp(0.0, 1.0);
        self.peak * if self.brightening { t } else { 1.0 - t }
    }

    fn advance(&mut self, seconds: f32, rng: &mut impl Rng) {
        self.clock += seconds;
        if self.clock >= self.duration {
            *self = Self::new(self.peak, !self.brightening, rng);
        }
    }
}

/// A uniform draw from `range.x` to `range.y`.
fn pick(rng: &mut impl Rng, range: Vec2) -> f32 {
    rng.random_range(range.x..=range.y)
}

/// A new flake for layer `layer`, drawn from its ranges and `images`, and the flake's sprite.
fn new_flake(layer: usize, images: &[Handle<Image>], rng: &mut impl Rng) -> (SnowFlake, Sprite) {
    let settings = &SNOW_LAYERS[layer];
    let flake = SnowFlake {
        layer,
        fall_speed: pick(rng, settings.fall_speed),
        wind: pick(rng, settings.wind),
        sway_amplitude: pick(rng, settings.sway_amplitude),
        sway_frequency: pick(rng, settings.sway_frequency),
        sway_phase: rng.random_range(0.0..=TAU),
        twinkle: Twinkle::new(pick(rng, settings.alpha), rng.random_bool(0.5), rng),
    };
    let tint = rng.random_range(0.92..=0.98);
    let sprite = Sprite {
        image: images.choose(rng).cloned().unwrap_or_default(),
        custom_size: Some(Vec2::splat(settings.flake_size(rng))),
        color: Color::srgba(tint, tint, 1.0, flake.twinkle.alpha()),
        ..default()
    };
    (flake, sprite)
}

/// Draws each layer's flake images and spawns its flakes, which the field's first scatter
/// places.
fn spawn_snow_field(mut commands: Commands, blur: Res<BackgroundBlurSettings>, mut assets: ResMut<Assets<Image>>) {
    let mut rng = rng();
    let images: Vec<Vec<_>> = SNOW_LAYERS
        .iter()
        .map(|layer| {
            let blur_px = blur.blur_for_group(layer.blur_group);
            (0..IMAGES_PER_LAYER).map(|_| assets.add(flake_image(layer, blur_px, &mut rng))).collect()
        })
        .collect();
    for (index, (layer, images)) in SNOW_LAYERS.iter().zip(&images).enumerate() {
        for _ in 0..layer.count {
            let (flake, sprite) = new_flake(index, images, &mut rng);
            commands.spawn((
                sprite,
                Transform::from_xyz(0.0, 0.0, layer.z),
                Parallax { depth: layer.depth },
                ParallaxOrigin::default(),
                flake,
                StateDespawnMarker,
                Name::new(format!("SnowLayer{}Particle", index + 1)),
            ));
        }
    }
    commands.insert_resource(SnowField { images, scatters_left: SETTLING_SCATTERS, last_scatter_at: f32::NEG_INFINITY });
}

/// Moves and twinkles the flakes across the view, or scatters them over the field around the
/// camera while it settles. A flake leaving the field's top or bottom comes back anew at the
/// other; one leaving a side wraps around to the other side.
fn update_snow_field(
    time: Res<PresentationTime>,
    scale: Res<GameplayPixelScale>,
    mut field: ResMut<SnowField>,
    camera: Single<&Transform, With<MainCamera>>,
    mut flakes: Query<(&mut SnowFlake, &mut Sprite, &mut ParallaxOrigin)>,
) {
    let camera = camera.translation.xy();
    let half = scale.visible_game_size() * (FIELD_SCALE / 2.0) + FIELD_PADDING;
    let scatter = field.scatter_due(time.elapsed_secs());
    let mut rng = rng();
    for (mut flake, mut sprite, mut origin) in &mut flakes {
        let depth = flake.layer().depth;
        // Where the flake shows, relative to the camera.
        let mut offset = (origin.0 - camera) / depth;
        if scatter {
            offset = Vec2::new(rng.random_range(-half.x..=half.x), rng.random_range(-half.y..=half.y));
        } else {
            flake.twinkle.advance(time.delta_secs(), &mut rng);
            offset += flake.velocity(time.elapsed_secs()) * time.delta_secs();
        }
        if offset.y.abs() > half.y {
            (*flake, *sprite) = new_flake(flake.layer, &field.images[flake.layer], &mut rng);
            offset = Vec2::new(rng.random_range(-half.x..=half.x), -offset.y.signum() * half.y);
        }
        if offset.x.abs() > half.x {
            offset.x -= offset.x.signum() * 2.0 * half.x;
        }
        origin.0 = camera + offset * depth;
        sprite.color.set_alpha(flake.twinkle.alpha());
    }
}

/// A soft flake for `layer`: a turned, stretched Gaussian blob, blurred like the backgrounds
/// at the layer's depth.
fn flake_image(layer: &SnowLayer, blur_px: f32, rng: &mut impl Rng) -> Image {
    // An even square with room for the layer's largest flake and its blur.
    let side = ((layer.size.y * 4.0 + blur_px * 6.0).ceil() as u32).clamp(24, 96).next_multiple_of(2);
    let major_sigma = side as f32 * rng.random_range(0.08..=0.15);
    let sigma = Vec2::new(major_sigma, major_sigma * rng.random_range(0.55..=0.9));
    let turn = Vec2::from_angle(rng.random_range(0.0..=TAU));
    let grey = (rng.random_range(0.82..=0.96_f32) * 255.0).round() as u8;
    let opacity = rng.random_range(0.55..=0.85);
    let center = Vec2::splat(side as f32 / 2.0);
    let data = (0..side * side)
        .flat_map(|i| {
            let offset = turn.rotate(UVec2::new(i % side, i / side).as_vec2() + 0.5 - center);
            let alpha = (-0.5 * (offset / sigma).length_squared()).exp() * opacity;
            [grey, grey, 255, (alpha.clamp(0.0, 1.0) * 255.0).round() as u8]
        })
        .collect();
    let size = Extent3d { width: side, height: side, depth_or_array_layers: 1 };
    let mut image =
        Image::new(size, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD);
    apply_background_blur(&mut image, blur_px);
    image.sampler = ImageSampler::linear();
    image
}

#[cfg(test)]
mod tests {
    use bevy::state::app::StatesPlugin;

    use super::*;

    /// Where each flake shows relative to a camera at `camera`.
    fn shown_offsets(app: &mut App, camera: Vec2) -> Vec<Vec2> {
        let mut flakes = app.world_mut().query::<(&SnowFlake, &ParallaxOrigin)>();
        flakes.iter(app.world()).map(|(flake, origin)| (origin.0 - camera) / flake.layer().depth).collect()
    }

    #[test]
    fn flakes_fill_the_field_around_the_camera_as_it_moves() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default(), StatesPlugin));
        app.init_asset::<Image>()
            .init_resource::<PresentationTime>()
            .init_resource::<BackgroundBlurSettings>()
            .insert_resource(GameplayPixelScale::fit(UVec2::new(1920, 1080)))
            .insert_state(AppState::InGame)
            .add_plugins(SnowPlugin);
        let camera = app.world_mut().spawn((MainCamera, Transform::default())).id();
        let half = Vec2::new(320.0, 180.0) * (FIELD_SCALE / 2.0) + FIELD_PADDING;

        // The flakes appear scattered over the whole field.
        app.update();
        let shown = shown_offsets(&mut app, Vec2::ZERO);
        assert_eq!(shown.len(), SNOW_LAYERS.iter().map(|layer| layer.count).sum::<usize>());
        let spread = shown.iter().fold(Vec2::ZERO, |spread, offset| spread.max(offset.abs()));
        assert!(spread.cmpgt(half * 0.9).all(), "flakes spread {spread} over a field of half size {half}");

        // Flakes the camera leaves behind come back into the field around it.
        for position in [Vec2::new(150.0, -90.0), Vec2::new(-40.0, 400.0)] {
            app.world_mut().get_mut::<Transform>(camera).expect("the camera").translation = position.extend(0.0);
            app.update();
            let outside = shown_offsets(&mut app, position).into_iter().find(|offset| offset.abs().cmpgt(half + 0.001).any());
            assert_eq!(outside, None, "a flake outside the field around a camera at {position}");
        }
    }
}
