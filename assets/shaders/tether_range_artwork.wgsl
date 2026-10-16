#import bevy_sprite::mesh2d_vertex_output::VertexOutput

const TAU: f32 = 6.283185307179586;
const CENTER_EPSILON: f32 = 0.001;

struct TetherRangeArtworkMaterial {
    // Source ring radius, tendril alpha, outline alpha, clip inset.
    settings: vec4<f32>,
};

@group(2) @binding(0) var artwork_texture: texture_2d<f32>;
@group(2) @binding(1) var artwork_sampler: sampler;
@group(2) @binding(2) var<storage, read> circles: array<vec4<f32>>;
@group(2) @binding(3) var<uniform> material: TetherRangeArtworkMaterial;

fn outline_seed(circle: vec4<f32>) -> f32 {
    return circle.x * 0.017 + circle.y * 0.023;
}

fn deformed_radius(circle: vec4<f32>, angle: f32) -> f32 {
    let seed = outline_seed(circle);
    let deformation =
        sin(angle * 2.0 + seed * 1.13) * 0.62
        + sin(angle * 3.0 - seed * 0.73) * 0.38;
    return max(circle.z + circle.w * deformation, CENTER_EPSILON);
}

fn artwork_rotation(circle: vec4<f32>) -> f32 {
    let random = fract(sin(outline_seed(circle) * 12.9898) * 43758.5453);
    return random * TAU;
}

fn circle_hides_current(
    current_index: u32,
    current: vec4<f32>,
    other_index: u32,
    other: vec4<f32>,
    world_position: vec2<f32>,
    clip_inset: f32,
) -> bool {
    let center_distance = distance(current.xy, other.xy);
    if center_distance <= CENTER_EPSILON {
        return other.z > current.z
            || (other.z == current.z && other_index < current_index);
    }

    let offset = world_position - other.xy;
    let angle = atan2(offset.y, offset.x);
    let clip_radius = max(
        deformed_radius(other, angle) - clip_inset,
        0.0,
    );
    return length(offset) < clip_radius;
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let circle_count = arrayLength(&circles);
    let circle_index = u32(round(mesh.color.x));
    if circle_index >= circle_count {
        discard;
    }

    let circle = circles[circle_index];
    let world_position = mesh.world_position.xy;
    let offset = world_position - circle.xy;
    let angle = atan2(offset.y, offset.x);
    let display_radius = deformed_radius(circle, angle);
    let world_direction = vec2<f32>(cos(angle), sin(angle));
    let root_world_position = circle.xy + world_direction * display_radius;
    let source_distance =
        length(offset) / display_radius * material.settings.x;
    let source_angle = angle + artwork_rotation(circle);
    let source_direction = vec2<f32>(cos(source_angle), sin(source_angle));
    let uv = vec2<f32>(0.5) + source_direction * source_distance;
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) {
        discard;
    }

    let artwork = textureSample(artwork_texture, artwork_sampler, uv);
    if artwork.a <= 0.01 {
        discard;
    }

    let is_tendril = source_distance < material.settings.x * 0.90;
    for (var other_index = 0u; other_index < circle_count; other_index += 1u) {
        if other_index == circle_index {
            continue;
        }
        if circle_hides_current(
            circle_index,
            circle,
            other_index,
            circles[other_index],
            world_position,
            material.settings.w,
        ) {
            discard;
        }
        if is_tendril && circle_hides_current(
            circle_index,
            circle,
            other_index,
            circles[other_index],
            root_world_position,
            -material.settings.w,
        ) {
            discard;
        }
    }

    let ring_strength = smoothstep(
        material.settings.x * 0.90,
        material.settings.x * 0.98,
        source_distance,
    );
    let alpha = mix(material.settings.y, material.settings.z, ring_strength);
    return vec4<f32>(0.0, 0.0, 0.0, artwork.a * alpha);
}
