#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput
#import game::preprocessing::floaters::compute

@fragment
fn floater_fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    // return vec4<f32>(0.0, 0.0, 0.0, 0.0);
}
