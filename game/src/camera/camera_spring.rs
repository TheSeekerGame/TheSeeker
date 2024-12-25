#![allow(warnings)]
use std::{cmp::Ordering, f32::consts::PI};

use bevy::prelude::*;
use strum_macros::Display;
use theseeker_engine::physics::{LinearVelocity, PhysicsWorld, ShapeCaster};

use crate::game::player::{CanDash, Dashing, Falling, Grounded, Player};

use super::CameraRig;

// TODO: Should depend on bevy Window Resolution
const CENTER_SCREEN: Vec2 = Vec2::new(1280./2., 720./2.);
const INITIAL_POSITION: Vec2 = Vec2::new(300.0, 605.6115);

#[derive(Default, Display, Debug)]
pub enum SpringPhase {
    #[default]
    Active,
    Snapped,
    Resetting,
    Snapping,
    Reset,
}

impl SpringPhase {
    pub fn debug_print(&self) {
        println!("---------------------------------");
        println!("SpringPhase Debug:");
        println!("---------------------------------");
        println!("  Current Phase: {}", self);
    }
}

#[derive(Default, Display)]
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

    pub fn update(mut spring: &mut Spring, player_tracker: &Res<PlayerTracker>) -> FollowStrategy {
        if player_tracker.ground_distance <= spring.floor {
            return FollowStrategy::GroundFollow;
        }
        if player_tracker.ground_distance > spring.floor && player_tracker.ground_distance < (spring.floor + spring.fall_buffer) {
            return FollowStrategy::JumpFollow;
        }
        if player_tracker.ground_distance > (spring.floor + spring.fall_buffer) {
            return FollowStrategy::FallFollow;
        }
        if player_tracker.just_dashed {
            return FollowStrategy::DashFollow;
        }
        /// TODO: add Init logic here and return FollowStrategy::Init
        return FollowStrategy::default();
       
    }

    
}

#[derive(Resource, Default)]
/// Data structure to configure simplified damped spring for camera movement
pub struct Spring {
    /// The vertical point at which the spring action kicks in
    pub(super) floor: f32,
    /// Never let the rig get this far away without following
    pub(super) ceiling: f32,
    /// Additional vertical space from ground required to consider the player "falling"
    pub(super) fall_buffer: f32,
    /// Used to override any limits placed on the spring 
    // TODO: Likely unnecessary for logic in final form, take out if possible
    pub(super) limit_override: bool,
    /// spring constant (k) is the constant used to calculate "restoring force"
    pub(super) k: f32,
    /// "faster" restoring force for "tighter" spring follows for spring constant (k) 
    pub(super) k_fast: f32,
    /// holds "normal" k-value so we can change current k back at any time
    pub(super) k_reg: f32,
    /// Damping ratio (damping coefficient or oscillation decay). Controls how quickly the system settles to equilibrium  
    pub(super) damping_ratio: f32,
    /// Current velocity of the spring
    pub(super) velocity: f32,
    /// determine if spring is reset or not
    pub(super) vertical_reset: bool,
    /// is the spring snapped to the horizontal target?
    pub(super) horizontal_snapped: bool,
    /// is the spring snapped to the vertical target?
    pub(super) vertical_snapped: bool,
    /// point at which the spring will be reset
    pub(super) reset_threshold: f32,
    /// distance from target when snapping will take effect
    pub(super) snap_threshold: f32,
    /// The horizontal "phase" of the spring.
    pub (super) x_phase: SpringPhase,
    /// The vertical "phase" of the spring.
    pub (super) y_phase: SpringPhase, 
    /// The current "strategy" used by the spring to follow the player.
    pub (super) follow_strategy: FollowStrategy, 
}

impl Spring {
    pub fn debug_print(&self) {
        println!("---------------------------------");
        println!("RigSpring Debug:");
        println!("---------------------------------");
        println!("  Floor: {}", self.floor);
        println!("  Ceiling: {}", self.ceiling);
        println!("  Fall Buffer: {}", self.fall_buffer);
        println!("  Limit Override: {}", self.limit_override);
        println!("  Spring Constant (k): {}", self.k);
        println!("  Fast Spring Constant (k_fast): {}", self.k_fast);
        println!("  Regular Spring Constant (k_reg): {}", self.k_reg);
        println!("  Damping Coefficient: {}", self.damping_ratio);
        println!("  Velocity: {}", self.velocity);
        println!("  Vertical Reset: {}", self.vertical_reset);
        println!("  Horizontal Snapped: {}", self.horizontal_snapped);
        println!("  Vertical Snapped: {}", self.vertical_snapped);
        println!("  Reset Threshold: {}", self.reset_threshold);
        println!("  Snap Threshold: {}", self.snap_threshold);
        println!("  Horizontal Phase: {}", self.x_phase);
        println!("  Vertical Phase: {}", self.y_phase);
        println!("  Follow Strategy: {}", self.follow_strategy);
        println!("---------------------------------");
    }
    pub fn default() -> Self {
        Spring {
            floor: 37.5,
            ceiling: 500.0,
            fall_buffer: 30.0,
            limit_override: false,
            k_reg: 3.553,
            k_fast: 7.106,
            k: 3.553,
            damping_ratio:  1.131,
            velocity: 0.0,
            vertical_reset: false,
            horizontal_snapped: false,
            vertical_snapped: false,
            reset_threshold: 1.0,
            snap_threshold: 0.25,
            x_phase: default(), 
            y_phase: default(),
            follow_strategy: default(),

        }
    }

    pub fn follow (&mut self,
        rig: &CameraRig,
        player_tracker: &PlayerTracker,
        delta_seconds: f32,
        vertical: bool,
     ) -> f32 {
        match self.follow_strategy {
            FollowStrategy::GroundFollow => {
                if !vertical {
                    self.calculate(&self, rig, delta_seconds, false)
                } else {
                        if !matches!(self.y_phase, SpringPhase::Snapping)   {
                            let displacement = rig.target.y - rig.camera_position.y;
                            self.reset(displacement, self, rig, delta_seconds, vertical)
                        } else {
                                rig.camera_position.y
                        } 
                }
            }
            FollowStrategy::JumpFollow => {
                self.calculate(self, rig, delta_seconds, vertical)
            }
            FollowStrategy::FallFollow => {
                self.k = self.k_fast;
                if !vertical {
                    self.calculate(self, rig, delta_seconds, false)
                } else {
                    
                        let displacement = rig.target.y - (rig.camera_position.y + 10.0);
                        self.reset(displacement, self, rig, delta_seconds, vertical)
                }
            }
            FollowStrategy::DashFollow => {
                self.k = self.k_fast;
                self.calculate(self, rig, delta_seconds, vertical)
            }
            _ => {
                self.k = self.k_reg;
                self.calculate(self, rig, delta_seconds, vertical)
            
            }

        }

    }

    fn calculate(
        &self,
        spring: &Spring, 
        rig: &CameraRig,
        delta_seconds: f32,
        vertical: bool,
    ) -> f32 {
        let displacement = if vertical { rig.displacement.y } else { rig.displacement.x };
        let position = if vertical { rig.camera_position.y } else { rig.camera_position.x };
        let velocity = if vertical { spring.velocity.abs() } else { 0.0 };
        let spring_force = spring.k * displacement;
        let damping_force = spring.damping_ratio * spring.velocity;
        let camera_acceleration = spring_force - damping_force;
        position + camera_acceleration * delta_seconds
    }

    fn reset(
        &self,
        displacement: f32, 
        spring: &Spring, 
        rig: &CameraRig,
        delta_seconds: f32,
        vertical: bool,
    ) -> f32 {
        let position = if vertical { rig.camera_position.y } else { rig.camera_position.x };
        let velocity = if vertical { spring.velocity.abs() } else { 0.0 };
        let spring_force = spring.k * displacement;
        let damping_force = spring.damping_ratio * spring.velocity;
        let camera_acceleration = spring_force - damping_force;
        position + camera_acceleration * delta_seconds
    }

    
    // should ONLY be in the active range, no other range
    pub fn is_in_active_range(&mut self, value: f32) -> bool{
        let full_range = value.abs() > self.floor && value.abs() < self.ceiling;
        let snap_zone = !self.is_in_snap_zone(value);
        let reset_zone = !self.is_in_reset_zone(value);
        full_range && snap_zone && reset_zone
    }

    pub fn is_in_snap_zone(&self, value: f32) -> bool {
        value.abs() < self.snap_threshold || value.abs() > self.ceiling
    }

    pub fn is_in_reset_zone(&self, value: f32) -> bool {
        value.abs() < self.reset_threshold || value.abs() > self.ceiling
    }

    pub fn snap_vertical(&mut self, rig: &mut ResMut<CameraRig>, player: &Vec3) {
        rig.target.y = player.y;
        rig.camera_position.y = rig.target.y;
        self.y_phase = SpringPhase::Snapped;
    }

    pub fn snap_horizontal(&mut self, rig: &mut ResMut<CameraRig>, player: &Vec3, fast: bool) -> bool {
        rig.camera_position.x = rig.target.x;
        self.k = if fast {self.k_fast} else {self.k_reg};
        
        self.x_phase = SpringPhase::Snapped;
        
        true
    }

    pub fn update_vertical_phase(&mut self, displacement: f32) {
        if self.is_in_active_range(displacement)  {
            self.y_phase = SpringPhase::Active;
        } 
        
        if self.is_in_reset_zone(displacement) && !self.is_in_snap_zone(displacement) {
            self.y_phase = SpringPhase::Resetting;
        } else

        if self.is_in_reset_zone(displacement) && self.is_in_snap_zone(displacement) {
            self.y_phase = SpringPhase::Snapping;
        }
        if self.vertical_snapped {
            self.y_phase = SpringPhase::Snapped;
        }
    }

    pub fn update_horizontal_phase(&mut self, displacement: f32) {
        
        if self.is_in_active_range(displacement)  {
            self.x_phase = SpringPhase::Active;
        } 
        
        if self.is_in_reset_zone(displacement) && !self.is_in_snap_zone(displacement) {
            self.x_phase = SpringPhase::Resetting;
        } else

        if self.is_in_reset_zone(displacement) && self.is_in_snap_zone(displacement) {
            self.x_phase = SpringPhase::Snapping;
        }
        if self.horizontal_snapped {
            self.x_phase = SpringPhase::Snapped;
        }
    }

    
}

pub(super) fn track_player(
    grounded_query: Query<(Entity, &Transform, &mut ShapeCaster), (Added<Grounded>, With<Player>)>,
    can_dash_query: Query<&Transform, (With<Player>, With<CanDash>)>,
    mut dashing_removed: RemovedComponents<Dashing>,
    mut player_tracker: ResMut<PlayerTracker>,
    player_query: Query<Entity, With<Player>>,
    transform_query: Query<&Transform, With<Player>>,
    mut removed_grounded: RemovedComponents<Grounded>,
    mut dashing_added: Query<Entity, (With<Player>, Added<Dashing>)>,
) {
    for transform in transform_query.iter() {
        player_tracker.position = transform.translation.xy();
    }
    for (_, t, _) in grounded_query.iter() {
        player_tracker.last_grounded_y = t.translation.y;
        player_tracker.is_grounded = true;
    }

    for entity in removed_grounded.read() {
        if let Ok(_player) = player_query.get(entity) {
            player_tracker.is_grounded = false;
        }
    }

    for entity in dashing_removed.read() {
        if let Ok(_player) = can_dash_query.get(entity) {
            //spring.k = spring.k_fast * 2.0;
            player_tracker.just_dashed = true;
        }
    }

    // player_tracker.debug_print();
}


pub(super) fn track_player_dashed(
    dashing_added: Query<Entity, (With<Player>, Added<Dashing>)>,
    player_query: Query<Entity, With<Player>>,
    mut player_tracker: ResMut<super::PlayerTracker>,
) {
    for player in dashing_added.iter() {
        player_tracker.just_dashed = false;
        println!("TEST");
        
    }
}


pub(super) fn track_player_ground_distance(
    spatial_query: Res<PhysicsWorld>,
    mut query: Query<(Entity, &mut ShapeCaster, &Transform), (With<Player>)>,
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

pub(super) fn snap_after_dash(
    player_query: Query<&Transform, (With<Player>, With<CanDash>)>,
    mut removed: RemovedComponents<Dashing>,
    mut spring: ResMut<Spring>,
    mut rig: ResMut<CameraRig>,
    time: Res<Time>,

) {
    
    for entity in removed.read() {
        if let Ok(player) = player_query.get(entity) {
            spring.k = spring.k_fast ;
            
        }
    }
}

#[derive(Resource, Default)]
pub(super) struct PlayerTracker {
    /// Track player last grounded height coordinate to share among camera mechanics
    pub(super) last_grounded_y: f32, 
    pub(super) ground_distance: f32,
    pub(super) is_grounded: bool,
    pub(super) velocity: Vec2, 
    pub(super) position: Vec2,
    pub(super) just_dashed: bool,
}

impl PlayerTracker {
    pub fn debug_print(&self) {
        //print!("\x1B[2J\x1B[1;1H");
        println!("PlayerTracker Debug:");
        println!("  Last Grounded Y: {}", self.last_grounded_y);
        println!("  Ground Distance: {}", self.ground_distance);
        println!("  Ground?: {}", self.is_grounded);
        println!("  Velocity: {}", self.velocity);
        println!("  Position: {}", self.position);
        println!("  Just Dashed: {}", self.just_dashed);
    }
}

pub(super) fn debug_update(
    spring: Res<Spring>,
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
        let mut spring = Spring {
            floor: 2.0,
            ceiling: 10.0,
            snap_threshold: 1.0,
            reset_threshold: 1.5,
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
