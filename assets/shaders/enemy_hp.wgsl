#import bevy_ui::ui_vertex_output::UiVertexOutput;

@group(1) @binding(0) var<uniform> health: f32;
@group(1) @binding(1) var<uniform> damage: f32;

const EMPTY_COLOR = vec4(0.15);
const HEALTH_COLOR = vec4(0.635, 0.196, 0.306, 1.0);
const DAMAGED_COLOR = vec4(1.0);

@fragment
fn fragment(mesh: UiVertexOutput) -> @location(0) vec4<f32> {
    var color = EMPTY_COLOR;

    color = mix(color, DAMAGED_COLOR, step(mesh.uv.x, damage));

    color = mix(color, HEALTH_COLOR, step(mesh.uv.x, health));

    let alpha = step(abs(mesh.uv.y - 0.5), 0.12);

    return to_linear(vec4(color.rgb, alpha));
}

fn to_linear(nonlinear: vec4<f32>) -> vec4<f32> {
    let cutoff = step(nonlinear, vec4<f32>(0.04045));
    let higher = pow((nonlinear + vec4<f32>(0.055)) / vec4<f32>(1.055), vec4<f32>(2.4));
    let lower = nonlinear / vec4<f32>(12.92);
    return mix(higher, lower, cutoff);
}
