#define_import_path game::preprocessing::floaters

#import noise::perlin3::perlinNoise3

// These constants are defined and filled in by the plugin
const FLOATER_SAMPLES_X: u32 = {{FLOATER_SAMPLES_X}}u;
const FLOATER_SAMPLES_Y: u32 = {{FLOATER_SAMPLES_Y}}u;

struct Floater {
    scale: f32,
    position: vec2<f32>,
}

struct FloaterBuffer {
    floaters: array<array<Floater, (FLOATER_SAMPLES_X * FLOATER_SAMPLES_Y)>>,
}

struct FloaterSettings {
    static_drift: vec2<f32>,
    spawn_spacing: vec2<f32>,
    particle_size: vec2<f32>,
    particle_size_variance_speed: f32,
    movement_speed: f32,
    movement_strength: f32,
    sprite_width: u32,
    spritesheet_width: u32,
    sprite_index: u32,
}

// Gets the spacing-based grid index of a floater
fn gid_to_floater_grid_index(camera_pos: vec2<f32>, gid: vec3<u32>, spacing: vec2<f32>) -> vec2<i32> {
    let scaled_spacing = spacing / (1 - get_layer_distance(gid.z));
    let camera_grid_pos = vec2<i32>(floor(camera_pos / scaled_spacing));
    let relative_idx = vec2<i32>(
        i32(gid.x) - i32(FLOATER_SAMPLES_X / 2u),
        i32(gid.y) - i32(FLOATER_SAMPLES_Y / 2u)
    );
    return relative_idx + camera_grid_pos;
}

// Computes the floater data
fn compute_floater(grid_idx: vec2<i32>, layer: u32, time: f32, settings: FloaterSettings) -> Floater {
    var floater = Floater();
    let scale = 1 / (1 - get_layer_distance(layer));

    // Simulate perspective scaling
    let scaled_spacing = settings.spawn_spacing * scale;

    // Base offset is obtained by hashing the floater grid coords, deconstructing it into
    // two normalized float values for xy and scaling it by the spacing to fit the grid
    let offset_hash = xxhash32_3d(vec3<u32>(bitcast<vec2<u32>>(grid_idx), layer));
    let offset = unpack2x16unorm(offset_hash) * scaled_spacing;
    let root_pos = vec2<f32>(grid_idx) * scaled_spacing;

    let drift_offset = settings.static_drift * time;

    // Random floater movement is just a noise function
    let movement_offset = vec2<f32>(
        perlinNoise3(vec3<f32>(root_pos.x, f32(layer), time * settings.movement_speed)),
        perlinNoise3(vec3<f32>(root_pos.y, f32(layer), time * settings.movement_speed)),
    ) * scaled_spacing * settings.movement_strength;

    floater.position = root_pos + offset + drift_offset + movement_offset;

    floater.scale = mix(settings.particle_size.x, settings.particle_size.y, perlinNoise3(vec3<f32>(
        time * settings.particle_size_variance_speed,
        root_pos.x + f32(layer * FLOATER_SAMPLES_Y),
        root_pos.y
    ))) * scale;

    return floater;
}

fn get_layer_distance(layer: u32) -> f32 {
    var layer_distances: array<f32, 6> = array<f32, 6>(
        0.1, -0.1, -0.2, -0.3, -0.4, -0.5
    );

    return layer_distances[layer];
}

fn apply_parallax(pos: vec2<f32>, cam_pos: vec2<f32>, layer_scale: f32) -> vec2<f32> {
    return cam_pos + (pos - cam_pos) * layer_scale;
}

// Fast 3d hash:
// https://github.com/Cyan4973/xxHash
// https://www.shadertoy.com/view/Xt3cDn
fn xxhash32_3d(p: vec3<u32>) -> u32 {
    let p2 = 2246822519u;
    let p3 = 3266489917u;
    let p4 = 668265263u;
    let p5 = 374761393u;
    var h32 =  p.z + p5 + p.x*p3;
    h32 = p4 * ((h32 << 17) | (h32 >> (32 - 17)));
    h32 += p.y * p3;
    h32 = p4 * ((h32 << 17) | (h32 >> (32 - 17)));
    h32 = p2 * (h32 ^ (h32 >> 15));
    h32 = p3 * (h32 ^ (h32 >> 13));
    return h32 ^ (h32 >> 16);
}
