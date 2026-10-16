use crate::physics::{Collider, LinearVelocity};
use crate::prelude::*;

pub struct GentPlugin;

impl Plugin for GentPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(GameTickPost, transform_gfx_from_gent.in_set(crate::presentation::PresentationSet::Prepare));
    }
}

// todo move to physics
#[derive(Bundle)]
pub struct GentPhysicsBundle {
    pub collider: Collider,
    pub linear_velocity: LinearVelocity,
}

#[derive(Component, Debug)]
pub struct Gent {
    pub e_gfx: Entity,
    pub e_effects_gfx: Entity,
}

#[derive(Component)]
pub struct TransformGfxFromGent {
    pub pixel_aligned: bool,
    // TODO: remove gent here and refactor transfor_gfx_from_gent to use player gfx?
    pub gent: Entity,
    pub offset: Option<Vec3>,
}

/// Update the authoritative graphics pose. Render-only proxies apply history and rounding.
pub fn transform_gfx_from_gent(
    mut targets: Query<(&mut Transform, &TransformGfxFromGent)>,
    sources: Query<&Transform, Without<TransformGfxFromGent>>,
) {
    for (mut target, binding) in &mut targets {
        let Ok(source) = sources.get(binding.gent) else { continue; };
        *target = *source;
        target.translation += binding.offset.unwrap_or(Vec3::ZERO);
    }
}
