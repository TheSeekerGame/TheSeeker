#import bevy_sprite::mesh2d_vertex_output::VertexOutput

// A tether's protection band as shares of its radius: inside its outline, and past it; see
// `PROTECTION_BAND` in `foreground_fog.rs`.
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> protection_band: vec2<f32>;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let local_position = mesh.uv * 2.0 - vec2<f32>(1.0);
    let radial_distance = length(local_position);
    var angle = 0.0;
    if radial_distance > 0.0001 {
        angle = atan2(local_position.y, local_position.x);
    }
    let base_radius = mesh.color.r;
    let deformation = mesh.color.g;
    let jaggedness = mesh.color.b;
    let seed = mesh.color.a;

    let irregularity = (
        sin(angle * 7.0 + seed)
        + sin(angle * 17.0 - seed * 1.37) * 0.55
        + sin(angle * 29.0 + seed * 0.71) * 0.25
    ) / 1.8;
    let pulse_deformation = deformation * (
        sin(angle * 2.0 + seed * 1.13) * 0.62
        + sin(angle * 3.0 - seed * 0.73) * 0.38
    );
    let outline_radius = base_radius
        + irregularity * jaggedness
        + pulse_deformation;
    let detail_inset = base_radius * protection_band.x;
    let outer_falloff = base_radius * protection_band.y;
    let protection = 1.0 - clamp(
        (radial_distance - (outline_radius - detail_inset))
            / (detail_inset + outer_falloff),
        0.0,
        1.0
    );
    return vec4<f32>(protection, protection, protection, protection);
}
