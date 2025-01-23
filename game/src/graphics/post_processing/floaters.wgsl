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
}

fn gid_to_floater_grid_index(camera_pos: vec2<f32>, gid: vec3<u32>, spacing: vec2<f32>) -> vec2<i32> {
    let camera_grid_pos = vec2<i32>(floor(camera_pos / spacing));
    let relative_idx = vec2<i32>(
        i32(gid.x) - i32(FLOATER_SAMPLES_X / 2u),
        i32(gid.y) - i32(FLOATER_SAMPLES_Y / 2u)
    );
    return relative_idx + camera_grid_pos;
}

fn compute_floater(grid_idx: vec2<i32>, layer: u32, time: f32, settings: FloaterSettings) -> Floater {
    var floater = Floater();
    floater.scale = 2.0;

    let offset_hash = xxhash32_3d(vec3<u32>(bitcast<vec2<u32>>(grid_idx), layer));
    let offset = vec2<f32>(f32(offset_hash & 0xFFFFu), f32(offset_hash >> 16u)) / 65535.0 * settings.spawn_spacing;
    let root_pos = vec2<f32>(grid_idx) * settings.spawn_spacing;
    let drift_offset = settings.static_drift * time;
    let movement_offset = vec2<f32>(
        perlinNoise3(vec3<f32>(root_pos.x, f32(layer), time * 0.1)),
        perlinNoise3(vec3<f32>(root_pos.y, f32(layer), time * 0.1)),
    ) * settings.spawn_spacing * 0.5;

    floater.position = root_pos + offset + drift_offset + movement_offset;

    return floater;
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
