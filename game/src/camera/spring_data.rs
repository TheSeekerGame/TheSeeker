use glam::Vec2;
use theseeker_engine::prelude::Resource;
use theseeker_engine::prelude::Component;

pub const CENTER_SCREEN: Vec2 = Vec2::new(1280./2., 720./2.);
pub const INITIAL_POSITION: Vec2 = Vec2::new(300.0, 605.6115);
/// "faster" restoring force for "tighter" spring follows for spring constant (k) 
pub const K_FAST: f32 = 7.106;
/// holds "normal" k-value so we can change current k back at any time
pub const K_REG: f32 = 3.553;
pub const FLOOR: f32 = 37.5;
pub const CEILING: f32 = 500.0;
pub const DAMPING_RATIO: f32 = 1.131;
/// point at which the spring will be reset
pub const RESET_THRESHOLD: f32 = 1.0;
/// distance from target when snapping will take effect
pub const SNAP_THRESHOLD: f32 = 0.25;
/// The horizontal "phase" of the spring.
pub const EQUALIZE_THRESHOLD: f32 = 25.0;
pub const FALL_BUFFER: f32 = 10.0;

#[derive(Resource, Default, Clone)]
pub struct SpringData {
    // TODO: fall buffer TO constant
    pub(super) fall_buffer: f32,
    pub(super) k: f32,
    pub(super) velocity: f32,
    pub(super) vertical_snapped: bool,
    pub(super) fall_factor: f32,
}

#[derive(Component, Default, Clone, Debug)]
pub enum FollowStrategy {
    InitFollow, 
    #[default]
    GroundFollow, 
    JumpFollow, 
    FallFollow,
    DashFollow,
}

#[derive(Component, Default, Debug)]
pub enum SpringPhaseX {
    #[default]
    Active,
    Snapped,
    Resetting,
}

#[derive(Component, Default, Debug)]
pub enum SpringPhaseY {
    #[default]
    Active,
    Snapped,
    Resetting,
}

#[derive(Component)]
pub struct DashTimer{
    pub remaining: f32, 
    pub just_dashed: bool,
}

#[derive(Component, Default)]
pub struct PlayerInfo {
    pub(super) grounded_y: f32, 
    pub(super) previous_grounded_y: f32, 
    pub(super) is_grounded: bool,
}

// impl Default for DashCamTimer {
//     fn default() -> Self {
//         Self {
//             remaining: 1.0,
//             just_dashed: false,
//         }
//     }
// }