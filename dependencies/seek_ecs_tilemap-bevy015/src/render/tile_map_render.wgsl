#import bevy_render::{
    view::View,
}

struct VertexInput {
    @builtin(vertex_index) vertex_id: u32,
    @builtin(instance_index) instance_id: u32,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) index_color: vec2<u32>,
#ifdef TILEMAP_HAS_TILE_TEXTURE
    @location(1) uv: vec2<f32>,
#endif
}

struct TilemapInfo {
    transform_affine: mat3x4<f32>,
    tile_size: vec2<f32>,
    grid_size: vec2<f32>,
    n_tiles_per_chunk: vec2<u32>,
    n_chunks: vec2<u32>,
}

@group(0) @binding(0) var<uniform> view: View;

@group(1) @binding(0) var<uniform> tilemap_info: TilemapInfo;
@group(1) @binding(1) var tilemap_texture: texture_2d_array<u32>;

#ifdef TILEMAP_HAS_TILE_TEXTURE
@group(2) @binding(0) var tile_texture: texture_2d_array<f32>;
@group(2) @binding(1) var tile_sampler: sampler;
#endif

@vertex
fn vertex(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;

    // WGSL quirk: must declare hardcoded array as `var`,
    // because if it is a const, it can only be indexed by a constant
    var TILE_VERT_POSITIONS = array(
        vec4<f32>(-0.5, -0.5, 0.0, 1.0),
        vec4<f32>( 0.5,  0.5, 0.0, 1.0),
        vec4<f32>(-0.5,  0.5, 0.0, 1.0),
        vec4<f32>(-0.5, -0.5, 0.0, 1.0),
        vec4<f32>( 0.5, -0.5, 0.0, 1.0),
        vec4<f32>( 0.5,  0.5, 0.0, 1.0),
    );

    // GPU programming trick: we can effectively "discard triangles"
    // by returning a vertex position with NaNs in it.
    // WGSL quirk: there is no way to hardcode a NaN, because
    // const-expressions that produce a NaN result in a compiler error.
    // Thus, we must come up with a clever roundabout way of creating NaNs.
    var zero = 0.0;
    var nan = zero / zero;
    // set position here so we can early-return
    // this will be overwritten later if we return normally
    output.position = vec4<f32>(nan, nan, nan, nan);

    // Figure out which tile we are in, and which vertex within
    // the tile, based on the vertex id. Every tile has 6 verts.
    let tile_index = input.vertex_id / 6u;
    let tile_vertex_index = input.vertex_id % 6u;

    // 2D "Tile Coordinates"
    let tile_pos = vec2<u32>(
        tile_index % tilemap_info.n_tiles_per_chunk.x,
        tile_index / tilemap_info.n_tiles_per_chunk.x,
    );

    // Load per-tile data out of our special texture
    let tile_texel =
        textureLoad(tilemap_texture, tile_pos, input.instance_id, 0);

    // "Discard" the vertex if the tile is not visible
    // (visibility bit is 0 or alpha in the color is 0)
    let check_visibility = tile_texel.r & (1u << 24u);
    let check_alpha = tile_texel.g & 0xFF000000;
    if check_visibility == 0u || check_alpha == 0u {
        return output; // FIXME: temporary
    }

    output.index_color = vec2(tile_texel.r & 0xFFFF, tile_texel.g);

    let tile_vertex_model = TILE_VERT_POSITIONS[tile_vertex_index];

#ifdef TILEMAP_HAS_TILE_TEXTURE
    // UVs: we can compute them from the positions ;)
    // Vertex positions are the corners of the quad.
    // Just take them and flip the sign if we need to flip the tile
    let flip_x = tile_texel.r & (1u << 25u);
    let flip_y = tile_texel.r & (1u << 26u);
    let flip_d = tile_texel.r & (1u << 27u);
    var uv = tile_vertex_model.xy;
    // for diagonal flip, check if the sign is the same;
    // this indicates we are processing one of the diagonal verts
    if flip_d != 0u && uv.x * uv.y > 0.0f {
        uv = -uv; // flip both x and y
    }
    if flip_x != 0u {
        uv.x = -uv.x;
    }
    if flip_y != 0u {
        uv.y = -uv.y;
    }
    // Add 0.5 to translate from `-0.5 .. 0.5` to `0.0 .. 1.0` range
    uv += vec2(0.5f, 0.5f);
    output.uv = uv;
#endif

    // Compute the translation where to position the tile:
    let chunk_grid_size = tilemap_info.grid_size
        * vec2<f32>(tilemap_info.n_tiles_per_chunk);
    let chunk_trans = vec2<f32>(
        f32(input.instance_id % tilemap_info.n_chunks.x),
        f32(input.instance_id / tilemap_info.n_chunks.x),
    ) * chunk_grid_size;
    let tile_trans = chunk_trans + tilemap_info.grid_size * vec2<f32>(tile_pos);

    // Compute the vertex position and run it through all the transforms
    let vertex_model = vec4(
        tile_vertex_model.x * tilemap_info.tile_size.x + tile_trans.x,
        tile_vertex_model.y * tilemap_info.tile_size.y + tile_trans.y,
        0.0, 1.0
    );
    let vertex_world =
        affine3_to_square(tilemap_info.transform_affine) * vertex_model;
    let vertex_clip = view.clip_from_world * vertex_world;

    output.position = vertex_clip;

    return output;
}

@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    let tile_color = vec4<f32>(
        f32((input.index_color[1] >>  0u) & 0xFFu) / 255.0f,
        f32((input.index_color[1] >>  8u) & 0xFFu) / 255.0f,
        f32((input.index_color[1] >> 16u) & 0xFFu) / 255.0f,
        f32((input.index_color[1] >> 24u) & 0xFFu) / 255.0f,
    );

#ifdef TILEMAP_HAS_TILE_TEXTURE
    let texture_color = textureSample(
        tile_texture, tile_sampler,
        input.uv, input.index_color[0],
    );
    let color = texture_color * tile_color;
#else
    let color = tile_color;
#endif

    if (color.a < 0.001) {
        discard;
    }
    return color;
}

fn affine3_to_square(affine: mat3x4<f32>) -> mat4x4<f32> {
    return transpose(mat4x4<f32>(
        affine[0],
        affine[1],
        affine[2],
        vec4<f32>(0.0, 0.0, 0.0, 1.0),
    ));
}
