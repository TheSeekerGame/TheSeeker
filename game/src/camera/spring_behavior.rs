use theseeker_engine::prelude::With;
use theseeker_engine::prelude::Query;
use super::{spring_data::*, MainCamera};

pub fn update_spring_phases(
    mut phase_query: Query<(&mut SpringPhaseX, &mut SpringPhaseY), With<MainCamera>>,
    mut spring_data: ResMut<SpringData>,
) {

    //spring.y_phase = SpringPhase::update(&mut spring, &player_tracker, rig.displacement.y, true);
    //spring.x_phase = SpringPhase::update(&mut spring, &player_tracker, rig.displacement.x,  false);
    if let Ok((mut phase_x, mut phase_y)) = phase_query.get_single_mut() {

    };
    
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

pub fn update_follow_strategy(

) {
    spring.follow_strategy = FollowStrategy::update(&mut *spring, &player_tracker);
    

}

pub fn follow(

) {
    spring.follow(&mut rig, &player_tracker, time.delta_seconds());
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

pub fn calculate_fall() {

}

pub fn equalize_y() {

}

pub fn reset_spring() {

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