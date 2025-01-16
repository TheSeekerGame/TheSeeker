#import game::preprocessing::floaters::{Floater, FloaterBuffer, FloaterSettings, FLOATER_SAMPLES_X, FLOATER_SAMPLES_Y}

@group(0) @binding(1) var<uniform> floater_settings: FloaterSettings;
@group(0) @binding(2) var<storage, read_write> floater_buffer: FloaterBuffer;

@compute @workgroup_size(FLOATER_SAMPLES_X, FLOATER_SAMPLES_Y, 1)
fn floater_prepass(@builtin(global_invocation_id) global_invocation_id: vec3<u32>,
                   @builtin(local_invocation_id) local_invocation_id: vec3<u32>) {
    let thread_index = global_invocation_id.x +
                       global_invocation_id.y * FLOATER_SAMPLES_X;

    var floater = Floater();
    floater.scale = 0.1;
    floater.opacity = 0.5;
    floater.position = vec2<f32>(global_invocation_id.xy);

    floater_buffer.floaters[local_invocation_id.y][thread_index] = floater;
}
