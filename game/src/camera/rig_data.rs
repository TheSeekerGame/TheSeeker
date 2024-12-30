use theseeker_engine::prelude::Component;
use glam::Vec2;
use theseeker_engine::prelude::Resource;

pub const LEAD_BUFFER: f32 = 10.0;
pub const LEAD_AMOUNT: f32 = 20.0;

    
#[derive(Resource)]
/// Tracks the target location of the camera, as well as internal state for interpolation.
pub struct RigData {
    /// The camera is moved towards this position smoothly.
    pub target: Vec2,
    /// the "base" position of the camera before screen shake is applied.
    //pub camera_next_pos: Vec2,
    /// The factor used in lerping to move the rig.
    //move_speed: f32,
    /// Keeps track if the camera is leading ahead, or behind the player.
    
    /// Defines how far ahead the camera will lead the player by.
    
    /// The rig's target minus the actual camera position
    pub displacement: Vec2,
    /// The rig's target minus the actual camera position
    pub equilibrium_y: f32,
}

#[derive(Component, Default)]
pub struct Rig{
    pub lead_direction: LeadDirection,
    /// The next position to move the camera to
    pub next_position: Vec2,
}

#[derive(Default)]
pub enum LeadDirection {
    Backward,
    #[default]
    Forward,
}