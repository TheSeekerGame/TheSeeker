#import bevy_ui::ui_vertex_output::UiVertexOutput;

@group(1) @binding(0) var<uniform> factor: f32;

const EMPTY_COLOR = vec3(0.1, 0.1, 0.1);
const HEALTH_COLOR = vec3(0.635, 0.196, 0.306);

@fragment
fn fragment(mesh: UiVertexOutput) -> @location(0) vec4<f32> {
    var color = EMPTY_COLOR;

    color = mix(EMPTY_COLOR, HEALTH_COLOR, step(mesh.uv.x, factor));

    let alpha = step(abs(mesh.uv.y - 0.5), 0.12);

    return vec4(color, alpha);
}
