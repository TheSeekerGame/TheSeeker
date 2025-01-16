#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput
#import game::preprocessing::floaters::{Floater, FloaterBuffer, FloaterSettings, FLOATER_SAMPLES_X, FLOATER_SAMPLES_Y}

@group(0) @binding(1) var<uniform> floater_settings: FloaterSettings;
@group(0) @binding(2) var<storage, read_write> floater_buffer: FloaterBuffer;

@fragment
fn floater_fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 1.0, 0.0, 0.0);
}
