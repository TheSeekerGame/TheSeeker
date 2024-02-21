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
    let white = vec4<f32>(0.87, 0.86, 1.0, 1.0);
    var world_pos = mesh.world_position.xy;
    let camera_pos = view.world_position;
    let emitter_pos_init = fog_mat.emitter1.xy;
    var delta = emitter_pos_init - camera_pos.xy;
    delta *= 1.0/ (fog_mat.depth);
    let emitter_pos_final = camera_pos.xy + delta.xy;
    let offset = emitter_pos_final - emitter_pos_init;
    let distance = distance(world_pos, emitter_pos_final);
    //let distance = distance(mesh.world_position.xy, vec2<f32>(0.0, 350.0));

    // Normalize the distance to use it for a gradient effect
    // Assuming a maximum distance that makes sense for your use case, for example, 1000 units
    let max_distance = fog_mat.emitter1.z;
    let scaledpos = (world_pos - offset)*0.05;
    var noise = perlinNoise3(vec3<f32>(globals.time*0.3 / fog_mat.depth, scaledpos.y, scaledpos.x, ));
    var noise1 = perlinNoise3(vec3<f32>(globals.time*0.3 / fog_mat.depth, scaledpos.y*0.5, scaledpos.x*0.5, ));
    noise = (noise*noise - noise1)*0.3 + 0.5;
    let normalized_distance = clamp((distance / max_distance) * noise, 0.0, 1.0) ;

    // Create a gradient based on the distance.
    let gradient_color = vec4<f32>(white.rgb, (1.0 - normalized_distance) *fog_mat.alpha); // Fading effect

    return gradient_color;
    //return ;
}