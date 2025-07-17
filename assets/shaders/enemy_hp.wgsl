#import bevy_ui::ui_vertex_output::UiVertexOutput;

@group(1) @binding(0) var<uniform> health: f32;
@group(1) @binding(1) var<uniform> damage: f32;
@group(1) @binding(2) var<uniform> spark: f32;

// --- Style constants -------------------------------------------------------
const INNER_COLOR   = vec4(0.0, 0.0, 0.0, 0.5);  // semi-transparent "empty" bar fill
const HEALTH_COLOR  = vec4(0.635, 0.196, 0.306, 1.0); // red current HP
const DAMAGE_COLOR  = vec4(1.0);                     // white damage trail
const BORDER_COLOR  = vec4(0.0);                     // black thin border

const SLOPE               = 0.05;   // horizontal shear factor
const BORDER_THICKNESS    = 0.06;  // black outline thickness (in UV units)
const SPARK_HALF_WIDTH    = 0.02;  // half-width of spark vertical line (UV units)

// ---------------------------------------------------------------------------

// Remap UV.y so the bar occupies only the center portion, leaving room for spark extensions
fn remap_y_for_bar(y: f32) -> f32 {
    // Map UV y from [0.25, 0.75] to [0, 1] for the bar
    return (y - 0.25) * 2.0;
}

// Returns left/right edges of the bar at a given vertical UV coordinate.
fn edges(y: f32) -> vec2<f32> {
    let left = SLOPE * y;         // shifts right as we go down (y increases)
    return vec2<f32>(left, left + (1.0 - SLOPE)); // constant bar width inside 0..1
}

fn inside_bar(uv: vec2<f32>) -> bool {
    // Bar only occupies center 50% vertically
    if uv.y < 0.25 || uv.y > 0.75 {
        return false;
    }
    let bar_y = remap_y_for_bar(uv.y);
    let e = edges(bar_y);
    return uv.x >= e.x && uv.x <= e.y;
}

fn border_mask(uv: vec2<f32>) -> f32 {
    // Remap y for bar calculations
    let bar_y = remap_y_for_bar(uv.y);
    let e = edges(bar_y);
    let in_outer = uv.x >= e.x && uv.x <= e.y && uv.y >= 0.25 && uv.y <= 0.75;
    // For inner, we need to consider the remapped coordinates
    let bar_border_y_min = 0.25 + BORDER_THICKNESS * 0.5;  // Scale border by 0.5 since bar is half height
    let bar_border_y_max = 0.75 - BORDER_THICKNESS * 0.5;
    let in_inner = uv.x >= e.x + BORDER_THICKNESS && uv.x <= e.y - BORDER_THICKNESS &&
                   uv.y >= bar_border_y_min && uv.y <= bar_border_y_max;
    return select(0.0, 1.0, in_outer && !in_inner);
}

@fragment
fn fragment(mesh: UiVertexOutput) -> @location(0) vec4<f32> {
    let uv = mesh.uv;

    // Compute edges using clamped y so we can still position spark even outside bar vertically
    let y_for_edges = clamp(remap_y_for_bar(clamp(uv.y, 0.25, 0.75)), 0.0, 1.0);
    let e = edges(y_for_edges);

    var color = vec4<f32>(0.0);
    var drawn = false;

    // Draw bar only when inside bar region
    if inside_bar(uv) {
        color = INNER_COLOR;
        drawn = true;

        // Only show damage/health inside inner area
        let inner_progress = (uv.x - e.x - BORDER_THICKNESS) / (e.y - e.x - 2.0 * BORDER_THICKNESS);

        if inner_progress >= 0.0 && inner_progress <= damage {
            color = DAMAGE_COLOR;
        }
        if inner_progress >= 0.0 && inner_progress <= health {
            color = HEALTH_COLOR;
        }

        // Border overlay
        if border_mask(uv) > 0.5 {
            color = BORDER_COLOR;
        }
    }

    // Spark overlay
    // Compute bar progress for spark using unclamped bar_y to maintain slope outside bar region
    let bar_y_spark = remap_y_for_bar(uv.y); // may be <0 or >1 for extended region
    let e_spark = edges(bar_y_spark);
    let spark_progress = (uv.x - e_spark.x) / (e_spark.y - e_spark.x);

    let spark_horiz = step(abs(spark_progress - health), SPARK_HALF_WIDTH);

    // Limit spark vertically while maintaining consistent slope
    // Spark extends 20% beyond the bar on each side
    let spark_vert = step(0.05, uv.y) * step(uv.y, 0.95);
    let spark_mask = spark_horiz * spark_vert * spark;

    if spark_mask > 0.0 {
        color = DAMAGE_COLOR;
        drawn = true;
    }

    if drawn {
        return to_linear(color);
    } else {
        return vec4<f32>(0.0);
    }
}

// sRGB to linear helper copied from Bevy's default UI shader
fn to_linear(nonlinear: vec4<f32>) -> vec4<f32> {
    let cutoff = step(nonlinear, vec4<f32>(0.04045));
    let higher = pow((nonlinear + vec4<f32>(0.055)) / vec4<f32>(1.055), vec4<f32>(2.4));
    let lower = nonlinear / vec4<f32>(12.92);
    return mix(higher, lower, cutoff);
}
