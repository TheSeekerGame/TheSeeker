use bevy::core_pipeline::core_2d;
use bevy::core_pipeline::fullscreen_vertex_shader::fullscreen_shader_vertex_state;
use bevy::ecs::query::QueryItem;
use bevy::prelude::*;
use bevy::render::camera::RenderTarget;
use bevy::render::extract_component::{
    ComponentUniforms, ExtractComponent, ExtractComponentPlugin,
    UniformComponentPlugin,
};
use bevy::render::render_graph::{
    NodeRunError, RenderGraphApp, RenderGraphContext, RenderLabel, ViewNode,
    ViewNodeRunner,
};
use bevy::render::render_resource::{
    AsBindGroup, BindGroupEntries, BindGroupLayout, BindGroupLayoutEntry, BindingType,
    CachedRenderPipelineId, ColorTargetState, ColorWrites, FragmentState,
    MultisampleState, Operations, PipelineCache, PrimitiveState,
    RenderPassColorAttachment, RenderPassDescriptor, RenderPipelineDescriptor,
    Sampler, SamplerBindingType, SamplerDescriptor, ShaderStages, ShaderType,
    TextureFormat, TextureSampleType, TextureViewDimension,
};
use bevy::render::renderer::{RenderContext, RenderDevice};
use bevy::render::view::{RenderLayers, ViewTarget};
use bevy::render::RenderApp;

use crate::parallax::Parallax;
use crate::StateDespawnMarker;
use super::vignette::VignettePostProcessLabel;

pub(crate) struct DarknessPlugin;

impl Plugin for DarknessPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ExtractComponentPlugin::<DarknessSettings>::default(),
            UniformComponentPlugin::<DarknessSettings>::default(),
        ));

        app.register_type::<DarknessSettings>();

        app.add_systems(Update, mark_light_source_layers);

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            warn!("Failed to get render app for DarknessPlugin");
            return;
        };

        render_app
            .add_render_graph_node::<ViewNodeRunner<DarknessPostProcessNode>>(
                core_2d::graph::Core2d,
                DarknessPostProcessLabel,
            )
            .add_render_graph_edges(
                core_2d::graph::Core2d,
                (
                    core_2d::graph::Node2d::EndMainPass,
                    DarknessPostProcessLabel,
                    VignettePostProcessLabel, // Ensure darkness runs before vignette
                    core_2d::graph::Node2d::EndMainPassPostProcessing,
                ),
            );
    }

    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app.init_resource::<DarknessPostProcessPipeline>();
    }
}

#[derive(Component)]
pub struct LightSource;

fn mark_light_source_layers(
    mut commands: Commands,
    all_parallax: Query<&Parallax>,
    unmarked_parallax: Query<
        (Entity, &Parallax),
        (With<Parallax>, Without<LightSource>),
    >,
) {
    if unmarked_parallax.is_empty() {
        return;
    }

    let max_depth = all_parallax
        .iter()
        .map(|p| p.depth)
        .fold(f32::NEG_INFINITY, |a, b| a.max(b));

    if max_depth == f32::NEG_INFINITY {
        return;
    }

    for (entity, parallax) in &unmarked_parallax {
        if (parallax.depth - max_depth).abs() < 0.001 {
            commands
                .entity(entity)
                .insert((LightSource, RenderLayers::layer(1)));
        }
    }
}

#[derive(Component, Clone, Copy, ExtractComponent, ShaderType, Reflect, AsBindGroup)]
pub struct DarknessSettings {
    pub bg_light_level: f32,
    pub darkness_intensity: f32,
    pub _padding: Vec2,
}

impl Default for DarknessSettings {
    fn default() -> Self {
        Self {
            bg_light_level: 0.3,
            darkness_intensity: 0.9,
            _padding: Vec2::ZERO,
        }
    }
}

#[derive(Default)]
struct DarknessPostProcessNode;

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub struct DarknessPostProcessLabel;

impl ViewNode for DarknessPostProcessNode {
    type ViewQuery = &'static ViewTarget;

    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        view_target: QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let post_process_pipeline =
            world.resource::<DarknessPostProcessPipeline>();

        let pipeline_cache = world.resource::<PipelineCache>();

        let Some(horizontal_pipeline) = pipeline_cache
            .get_render_pipeline(post_process_pipeline.horizontal_pipeline_id)
        else {
            return Ok(());
        };
        let Some(vertical_pipeline) = pipeline_cache
            .get_render_pipeline(post_process_pipeline.vertical_pipeline_id)
        else {
            return Ok(());
        };

        let settings_uniforms =
            world.resource::<ComponentUniforms<DarknessSettings>>();
        let Some(settings_binding) = settings_uniforms.uniforms().binding()
        else {
            return Ok(());
        };

        // Note: This post-processing effect will be applied to the rendered content
        // from layers 0 and 1 (world and light sources). Layer 2 (player) will be
        // rendered afterwards by the normal rendering pipeline, effectively appearing
        // on top of the darkness effect.

        // Pass 1: Horizontal
        let post_process = view_target.post_process_write();
        let horizontal_bind_group = render_context.render_device().create_bind_group(
            "darkness_h_bind_group",
            &post_process_pipeline.layout,
            &BindGroupEntries::sequential((
                post_process.source,
                &post_process_pipeline.sampler,
                settings_binding.clone(),
            )),
        );

        let mut render_pass =
            render_context.begin_tracked_render_pass(RenderPassDescriptor {
                label: Some("darkness_h_pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: post_process.destination,
                    resolve_target: None,
                    ops: Operations::default(),
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

        render_pass.set_render_pipeline(horizontal_pipeline);
        render_pass.set_bind_group(0, &horizontal_bind_group, &[]);
        render_pass.draw(0..3, 0..1);
        drop(render_pass);

        // Pass 2: Vertical
        let post_process = view_target.post_process_write();
        let vertical_bind_group = render_context.render_device().create_bind_group(
            "darkness_v_bind_group",
            &post_process_pipeline.layout,
            &BindGroupEntries::sequential((
                post_process.source,
                &post_process_pipeline.sampler,
                settings_binding.clone(),
            )),
        );

        let mut render_pass =
            render_context.begin_tracked_render_pass(RenderPassDescriptor {
                label: Some("darkness_v_pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: post_process.destination,
                    resolve_target: None,
                    ops: Operations::default(),
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

        render_pass.set_render_pipeline(vertical_pipeline);
        render_pass.set_bind_group(0, &vertical_bind_group, &[]);
        render_pass.draw(0..3, 0..1);

        Ok(())
    }
}

#[derive(Resource)]
struct DarknessPostProcessPipeline {
    layout: BindGroupLayout,
    sampler: Sampler,
    horizontal_pipeline_id: CachedRenderPipelineId,
    vertical_pipeline_id: CachedRenderPipelineId,
}

impl FromWorld for DarknessPostProcessPipeline {
    fn from_world(world: &mut World) -> Self {
        let render_device = world.resource::<RenderDevice>();

        let layout = render_device.create_bind_group_layout(
            "darkness_post_process_bind_group_layout",
            &[
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
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: bevy::render::render_resource::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(DarknessSettings::min_size()),
                    },
                    count: None,
                },
            ],
        );

        let sampler =
            render_device.create_sampler(&SamplerDescriptor::default());

        let shader = world
            .resource::<AssetServer>()
            .load("shaders/post_processing/darkness.wgsl");

        let pipeline_cache = world.resource_mut::<PipelineCache>();

        let horizontal_pipeline_id =
            pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
                label: Some("darkness_h_pipeline".into()),
                layout: vec![layout.clone()],
                vertex: fullscreen_shader_vertex_state(),
                fragment: Some(FragmentState {
                    shader: shader.clone(),
                    shader_defs: vec!["HORIZONTAL".into()],
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
                zero_initialize_workgroup_memory: false,
            });

        let vertical_pipeline_id =
            pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
                label: Some("darkness_v_pipeline".into()),
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
                zero_initialize_workgroup_memory: false,
            });

        Self {
            layout,
            sampler,
            horizontal_pipeline_id,
            vertical_pipeline_id,
        }
    }
}
