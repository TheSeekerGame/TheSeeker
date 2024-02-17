#import bevy_sprite::{mesh2d_view_bindings::globals,}
#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import "shaders/perlin_noise_3d.wgsl"::perlinNoise3

struct FogMaterial {
    color: vec4<f32>,
    emitter1: vec4<f32>,
    emitter2: vec4<f32>,
    emitter3: vec4<f32>,
}

@group(1) @binding(0) var<uniform> fog_mat: FogMaterial;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let white = vec4<f32>(0.87, 0.86, 1.0, 1.0);
    let world_pos = mesh.world_position.xy;
    let distance = distance(world_pos, fog_mat.emitter1.xy*200.0);
    //let distance = distance(mesh.world_position.xy, vec2<f32>(0.0, 350.0));

    // Normalize the distance to use it for a gradient effect
    // Assuming a maximum distance that makes sense for your use case, for example, 1000 units
    let max_distance = 100.0;
    let scaledpos = world_pos*0.05;
    let noise = (perlinNoise3(vec3<f32>(globals.time*0.3, scaledpos.y, scaledpos.x, )))*0.1 + 0.6;
    let normalized_distance = clamp((distance / max_distance) * noise, 0.0, 1.0) ;

    // Create a gradient based on the distance. This example modifies the alpha value,
    // but you could also adjust the color intensity or create a color gradient
    let gradient_color = vec4<f32>(white.rgb, (1.0 - normalized_distance) *0.25); // Fading effect

    return gradient_color;
    //return ;
}