//! Camera-relative presentation math shared by meshes, proxies, and compositing.
use bevy::camera::visibility::RenderLayers;
use bevy::transform::TransformSystems;
use bevy::window::PrimaryWindow;
use theseeker_engine::presentation::{PresentationAlignment, PresentationProxy, PresentationSet};
use crate::camera::{GameplayPixelScale, MainCamera, update_camera};
use crate::prelude::*;

#[derive(Clone, Copy)]
pub enum RenderBand {
    World,
    PixelatedWorld,
    ActorOverlay,
}

impl RenderBand {
    pub fn layers(self) -> RenderLayers {
        match self {
            Self::World => RenderLayers::from_layers(&[0, 1]),
            Self::PixelatedWorld => RenderLayers::layer(3),
            Self::ActorOverlay => RenderLayers::layer(2),
        }
    }
}

#[derive(Clone, Copy)]
pub struct PixelViewMapping {
    pub camera: Vec3,
    pub divisor: u32,
    pub viewport: UVec2,
    pub raster_center: Vec2,
    pub residual: Vec2,
}

impl PixelViewMapping {
    pub fn new(camera: Vec3, divisor: u32, viewport: UVec2) -> Self {
        let raster_center = camera.xy().round();
        Self {
            camera,
            divisor: divisor.max(1),
            viewport,
            raster_center,
            residual: snap_world_residual_to_physical_pixels(camera.xy() - raster_center, divisor),
        }
    }

    pub fn displayed_center(self) -> Vec2 {
        self.raster_center + self.residual
    }

    pub fn mesh_offset(self) -> Vec2 {
        self.camera.xy() - self.displayed_center()
    }

    pub fn align_overlay(self, position: Vec2) -> Vec2 {
        self.camera.xy() + snap_world_position_to_physical_pixels(position - self.camera.xy(), self.divisor)
    }

    pub fn composite_transform(self, transform: &GlobalTransform) -> GlobalTransform {
        let mut displayed = transform.compute_transform();
        let center = self.displayed_center();
        displayed.translation.x = center.x;
        displayed.translation.y = center.y;
        GlobalTransform::from(displayed)
    }
}

#[derive(Resource, Default)]
pub struct PixelView(pub Option<PixelViewMapping>);

/// Batched vertices are already in world coordinates; only compensate the camera residual.
#[derive(Component)]
pub struct PixelAlignedWorldMesh;

pub struct PixelViewPlugin;

impl Plugin for PixelViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PixelView>();
        app.add_systems(Update, update_pixel_view
            .after(update_camera).in_set(GameTickSet::Post));
        app.add_systems(PostUpdate, (align_world_meshes, align_sprite_proxies)
            .after(PresentationSet::Present)
            .before(TransformSystems::Propagate));
    }
}

pub fn update_pixel_view(
    mut view: ResMut<PixelView>,
    scale: Res<GameplayPixelScale>,
    camera: Query<(&Transform, Option<&Camera>), With<MainCamera>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    view.0 = camera.single().ok().map(|(transform, camera)| {
        let viewport = camera.and_then(Camera::physical_viewport_size)
            .or_else(|| windows.single().ok().map(Window::physical_size)).unwrap_or_default();
        PixelViewMapping::new(transform.translation, scale.divisor(), viewport)
    });
}

pub fn align_world_meshes(
    view: Res<PixelView>,
    mut meshes: Query<&mut Transform, With<PixelAlignedWorldMesh>>,
) {
    let Some(view) = view.0 else { return; };
    let offset = view.mesh_offset();
    for mut transform in &mut meshes {
        transform.translation.x = offset.x;
        transform.translation.y = offset.y;
    }
}

pub fn align_sprite_proxies(
    view: Res<PixelView>,
    mut proxies: Query<(&mut Transform, &PresentationAlignment), With<PresentationProxy>>,
) {
    let Some(view) = view.0 else { return; };
    for (mut transform, alignment) in &mut proxies {
        if *alignment == PresentationAlignment::None { continue; }
        transform.translation = view.align_overlay(transform.translation.xy()).extend(transform.translation.z);
        if *alignment == PresentationAlignment::UprightCameraPixels {
            transform.rotation = Quat::IDENTITY;
            transform.scale = Vec3::ONE;
        }
    }
}

pub(crate) fn snap_world_position_to_physical_pixels(
    position: Vec2,
    divisor: u32,
) -> Vec2 {
    let snapped_position = position.round();
    snapped_position
        + snap_world_residual_to_physical_pixels(
            position - snapped_position,
            divisor,
        )
}

pub(crate) fn snap_world_residual_to_physical_pixels(
    residual: Vec2,
    divisor: u32,
) -> Vec2 {
    let divisor = divisor.max(1) as f32;
    (residual * divisor).round() / divisor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_stage_rounding_preserves_half_grid_ties_on_both_sides_of_zero() {
        for (camera, expected) in [(-1.5, -1.0), (-0.5, 0.0), (0.5, 0.0), (1.5, 1.0)] {
            let mapping = PixelViewMapping::new(Vec3::splat(camera), 1, UVec2::new(365, 205));
            assert_eq!(mapping.displayed_center(), Vec2::splat(expected));
            let enlarged = PixelViewMapping::new(Vec3::splat(camera), 2, UVec2::new(730, 410));
            assert_eq!(enlarged.displayed_center(), Vec2::splat(camera));
        }
    }

    #[test]
    fn meshes_and_overlays_remain_on_physical_pixels_across_camera_and_scale_changes() {
        let cases = [0, 1, 2, 3, 6, 9].into_iter().flat_map(|divisor| {
            [-32.5, -1.18, -0.5, 0.0, 0.5, 1.18, 32.5].into_iter().map(move |camera| (divisor, camera))
        });
        for (divisor, camera) in cases {
            let mapping = PixelViewMapping::new(Vec3::new(camera, -camera, 7.0), divisor, UVec2::new(2934, 1836));
            let physical_scale = mapping.divisor as f32;
            let mesh = (Vec2::new(-15.0, 23.0) + mapping.mesh_offset() - mapping.camera.xy()) * physical_scale;
            let overlay = (mapping.align_overlay(Vec2::new(-3.18, 10.18)) - mapping.camera.xy()) * physical_scale;
            assert!((mesh - mesh.round()).abs().max_element() < 0.0001);
            assert!((overlay - overlay.round()).abs().max_element() < 0.0001);
        }
    }

    #[test]
    fn snaps_composite_residual_to_physical_pixels() {
        let snapped =
            snap_world_residual_to_physical_pixels(Vec2::new(0.18, -0.18), 6);

        assert_eq!(snapped, Vec2::new(1.0 / 6.0, -1.0 / 6.0));
    }

    #[test]
    fn keeps_residual_unchanged_when_it_is_already_pixel_aligned() {
        let snapped =
            snap_world_residual_to_physical_pixels(Vec2::new(0.25, -0.5), 4);

        assert_eq!(snapped, Vec2::new(0.25, -0.5));
    }

    #[test]
    fn snaps_world_position_to_composited_pixel_grid() {
        let snapped =
            snap_world_position_to_physical_pixels(Vec2::new(10.18, -3.18), 6);

        assert_eq!(snapped, Vec2::new(10.0 + 1.0 / 6.0, -3.0 - 1.0 / 6.0));
    }
}
