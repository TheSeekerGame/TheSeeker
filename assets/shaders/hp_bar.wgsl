#import bevy_ui::ui_vertex_output::UiVertexOutput;

@group(1) @binding(0) var<uniform> factor: f32;
@group(1) @binding(1) var<uniform> background_color: vec4<f32>;
@group(1) @binding(2) var<uniform> filled_color: vec4<f32>;

@fragment
fn fragment(mesh: UiVertexOutput) -> @location(0) vec4<f32> {
    if mesh.uv.x <= factor {
        return filled_color;
    } else {
        return background_color;
    }
}