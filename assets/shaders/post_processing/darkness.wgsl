#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct DarknessSettings {
    bg_light_level: f32,
    darkness_intensity: f32,
    _padding: vec2<f32>,
}

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var texture_sampler: sampler;
@group(0) @binding(2) var<uniform> settings: DarknessSettings;

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    // Sample the original screen color
    let screen_color = textureSample(screen_texture, texture_sampler, in.uv);
    
    // Target light color: RGB [245,250,253] converted to 0-1 range
    let target_light_color = vec3<f32>(245.0/255.0, 250.0/255.0, 253.0/255.0);
    
    // --- Light Propagation with Gaussian Falloff ---
    // We use a large, sparse sampling kernel for a performant blur.
    // This gives a wide, soft falloff approximating a Gaussian distribution.
    
    let pixel_size = 1.0 / vec2<f32>(textureDimensions(screen_texture, 0));
    
    // Parameters for the blur effect
    let kernel_radius = 200; // How far the light spreads
    let kernel_step = 10;    // Step between samples for performance
    let sigma = 30.0;       // Controls the softness of the falloff (gradual)

    var total_light = 0.0;
    var total_weight = 0.0;

    // By adding a randomized jitter to our sampling grid for each pixel, we can
    // break up the hard edges caused by the sparse sampling pattern. This turns
    // blocky artifacts into a smoother, more natural-looking noise pattern.
    let jitter_angle = random(in.uv) * 6.28318; // 2 * PI
    let jitter_radius = random(in.uv + vec2(0.1,0.2)) * f32(kernel_step);
    let jitter = vec2(cos(jitter_angle), sin(jitter_angle)) * jitter_radius;

    // Loop over a sparse grid to gather light from nearby sources
    for (var y = -kernel_radius; y <= kernel_radius; y = y + kernel_step) {
        for (var x = -kernel_radius; x <= kernel_radius; x = x + kernel_step) {
            let offset = vec2<f32>(f32(x), f32(y));
            // Apply the jitter to the sample position
            let sample_uv = in.uv + (offset + jitter) * pixel_size;

            // Ensure we are sampling within the texture bounds
            if (sample_uv.x >= 0.0 && sample_uv.x <= 1.0 && sample_uv.y >= 0.0 && sample_uv.y <= 1.0) {
                let sample_color = textureSample(screen_texture, texture_sampler, sample_uv);
                
                // Check if the sampled pixel is a light source
                let color_diff = abs(sample_color.rgb - target_light_color);
                let max_diff = max(max(color_diff.r, color_diff.g), color_diff.b);
                
                // Use smoothstep to create a soft falloff around the light source color
                // instead of a hard threshold. This further reduces sharp edges.
                let threshold = 0.05;
                let softness = 0.02;
                let is_light = 1.0 - smoothstep(threshold - softness, threshold + softness, max_diff);
                
                // Weigh the light source by its distance using a Gaussian function
                let dist = length(offset);
                let weight = exp(-(dist * dist) / (2.0 * sigma * sigma));
                
                total_light += is_light * weight;
                total_weight += weight;
            }
        }
    }

    // Calculate the final light visibility for the current pixel
    var light_visibility = 0.0;
    if (total_weight > 0.0) {
        light_visibility = total_light / total_weight;
    }
    
    // Calculate final brightness
    let final_brightness = mix(
        settings.bg_light_level, // 20% brightness for non-light areas (80% darker)
        1.0, // Full brightness where light is propagated
        light_visibility
    );
    
    // Apply the lighting to the screen color
    let final_color = screen_color.rgb * final_brightness;
    
    return vec4<f32>(final_color, screen_color.a);
}

// A simple hash function to create a pseudo-random number from a vec2.
// Used for jittering the sampling grid to smooth out artifacts.
fn random(co: vec2<f32>) -> f32 {
    return fract(sin(dot(co.xy, vec2<f32>(12.9898, 78.233))) * 43758.5453);
} 