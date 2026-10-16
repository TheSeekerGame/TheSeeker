#import bevy_sprite::mesh2d_functions

@group(2) @binding(0) var foliage_atlas: texture_2d<f32>;
@group(2) @binding(1) var foliage_sampler: sampler;
@group(2) @binding(2) var interaction_field: texture_2d<f32>;
@group(2) @binding(3) var interaction_sampler: sampler;

struct FoliageParams {
    time_seconds: f32,
    max_interaction_bend: f32,
    primary_wind_angular_speed: f32,
    secondary_wind_angular_speed: f32,
    interaction_field_params: vec4<f32>,
    camera_alignment_offset: vec2<f32>,
    _padding: vec2<f32>,
};

@group(2) @binding(4) var<uniform> params: FoliageParams;

struct VertexInput {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) foliage_data: vec4<f32>,
    @location(3) bend_weight: f32,
    @location(4) flash: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) flash: f32,
};

fn sample_interaction_field(root: vec2<f32>) -> f32 {
    let origin = params.interaction_field_params.xy;
    let interpolation_alpha = params.interaction_field_params.z;
    let cell_size = params.interaction_field_params.w;
    let field_world_size = vec2<f32>(320.0, 192.0) * cell_size;
    let uv = (root - origin) / field_world_size;
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) {
        return 0.0;
    }
    let encoded = textureSampleLevel(
        interaction_field,
        interaction_sampler,
        uv,
        0.0,
    ).rg;
    let displacement = (encoded * 2.0 - 1.0)
        * params.max_interaction_bend;
    return mix(displacement.x, displacement.y, interpolation_alpha);
}

@vertex
fn vertex(vertex: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let model = mesh2d_functions::get_world_from_local(vertex.instance_index);
    let root_world = mesh2d_functions::mesh2d_position_local_to_world(
        model,
        vec4<f32>(vertex.foliage_data.xy, 0.0, 1.0),
    );

    let phase = vertex.foliage_data.z;
    let wind_amplitude = vertex.foliage_data.w;
    let primary = sin(
        params.time_seconds * params.primary_wind_angular_speed
            + root_world.x * 0.035
            + phase,
    );
    let secondary = sin(
        params.time_seconds * params.secondary_wind_angular_speed
            - root_world.x * 0.019
            + phase * 1.71,
    );
    let wind = wind_amplitude * (primary * 0.72 + secondary * 0.28);

    let interaction = sample_interaction_field(root_world.xy);

    var local_position = vertex.position;
    local_position.x += (wind + interaction) * vertex.bend_weight;
    let world = mesh2d_functions::mesh2d_position_local_to_world(
        model,
        vec4<f32>(local_position, 1.0),
    );
    let aligned_world = world + vec4<f32>(
        params.camera_alignment_offset,
        0.0,
        0.0,
    );
    out.clip_position = mesh2d_functions::mesh2d_position_world_to_clip(
        aligned_world,
    );
    out.uv = vertex.uv;
    out.flash = vertex.flash;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(foliage_atlas, foliage_sampler, in.uv);
    if color.a < 0.5 {
        discard;
    }
    return vec4<f32>(color.rgb * in.flash, 1.0);
}
