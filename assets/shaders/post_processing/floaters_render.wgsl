#import game::preprocessing::floaters::{Floater, FloaterBuffer, FloaterSettings, FLOATER_SAMPLES_X, FLOATER_SAMPLES_Y}

@group(0) @binding(1) var<uniform> floater_settings: FloaterSettings;
@group(0) @binding(2) var<storage, read_write> floater_buffer: FloaterBuffer;

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
    var output: VertexOutput;
    output.position = vec4<f32>(f32(vertex_index % 2), f32(vertex_index / 2), 0.0, 1.0);
    output.color = vec4<f32>(0.0, 1.0, 0.0, 0.0);
    output.uv = vec2<f32>(f32(vertex_index % 2), f32(vertex_index / 2));
    return output;
}

@fragment
fn floater_fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 1.0, 0.0, 0.0);
}
