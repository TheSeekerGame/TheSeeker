#import game::preprocessing::floaters

@group(1) @binding(0) var<storage, read_write> floater_buffer: FloaterBuffer;
@group(1) @binding(1) var<uniform> floater_settings: FloaterSettings;

@compute @workgroup_size(FLOATER_SAMPLES_X, FLOATER_SAMPLES_Y, 1)
fn floater_compute(@builtin(global_invocation_id) global_invocation_id: vec3<u32>,
                   @builtin(local_invocation_id) local_invocation_id: vec3<u32>) {
    let thread_index = global_invocation_id.x +
                       global_invocation_id.y * FLOATER_SAMPLES_X +
                       local_invocation_id.z * FLOATER_SAMPLES_X * FLOATER_SAMPLES_Y;

    var floater = Floater();
    floater.scale = 0.1;
    floater.opacity = 0.5;
    floater.position = floater_settings.static_drift + floater_settings.spawn_spacing * vec2<f32>(global_invocation_id.xy);
}
