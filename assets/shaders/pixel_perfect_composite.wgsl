// Pixel-perfect composite shader
// Blends the pixel-perfect render layer onto the main view
// with sub-texel offset to maintain smooth camera movement.

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

// Main view (source from previous passes)
@group(0) @binding(0) var main_texture: texture_2d<f32>;

// Pixel-perfect layer
@group(0) @binding(1) var pp_texture: texture_2d<f32>;
@group(0) @binding(2) var pp_sampler: sampler;

struct Settings {
    // Sub-pixel offset in UV space (from camera snap residual)
    offset_uv: vec2<f32>,
    // Visible sub-rectangle of the PP texture that should map to the viewport.
    sample_uv_min: vec2<f32>,
    sample_uv_size: vec2<f32>,
}

@group(0) @binding(3) var<uniform> settings: Settings;

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    // The main view's pixel under this fragment
    let main_color = textureLoad(main_texture, vec2<i32>(in.position.xy), 0);

    // Apply the sub-texel offset for smooth camera feel on the pixel-perfect layer
    let pp_uv = settings.sample_uv_min
        + in.uv * settings.sample_uv_size
        - settings.offset_uv;
    let pp_color = textureSample(pp_texture, pp_sampler, pp_uv);

    // Alpha blend: pixel-perfect layer on top of main view
    let blended = mix(main_color.rgb, pp_color.rgb, pp_color.a);
    let out_alpha = main_color.a + pp_color.a * (1.0 - main_color.a);

    return vec4<f32>(blended, out_alpha);
}
