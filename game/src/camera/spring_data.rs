#[derive(Resource, Default, Clone)]
pub struct CameraSpring {
    pub(super) fall_buffer: f32,
    pub(super) k: f32,
    pub(super) velocity: f32,
    pub(super) vertical_snapped: bool,
    pub(super) fall_factor: f32,
    
    pub (super) x_phase: SpringPhase,
    
    pub (super) y_phase: SpringPhase, 
  
    pub (super) follow_strategy: FollowStrategy, 
}

#[derive(Component)]
pub enum SpringPhase {
    #[default]
    Active,
    Snapped,
    Resetting,
}

#[derive(Component)]
struct DashCamTimer{
    remaining: f32, 
    just_dashed: bool,
}

// impl Default for DashCamTimer {
//     fn default() -> Self {
//         Self {
//             remaining: 1.0,
//             just_dashed: false,
//         }
//     }
// }