#import bevy_render::globals::Globals;
#import bevy_render::view::View;
#import game::preprocessing::floaters::{Floater, FloaterBuffer, FloaterSettings, FLOATER_SAMPLES_X, FLOATER_SAMPLES_Y, gid_to_floater_grid_index, compute_floater}

@group(0) @binding(0) var<uniform> view: View;
@group(0) @binding(1) var<uniform> globals: Globals;
@group(0) @binding(2) var<uniform> floater_settings: FloaterSettings;
@group(0) @binding(3) var<storage, read_write> floater_buffer: FloaterBuffer;

@compute @workgroup_size(FLOATER_SAMPLES_X, FLOATER_SAMPLES_Y, 1)
fn floater_prepass(@builtin(global_invocation_id) global_invocation_id: vec3<u32>) {
    let thread_index = global_invocation_id.x +
                       global_invocation_id.y * FLOATER_SAMPLES_X;

    let drift_offset = floater_settings.static_drift * globals.time;

    let floater_idx = gid_to_floater_grid_index(
        view.world_position.xy - drift_offset,
        global_invocation_id,
        floater_settings.spawn_spacing
    );

    floater_buffer.floaters[global_invocation_id.z][thread_index] = compute_floater(
        floater_idx,
        global_invocation_id.z,
        globals.time,
        floater_settings
    );
}
