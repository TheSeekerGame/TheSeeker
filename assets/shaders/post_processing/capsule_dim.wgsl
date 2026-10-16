#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

// Laid out and documented as `CapsuleDimSettings` in `capsule_dim.rs`; a test checks the layout.
struct CapsuleDimSettings {
    view_min: vec2<f32>,
    view_size: vec2<f32>,
    dim: f32,
    lit_count: u32,
    lit_rects: array<vec4<f32>, 64>,
}

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var<uniform> settings: CapsuleDimSettings;

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let screen_color = textureLoad(screen_texture, vec2<i32>(in.position.xy), 0);
    let world = settings.view_min + vec2<f32>(in.uv.x, 1.0 - in.uv.y) * settings.view_size;
    for (var i = 0u; i < min(settings.lit_count, 64u); i++) {
        let rect = settings.lit_rects[i];
        if (all(world >= rect.xy) && all(world < rect.zw)) {
            return screen_color;
        }
    }
    return vec4<f32>(screen_color.rgb * (1.0 - settings.dim), screen_color.a);
}
