use std::cmp::Ordering;
use std::time::Instant;
use theseeker_engine::physics::{PhysicsWorld, ShapeCaster};
use theseeker_engine::prelude::*;
use crate::game::player::{CanDash, Dashing, Grounded, Player};

use super::{Rig, RigData};
use super::{spring_data::*, MainCamera};

pub fn update_spring_phases(
    mut phase_query: Query<(&mut SpringPhaseX, &mut SpringPhaseY), With<MainCamera>>,
    rig_data: Res<RigData>,
) {
    let start = Instant::now();
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
    let duration = start.elapsed();
    println!("Update Spring Phases took: {:?}", duration);
}

pub fn update_follow_strategy(
    mut follow_query: Query<&mut FollowStrategy, With<MainCamera>>,
    mut player_info_query: Query<&mut PlayerInfo, With<MainCamera>>, 
    spatial_query: Res<PhysicsWorld>,
    ground_query: Query<(Entity, &ShapeCaster, &Transform), With<Player>>,
    player_query: Query<&Transform, With<Player>>,
) {
    let start = Instant::now();
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
                println!("Warning: No ground distance found for entity {:?}", entity);
                f32::INFINITY 
            });
            
            // mutable res. NEEDS TO MOVE
            //TODO: rig.equilibrium_y = rig.target.y;
           
            //player_info.is_grounded = true;
            //rig_data.equilibrium
            
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

    let duration = start.elapsed();
    println!("Update Follow Strategy took: {:?}", duration);
    

}

pub fn follow(
    mut spring_data: ResMut<SpringData>,
    mut phase_query: Query<(&mut SpringPhaseX, &mut SpringPhaseY), With<MainCamera>>,
    mut rig_query: Query<&mut Rig, With<MainCamera>>,
    follow_query: Query<&FollowStrategy, With<MainCamera>>,
    time: Res<Time>,
    dashed_query: Query<Entity, (With<Player>, Added<CanDash>)>,
    player_info_query: Query<&PlayerInfo, With<MainCamera>>, 
    rig_data: Res<RigData>,
    dash_timer_query: Query<&DashTimer, With<MainCamera>>,
) {
    let start = Instant::now();
    //spring.follow(&mut rig, &player_tracker, time.delta_seconds());
    let follow_strategy = match follow_query.get_single() {
        Ok(follow) => follow, 
        Err(_) => return,
    };

    let mut rig = match rig_query.get_single_mut() {
        Ok(rig) => rig, 
        Err(_) => return,
    };
    let dash_timer = match dash_timer_query.get_single() {
        Ok(dash_timer) => dash_timer, 
        Err(_) => return,
    };
    // let just_dashed = match dashed_query.get_single() {
    //     Ok(_) => true,
    //     Err(_) => false,
    // };
    //println!(" Just Dashed: {:?}", just_dashed);

    let player_info = if let Ok (player_info) = player_info_query.get_single(){
        player_info
    }else {return;};

    let (mut phase_x, mut phase_y) = match phase_query.get_single_mut() {
        Ok((phase_x, phase_y)) => (phase_x, phase_y), 
        Err(_) => return,
    };
    let delta_seconds = time.delta_seconds();
    let (displacement_x, displacement_y) = (rig_data.displacement.x, rig_data.displacement.y);
    let equilibrium_y = rig_data.equilibrium_y;
    let equalized_displacement_y = equilibrium_y - rig.next_position.y;

    let just_dashed = dash_timer.just_dashed;

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
                    && equilibrium_y.abs() < FLOOR {
                        equalize_y(equalized_displacement_y, spring_data.k, rig.next_position.y, delta_seconds)
                    } else {
                        calculate_spring(displacement_y, spring_data.k, rig.next_position.y,  delta_seconds)
                    }
                    
                }
                SpringPhaseY::Resetting => {
                    //rig.target.y = rig.equilibrium_y;
                    if player_info.previous_grounded_y < (player_info.grounded_y + FALL_BUFFER) {
                        equalize_y(equalized_displacement_y, spring_data.k, rig.next_position.y, delta_seconds)
                    } else {
                        calculate_spring(displacement_y, spring_data.k, rig.next_position.y,  delta_seconds)
                    }
                }
                SpringPhaseY::Snapped => {
                    //rig.target.y = rig.equilibrium_y;
                    if player_info.previous_grounded_y < (player_info.grounded_y + FALL_BUFFER) {
                        equalize_y(equalized_displacement_y, spring_data.k, rig.next_position.y, delta_seconds)
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
                    reset_spring(displacement_x, K_REG, rig.next_position.x, delta_seconds)
                }
            };
        }
        FollowStrategy::JumpFollow => {
            spring_data.k = if just_dashed {K_FAST} else {K_REG};
            // TODO: make return or set, keep consistent
            rig.next_position.x = calculate_spring(displacement_x, spring_data.k, rig.next_position.x,  delta_seconds);
            rig.next_position.y = calculate_spring(displacement_y, spring_data.k, rig.next_position.y,  delta_seconds);
        }
        FollowStrategy::FallFollow => {
            rig.next_position.y = calculate_fall(displacement_y, K_FAST*2.0, rig.next_position.y,  delta_seconds);
             rig.next_position.x = reset_spring(displacement_x, spring_data.k, rig.next_position.x,  delta_seconds);
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
    let duration = start.elapsed();
    println!("Follow took: {:?}", duration);
}

pub(super) fn update_player_grounded (
    mut player_info_query: Query<&mut PlayerInfo, With<MainCamera>>,
    grounded_query: Query<&Transform, (Added<Grounded>, With<Player>)>,
    airborne_query: Query<&Transform, (With<Player>, Without<Grounded>)>,

    //mut removed_grounded: RemovedComponents<Grounded>,
) {
    let start = Instant::now();
    let mut player_info = if let Ok (mut player_info) = player_info_query.get_single_mut(){
        player_info
    }else {return;};
    // when ground is added, player is grounded

    if let Ok(t) = grounded_query.get_single() {
        player_info.previous_grounded_y = player_info.grounded_y;
        player_info.is_grounded = true;
        player_info.grounded_y = t.translation.y;
    } else if let Ok(_) = airborne_query.get_single() {
        player_info.is_grounded = false;
    } else {
        return;
    }
    /*for t in grounded_query.iter() {
        player_info.previous_grounded_y = player_info.grounded_y;
        player_info.is_grounded = true;
        player_info.grounded_y = t.translation.y;
        
    }
    for t in airborne_query.iter() {
        
        player_info.is_grounded = false;
        
    }*/
    
    let duration = start.elapsed();
    println!("Update Player Grounded took: {:?}", duration);
    
}



fn calculate_spring(displacement: f32, k: f32, next_position: f32, delta_seconds: f32) -> f32 {
    let spring_force = k * displacement;
    let damping_force = DAMPING_RATIO * 0.0; // Velocity is intentionally zero
    let camera_acceleration = spring_force - damping_force;
    next_position + camera_acceleration * delta_seconds
}

fn equalize_y(displacement: f32, k: f32, next_position: f32, delta_seconds: f32) -> f32 {
    calculate_spring(displacement, k, next_position, delta_seconds)
}

fn reset_spring(displacement: f32, k: f32, next_position: f32, delta_seconds: f32) -> f32 {
    calculate_spring(displacement, k, next_position, delta_seconds)
}

fn calculate_fall(displacement: f32, k: f32, next_position: f32, delta_seconds: f32 ) -> f32 {
    calculate_spring(displacement, k, next_position, delta_seconds)
}

pub(super) fn update_dash_timer(
    mut query: Query<&mut DashTimer, With<MainCamera>>,
    time: Res<Time>,
    //mut player_tracker: ResMut<PlayerTracker>,
    dashed_query: Query<Entity, (With<Player>, Added<CanDash>)>,
) { 
    
    let mut timer = match query.get_single_mut() {
        Ok(timer) => timer,
        Err(_) => return,
    };
    let dash_triggered = match dashed_query.get_single() {
        Ok(_) => { true }
        Err(_) => { false }
    };
    if dash_triggered && timer.remaining == 1.0 {
        timer.just_dashed = dash_triggered;
    }

    
        if timer.remaining > 0.0 && timer.just_dashed {
            timer.remaining -= time.delta_seconds();
        } else if timer.remaining <= 0.0  {
            timer.remaining = 1.0;
            timer.just_dashed = false;
        }
       
    
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

#[cfg(test)]
mod spring_tests {
    use super::*;

    #[test]
    fn test_is_in_active_range() {
        assert!(is_in_active_range(100.0)); // Within active range
        assert!(!is_in_active_range(20.0)); // Below floor
        assert!(!is_in_active_range(600.0)); // Above ceiling
        assert!(!is_in_active_range(0.1)); // In snap zone
        assert!(!is_in_active_range(0.5)); // In reset zone
    }

    #[test]
    fn test_is_in_snap_zone() {
        assert!(is_in_snap_zone(0.1)); // Below snap threshold
        assert!(is_in_snap_zone(600.0)); // Above ceiling
        assert!(!is_in_snap_zone(100.0)); // Outside snap zone
    }

    #[test]
    fn test_is_in_reset_zone() {
        assert!(is_in_reset_zone(0.5)); // Below reset threshold
        assert!(is_in_reset_zone(600.0)); // Above ceiling
        assert!(!is_in_reset_zone(100.0)); // Outside reset zone
    }

    #[test]
    fn test_equalize_y() {
        let displacement = 10.0;
        let next_position = 20.0;
        let k = 1.0;
        let delta_seconds = 0.5;

        // Assert expected result
        let result = equalize_y(displacement, next_position, k, delta_seconds);
        assert!((result - 25.0).abs() < f32::EPSILON, "Equalize Y failed for positive displacement");

        // Test with zero displacement
        let result = equalize_y(0.0, next_position, k, delta_seconds);
        assert!((result - 20.0).abs() < f32::EPSILON, "Equalize Y failed for zero displacement");

        // Test with negative displacement
        let result = equalize_y(-10.0, next_position, k, delta_seconds);
        assert!((result - 15.0).abs() < f32::EPSILON, "Equalize Y failed for negative displacement");

        // #[should_panic] Test with invalid delta_seconds (negative value)
        
        
    }
    // TODO: Make this test fail, and by fail i mean pass
    #[test]
    #[should_panic]
    fn test_reset_spring_panic() {
        let displacement = 10.0;
        let next_position = 20.0;
        let k = 1.0;


        let _ = equalize_y(displacement, next_position, k, -1.0);
    }

    #[test]
    #[should_panic]
    fn test_reset_spring() {
        let displacement = 5.0;
        let next_position = 10.0;
        let k = 2.0;
        let delta_seconds = 0.1;

        // Assert expected result
        let result = reset_spring(displacement, next_position, k, delta_seconds);
        assert!((result - 11.0).abs() < f32::EPSILON, "Reset Spring failed for positive displacement");

        // Test with zero displacement
        let result = reset_spring(0.0, next_position, k, delta_seconds);
        assert!((result - 10.0).abs() < f32::EPSILON, "Reset Spring failed for zero displacement");

        // Test with negative displacement
        let result = reset_spring(-5.0, next_position, k, delta_seconds);
        assert!((result - 9.0).abs() < f32::EPSILON, "Reset Spring failed for negative displacement");
    }

    #[test]
    fn test_calculate_spring() {
        let displacement = 15.0;
        let k = 1.5;
        let next_position = 10.0;
        let delta_seconds = 0.2;

        // Assert expected result
        let result = calculate_spring(displacement, k, next_position, delta_seconds);
        assert!((result - 13.0).abs() < f32::EPSILON, "Calculate Spring failed for positive displacement");

        // Test with zero displacement
        let result = calculate_spring(0.0, k, next_position, delta_seconds);
        assert!((result - 10.0).abs() < f32::EPSILON, "Calculate Spring failed for zero displacement");

        // Test with negative displacement
        let result = calculate_spring(-15.0, k, next_position, delta_seconds);
        assert!((result - 7.0).abs() < f32::EPSILON, "Calculate Spring failed for negative displacement");

        
    }

    // #[should_panic] Test with invalid spring constant (zero value)
    #[test]
    #[should_panic]
    fn test_calculate_spring_invalid_constant() {
        let displacement = 15.0;
        let next_position = 10.0;
        let delta_seconds = 0.2;

        let _ = calculate_spring(displacement, 0.0, next_position, delta_seconds);
    }

    // #[should_panic] Test with invalid delta_seconds (negative value)
    #[test]
    #[should_panic]
    fn test_calculate_spring_invalid_delta_seconds() {
        let displacement = 15.0;
        let k = 1.5;
        let next_position = 10.0;

        let _ = calculate_spring(displacement, k, next_position, -0.1);
    }
    
}