use std::time::Instant;
use theseeker_engine::physics::ShapeCaster;
use theseeker_engine::prelude::*;
use crate::game::player::Dashing;
use crate::game::player::Player;

use super::rig_data::*;
use super::spring_data;
use super::MainCamera;
use super::PlayerInfo;

pub fn camera_rig_follow_player(
    mut rig_data: ResMut<RigData>,
    rig_query: Query<&Rig, With<MainCamera>>,
    player_query: Query<&Transform, (With<Player>, Without<Dashing>)>,
    time: Res<Time>,
) {
    let start = Instant::now();

    let player = if let Ok(transform) = player_query.get_single() {
        transform.translation
    } else {
        return;
    };

    let rig = match rig_query.get_single() {
        Ok(rig) => rig, 
        Err(_) => return,
    };
    //rig.calculate_rig_lead(player.x);
    
    //rig.calculate_displacement();
    rig_data.target.y = player.y; 
    rig_data.displacement = calculate_displacement(rig_data.target, rig.next_position);
    
    // for phases in phase query update phase
    let duration = start.elapsed();
    println!("Function took: {:?}", duration);
}

pub fn update_rig_lead(
    mut rig_data: ResMut<RigData>, 
    player_query: Query<&Transform, With<Player>>,
    mut rig_query: Query<&mut Rig, With<MainCamera>>,
) -> () {
    let player = if let Ok(transform) = player_query.get_single() {
        transform.translation
    } else {
        return;
    };
    if let Ok(mut rig) = rig_query.get_single_mut() {
        let delta_x = player.x - rig_data.target.x;
        match rig.lead_direction {
            LeadDirection::Backward => {
                if delta_x < LEAD_AMOUNT {
                    rig_data.target.x = player.x - LEAD_AMOUNT
                } else if delta_x > LEAD_AMOUNT + LEAD_AMOUNT {
                    rig.lead_direction = LeadDirection::Forward
                }
            },
            LeadDirection::Forward => {
                if delta_x > - LEAD_AMOUNT {
                    rig_data.target.x = player.x + LEAD_AMOUNT
                } else if delta_x < -LEAD_AMOUNT - LEAD_BUFFER {
                    rig.lead_direction = LeadDirection::Backward
                }
            },
        }
    }
    // Default state is to predict the player goes forward, ie "right"
    
}

pub(super)fn update_rig_equilibrium(
    mut rig_data: ResMut<RigData>,
    //ground_query: Query<(Entity, &ShapeCaster, &Transform), With<Player>>,
    player_info_query: Query<&PlayerInfo, With<MainCamera>>, 
) {
    let player_info = match player_info_query.get_single() {
        Ok(player_info) => player_info, 
        Err(_) => return, 
    };

    if player_info.is_grounded {
        rig_data.equilibrium_y = rig_data.target.y;
    }
}

pub fn calculate_displacement(rig_target: Vec2, next_position: Vec2) -> Vec2 {
    //self.displacement = self.target - self.next_position;
    Vec2::new(rig_target.x - next_position.x, rig_target.y - next_position.y)
}