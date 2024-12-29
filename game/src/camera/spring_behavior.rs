use std::cmp::Ordering;

use theseeker_engine::physics::{PhysicsWorld, ShapeCaster};
use theseeker_engine::prelude::*;
use crate::game::player::{Dashing, Player};

use super::{Rig, RigData};
use super::{spring_data::*, MainCamera};

pub fn update_spring_phases(
    mut phase_query: Query<(&mut SpringPhaseX, &mut SpringPhaseY), With<MainCamera>>,
    rig_data: Res<RigData>,
) {
    let (displacement_x, displacement_y) = (rig_data.displacement.x, rig_data.displacement.y);

    if let Ok((mut phase_x, mut phase_y)) = phase_query.get_single_mut() {
        // X phase
        if is_in_active_range(displacement_x) {
            *phase_x = SpringPhaseX::Active;
        }
        if is_in_reset_zone(displacement_x) && !is_in_snap_zone(displacement_x) {
            *phase_x = SpringPhaseX::Resetting;
        }
        if is_in_reset_zone(displacement_x) && is_in_snap_zone(displacement_x) {
            *phase_x = SpringPhaseX::Snapped;
        }
        // Y phase
        if is_in_active_range(displacement_y) {
            *phase_y = SpringPhaseY::Active;
        }
        if is_in_reset_zone(displacement_y) && !is_in_snap_zone(displacement_y) {
            *phase_y = SpringPhaseY::Resetting;
        }
        if is_in_reset_zone(displacement_y) && is_in_snap_zone(displacement_y) {
            *phase_y = SpringPhaseY::Snapped;
        }

    };
}

pub fn update_follow_strategy(
    spatial_query: Res<PhysicsWorld>,
    ground_query: Query<(Entity, &ShapeCaster, &Transform), With<Player>>,
    mut follow_query: Query<&mut FollowStrategy, With<MainCamera>>,
    mut player_info_query: Query<&mut PlayerInfo, With<MainCamera>>, 
    player_query: Query<&Transform, With<Player>>,
) {
    //spring.follow_strategy = FollowStrategy::update(&mut *spring, &player_tracker);
    //spring_data.follow_strategy = 
    let mut player_info = if let Ok (mut player_info) = player_info_query.get_single_mut(){
        player_info
    }else {return;};
    let mut follow_strategy = if let Ok (mut follow_strategy) = follow_query.get_single_mut(){
        follow_strategy
    }else {return;};
    let player = if let Ok (player_transform) = player_query.get_single(){
        player_transform.translation
    }else {return;};
    // get ground distance from query
    for (entity, ray_cast_info, position) in ground_query.iter() {
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
           
            player_info.previous_grounded_y = player_info.grounded_y;
            // mutable res. NEEDS TO MOVE
            //TODO: rig.equilibrium_y = rig.target.y;
            player_info.grounded_y = position.translation.y;
            player_info.is_grounded = true;
            
            if (ground_distance <= FLOOR) 
            && (player_info.grounded_y - player_info.previous_grounded_y).abs() <=
            (player_info.previous_grounded_y + FALL_BUFFER) {
                *follow_strategy = FollowStrategy::GroundFollow;
            } else if (player.y - player_info.grounded_y) > FLOOR {
                *follow_strategy = FollowStrategy::JumpFollow;
            }
            else if (ground_distance > (FLOOR + FALL_BUFFER))
                && (player.y < (player.y + FALL_BUFFER)){
                *follow_strategy = FollowStrategy::FallFollow;
            }
                else {
                    // keep same strategy
            // TODO: add Init logic here and return FollowStrategy::Init
                //return spring.follow_strategy.clone();

            }
    }

    
    

}

pub fn follow(
    follow_query: Query<&FollowStrategy, With<MainCamera>>,
    mut spring_data: ResMut<SpringData>,
    time: Res<Time>,
    dashed_query: Query<Entity, (With<Player>, Added<Dashing>)>,
    mut phase_query: Query<(&mut SpringPhaseX, &mut SpringPhaseY), With<MainCamera>>,
    player_info_query: Query<&PlayerInfo, With<MainCamera>>, 
    rig_data: Res<RigData>,
    mut rig_query: Query<&mut Rig, With<MainCamera>>,
) {
    //spring.follow(&mut rig, &player_tracker, time.delta_seconds());
    let follow_strategy = match follow_query.get_single() {
        Ok(follow) => follow, 
        Err(_) => return,
    };

    let mut rig = match rig_query.get_single_mut() {
        Ok(rig) => rig, 
        Err(_) => return,
    };

    let just_dashed = match dashed_query.get_single() {
        Ok(_) => true,
        Err(_) => false,
    };

    let player_info = if let Ok (player_info) = player_info_query.get_single(){
        player_info
    }else {return;};

    let (mut phase_x, mut phase_y) = match phase_query.get_single_mut() {
        Ok((phase_x, phase_y)) => (phase_x, phase_y), 
        Err(_) => return,
    };
    let delta_seconds = time.delta_seconds();
    let (displacement_x, displacement_y) = (rig_data.displacement.x, rig_data.displacement.y);

    

    match follow_strategy {
        FollowStrategy::InitFollow => {
            rig.next_position.x = calculate_spring(rig_data.displacement.x, spring_data.k, rig.next_position.x,  delta_seconds);
            rig.next_position.x = calculate_spring(rig_data.displacement.y, spring_data.k,rig.next_position.y,   delta_seconds);
        }
        FollowStrategy::GroundFollow => {
            // TODO: Update K from separate system 
            // Can i do this without tracking the k value??
            spring_data.k = if just_dashed {K_FAST} else {K_REG};
            // vertical spring phases
            rig.next_position.y = 
            match *phase_y {
                SpringPhaseY::Active => {
                    //rig.target.y = rig.equilibrium_y;
                    if player_info.previous_grounded_y < (player_info.grounded_y + FALL_BUFFER)
                    && (rig_data.target.y - rig_data.equilibrium_y).abs() < FLOOR {
                        equalize_y(displacement_y, rig.next_position.y, spring_data.k, delta_seconds)
                    } else {
                        calculate_spring(displacement_y, spring_data.k, rig.next_position.y,  delta_seconds)
                    }
                    
                }
                SpringPhaseY::Resetting => {
                    //rig.target.y = rig.equilibrium_y;
                    if player_info.previous_grounded_y < (player_info.grounded_y + FALL_BUFFER) {
                        equalize_y(displacement_y, rig.next_position.y, spring_data.k, delta_seconds)
                    } else {
                        calculate_spring(displacement_y, spring_data.k, rig.next_position.y,  delta_seconds)
                    }
                }
                SpringPhaseY::Snapped => {
                    //rig.target.y = rig.equilibrium_y;
                    if player_info.previous_grounded_y < (player_info.grounded_y + FALL_BUFFER) {
                        equalize_y(displacement_y, rig.next_position.y, spring_data.k, delta_seconds)
                    } else {
                        calculate_spring(displacement_y, spring_data.k, rig.next_position.y,  delta_seconds)
                    }
                }
                
            };
            // horizontal spring phases
            rig.next_position.x = 
            match *phase_x {
                SpringPhaseX::Active => {
                    calculate_spring(displacement_x, spring_data.k, rig.next_position.x,  delta_seconds)
                }
                SpringPhaseX::Resetting => { calculate_spring(displacement_x, spring_data.k, rig.next_position.x,  delta_seconds)}
                SpringPhaseX::Snapped => {
                    *phase_x = SpringPhaseX::Snapped;
                    spring_data.vertical_snapped = true;
                    rig_data.target.x  
                }
            };
            /* 
            if !vertical {
                self.calculate(&self, rig, delta_seconds, false)
            } else {
                    if !matches!(self.y_phase, SpringPhase::Snapping)   {
                        let displacement = rig.target.y - rig.next_position.y;
                        self.reset(displacement, self, rig, delta_seconds, vertical)
                    } else {
                            rig.next_position.y
                    } 
            }*/
        }
        FollowStrategy::JumpFollow => {
            spring_data.k = if just_dashed {K_FAST} else {K_REG};
            *phase_y = SpringPhaseY::Resetting; 
            // TODO: make return or set, keep consistent
            rig.next_position.x = calculate_spring(displacement_x, spring_data.k, rig.next_position.x,  delta_seconds);
            rig.next_position.y = calculate_spring(displacement_y, spring_data.k, rig.next_position.y,  delta_seconds);
        }
        FollowStrategy::FallFollow => {
            // TODO: 
            spring_data.k = K_REG;
            rig.next_position.y = calculate_fall(displacement_y, spring_data.k, rig.next_position.y,  delta_seconds);
            let displacement_x = rig_data.target.x - (rig.next_position.x);
            rig.next_position.x = reset_spring(displacement_y, spring_data.k, rig.next_position.y,  delta_seconds);
        }
        FollowStrategy::DashFollow => {
            spring_data.k = if just_dashed {K_FAST} else {K_REG};
            rig.next_position.x = calculate_spring(displacement_x, spring_data.k, rig.next_position.x,  delta_seconds);
            rig.next_position.y = calculate_spring(displacement_y, spring_data.k, rig.next_position.y,  delta_seconds);
        }
        _ => {
            spring_data.k = if just_dashed {K_FAST} else {K_REG};
            rig.next_position.x = calculate_spring(displacement_x, spring_data.k, rig.next_position.x,  delta_seconds);
            rig.next_position.y = calculate_spring(displacement_y, spring_data.k, rig.next_position.y,  delta_seconds);
        
        }

    }
}

fn calculate_spring(
    displacement: f32,
    k: f32, 
    next_position: f32,
    delta_seconds: f32,
    //vertical: bool, 
) -> f32 {
    //let displacement = if vertical { rig.displacement.y } else { rig.displacement.x };
    //let position = if vertical { rig.camera_position.y } else { rig.camera_position.x };
    //let velocity = if vertical { spring.velocity.abs() } else { 0.0 };
    // TODO: take velocity out of system if it's still intended to have no effect
    let velocity = 0.0;
    let spring_force = k * displacement;
    let damping_force = DAMPING_RATIO * velocity; 
    let camera_acceleration = spring_force - damping_force;
    next_position + camera_acceleration * delta_seconds
}

fn calculate_fall(
    displacement: f32, 
    next_position: f32,
    k: f32,
    delta_seconds: f32,
) -> f32 {
    //let velocity = if vertical { spring.velocity.abs() } else { 0.0 };
    let velocity = 0.0;
    let spring_force = k * displacement;
    let damping_force = DAMPING_RATIO * velocity;
    let camera_acceleration = spring_force - damping_force;
    next_position + camera_acceleration * delta_seconds
}

// pub(super) fn update_dash_timer(
//     mut query: Query<&mut DashCamTimer, With<MainCamera>>,
//     time: Res<Time>,
//     mut player_tracker: ResMut<PlayerTracker>,
//     dashing_added: Query<Entity, (With<Player>, Added<Dashing>)>,
// ) { 
//     if let Ok(mut timer) = query.get_single_mut() {
//         // Use the timer as needed
//         if let Ok(_) = dashing_added.get_single() {
//             timer.just_dashed = false;
//         } else { println!("Not hitting")}
        
//         if timer.remaining > 0.0 && timer.just_dashed == true {
//             timer.remaining -= time.delta_seconds();
//         } else if timer.remaining <= 0.0 {
//             timer.remaining = 1.0;
//         }
//         } else {
//             warn!("Expected exactly one Dash Cam Timer component, but found none or multiple.");
//         }
    
// }

// pub fn track_player_grounded(
//     grounded_query: Query<(Entity, &Transform, &ShapeCaster), (Added<Grounded>, With<Player>)>,
// ) {
//     for (_, t, _) in grounded_query.iter() {
//         player_tracker.previous_grounded_y = player_tracker.grounded_y;
//         rig.equilibrium_y = rig.target.y;
//         player_tracker.grounded_y = t.translation.y;
//         player_tracker.is_grounded = true;
//     }
// }


fn equalize_y(
    displacement: f32,
    next_position: f32, 
    k: f32,
    delta_seconds: f32,
) -> f32 {
    // TODO: Remove Velocity from system
    //let velocity = spring.velocity.abs();
    let spring_force = k * displacement;
    // Velocity is having zero effect currently
    let damping_force = DAMPING_RATIO * 0.0;//spring.velocity;
    let camera_acceleration = spring_force - damping_force;
    next_position + camera_acceleration * delta_seconds
}

pub fn reset_spring(displacement: f32, next_position: f32, k: f32, delta_seconds: f32) -> f32{
    let velocity = 0.0;
    let spring_force = k * displacement;
    let damping_force = DAMPING_RATIO * velocity;
    let camera_acceleration = spring_force - damping_force;
    next_position + camera_acceleration * delta_seconds
}

// should ONLY be in the active range, no other range
pub fn is_in_active_range(value: f32) -> bool{
    let full_range = value.abs() > FLOOR && value.abs() < CEILING;
    let snap_zone = !is_in_snap_zone(value);
    let reset_zone = !is_in_reset_zone(value);
    full_range && snap_zone && reset_zone
}

pub fn is_in_snap_zone(value: f32) -> bool {
    value.abs() < SNAP_THRESHOLD || value.abs() > CEILING
}

pub fn is_in_reset_zone(value: f32) -> bool {
    value.abs() < RESET_THRESHOLD || value.abs() > CEILING
}