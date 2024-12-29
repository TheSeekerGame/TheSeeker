pub fn calculate_rig_lead(
    mut rig: ResMut<CameraRig>, 
    player_query: Query<&Transform, With<Player>>,
) -> () {
    let player = if let Ok(transform) = player_query.get_single() {
        transform.translation
    } else {
        return;
    };
    // Default state is to predict the player goes forward, ie "right"
    let delta_x = player.x - rig.target.x;
    match rig.lead_direction {
        LeadDirection::Backward => {
            if delta_x < rig.lead_amount {
                rig.target.x = player.x - rig.lead_amount
            } else if delta_x > rig.lead_amount + rig.lead_buffer {
                rig.lead_direction = LeadDirection::Forward
            }
        },
        LeadDirection::Forward => {
            if delta_x > -rig.lead_amount {
                rig.target.x = player.x + rig.lead_amount
            } else if delta_x < -rig.lead_amount - rig.lead_buffer {
                rig.lead_direction = LeadDirection::Backward
            }
        },
    }
}