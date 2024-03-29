use crate::prelude::*;

pub struct GentPlugin;

impl Plugin for GentPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            (transform_gfx_from_gent.after(bevy::transform::TransformSystem::TransformPropagate),),
        );
    }
}

#[derive(Bundle)]
pub struct GentPhysicsBundle {
    pub rb: RigidBody,
    pub collider: Collider,
    pub shapecast: ShapeCaster,
}

#[derive(Component)]
pub struct TransformGfxFromGent {
    pub pixel_aligned: bool,
    //TODO: remove gent here and refactor transfor_gfx_from_gent to use player gfx?
    pub gent: Entity,
    //potential to add offset here?... or does it not make sense
}

fn transform_gfx_from_gent(
    mut q_target: Query<(
        &mut GlobalTransform,
        &TransformGfxFromGent,
    )>,
    q_src: Query<&GlobalTransform, Without<TransformGfxFromGent>>,
) {
    for (mut xf_target, gfx2gent) in &mut q_target {
        let Ok(xf_src) = q_src.get(gfx2gent.gent) else {
            continue;
        };
        *xf_target = *xf_src;
        if gfx2gent.pixel_aligned {
            let mut xf = xf_target.compute_transform();
            xf.translation = xf.translation.round();
            *xf_target = xf.into();
        }
    }
}
