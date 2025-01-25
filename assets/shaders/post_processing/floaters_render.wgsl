#import bevy_render::view::View;
#import bevy_render::globals::Globals;
#import game::preprocessing::floaters::{Floater, FloaterBuffer, FloaterSettings, get_layer_distance, apply_parallax, FLOATER_SAMPLES_X, FLOATER_SAMPLES_Y}

@group(0) @binding(0) var<uniform> view: View;
@group(0) @binding(1) var<uniform> globals: Globals;
@group(0) @binding(2) var<uniform> floater_settings: FloaterSettings;
@group(0) @binding(3) var<storage, read_write> floater_buffer: FloaterBuffer;
@group(0) @binding(4) var floater_texture: texture_2d<f32>;
@group(0) @binding(5) var floater_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn floater_vertex(
        @builtin(vertex_index) vertex_index: u32,
        @builtin(instance_index) instance_index: u32
        ) -> VertexOutput {


    var output: VertexOutput;
    let layer_size = FLOATER_SAMPLES_X * FLOATER_SAMPLES_Y;
    let layer = instance_index / layer_size;
    let layer_distance = get_layer_distance(layer);
    let layer_scale = 1.0 / (1.0 - layer_distance);
    let floater = floater_buffer.floaters[layer][instance_index % layer_size];

    var model = mat4x4<f32>(
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0
    );

    output.uv = vec2<f32>(
        select(0.0, 1.0, vertex_index == 1u || vertex_index == 4u || vertex_index == 5u),
        select(0.0, 1.0, vertex_index == 2u || vertex_index == 3u || vertex_index == 5u)
    );
    model[3] = vec4<f32>(output.uv * floater.scale, 0.0, 1.0);
    let parallax_position = apply_parallax(floater.position, view.world_position.xy, layer_scale);
    output.position = view.view_proj * model * vec4<f32>(parallax_position, layer_distance, 1.0);
    return output;
}

@fragment
fn floater_fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let tile_uv_width = f32(floater_settings.sprite_width) / f32(floater_settings.spritesheet_width);
    let uv = vec2<f32>(in.uv.x * tile_uv_width + f32(floater_settings.sprite_index) * tile_uv_width, in.uv.y);

    let value = textureSample(floater_texture, floater_sampler, uv);
    if (value.a < 0.6) {
        discard;
    }
    return value;
}
