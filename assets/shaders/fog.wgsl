#import bevy_sprite::mesh2d_vertex_output::VertexOutput

struct FogMaterial {
    color: vec4<f32>,
    emitter1: vec4<f32>,
    emitter2: vec4<f32>,
    emitter3: vec4<f32>,
}

@group(1) @binding(0) var<uniform> fog_mat: FogMaterial;


@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
       // Calculate the distance from the fragment to emitter 1
       let distance = distance(mesh.world_position.xy, fog_mat.emitter1.xy*200.0);
       //let distance = distance(mesh.world_position.xy, vec2<f32>(0.0, 350.0));

       // Normalize the distance to use it for a gradient effect
       // Assuming a maximum distance that makes sense for your use case, for example, 1000 units
       let max_distance = 100.0;
       let normalized_distance = clamp(distance / max_distance, 0.0, 1.0);

       // Create a gradient based on the distance. This example modifies the alpha value,
       // but you could also adjust the color intensity or create a color gradient
       let gradient_color = vec4<f32>(fog_mat.color.rgb, 1.0 - normalized_distance); // Fading effect

       return gradient_color;
      //return vec4<f32>(1.0, 1.0, 1.0, 0.5);
}