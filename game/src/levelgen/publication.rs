//! Readiness crosses the main/render-world asset publication boundary.
use super::*;
use bevy::render::{Render, RenderApp, RenderSystems};
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::render::render_asset::RenderAssets;
use bevy::render::texture::GpuImage;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

#[derive(Resource, Clone, Default)]
pub(super) struct Publication {
    pub id: u64,
    pub images: Vec<Handle<Image>>,
    pub presented: Arc<AtomicBool>,
}

impl ExtractResource for Publication {
    type Source = Self;
    fn extract_resource(source: &Self) -> Self { source.clone() }
}

pub(super) struct PublicationPlugin;

impl Plugin for PublicationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Publication>().add_plugins(ExtractResourcePlugin::<Publication>::default());
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render.add_systems(Render, mark_uploaded.after(RenderSystems::Render));
        }
    }
}

fn mark_uploaded(publication: Res<Publication>, images: Res<RenderAssets<GpuImage>>, pipelines: Res<bevy::render::render_resource::PipelineCache>) {
    if publication.id != 0 && pipelines.waiting_pipelines().next().is_none() && publication.images.iter().all(|handle| images.get(handle).is_some()) {
        publication.presented.store(true, Ordering::Release);
    }
}

pub(super) fn request(world: &mut World, id: u64) {
    let mut handles: Vec<Handle<Image>> = world.resource::<Assets<ColorMaterial>>().iter()
        .filter_map(|(_, material)| material.texture.clone()).collect();
    handles.extend(world.query::<&Sprite>().iter(world).map(|sprite| sprite.image.clone()));
    handles.extend(foliage::publication_images(world));
    let light = world.resource::<crate::graphics::post_processing::darkness::DarknessLightMapTarget>();
    handles.extend([light.handle.clone(), light.table.clone()]);
    world.insert_resource(Publication { id, images: handles, presented: Arc::default() });
}

pub(super) fn ready(world: &World, id: u64) -> bool {
    let publication = world.resource::<Publication>();
    publication.id == id && publication.presented.load(Ordering::Acquire)
}
