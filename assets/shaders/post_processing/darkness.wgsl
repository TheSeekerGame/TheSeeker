#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct DarknessSettings {
    bg_light_level: f32,
    darkness_intensity: f32,
    _padding: vec2<f32>,
}

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var texture_sampler: sampler;
@group(0) @binding(2) var<uniform> settings: DarknessSettings;

// A simple hash function to create a pseudo-random number from a vec2.
// Used for jittering the sampling grid to smooth out artifacts.
fn random(co: vec2<f32>) -> f32 {
    return fract(sin(dot(co.xy, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let screen_color = textureSample(screen_texture, texture_sampler, in.uv);
    let target_light_color = vec3<f32>(245.0/255.0, 250.0/255.0, 253.0/255.0);
    let pixel_size = 1.0 / vec2<f32>(textureDimensions(screen_texture, 0));

    let kernel_radius = 50; // Reduced from 100 for better performance
    let kernel_step = 8;    // Reduced from 10 for slightly better quality
    let sigma = 25.0;       // Adjusted for new kernel size

    var total_light = 0.0;
    var total_weight = 0.0;

    let jitter_angle = random(in.uv) * 6.28318; // 2 * PI
    let jitter_radius = random(in.uv + vec2(0.1,0.2)) * f32(kernel_step);
    let jitter = vec2(cos(jitter_angle), sin(jitter_angle)) * jitter_radius;

#ifdef HORIZONTAL
    // --- Horizontal Pass ---
    // This pass identifies light sources and blurs their influence horizontally.
    
    // First, check if current pixel is part of a light source (including player)
    let center_color_diff = abs(screen_color.rgb - target_light_color);
    let center_max_diff = max(max(center_color_diff.r, center_color_diff.g), center_color_diff.b);
    let center_threshold = 0.05;
    let center_softness = 0.02;
    let center_is_light = 1.0 - smoothstep(center_threshold - center_softness, center_threshold + center_softness, center_max_diff);
    
    // Check if this is likely a player sprite by looking for skin-like colors
    // Player sprites typically have warmer, flesh-toned colors
    let player_color1 = vec3<f32>(0.9, 0.7, 0.6); // Light skin tone
    let player_color2 = vec3<f32>(0.8, 0.6, 0.5); // Medium skin tone  
    let player_color3 = vec3<f32>(0.7, 0.5, 0.4); // Darker skin tone
    let player_color4 = vec3<f32>(0.6, 0.4, 0.3); // Brown/red clothing
    
    let player_diff1 = length(screen_color.rgb - player_color1);
    let player_diff2 = length(screen_color.rgb - player_color2);
    let player_diff3 = length(screen_color.rgb - player_color3);
    let player_diff4 = length(screen_color.rgb - player_color4);
    
    let player_threshold = 0.3;
    let is_player_color = (player_diff1 < player_threshold) || 
                         (player_diff2 < player_threshold) || 
                         (player_diff3 < player_threshold) ||
                         (player_diff4 < player_threshold);
    
    for (var i = -kernel_radius; i <= kernel_radius; i = i + kernel_step) {
        let offset = vec2<f32>(f32(i) + jitter.x, jitter.y);
        let sample_uv = in.uv + offset * pixel_size;
        let sample_color = textureSample(screen_texture, texture_sampler, sample_uv);

        let color_diff = abs(sample_color.rgb - target_light_color);
        let max_diff = max(max(color_diff.r, color_diff.g), color_diff.b);

        let threshold = 0.05;
        let softness = 0.02;
        let is_light = 1.0 - smoothstep(threshold - softness, threshold + softness, max_diff);
        
        // Also check for player colors in the sample
        let sample_player_diff1 = length(sample_color.rgb - player_color1);
        let sample_player_diff2 = length(sample_color.rgb - player_color2);
        let sample_player_diff3 = length(sample_color.rgb - player_color3);
        let sample_player_diff4 = length(sample_color.rgb - player_color4);
        
        let sample_is_player = (sample_player_diff1 < player_threshold) || 
                              (sample_player_diff2 < player_threshold) || 
                              (sample_player_diff3 < player_threshold) ||
                              (sample_player_diff4 < player_threshold);

        let combined_light = max(is_light, f32(sample_is_player));

        let dist = abs(f32(i));
        let weight = exp(-(dist * dist) / (2.0 * sigma * sigma));

        total_light += combined_light * weight;
        total_weight += weight;
    }

    var light_visibility = 0.0;
    if (total_weight > 0.0) {
        light_visibility = total_light / total_weight;
    }
    
    // If this pixel is a player color, ensure it gets full lighting
    if (is_player_color) {
        light_visibility = max(light_visibility, 1.0);
    }

    // Store horizontal visibility in the alpha channel for the next pass.
    // We keep the original color in rgb to reconstruct it later.
    return vec4<f32>(screen_color.rgb, light_visibility);

#else
    // --- Vertical Pass ---
    // This pass reads the horizontally blurred light visibility and blurs it vertically.
    for (var i = -kernel_radius; i <= kernel_radius; i = i + kernel_step) {
        let offset = vec2<f32>(jitter.x, f32(i) + jitter.y);
        let sample_uv = in.uv + offset * pixel_size;
        let sample_data = textureSample(screen_texture, texture_sampler, sample_uv);

        // The alpha channel now contains the horizontally-blurred light visibility.
        let horizontal_light = sample_data.a;

        let dist = abs(f32(i));
        let weight = exp(-(dist * dist) / (2.0 * sigma * sigma));

        total_light += horizontal_light * weight;
        total_weight += weight;
    }

    var light_visibility = 0.0;
    if (total_weight > 0.0) {
        light_visibility = total_light / total_weight;
    }
    
    // Final check: if this pixel contains player colors, ensure full brightness
    let center_player_diff1 = length(screen_color.rgb - vec3<f32>(0.9, 0.7, 0.6));
    let center_player_diff2 = length(screen_color.rgb - vec3<f32>(0.8, 0.6, 0.5));
    let center_player_diff3 = length(screen_color.rgb - vec3<f32>(0.7, 0.5, 0.4));
    let center_player_diff4 = length(screen_color.rgb - vec3<f32>(0.6, 0.4, 0.3));
    
    let player_threshold = 0.3;
    let center_is_player = (center_player_diff1 < player_threshold) || 
                          (center_player_diff2 < player_threshold) || 
                          (center_player_diff3 < player_threshold) ||
                          (center_player_diff4 < player_threshold);
    
    if (center_is_player) {
        light_visibility = 1.0;
    }

    let final_brightness = mix(
        settings.bg_light_level,
        1.0,
        light_visibility
    );

    // Apply the lighting to the screen color
    let final_color = screen_color.rgb * final_brightness;

    return vec4<f32>(final_color, screen_color.a);

#endif
}