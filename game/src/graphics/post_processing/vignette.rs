use bevy::core_pipeline::core_3d;
use bevy::core_pipeline::fullscreen_vertex_shader::fullscreen_shader_vertex_state;
use bevy::ecs::query::QueryItem;
use bevy::prelude::*;
use bevy::render::extract_component::{
    ComponentUniforms, ExtractComponent, ExtractComponentPlugin,
    UniformComponentPlugin,
};
use bevy::render::render_graph::{
    NodeRunError, RenderGraphApp, RenderGraphContext, RenderLabel, ViewNode,
    ViewNodeRunner,
};
use bevy::render::render_resource::{
    BindGroupEntries, BindGroupLayout, BindGroupLayoutEntry, BindingType,
    CachedRenderPipelineId, ColorTargetState, ColorWrites, FragmentState,
    MultisampleState, Operations, PipelineCache, PrimitiveState,
    RenderPassColorAttachment, RenderPassDescriptor, RenderPipelineDescriptor,
    Sampler, SamplerBindingType, SamplerDescriptor, ShaderStages, ShaderType,
    TextureFormat, TextureSampleType, TextureViewDimension,
};
use bevy::render::renderer::{RenderContext, RenderDevice};
use bevy::render::view::ViewTarget;
use bevy::render::RenderApp;

use super::darkness::DarknessPostProcessLabel;

pub(crate) struct VignettePlugin;

impl Plugin for VignettePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ExtractComponentPlugin::<VignetteSettings>::default(),
            UniformComponentPlugin::<VignetteSettings>::default(),
        ));

        app.register_type::<VignetteSettings>();

        let Ok(render_app) = app.get_sub_app_mut(RenderApp) else {
            warn!("Failed to get render app for VignettePlugin");
            return;
        };

        render_app
            .add_render_graph_node::<ViewNodeRunner<VignettePostProcessNode>>(
                core_3d::graph::Core3d,
                VignettePostProcessLabel,
            )
            .add_render_graph_edges(
                core_3d::graph::Core3d,
                (
                    // We want vignette on top of everything else
                    DarknessPostProcessLabel,
                    VignettePostProcessLabel,
                    core_3d::graph::Node3d::EndMainPassPostProcessing,
                ),
            );
    }

    fn finish(&self, app: &mut App) {
        let Ok(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app.init_resource::<VignettePostProcessPipeline>();
    }
}

/// Component that controls the vignette post processing effect
#[derive(Component, Clone, Copy, ExtractComponent, ShaderType, Reflect)]
pub struct VignetteSettings {
    /// The color of the vignette
    pub color: Vec3,
    /// brightness clamp for the top value of vignette gradient
    pub base_brightness: f32,
    /// The radius of the vignette
    pub radius: f32,
    /// How smooth the vignette gradient is
    pub smoothness: f32,
    /// Offset for the vignette center
    pub offset: Vec2,
}

// Default configuration for a mild black vignette
impl Default for VignetteSettings {
    fn default() -> Self {
        Self {
            color: Vec3::new(0.0, 0.0, 0.0),
            base_brightness: 0.15,
            radius: 0.1,
            smoothness: 0.6,
            offset: Vec2::ZERO,
        }
    }
}

#[derive(Default)]
struct VignettePostProcessNode;

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub(crate) struct VignettePostProcessLabel;

impl ViewNode for VignettePostProcessNode {
    type ViewQuery = &'static ViewTarget;

    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        view_target: QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let post_process_pipeline =
            world.resource::<VignettePostProcessPipeline>();

        let pipeline_cache = world.resource::<PipelineCache>();

        let Some(pipeline) = pipeline_cache
            .get_render_pipeline(post_process_pipeline.pipeline_id)
        else {
            return Ok(());
        };

        let settings_uniforms =
            world.resource::<ComponentUniforms<VignetteSettings>>();
        let Some(settings_binding) = settings_uniforms.uniforms().binding()
        else {
            return Ok(());
        };

        let post_process = view_target.post_process_write();

        let bind_group = render_context.render_device().create_bind_group(
            "darkness_post_process_bind_group",
            &post_process_pipeline.layout,
            // It's important for this to match the BindGroupLayout defined in the PostProcessPipeline
            &BindGroupEntries::sequential((
                // Make sure to use the source view
                post_process.source,
                // Use the sampler created for the pipeline
                &post_process_pipeline.sampler,
                // Set the settings binding
                settings_binding.clone(),
            )),
        );

        let mut render_pass =
            render_context.begin_tracked_render_pass(RenderPassDescriptor {
                label: Some("darkness_post_process_pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    // We need to specify the post process destination view here
                    // to make sure we write to the appropriate texture.
                    view: post_process.destination,
                    resolve_target: None,
                    ops: Operations::default(),
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

        render_pass.set_render_pipeline(pipeline);
        render_pass.set_bind_group(0, &bind_group, &[]);
        render_pass.draw(0..3, 0..1);

        Ok(())
    }
}

#[derive(Resource)]
struct VignettePostProcessPipeline {
    layout: BindGroupLayout,
    sampler: Sampler,
    pipeline_id: CachedRenderPipelineId,
}

impl FromWorld for VignettePostProcessPipeline {
    fn from_world(world: &mut World) -> Self {
        let render_device = world.resource::<RenderDevice>();

        // We need to define the bind group layout used for our pipeline
        let layout = render_device.create_bind_group_layout(
            "darkness_post_process_bind_group_layout",
            &[
                // The screen texture
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // The sampler that will be used to sample the screen texture
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
                // The settings uniform that will control the effect
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: bevy::render::render_resource::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(VignetteSettings::min_size()),
                    },
                    count: None,
                },
            ],
        );

        let sampler =
            render_device.create_sampler(&SamplerDescriptor::default());

        let shader = world
            .resource::<AssetServer>()
            .load("shaders/post_processing/vignette.wgsl");

        let pipeline_id = world
            .resource_mut::<PipelineCache>()
            .queue_render_pipeline(RenderPipelineDescriptor {
                label: Some("darkness_post_process_pipeline".into()),
                layout: vec![layout.clone()],
                vertex: fullscreen_shader_vertex_state(),
                fragment: Some(FragmentState {
                    shader,
                    shader_defs: vec![],
                    entry_point: "fragment".into(),
                    targets: vec![Some(ColorTargetState {
                        format: TextureFormat::Rgba16Float,
                        blend: None,
                        write_mask: ColorWrites::ALL,
                    })],
                }),
                primitive: PrimitiveState::default(),
                depth_stencil: None,
                multisample: MultisampleState::default(),
                push_constant_ranges: vec![],
            });

        Self {
            layout,
            sampler,
            pipeline_id,
        }
    }
}
