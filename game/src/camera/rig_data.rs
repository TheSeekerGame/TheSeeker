#[derive(Resource)]
/// Tracks the target location of the camera, as well as internal state for interpolation.
pub struct CameraRig {
    /// The camera is moved towards this position smoothly.
    target: Vec2,
    /// the "base" position of the camera before screen shake is applied.
    camera_position: Vec2,
    /// The factor used in lerping to move the rig.
    //move_speed: f32,
    /// Keeps track if the camera is leading ahead, or behind the player.
    lead_direction: LeadDirection,
    /// Defines how far ahead the camera will lead the player by.
    lead_amount: f32,
    /// Defines how far away the player can get going in the unanticipated direction
    /// before the camera switches to track that direction.
    lead_buffer: f32,
    /// The rig's target minus the actual camera position
    displacement: Vec2,
    /// The rig's target minus the actual camera position
    equilibrium_y: f32,
}

enum LeadDirection {
    Backward,
    Forward,
}