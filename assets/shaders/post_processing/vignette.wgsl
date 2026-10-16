#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

// Laid out and documented as `VignetteSettings` in `vignette.rs`; a test checks the layout.
struct VignetteSettings {
    color: vec3<f32>,
    base_brightness: f32,
    radius: f32,
    smoothness: f32,
    offset: vec2<f32>,
    fog_strength: f32,
}

// Laid out and documented as `FogLayer` in `foreground_fog.rs`.
struct FogLayer {
    density_range: vec2<f32>,
    frame_size_px: f32,
    tile_world_size: f32,
    parallax_factor: f32,
    optical_thickness: f32,
    minimum_density: f32,
    protection_reach: f32,
}

// Laid out and documented as `ForegroundFogSettings` in `foreground_fog.rs`; a test checks
// the layout.
struct ForegroundFogSettings {
    rear: FogLayer,
    detail: FogLayer,
    fog_color: vec3<f32>,
    protection_strength: f32,
    protection_band: vec2<f32>,
    camera_center: vec2<f32>,
    view_world_size: vec2<f32>,
    atlas_grid: vec2<u32>,
    frame_index: u32,
    gutter_px: f32,
    // Open sky: x range (x, y) and the fade band (z below, w above) where fog clears upward.
    open_sky: vec4<f32>,
}

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var vignette_noise_texture: texture_2d<f32>;
@group(0) @binding(2) var noise_sampler: sampler;
@group(0) @binding(3) var<uniform> settings: VignetteSettings;
@group(0) @binding(4) var<uniform> fog_settings: ForegroundFogSettings;
@group(0) @binding(5) var rear_fog_atlas: texture_2d<f32>;
@group(0) @binding(6) var detail_fog_atlas: texture_2d<f32>;
@group(0) @binding(7) var fog_protection_mask: texture_2d<f32>;

const NOISE_MARGIN_UV: f32 = 0.32;
const NOISE_DOMAIN_SCALE: f32 = 1.0 + NOISE_MARGIN_UV * 2.0;

fn interleaved_gradient_noise(pixel_coord: vec2<f32>) -> f32 {
    return fract(
        52.9829189
            * fract(
                0.06711056 * pixel_coord.x + 0.00583715 * pixel_coord.y,
            ),
    );
}

// The center of the game pixel under `uv`, so the fog lies on the game pixel grid.
fn pixel_perfect_fog_uv(uv: vec2<f32>) -> vec2<f32> {
    let grid_size = max(
        floor(fog_settings.view_world_size + vec2<f32>(0.5)),
        vec2<f32>(1.0),
    );
    let grid_pixel = clamp(
        floor(uv * grid_size),
        vec2<f32>(0.0),
        grid_size - vec2<f32>(1.0),
    );
    return (grid_pixel + vec2<f32>(0.5)) / grid_size;
}

// Where `uv` samples the layer's current flipbook frame in its atlas, the frame tiled across
// the world and parallaxed against the camera.
fn fog_atlas_uv(layer: FogLayer, uv: vec2<f32>) -> vec2<f32> {
    let screen_offset = vec2<f32>(
        (uv.x - 0.5) * fog_settings.view_world_size.x,
        (0.5 - uv.y) * fog_settings.view_world_size.y,
    );
    let parallax_world_position = screen_offset
        + fog_settings.camera_center * layer.parallax_factor;
    let local_uv = fract(
        parallax_world_position / max(layer.tile_world_size, 0.0001),
    );
    let grid = max(fog_settings.atlas_grid, vec2<u32>(1u));
    let frame = fog_settings.frame_index % (grid.x * grid.y);
    let cell = vec2<f32>(f32(frame % grid.x), f32(frame / grid.x));
    let cell_size = layer.frame_size_px + 2.0 * fog_settings.gutter_px;
    let sample_pixel = cell * cell_size
        + vec2<f32>(fog_settings.gutter_px + 0.5)
        + local_uv * layer.frame_size_px;
    return sample_pixel / (cell_size * vec2<f32>(grid));
}

// Lays the layer's fog over `color`: `noise` shapes its density, which tethers thin where
// `protection_offset` lies within the layer's protection, and open sky thins by `presence`.
fn over_fog_layer(
    color: vec3<f32>,
    layer: FogLayer,
    noise: f32,
    protection_offset: f32,
    presence: f32,
) -> vec3<f32> {
    let shaped_density = clamp(
        (noise - layer.density_range.x)
            / max(layer.density_range.y - layer.density_range.x, 0.0001),
        0.0,
        1.0,
    );
    let protection = 1.0 - smoothstep(
        -layer.protection_reach,
        fog_settings.protection_band.y,
        protection_offset,
    );
    let density = mix(layer.minimum_density, 1.0, shaped_density)
        * (1.0 - protection * fog_settings.protection_strength)
        * presence;
    let alpha = 1.0 - exp2(-density * layer.optical_thickness);
    return mix(color, fog_settings.fog_color, clamp(alpha, 0.0, 1.0));
}

fn open_sky_clearing(uv: vec2<f32>) -> f32 {
    let world = fog_settings.camera_center + vec2<f32>(
        (uv.x - 0.5) * fog_settings.view_world_size.x,
        (0.5 - uv.y) * fog_settings.view_world_size.y,
    );
    let sky = fog_settings.open_sky;
    if (world.x < sky.x || world.x > sky.y) {
        return 0.0;
    }
    return smoothstep(sky.z, sky.w, world.y);
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let scene_color = textureLoad(screen_texture, vec2<i32>(in.position.xy), 0);
    let fog_uv = pixel_perfect_fog_uv(in.uv);
    // How far past a tether's outline this pixel lies, in shares of its radius, from the
    // protection the tethers drew into the mask.
    let protection = clamp(textureSample(fog_protection_mask, noise_sampler, fog_uv).r, 0.0, 1.0);
    let band = fog_settings.protection_band;
    let protection_offset = (1.0 - protection) * (band.x + band.y) - band.x;
    let presence = 1.0 - open_sky_clearing(in.uv);
    let rear = fog_settings.rear;
    let detail = fog_settings.detail;
    let rear_noise = textureSample(rear_fog_atlas, noise_sampler, fog_atlas_uv(rear, fog_uv)).r;
    let detail_noise = textureSample(detail_fog_atlas, noise_sampler, fog_atlas_uv(detail, fog_uv)).r;
    let fogged_scene_color = over_fog_layer(
        over_fog_layer(scene_color.rgb, rear, rear_noise, protection_offset, presence),
        detail,
        detail_noise,
        protection_offset,
        presence,
    );
    let centered_uv = in.uv - vec2<f32>(0.5, 0.5) - settings.offset;
    let max_distance = length(vec2<f32>(0.5, 0.5));
    let normalized_distance =
        clamp(length(centered_uv) / max(max_distance, 0.0001), 0.0, 1.0);
    let edge_mix = smoothstep(
        settings.radius,
        settings.radius + settings.smoothness,
        normalized_distance,
    );
    let dither = (interleaved_gradient_noise(in.position.xy) - 0.5) / 255.0;
    let brightness = clamp(
        mix(1.0, settings.base_brightness, edge_mix) + dither,
        settings.base_brightness,
        1.0,
    );
    let vignette_tint = mix(settings.color, vec3<f32>(1.0, 1.0, 1.0), brightness);
    let fog_edge_mask = smoothstep(
        settings.radius + settings.smoothness * 0.35,
        1.05,
        normalized_distance,
    );
    let noise_lookup_uv =
        (in.uv - settings.offset + vec2<f32>(NOISE_MARGIN_UV)) / NOISE_DOMAIN_SCALE;
    let fog_noise = textureSample(
        vignette_noise_texture,
        noise_sampler,
        clamp(noise_lookup_uv, vec2<f32>(0.0), vec2<f32>(1.0)),
    ).r;
    let fog_mask = fog_edge_mask * smoothstep(0.34, 0.82, fog_noise);
    let fog_darken = clamp(fog_mask * settings.fog_strength, 0.0, 0.95);
    let fog_brightness = 1.0 - fog_darken;

    return vec4<f32>(
        fogged_scene_color * vignette_tint * fog_brightness,
        scene_color.a,
    );
}
