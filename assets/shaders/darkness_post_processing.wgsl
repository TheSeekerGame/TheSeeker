// Since post processing is a fullscreen effect, we use the fullscreen vertex shader provided by bevy.
// This will import a vertex shader that renders a single fullscreen triangle.
#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput
@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var texture_sampler: sampler;
struct PostProcessSettings {
    bg_light_level: f32,
    lantern_position: vec2<f32>,
    lantern: f32,
    lantern_color: vec3<f32>,
    bg_light_color: vec3<f32>,
#ifdef SIXTEEN_BYTE_ALIGNMENT
    // WebGL2 structs must be 16 byte aligned.
    _webgl2_padding: vec2<f32>
#endif
}
@group(0) @binding(2) var<uniform> settings: PostProcessSettings;
    
fn sqr_magnitude(v: vec2<f32>) -> f32 {
    return v.x * v.x + v.y * v.y;
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    // original screen color fragment:
    let bg_color = textureSample(screen_texture, texture_sampler, in.uv);
    let width = textureDimensions(screen_texture, 0);
    let widthf32 = vec2<f32>(f32(width.x), f32(width.y));

    let pos = in.position.xy;
    let lantern_pos = settings.lantern_position.xy + vec2(0.0, 15.0);

    // use inverse square law for light intensity (ie: player must be carrying a light source)
    // and treat light as if it is 3d, and dist_a away from the 2d screen. (fixes sharp falloff)

    // dist of 3d light from 2d plane
    let dist_a = 40.0;
    // dist of *2d lantern* from screen fragment
    let dist_b_sqrd = sqr_magnitude(pos-(lantern_pos + widthf32*0.5));
    // "3d" dist of screen fragment to 3d light.
    let dist_c = /*sqrt*/(dist_a * dist_a + dist_b_sqrd);
    // inverse square law for intensity (sqrt cancels out)
    let intensity = 1.0/dist_c/*^2*/;

    let light =  intensity * 900.0;
    let color = light*settings.lantern*settings.lantern_color;
    let final_brightness = mix(color, vec3(1.0), settings.bg_light_level);

    return vec4(bg_color.rgb * final_brightness  , 1.0);
}