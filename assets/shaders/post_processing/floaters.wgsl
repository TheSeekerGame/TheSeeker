#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput
#import bevy_pbr::view_transformations::depth_ndc_to_view_z

#ifdef MULTISAMPLED
@group(0) @binding(1) var depth_texture: texture_depth_multisampled_2d;
#else   // MULTISAMPLED
@group(0) @binding(1) var depth_texture: texture_depth_2d;
#endif  // MULTISAMPLED

@group(0) @binding(2) var screen_texture: texture_2d<f32>;
@group(0) @binding(3) var texture_sampler: sampler;

@fragment
fn floater_fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let frag_coord = vec2<i32>(floor(in.position.xy));
    let raw_depth = textureLoad(depth_texture, frag_coord, 0);
    let depth = -depth_ndc_to_view_z(raw_depth);
    return vec4<f32>(1.0, depth, depth, 1.0);
}
