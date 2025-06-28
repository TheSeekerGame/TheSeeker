#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct DarknessSettings {
    bg_light_level: f32,
    darkness_intensity: f32,
    _padding: vec2<f32>,
}

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var texture_sampler: sampler;
@group(0) @binding(2) var<uniform> settings: DarknessSettings;

// A simple hash function to create a pseudo-random number from a vec2.
// Used for jittering the sampling grid to smooth out artifacts.
fn random(co: vec2<f32>) -> f32 {
    return fract(sin(dot(co.xy, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let screen_color = textureSample(screen_texture, texture_sampler, in.uv);
    let target_light_color = vec3<f32>(245.0/255.0, 250.0/255.0, 253.0/255.0);
    let pixel_size = 1.0 / vec2<f32>(textureDimensions(screen_texture, 0));

    let kernel_radius = 200;
    let kernel_step = 10;
    let sigma = 100.0;

    var total_light = 0.0;
    var total_weight = 0.0;

    let jitter_angle = random(in.uv) * 6.28318; // 2 * PI
    let jitter_radius = random(in.uv + vec2(0.1,0.2)) * f32(kernel_step);
    let jitter = vec2(cos(jitter_angle), sin(jitter_angle)) * jitter_radius;

#ifdef HORIZONTAL
    // --- Horizontal Pass ---
    // This pass identifies light sources and blurs their influence horizontally.
    for (var i = -kernel_radius; i <= kernel_radius; i = i + kernel_step) {
        let offset = vec2<f32>(f32(i) + jitter.x, jitter.y);
        let sample_uv = in.uv + offset * pixel_size;
        let sample_color = textureSample(screen_texture, texture_sampler, sample_uv);

        let color_diff = abs(sample_color.rgb - target_light_color);
        let max_diff = max(max(color_diff.r, color_diff.g), color_diff.b);

        let threshold = 0.05;
        let softness = 0.02;
        let is_light = 1.0 - smoothstep(threshold - softness, threshold + softness, max_diff);

        let dist = abs(f32(i));
        let weight = exp(-(dist * dist) / (2.0 * sigma * sigma));

        total_light += is_light * weight;
        total_weight += weight;
    }

    var light_visibility = 0.0;
    if (total_weight > 0.0) {
        light_visibility = total_light / total_weight;
    }

    // Store horizontal visibility in the alpha channel for the next pass.
    // We keep the original color in rgb to reconstruct it later.
    return vec4<f32>(screen_color.rgb, light_visibility);

#else
    // --- Vertical Pass ---
    // This pass reads the horizontally blurred light visibility and blurs it vertically.
    for (var i = -kernel_radius; i <= kernel_radius; i = i + kernel_step) {
        let offset = vec2<f32>(jitter.x, f32(i) + jitter.y);
        let sample_uv = in.uv + offset * pixel_size;
        let sample_data = textureSample(screen_texture, texture_sampler, sample_uv);

        // The alpha channel now contains the horizontally-blurred light visibility.
        let horizontal_light = sample_data.a;

        let dist = abs(f32(i));
        let weight = exp(-(dist * dist) / (2.0 * sigma * sigma));

        total_light += horizontal_light * weight;
        total_weight += weight;
    }

    var light_visibility = 0.0;
    if (total_weight > 0.0) {
        light_visibility = total_light / total_weight;
    }

    let final_brightness = mix(
        settings.bg_light_level,
        1.0,
        light_visibility
    );

    // Reconstruct the original color from the G and B channels passed from the first pass
    // and apply the final combined brightness.
    let final_color = screen_color.rgb * final_brightness;

    return vec4<f32>(final_color, screen_color.a);

#endif
}