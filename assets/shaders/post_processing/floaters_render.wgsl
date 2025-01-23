#import bevy_render::view::View;
#import bevy_render::globals::Globals;
#import game::preprocessing::floaters::{Floater, FloaterBuffer, FloaterSettings, FLOATER_SAMPLES_X, FLOATER_SAMPLES_Y}

@group(0) @binding(0) var<uniform> view: View;
@group(0) @binding(1) var<uniform> globals: Globals;
@group(0) @binding(2) var<uniform> floater_settings: FloaterSettings;
@group(0) @binding(3) var<storage, read_write> floater_buffer: FloaterBuffer;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
}

@vertex
fn floater_vertex(
        @builtin(vertex_index) vertex_index: u32,
        @builtin(instance_index) instance_index: u32
        ) -> VertexOutput {

    var layer_distances: array<f32, 6> = array<f32, 6>(
        0.1, -0.1, -0.2, -0.3, -0.4, -0.5
    );

    var output: VertexOutput;
    let layer_size = FLOATER_SAMPLES_X * FLOATER_SAMPLES_Y;
    let layer = instance_index / layer_size;
    let floater = floater_buffer.floaters[layer][instance_index % layer_size];

    var model = mat4x4<f32>(
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0
    );

    output.uv = vec2<f32>(
        select(-1.0, 1.0, vertex_index == 1u || vertex_index == 4u || vertex_index == 5u),
        select(-1.0, 1.0, vertex_index == 2u || vertex_index == 3u || vertex_index == 5u)
    );
    model[3] = vec4<f32>(output.uv * floater.scale / 2, 0.0, 1.0);
    output.position = view.view_proj * model * vec4<f32>(floater.position, layer_distances[layer], 1.0);
    output.color = vec4<f32>(f32(layer) / 4.0, 0.0, 0.3, 1.0);
    return output;
}

@fragment
fn floater_fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
