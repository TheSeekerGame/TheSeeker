// Since post processing is a fullscreen effect, we use the fullscreen vertex shader provided by bevy.
// This will import a vertex shader that renders a single fullscreen triangle.
#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput
@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var texture_sampler: sampler;
struct PostProcessSettings {
    intensity: f32,
    character_position: vec2<f32>,
    dimensions: vec2<f32>,
#ifdef SIXTEEN_BYTE_ALIGNMENT
    // WebGL2 structs must be 16 byte aligned.
    _webgl2_padding: vec3<f32>
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

    // use inverse square law for light intensity (ie: player must be carrying a light source)
    let x = sqr_magnitude(pos-(settings.character_position.xy + widthf32*0.5)) * (1.0/500.0);

    // Adds a falloff curve if "lantern" bright point is undesired and so is a sharp cutoff
    //let sigmoid = 1.0/(1.0 + exp(-2.0*(x - 1.4)));
    //let lantern = clamp(((1.0/x)*sigmoid + (1.0-sigmoid)), 0.0, 5.0);

    let lantern = clamp(1.0/x, 0.0, 5.0);

    let base_light = settings.intensity;

    // Sample each color channel with an arbitrary shift
    return vec4(bg_color.rgb * mix(lantern, 1.0, base_light), 1.0);
}