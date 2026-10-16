#import bevy_ui::ui_vertex_output::UiVertexOutput;

@group(1) @binding(0) var<uniform> intensity: f32;
@group(1) @binding(1) var<uniform> pulse_color: vec4<f32>;
@group(1) @binding(2) var<uniform> flip: vec4<f32>;
@group(1) @binding(3) var color_texture: texture_2d<f32>;
@group(1) @binding(4) var color_sampler: sampler;

@fragment
fn fragment(mesh: UiVertexOutput) -> @location(0) vec4<f32> {
    var uv = mesh.uv;
    if flip.x > 0.5 {
        uv.x = 1.0 - uv.x;
    }
    if flip.y > 0.5 {
        uv.y = 1.0 - uv.y;
    }

    let source = textureSample(color_texture, color_sampler, uv);
    let brightened = min(
        source.rgb + pulse_color.rgb * intensity,
        vec3<f32>(1.0),
    );
    return vec4<f32>(brightened, source.a);
}
