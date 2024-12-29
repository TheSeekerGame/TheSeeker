//#![allow(warnings)]
use std::{cmp::Ordering, f32::consts::PI};
use bevy::prelude::*;
use strum_macros::Display;
use theseeker_engine::physics::{LinearVelocity, PhysicsWorld, ShapeCaster};
use crate::game::player::{self, CanDash, Dashing, Falling, Grounded, Player, PlayerConfig};
use super::{CameraRig, DashCamTimer, MainCamera};



#[derive(Component, Default, Display, Debug,)]
pub enum SpringPhase {
    #[default]
    Active,
    Snapped,
    Resetting,
}

/*#[derive(Default, Display, Debug, Clone)]
pub enum SpringPhase {
    #[default]
    Active,
    Snapped,
    Resetting,
}*/

impl SpringPhase {
    pub fn debug_print(&self) {
        println!("---------------------------------");
        println!("SpringPhase Debug:");
        println!("---------------------------------");
        println!("  Current Phase: {}", self);
    }

    pub fn update(spring: &mut ResMut<CameraSpring>, player_tracker: &Res<PlayerTracker>, displacement: f32, vertical: bool) -> SpringPhase {
        let mut phase = if vertical {&spring.y_phase} else {&spring.x_phase};
        if CameraSpring::is_in_active_range(&mut spring.clone(), displacement)  {
            phase = &SpringPhase::Active;
        } 
        if spring.is_in_reset_zone(displacement) && !spring.is_in_snap_zone(displacement) {
            phase = &SpringPhase::Resetting
        } else { }

        if spring.is_in_reset_zone(displacement) && spring.is_in_snap_zone(displacement) {
                phase = &SpringPhase::Snapped
            
        } 
        phase.clone()
    }

}

pub(super) fn update_spring_phases(
    mut query: Query<&mut SpringPhase>, 
) { 

}


#[derive(Default, Display, Clone)]
pub enum FollowStrategy {
    InitFollow, 
    #[default]
    GroundFollow, 
    JumpFollow, 
    FallFollow,
    DashFollow,
}

impl FollowStrategy {
    pub fn debug_print(&self) {
        println!("---------------------------------");
        println!("FollowStrategy Debug:");
        println!("---------------------------------");
        println!("  Current Phase: {}", self);
        println!("---------------------------------");
    }

    pub fn update(mut spring: &mut CameraSpring, player_tracker: &Res<PlayerTracker>) -> FollowStrategy {
        if (player_tracker.ground_distance <= FLOOR) 
        && (player_tracker.grounded_y - player_tracker.previous_grounded_y).abs() <=
        (player_tracker.previous_grounded_y + spring.fall_buffer) {
            return FollowStrategy::GroundFollow;
        } else if (player_tracker.position.y - player_tracker.grounded_y) > FLOOR {
            return FollowStrategy::JumpFollow;
        }
        else if (player_tracker.ground_distance > (FLOOR + spring.fall_buffer))
         && (player_tracker.position.y < (player_tracker.grounded_y + spring.fall_buffer)){
            return FollowStrategy::FallFollow;
        }
         else {
        /// TODO: add Init logic here and return FollowStrategy::Init
            return spring.follow_strategy.clone();
        }
       
    }

    
}

#[derive(Resource, Default, Clone)]
/// Data structure to configure simplified damped spring for camera movement
pub struct CameraSpring {
    /// The vertical point at which the spring action kicks in
    /// Never let the rig get this far away without following
    /// 
    /// Additional vertical space from ground required to consider the player "falling"
    pub(super) fall_buffer: f32,
    /// Used to override any limits placed on the spring 
    // TODO: Likely unnecessary for logic in final form, take out if possible
    //pub(super) limit_override: bool,
    /// spring constant (k) is the constant used to calculate "restoring force"
    pub(super) k: f32,
    /// Damping ratio (damping coefficient or oscillation decay). Controls how quickly the system settles to equilibrium  
    
    /// Current velocity of the spring
    pub(super) velocity: f32,
    /// determine if spring is reset or not
    /// is the spring snapped to the vertical target?
    pub(super) vertical_snapped: bool,
    
    
    /// Used when calculating the fall
    pub(super) fall_factor: f32,
    /// The horizontal "phase" of the spring.
    pub (super) x_phase: SpringPhase,
    /// The vertical "phase" of the spring.
    pub (super) y_phase: SpringPhase, 
    /// The current "strategy" used by the spring to follow the player.
    pub (super) follow_strategy: FollowStrategy, 
}

impl CameraSpring {
    pub fn debug_print(&self) {
        println!("---------------------------------");
        println!("CameraSpring Debug:");
        println!("---------------------------------");
        println!("  Floor: {}", FLOOR);
        println!("  Ceiling: {}", CEILING);
        println!("  Fall Buffer: {}", self.fall_buffer);
        //println!("  Limit Override: {}", self.limit_override);
        println!("  Spring Constant (k): {}", self.k);
        println!("  Fast Spring Constant (k_fast): {}", K_FAST);
        println!("  Regular Spring Constant (k_reg): {}", K_REG);
        println!("  Damping Coefficient: {}", DAMPING_RATIO);
        println!("  Velocity: {}", self.velocity);
        // println!("  Vertical Reset: {}", self.vertical_reset);
        // println!("  Horizontal Snapped: {}", self.horizontal_snapped);
        println!("  Vertical Snapped: {}", self.vertical_snapped);
        println!("  Reset Threshold: {}", RESET_THRESHOLD);
        println!("  Snap Threshold: {}", SNAP_THRESHOLD);
        println!("  Equalize Threshold: {}", EQUALIZE_THRESHOLD);
        println!("  Displacement Factor: {}", self.fall_factor);
        println!("  Horizontal Phase: {}", self.x_phase);
        println!("  Vertical Phase: {}", self.y_phase);
        println!("  Follow Strategy: {}", self.follow_strategy);
        println!("---------------------------------");
    }
    pub fn default() -> Self {
        CameraSpring {
            
            fall_buffer: 10.0,
            //limit_override: false,
            
            k: 3.553,
            
            velocity: 0.0,
            vertical_snapped: false,
            
            // used for increasing camera speed when calculating fall
            fall_factor: 5.0, // 0.0 prev
            x_phase: default(), 
            y_phase: default(),
            follow_strategy: default(),

        }
    }

    pub fn follow (&mut self,
        rig: &mut ResMut<CameraRig>,
        player_tracker: &Res<PlayerTracker>,
        delta_seconds: f32,
     ) -> () {
        match self.follow_strategy {
            FollowStrategy::InitFollow => {
                rig.camera_position.x = self.calculate(&self, rig, delta_seconds, false);
                rig.camera_position.y = self.calculate(&self, rig, delta_seconds, true);
            }
            FollowStrategy::GroundFollow => {
                self.k = if player_tracker.just_dashed {K_FAST} else {K_REG};
                // vertical spring phases
                rig.camera_position.y = 
                match self.y_phase {
                    SpringPhase::Active => {
                        //rig.target.y = rig.equilibrium_y;
                        if player_tracker.previous_grounded_y < (player_tracker.grounded_y + self.fall_buffer)
                        && (rig.target.y - rig.equilibrium_y).abs() < FLOOR {
                            self.equalize_y(&self, rig, delta_seconds)
                        } else {
                            self.calculate(&self, rig, delta_seconds, true)
                        }
                        
                    }
                    SpringPhase::Resetting => {
                        //rig.target.y = rig.equilibrium_y;
                        if player_tracker.previous_grounded_y < (player_tracker.grounded_y + self.fall_buffer) {
                            self.equalize_y(&self, rig, delta_seconds)
                        } else {
                            self.calculate(&self, rig, delta_seconds, true)
                        }
                    }
                    SpringPhase::Snapped => {
                        //rig.target.y = rig.equilibrium_y;
                        if player_tracker.previous_grounded_y < (player_tracker.grounded_y + self.fall_buffer) {
                            self.equalize_y(&self, rig, delta_seconds)
                        } else {
                            self.calculate(&self, rig, delta_seconds, true)
                        }
                    }
                    
                };
                // horizontal spring phases
                rig.camera_position.x = 
                match self.x_phase {
                    SpringPhase::Active => {
                        self.calculate(&self, rig, delta_seconds, false)
                    }
                    SpringPhase::Resetting => { self.calculate(&self, rig, delta_seconds, false) }
                    SpringPhase::Snapped => {
                        self.x_phase = SpringPhase::Snapped;
                        self.vertical_snapped = true;
                        rig.target.x
                            
                    }
                };
                /* 
                if !vertical {
                    self.calculate(&self, rig, delta_seconds, false)
                } else {
                        if !matches!(self.y_phase, SpringPhase::Snapping)   {
                            let displacement = rig.target.y - rig.camera_position.y;
                            self.reset(displacement, self, rig, delta_seconds, vertical)
                        } else {
                                rig.camera_position.y
                        } 
                }*/
            }
            FollowStrategy::JumpFollow => {
                self.k = if player_tracker.just_dashed {K_FAST} else {K_REG};
                self.y_phase = SpringPhase::Resetting; 
                rig.camera_position.x = self.calculate(self, rig, delta_seconds, false);
                rig.camera_position.y = self.calculate(self, rig, delta_seconds, true);
            }
            FollowStrategy::FallFollow => {
                self.k = K_REG;
                rig.camera_position.y = self.calculate_fall(self, rig, delta_seconds, true);
                let displacement = rig.target.x - (rig.camera_position.x);
                rig.camera_position.x = self.reset(displacement, rig,  delta_seconds, false);
                
                
            }
            FollowStrategy::DashFollow => {
                self.k = if player_tracker.just_dashed {K_FAST} else {K_REG};
                rig.camera_position.x = self.calculate(self, rig, delta_seconds, false);
                rig.camera_position.y = self.calculate(self, rig, delta_seconds, true);
            }
            _ => {
                self.k = if player_tracker.just_dashed {K_FAST} else {K_REG};
                rig.camera_position.x = self.calculate(self, rig, delta_seconds, false);
                rig.camera_position.y = self.calculate(self, rig, delta_seconds, true);
            
            }

        }

    }

    fn calculate(
        &self,
        spring: &CameraSpring, 
        rig: &CameraRig,
        delta_seconds: f32,
        vertical: bool, 
    ) -> f32 {
        let displacement = if vertical { rig.displacement.y } else { rig.displacement.x };
        let position = if vertical { rig.camera_position.y } else { rig.camera_position.x };
        let velocity = if vertical { spring.velocity.abs() } else { 0.0 };
        let spring_force = spring.k * displacement;
        let damping_force = DAMPING_RATIO * spring.velocity;
        let camera_acceleration = spring_force - damping_force;
        position + camera_acceleration * delta_seconds
    }

    fn calculate_fall(
        &self,
        spring: &CameraSpring, 
        rig: &CameraRig,
        delta_seconds: f32,
        vertical: bool, 
    ) -> f32 {
        let displacement = if vertical { rig.displacement.y - spring.fall_factor } else { rig.displacement.x };
        let position = if vertical { rig.camera_position.y } else { rig.camera_position.x };
        let velocity = if vertical { spring.velocity.abs() } else { 0.0 };
        let spring_force = spring.k * displacement;
        let damping_force = DAMPING_RATIO * spring.velocity;
        let camera_acceleration = spring_force - damping_force;
        position + camera_acceleration * delta_seconds
    }

    fn equalize_y(
        &self,
        spring: &CameraSpring, 
        rig: &CameraRig,
        delta_seconds: f32,
    ) -> f32 {
        let displacement = rig.equilibrium_y - rig.camera_position.y;
        let position = rig.camera_position.y;
        let velocity = spring.velocity.abs();
        let spring_force = spring.k * displacement;
        let damping_force = DAMPING_RATIO * spring.velocity;
        let camera_acceleration = spring_force - damping_force;
        position + camera_acceleration * delta_seconds
    }

    fn reset(
        &self,
        displacement: f32, 
        rig: &CameraRig,
        delta_seconds: f32,
        vertical: bool,
    ) -> f32 {
        let position = if vertical { rig.camera_position.y } else { rig.camera_position.x };
        let velocity = if vertical { self.velocity.abs() } else { 0.0 };
        let spring_force = self.k * displacement;
        let damping_force = DAMPING_RATIO * self.velocity;
        let camera_acceleration = spring_force - damping_force;
        position + camera_acceleration * delta_seconds
    }

    
    // should ONLY be in the active range, no other range
    pub fn is_in_active_range(&mut self, value: f32) -> bool{
        let full_range = value.abs() > FLOOR && value.abs() < CEILING;
        let snap_zone = !self.is_in_snap_zone(value);
        let reset_zone = !self.is_in_reset_zone(value);
        full_range && snap_zone && reset_zone
    }

    pub fn is_in_snap_zone(&self, value: f32) -> bool {
        value.abs() < SNAP_THRESHOLD || value.abs() > CEILING
    }

    pub fn is_in_reset_zone(&self, value: f32) -> bool {
        value.abs() < RESET_THRESHOLD || value.abs() > CEILING
    }


    
}

pub(super) fn track_player(
    grounded_query: Query<(Entity, &Transform, &mut ShapeCaster), (Added<Grounded>, With<Player>)>,
    mut player_tracker: ResMut<PlayerTracker>,
    player_query: Query<Entity, With<Player>>,
    transform_query: Query<&Transform, With<Player>>,
    mut removed_grounded: RemovedComponents<Grounded>,
    dashing_added: Query<Entity, (With<Player>, Added<Dashing>)>,
    mut rig: ResMut<CameraRig>,
) {
    for transform in transform_query.iter() {
        player_tracker.position = transform.translation.xy();
    }
    for (_, t, _) in grounded_query.iter() {
        player_tracker.previous_grounded_y = player_tracker.grounded_y;
        rig.equilibrium_y = rig.target.y;
        player_tracker.grounded_y = t.translation.y;
        player_tracker.is_grounded = true;
    }

    for entity in removed_grounded.read() {
        if let Ok(_player) = player_query.get(entity) {
            player_tracker.is_grounded = false;
        }
    }

    if let Ok(_player) = dashing_added.get_single() {
        player_tracker.just_dashed = false;
    }
}

pub(super) fn update_fall_factor(
    mut spring: ResMut<CameraSpring>,
    spatial_query: Res<PhysicsWorld>,
    mut query: Query<(Entity, &mut ShapeCaster, &Transform), With<Player>>,
) {
    
    for (entity, ray_cast_info, position) in query.iter_mut() {
        let ground_distance = ray_cast_info
            .cast(&spatial_query, &position, Some(entity))
            .into_iter().filter_map(|hit| {
                // Extract the time of impact (toi) directly as the ground distance
                Some(hit.1.toi)
            })
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal))
            .unwrap_or_else(|| {
                eprintln!("Warning: No ground distance found for entity {:?}", entity);
                f32::INFINITY 
            });
            let steepness = 4.0;
            let normalized_distance = ground_distance / 100.0; // Scale to a range of 0 to 1
            let divisor = 12.0 * (1.0 - (-normalized_distance * steepness).exp());
            spring.fall_factor =  ground_distance / divisor;

    }
    // spring.fall_factor = (player_tracker.ground_distance / 3.0).powf(1.5);
    
    
}

pub(super) fn update_dash_timer(
    mut query: Query<&mut DashCamTimer, With<MainCamera>>,
    time: Res<Time>,
    mut player_tracker: ResMut<PlayerTracker>,
    dashing_added: Query<Entity, (With<Player>, Added<Dashing>)>,
) { 
    
    
    if let Ok(mut timer) = query.get_single_mut() {
        // Use the timer as needed
        if let Ok(_) = dashing_added.get_single() {
            timer.just_dashed = false;
        } else { println!("Not hitting")}
        
        if timer.remaining > 0.0 && timer.just_dashed == true {
            timer.remaining -= time.delta_seconds();
        } else if timer.remaining <= 0.0 {
            timer.remaining = 1.0;
        }
        } else {
            warn!("Expected exactly one Dash Cam Timer component, but found none or multiple.");
        }
    
}

pub(super) fn track_player_dashed(
    mut dashing_removed: RemovedComponents<Dashing>,
    can_dash_query: Query<&Transform, (With<Player>, With<CanDash>)>,
    mut player_tracker: ResMut<super::PlayerTracker>,
    mut dash_cam_timer: Query<&mut DashCamTimer, With<MainCamera>>,
) {
    if let Ok (mut timer) = dash_cam_timer.get_single_mut() {
        for entity in dashing_removed.read() {
            if let Ok(_player) = can_dash_query.get(entity) {
                player_tracker.just_dashed = true;
                timer.just_dashed = true;
            }
        }
        dbg!(timer.just_dashed);
    } else {};
    
}

pub(super) fn track_player_ground_distance(
    spatial_query: Res<PhysicsWorld>,
    mut query: Query<(Entity, &mut ShapeCaster, &Transform), With<Player>>,
    mut player_tracker: ResMut<super::PlayerTracker>,
) {
    for (entity, ray_cast_info, position) in query.iter_mut() {
        player_tracker.ground_distance = ray_cast_info
            .cast(&spatial_query, &position, Some(entity))
            .into_iter().filter_map(|hit| {
                // Extract the time of impact (toi) directly as the ground distance
                Some(hit.1.toi)
            })
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal))
            .unwrap_or_else(|| {
                eprintln!("Warning: No ground distance found for entity {:?}", entity);
                f32::INFINITY 
            });
    }
}

pub(super) fn track_player_velocity(
    velocity_query: Query<&LinearVelocity, With<Player>>,
    mut player_tracker: ResMut<super::PlayerTracker>,
) {
    if let Ok(player_velocity) = velocity_query.get_single() {
        player_tracker.velocity = Vec2::new(player_velocity.x, player_velocity.y);
    }
}

/*pub(super) fn snap_after_dash(
    player_query: Query<&Transform, (With<Player>, With<CanDash>)>,
    mut removed: RemovedComponents<Dashing>,
    mut spring: ResMut<CameraSpring>,

) {
    
    for entity in removed.read() {
        if let Ok(_player) = player_query.get(entity) {
            
        }
    }
}*/

#[derive(Resource, Default, Clone)]
pub(super) struct PlayerTracker {
    pub(super) previous_grounded_y: f32, 
    pub(super) grounded_y: f32, 
    pub(super) ground_distance: f32,
    pub(super) is_grounded: bool,
    pub(super) velocity: Vec2, 
    pub(super) position: Vec2,
    pub(super) just_dashed: bool,
    //pub(super) after_dash_timer: f32,
}

impl PlayerTracker {
    pub fn debug_print(&self) {
        println!("PlayerTracker Debug:");
        println!("  Grounded Y: {}", self.grounded_y);
        println!("  Previous Grounded Y: {}", self.previous_grounded_y);
        println!("  Ground Distance: {}", self.ground_distance);
        println!("  Ground?: {}", self.is_grounded);
        println!("  Velocity: {}", self.velocity);
        println!("  Position: {}", self.position);
        println!("  Just Dashed: {}", self.just_dashed);
        //println!("  Dash Timer: {}", self.after_dash_timer);
    }
}

pub(super) fn debug_update(
    spring: Res<CameraSpring>,
    rig: Res<CameraRig>,
    player_tracker: Res<PlayerTracker>,
    camera_query: Query<&Transform, With<Camera>>,
) {
    let Ok((mut camera)) = camera_query.get_single() else {return;};

    print!("\x1B[2J\x1B[1;1H");
    rig.debug_print();
    player_tracker.debug_print();
    spring.debug_print();
    spring.follow_strategy.debug_print();
    if let Err(e) = std::panic::catch_unwind(|| {
        println!("CAMERA_X: {:.2}, CAMERA_Y: {:.2}", camera.translation.x, camera.translation.y);
    }) {
        println!("Oops, something went wrong while updating the camera position. Error: {:?}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::*;

    #[test]
    fn test_is_in_active_range() {
        let mut spring = CameraSpring {
            // SNAP_THRESHOLD: 1.0,
            // RESET_THRESHOLD: 1.5,
            ..Default::default()
        };

        // Case 1: Within active range (above floor, below ceiling, not in snap or reset zones)
        assert!(spring.is_in_active_range(5.0));

        // Case 2: Below floor (outside active range)
        assert!(!spring.is_in_active_range(1.5));

        // Case 3: Above ceiling (outside active range)
        assert!(!spring.is_in_active_range(11.0));

        // Case 4: Within snap zone (below snap threshold)
        assert!(!spring.is_in_active_range(0.5));

        // Case 5: Within reset zone (below reset threshold)
        assert!(!spring.is_in_active_range(1.4));
    }
    #[test]
    fn test_calculate_spring_vertical() {
        return
    }

    #[test]
    fn test_calculate_spring_horizontal() {
        return 
    }
}
