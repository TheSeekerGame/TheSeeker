use std::cmp::Ordering;
use bevy::prelude::*;
use strum_macros::Display;
use theseeker_engine::physics::{LinearVelocity, PhysicsWorld, ShapeCaster};
use crate::game::player::{CanDash, Dashing, Grounded, Player};
use super::CameraRig;

#[derive(Default, Display, Debug, Clone)]
pub enum SpringPhase {
    #[default]
    Active,
    Snapped,
    Resetting,
}

impl SpringPhase {
    pub fn update(spring: &mut ResMut<CameraSpring>, _player_tracker: &Res<PlayerTracker>, displacement: f32, vertical: bool) -> SpringPhase {
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
    pub fn update(mut spring: &mut CameraSpring, player_tracker: &Res<PlayerTracker>) -> FollowStrategy {
        if (player_tracker.ground_distance <= spring.floor) 
        && (player_tracker.grounded_y - player_tracker.previous_grounded_y).abs() <=
        (player_tracker.previous_grounded_y + spring.fall_buffer) {
            return FollowStrategy::GroundFollow;
        } else if (player_tracker.position.y - player_tracker.grounded_y) > spring.floor {
            return FollowStrategy::JumpFollow;
        }
        else if (player_tracker.ground_distance > (spring.floor + spring.fall_buffer))
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
    pub(super) equalize_threshold: f32,
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
    pub fn default() -> Self {
        CameraSpring {
            floor: 37.5,
            ceiling: 1000.0,
            fall_buffer: 10.0,
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
            // equalizing threshold should be the floor value
            equalize_threshold: 25.0,
            // used for increasing camera speed when calculating fall
            fall_factor: 0.0,
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
                self.k = if player_tracker.just_dashed {self.k_fast} else {self.k_reg};
                // vertical spring phases
                rig.camera_position.y = 
                match self.y_phase {
                    SpringPhase::Active => {
                        if player_tracker.previous_grounded_y < (player_tracker.grounded_y + self.fall_buffer)
                        && (rig.target.y - rig.equilibrium_y).abs() < self.floor {
                            self.equalize_y(&self, rig, delta_seconds)
                        } else {
                            self.calculate(&self, rig, delta_seconds, true)
                        }
                        
                    }
                    SpringPhase::Resetting => {
                        if player_tracker.previous_grounded_y < (player_tracker.grounded_y + self.fall_buffer) {
                            self.equalize_y(&self, rig, delta_seconds)
                        } else {
                            self.calculate(&self, rig, delta_seconds, true)
                        }
                    }
                    SpringPhase::Snapped => {
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
            }
            FollowStrategy::JumpFollow => {
                self.k = if player_tracker.just_dashed {self.k_fast} else {self.k_reg};
                self.y_phase = SpringPhase::Resetting; 
                rig.camera_position.x = self.calculate(self, rig, delta_seconds, false);
                rig.camera_position.y = self.calculate(self, rig, delta_seconds, true);
            }
            FollowStrategy::FallFollow => {
                self.k = self.k_reg;
                rig.camera_position.y = self.calculate_fall(self, rig, delta_seconds, true);
                let displacement = rig.target.x - (rig.camera_position.x);
                rig.camera_position.x = self.reset(displacement, rig,  delta_seconds, false);
                
                
            }
            FollowStrategy::DashFollow => {
                self.k = if player_tracker.just_dashed {self.k_fast} else {self.k_reg};
                rig.camera_position.x = self.calculate(self, rig, delta_seconds, false);
                rig.camera_position.y = self.calculate(self, rig, delta_seconds, true);
            }
            _ => {
                self.k = if player_tracker.just_dashed {self.k_fast} else {self.k_reg};
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
        let damping_force = spring.damping_ratio * spring.velocity;
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
        let damping_force = spring.damping_ratio * spring.velocity;
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
        let damping_force = spring.damping_ratio * spring.velocity;
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
        let damping_force = self.damping_ratio * self.velocity;
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

    pub fn snap_horizontal(&mut self, rig: &mut ResMut<CameraRig>, _player: &Vec3, fast: bool) -> bool {
        rig.camera_position.x = rig.target.x;
        self.k = if fast {self.k_fast} else {self.k_reg};
        
        self.x_phase = SpringPhase::Snapped;
        
        true
    }

    
}

pub(super) fn track_player(
    grounded_query: Query<(Entity, &Transform, &mut ShapeCaster), (Added<Grounded>, With<Player>)>,
    mut player_tracker: ResMut<PlayerTracker>,
    player_query: Query<Entity, With<Player>>,
    transform_query: Query<&Transform, With<Player>>,
    mut removed_grounded: RemovedComponents<Grounded>,
    mut dashing_added: Query<Entity, (With<Player>, Added<Dashing>)>, 
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
    player_tracker: Res<PlayerTracker>,
) {
    let steepness = 4.0;
    let normalized_distance = player_tracker.ground_distance / 100.0; // Scale to a range of 0 to 1
    let divisor = 12.0 * (1.0 - (-normalized_distance * steepness).exp());
    spring.fall_factor =  player_tracker.ground_distance / divisor;
    
}

pub(super) fn update_dash_timer(
    mut player_tracker: ResMut<PlayerTracker>, 
    time: Res<Time>,
) { 
    if player_tracker.after_dash_timer > 0.0 && player_tracker.just_dashed == true {
        player_tracker.after_dash_timer -= time.delta_seconds();
    } else if player_tracker.after_dash_timer <= 0.0 {
        player_tracker.just_dashed = false;
        player_tracker.after_dash_timer = 1.0;
    }
}

pub(super) fn track_player_dashed(
    mut dashing_removed: RemovedComponents<Dashing>,
    can_dash_query: Query<&Transform, (With<Player>, With<CanDash>)>,
    mut player_tracker: ResMut<super::PlayerTracker>,
) {
    for entity in dashing_removed.read() {
        if let Ok(_player) = can_dash_query.get(entity) {
            player_tracker.just_dashed = true;
        }
    }
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

#[derive(Resource, Default, Clone)]
pub(super) struct PlayerTracker {
    pub(super) previous_grounded_y: f32, 
    pub(super) grounded_y: f32, 
    pub(super) ground_distance: f32,
    pub(super) is_grounded: bool,
    pub(super) velocity: Vec2, 
    pub(super) position: Vec2,
    pub(super) just_dashed: bool,
    pub(super) after_dash_timer: f32,
}
