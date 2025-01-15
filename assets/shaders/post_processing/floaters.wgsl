#define_import_path game::preprocessing::floaters

const FLOATER_SAMPLES_X: u32 = 64u;
const FLOATER_SAMPLES_Y: u32 = 64u;

struct Floater {
    scale: f32,
    opacity: f32,
    position: vec2<f32>,
}

struct FloaterBuffer {
    floaters: array<array<Floater, (FLOATER_SAMPLES_X * FLOATER_SAMPLES_Y)>>,
}

struct FloaterSettings {
    static_drift: vec2<f32>,
    spawn_spacing: vec2<f32>, // Directly relates to compute workgroup size
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
