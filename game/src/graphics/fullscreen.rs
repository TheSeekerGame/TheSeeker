//! Fullscreen effects. Each owns its shader, uniforms, and targets, and draws through a
//! `FullscreenPass` from the view's current image into its next, within the camera's viewport.
//! The image spans the whole render target, so shaders read it at `in.position`; `in.uv`
//! spans the viewport, which is what the camera shows.
use bevy::prelude::*;
use bevy::core_pipeline::FullscreenShader;
use bevy::render::camera::ExtractedCamera;
use bevy::render::render_resource::binding_types::{sampler as sampler_binding, texture_2d};
use bevy::render::render_resource::*;
use bevy::render::renderer::RenderContext;
use bevy::render::view::ViewTarget;

/// A filterable 2D color texture, such as the view's image.
pub fn texture() -> BindGroupLayoutEntryBuilder {
    texture_2d(TextureSampleType::Float { filterable: true })
}

/// A filtering sampler.
pub fn sampler() -> BindGroupLayoutEntryBuilder {
    sampler_binding(SamplerBindingType::Filtering)
}

/// What a fullscreen pass draws on: the view's images, and the camera whose viewport bounds it.
pub type FullscreenView = (&'static ViewTarget, &'static ExtractedCamera);

/// A fullscreen effect's pipeline, which reads the view's current image and writes its next.
pub struct FullscreenPass {
    label: &'static str,
    layout: BindGroupLayoutDescriptor,
    pipeline: CachedRenderPipelineId,
    /// Whether the effect blends onto the view's next image, which it must then keep.
    blends: bool,
}

impl FullscreenPass {
    /// Queues the pass drawing the fragment shader at `shader` with `bindings`, the first of
    /// them the view's current image. With `blend`, it blends onto the view's next image.
    pub fn new<const N: usize>(world: &World, label: &'static str, shader: &'static str,
        bindings: impl IntoBindGroupLayoutEntryBuilderArray<N>, blend: Option<BlendState>,
    ) -> Self {
        let layout = BindGroupLayoutDescriptor::new(label, &BindGroupLayoutEntries::sequential(ShaderStages::FRAGMENT, bindings));
        let pipeline = world.resource::<PipelineCache>().queue_render_pipeline(RenderPipelineDescriptor {
            label: Some(label.into()),
            layout: vec![layout.clone()],
            vertex: world.resource::<FullscreenShader>().to_vertex_state(),
            fragment: Some(FragmentState {
                shader: world.resource::<AssetServer>().load(shader),
                shader_defs: vec![],
                entry_point: Some("fragment".into()),
                targets: vec![Some(ColorTargetState { format: TextureFormat::Rgba8UnormSrgb, blend, write_mask: ColorWrites::ALL })],
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            immediate_size: 0,
            zero_initialize_workgroup_memory: false,
        });
        Self { label, layout, pipeline, blends: blend.is_some() }
    }

    /// Draws the pass over `view` once its pipeline has compiled. `bindings` is given the
    /// view's current image and binds it first, then the effect's own resources.
    pub fn draw<'a, const N: usize>(&self, world: &'a World, render_context: &mut RenderContext,
        (view, camera): (&'a ViewTarget, &ExtractedCamera),
        bindings: impl FnOnce(&'a TextureView) -> BindGroupEntries<'a, N>, dynamic_offsets: &[u32],
    ) {
        let cache = world.resource::<PipelineCache>();
        let Some(pipeline) = cache.get_render_pipeline(self.pipeline) else {
            return;
        };
        // Taking the next image swaps the view's images, so nothing may skip the draw from here.
        let images = view.post_process_write();
        let layout = cache.get_bind_group_layout(&self.layout);
        let bind_group = render_context.render_device().create_bind_group(self.label, &layout, &bindings(images.source));
        let load = if self.blends { LoadOp::Load } else { LoadOp::Clear(default()) };
        let mut pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
            label: Some(self.label),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: images.destination,
                resolve_target: None,
                depth_slice: None,
                ops: Operations { load, store: StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if let Some(viewport) = &camera.viewport {
            pass.set_camera_viewport(viewport);
        }
        pass.set_render_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, dynamic_offsets);
        pass.draw(0..3, 0..1);
    }
}
