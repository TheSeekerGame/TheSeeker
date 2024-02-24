#import bevy_sprite::{mesh2d_view_bindings::globals}
#import bevy_sprite::{mesh2d_view_bindings::view}
#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import "shaders/perlin_noise_3d.wgsl"::perlinNoise3

struct FogMaterial {
    depth: f32,
    alpha: f32,
    color: vec4<f32>,
    emitter1: vec4<f32>,
    emitter2: vec4<f32>,
    emitter3: vec4<f32>,
}

@group(1) @binding(0) var<uniform> fog_mat: FogMaterial;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    var world_pos = mesh.world_position.xy;
    let camera_pos = view.world_position;

    // calculates parallax offset
    // works by scaling the effective distance of the emitter from the camera
    let emitter_pos_init = fog_mat.emitter1.xy + vec2(100.0, 0.0);
    var delta = emitter_pos_init - camera_pos.xy;
    delta *= 1.0/ (fog_mat.depth);
    let emitter_pos_final = camera_pos.xy + delta.xy;

    // offset is needed so that world_space moves with the parallax
    // this way fog won't "roll" whenever the camera moves.
    let offset = emitter_pos_final - emitter_pos_init;
    let scaledpos = (world_pos - offset)*0.05;

    // low freq noise
    var low_f_noise = perlinNoise3(vec3<f32>(globals.time*0.1 + 1.0 / fog_mat.depth, scaledpos.y*0.32, scaledpos.x*0.32, ));
    // Generates cool noise affect by combining squared noise with subtracted, different scaled noise.
    var noise = perlinNoise3(vec3<f32>(globals.time*0.25 + 10.0 / fog_mat.depth, scaledpos.y, scaledpos.x, ));
    var noise1 = perlinNoise3(vec3<f32>(globals.time*0.25 + 5.0 / fog_mat.depth, scaledpos.y*0.5, scaledpos.x*0.5, ));
    noise = (noise*noise - noise1- low_f_noise)*0.3 + 0.5;

    // makes fog falloff farther from emitter center
    let distance = distance(world_pos, emitter_pos_final);
    let max_distance = fog_mat.emitter1.z;

    //let inv_dist = max(max_distance-distance, 0.0);

    let max_lfn = clamp(low_f_noise, 0.0, 1.0);
    let normalized_distance = clamp(max((distance / (max_distance)), 1.9) * noise, 0.0, 1.0);

    let gradient_color = vec4<f32>(fog_mat.color.rgb, clamp(1.0 - normalized_distance, 0.0, 1.0) *fog_mat.alpha); // Fading effect

    return gradient_color;
    //return vec4(low_f_noise, low_f_noise, low_f_noise, 1.0);
}