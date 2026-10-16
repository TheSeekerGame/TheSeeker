#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

// Laid out and documented as `DarknessSettings` in `darkness.rs`; a test checks the layout.
struct DarknessSettings {
    view_min: vec2<f32>,
    view_size: vec2<f32>,
    level_min: vec2<f32>,
    level_size: vec2<f32>,
    lightmap_size: vec2<f32>,
    lightmap_page_edge: f32,
    depth_start: f32,
    depth_ramp: f32,
    depth_ramp_strength: f32,
    depth_strength_per_unit: f32,
    player_light_center: vec2<f32>,
    player_light_radius: f32,
    player_light_strength: f32,
    player_light_falloff: f32,
}

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var lightmap_texture: texture_2d_array<f32>;
@group(0) @binding(2) var lightmap_sampler: sampler;
@group(0) @binding(3) var<uniform> settings: DarknessSettings;
@group(0) @binding(4) var lightmap_pages: texture_2d<u32>;

// The baked light at `world`, and none outside the level. The lightmap lies in square atlas
// pages, which `lightmap_pages` finds, each with a one-pixel gutter of its neighbours' light.
fn baked_light(world: vec2<f32>) -> f32 {
    let from_top_left = vec2<f32>(world.x - settings.level_min.x, settings.level_min.y + settings.level_size.y - world.y);
    let uv = from_top_left / settings.level_size;
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
        return 0.0;
    }
    let edge = settings.lightmap_page_edge;
    let pixel = min(uv * settings.lightmap_size, settings.lightmap_size - vec2<f32>(0.0001));
    let page = vec2<i32>(floor(pixel / edge));
    let layer = textureLoad(lightmap_pages, page, 0).r;
    let page_uv = (pixel - vec2<f32>(page) * edge + vec2<f32>(1.0)) / (edge + 2.0);
    return textureSampleLevel(lightmap_texture, lightmap_sampler, page_uv, i32(layer), 0.0).r;
}

fn interleaved_gradient_noise(pixel: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(0.06711056 * pixel.x + 0.00583715 * pixel.y));
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let pixel = floor(in.position.xy);
    let screen_color = textureLoad(screen_texture, vec2<i32>(pixel), 0);
    let world = settings.view_min + vec2<f32>(in.uv.x, 1.0 - in.uv.y) * settings.view_size;

    // Darkness deepens below its start: eased in over the ramp, then steadily.
    let depth = settings.depth_start - world.y;
    if (settings.depth_ramp <= 0.0 || depth <= 0.0) {
        return screen_color;
    }
    let darkness = clamp(
        settings.depth_ramp_strength * smoothstep(0.0, settings.depth_ramp, depth)
            + max(depth - settings.depth_ramp, 0.0) * settings.depth_strength_per_unit,
        0.0,
        1.0,
    );
    if (darkness <= 0.0001) {
        return screen_color;
    }

    // Baked light and the player's backlight each lift a share of the darkness. The backlight
    // fades from its very center, so it shows no flat bright disk.
    let t = min(distance(world, settings.player_light_center) / settings.player_light_radius, 1.0);
    let backlight = settings.player_light_strength * pow(1.0 - t, settings.player_light_falloff);
    let overlay = darkness * (1.0 - baked_light(world)) * (1.0 - backlight);

    // Dithering hides the banding of smooth darkness in 8-bit color.
    let dither = (interleaved_gradient_noise(pixel) - 0.5) * step(0.0001, overlay) * mix(0.35, 1.0, overlay) / 255.0;
    return vec4<f32>(screen_color.rgb * clamp(1.0 - overlay + dither, 0.0, 1.0), screen_color.a);
}
