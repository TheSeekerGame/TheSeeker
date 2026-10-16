// Screen-space ripple / shockwave distortion.

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

const MAX_RIPPLES: u32 = 8u;

struct Ripple {
    center: vec2<f32>,
    radius_px: f32,
    width_px: f32,
    amplitude_px: f32,
    _pad: vec2<f32>,
}

struct RippleSettings {
    viewport_size: vec2<f32>,
    ripple_count: u32,
    _pad0: u32,
    ripples: array<Ripple, MAX_RIPPLES>,
}

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var<uniform> settings: RippleSettings;

// How far the ripples displace the view at `uv`, in pixels.
fn ripple_offset_px(uv: vec2<f32>) -> vec2<f32> {
    var offset_px = vec2<f32>(0.0);
    let count = min(settings.ripple_count, MAX_RIPPLES);
    for (var i: u32 = 0u; i < count; i = i + 1u) {
        let ripple = settings.ripples[i];
        let delta_px = (uv - ripple.center) * settings.viewport_size;
        let dist = length(delta_px);
        if dist < 0.0001 {
            continue;
        }

        let dir = delta_px / dist;
        let inner = smoothstep(ripple.radius_px - ripple.width_px, ripple.radius_px, dist);
        let outer = 1.0 - smoothstep(ripple.radius_px, ripple.radius_px + ripple.width_px, dist);
        let ring = inner * outer;

        offset_px += dir * (ripple.amplitude_px * ring);
    }

    return offset_px;
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    // `in.uv` spans the viewport and `in.position` the whole image, so this fragment's place in
    // both gives the viewport's corner in the image. Displaced reads stay inside the viewport.
    let viewport_min = round(in.position.xy - in.uv * settings.viewport_size);
    let viewport_max = viewport_min + settings.viewport_size - 1.0;
    let pixel = clamp(floor(in.position.xy + ripple_offset_px(in.uv)), viewport_min, viewport_max);
    return textureLoad(screen_texture, vec2<i32>(pixel), 0);
}
