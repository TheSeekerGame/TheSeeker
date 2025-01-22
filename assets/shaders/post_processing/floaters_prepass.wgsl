#import bevy_render::globals::Globals;
#import bevy_render::view::View;
#import game::preprocessing::floaters::{Floater, FloaterBuffer, FloaterSettings, FLOATER_SAMPLES_X, FLOATER_SAMPLES_Y, gid_to_floater_grid_index}

@group(0) @binding(0) var<uniform> view: View;
@group(0) @binding(1) var<uniform> globals: Globals;
@group(0) @binding(2) var<uniform> floater_settings: FloaterSettings;
@group(0) @binding(3) var<storage, read_write> floater_buffer: FloaterBuffer;

@compute @workgroup_size(FLOATER_SAMPLES_X, FLOATER_SAMPLES_Y, 1)
fn floater_prepass(@builtin(global_invocation_id) global_invocation_id: vec3<u32>,
                   @builtin(local_invocation_id) local_invocation_id: vec3<u32>) {
    let thread_index = global_invocation_id.x +
                       global_invocation_id.y * FLOATER_SAMPLES_X;

    let floater_idx = gid_to_floater_grid_index(
        view.world_position.xy,
        global_invocation_id,
        floater_settings.spawn_spacing
    );

    var floater = Floater();
    floater.scale = 2.0;
    floater.opacity = 0.5;
    floater.position = vec2<f32>(vec2<f32>(floater_idx) * floater_settings.spawn_spacing);

    floater_buffer.floaters[local_invocation_id.z][thread_index] = floater;
}
