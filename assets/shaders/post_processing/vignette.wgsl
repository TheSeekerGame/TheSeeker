#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct VignetteSettings {
    color: vec3<f32>,
    base_brightness: f32,
    radius: f32,
    smoothness: f32,
    offset: vec2<f32>,
}

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var texture_sampler: sampler;
@group(0) @binding(2) var<uniform> settings: VignetteSettings;

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let bg_color = textureSample(screen_texture, texture_sampler, in.uv);
    let dimensions = textureDimensions(screen_texture, 0);
    let width = vec2<f32>(f32(dimensions.x), f32(dimensions.y));

    // Offset so we have a range from -0.5 to 0.5
    let pos = in.uv - 0.5;
    let dist = length(pos - settings.offset) - settings.radius;
    let gradient = min(1.0 - smoothstep(0.0, settings.smoothness, dist) + settings.base_brightness, 1.0);
    return bg_color * mix(vec4<f32>(settings.color, 1.0), vec4<f32>(1.0, 1.0, 1.0, 1.0), gradient);
}
