use bevy::asset::load_internal_asset;
use bevy::core_pipeline::core_3d;
use bevy::core_pipeline::core_3d::graph::Node3d;
use bevy::core_pipeline::fullscreen_vertex_shader::fullscreen_shader_vertex_state;
use bevy::ecs::query::QueryItem;
use bevy::ecs::system::lifetimeless::Read;
use bevy::prelude::*;
use bevy::render::extract_component::{
    ComponentUniforms, DynamicUniformIndex, ExtractComponent,
    ExtractComponentPlugin, UniformComponentPlugin,
};
use bevy::render::render_graph::{
    NodeRunError, RenderGraphApp, RenderGraphContext, RenderLabel, ViewNode,
    ViewNodeRunner,
};
use bevy::render::render_resource::binding_types::{
    storage_buffer, uniform_buffer,
};
use bevy::render::render_resource::{
    BindGroupEntries, BindGroupLayout, BindGroupLayoutEntries,
    CachedComputePipelineId, CachedRenderPipelineId, ColorTargetState,
    ColorWrites, ComputePassDescriptor, ComputePipelineDescriptor,
    FragmentState, MultisampleState, PipelineCache, PrimitiveState,
    RenderPassDescriptor, RenderPipelineDescriptor, ShaderStages, ShaderType,
    TextureFormat,
};
use bevy::render::renderer::{RenderContext, RenderDevice};
use bevy::render::view::{ViewUniform, ViewUniformOffset, ViewUniforms};
use bevy::render::RenderApp;

const FLOATER_BUFFER_SIZE: usize = 64 * 64;
const FLOATER_BUFFER_LAYERS: usize = 4;

const FLOATER_SHADER_HANDLE: Handle<Shader> =
    Handle::weak_from_u128(134392054226504942300212274996024942407);

pub(crate) struct FloaterPlugin;

impl Plugin for FloaterPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ExtractComponentPlugin::<FloaterSettings>::default(),
            UniformComponentPlugin::<FloaterSettings>::default(),
        ));

        load_internal_asset!(
            app,
            FLOATER_SHADER_HANDLE,
            "floaters.wgsl",
            Shader::from_wgsl
        );

        app.register_type::<FloaterSettings>();

        let Ok(render_app) = app.get_sub_app_mut(RenderApp) else {
            warn!("Failed to get render app for FloaterPlugin");
            return;
        };

        render_app
            .add_render_graph_node::<ViewNodeRunner<FloaterPrepassNode>>(
                core_3d::graph::Core3d,
                FloaterPrepassLabel,
            )
            .add_render_graph_node::<ViewNodeRunner<FloaterPostProcessNode>>(
                core_3d::graph::Core3d,
                FloaterPostProcessLabel,
            )
            .add_render_graph_edges(
                core_3d::graph::Core3d,
                (
                    FloaterPrepassLabel,
                    Node3d::EndMainPass,
                    FloaterPostProcessLabel,
                    // Might want to position it before dof
                    Node3d::EndMainPassPostProcessing,
                ),
            );
    }

    fn finish(&self, app: &mut App) {
        let Ok(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app.init_resource::<FloaterPipeline>();
    }
}

#[derive(Debug, ShaderType)]
struct Floater {
    scale: f32,
    opacity: f32,
    position: Vec2,
}

#[derive(Debug, ShaderType)]
struct FloaterBuffer {
    pub floaters: [[Floater; FLOATER_BUFFER_SIZE]; FLOATER_BUFFER_LAYERS],
}

/// Component that controls the vignette post processing effect
#[derive(Component, Clone, Copy, ExtractComponent, ShaderType, Reflect)]
pub struct FloaterSettings {
    pub static_drift: Vec2,
    pub spawn_spacing: Vec2,
}

impl Default for FloaterSettings {
    fn default() -> Self {
        Self {
            static_drift: Vec2::ZERO,
            spawn_spacing: Vec2::splat(1.0),
        }
    }
}

#[derive(Default)]
struct FloaterPostProcessNode;

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub(crate) struct FloaterPostProcessLabel;

impl ViewNode for FloaterPostProcessNode {
    type ViewQuery = (
        Read<ViewUniformOffset>,
        Read<DynamicUniformIndex<FloaterSettings>>,
    );

    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (view_uniform_offset, floater_settings_uniform_index): QueryItem<
            Self::ViewQuery,
        >,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let post_process_pipeline = world.resource::<FloaterPipeline>();

        let pipeline_cache = world.resource::<PipelineCache>();
        let view_uniforms = world.resource::<ViewUniforms>();

        let (Some(pipeline), Some(view_uniforms_binding)) = (
            pipeline_cache
                .get_render_pipeline(post_process_pipeline.render_pipeline_id),
            view_uniforms.uniforms.binding(),
        ) else {
            return Ok(());
        };

        let settings_uniforms =
            world.resource::<ComponentUniforms<FloaterSettings>>();
        let Some(settings_binding) = settings_uniforms.uniforms().binding()
        else {
            return Ok(());
        };

        let view_bind_group = render_context.render_device().create_bind_group(
            "floater_post_process_view_bind_group",
            &post_process_pipeline.layout,
            &BindGroupEntries::sequential((
                view_uniforms_binding,
                settings_binding.clone(),
            )),
        );

        let mut render_pass =
            render_context.begin_tracked_render_pass(RenderPassDescriptor {
                label: Some("floater_post_process_pass"),
                color_attachments: &[],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

        render_pass.set_render_pipeline(pipeline);
        render_pass.set_bind_group(
            0,
            &view_bind_group,
            &[
                view_uniform_offset.offset,
                floater_settings_uniform_index.index(),
            ],
        );
        // render_pass.draw(0..3, 0..1);

        Ok(())
    }
}

#[derive(Resource)]
struct FloaterPipeline {
    layout: BindGroupLayout,
    prepass_pipeline_id: CachedComputePipelineId,
    render_pipeline_id: CachedRenderPipelineId,
}

impl FromWorld for FloaterPipeline {
    fn from_world(world: &mut World) -> Self {
        let render_device = world.resource::<RenderDevice>();
        let layout = render_device.create_bind_group_layout(
            "floater_postprocess_bind_group_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT
                    | ShaderStages::COMPUTE
                    | ShaderStages::VERTEX,
                (
                    uniform_buffer::<ViewUniform>(true),
                    uniform_buffer::<FloaterSettings>(true),
                    storage_buffer::<FloaterBuffer>(false),
                ),
            ),
        );

        let compute_shader = world
            .resource::<AssetServer>()
            .load("shaders/post_processing/floaters_prepass.wgsl");

        let prepass_pipeline_id = world
            .resource_mut::<PipelineCache>()
            .queue_compute_pipeline(ComputePipelineDescriptor {
                label: Some("floater_prepass_pipeline".into()),
                layout: vec![layout.clone()],
                shader: compute_shader,
                shader_defs: vec![],
                entry_point: "floater_prepass".into(),
                push_constant_ranges: vec![],
            });

        let render_shader = world
            .resource::<AssetServer>()
            .load("shaders/post_processing/floaters_render.wgsl");

        let render_pipeline_id = world
            .resource_mut::<PipelineCache>()
            .queue_render_pipeline(RenderPipelineDescriptor {
                label: Some("floater_post_process_pipeline".into()),
                layout: vec![layout.clone()],
                vertex: fullscreen_shader_vertex_state(),
                fragment: Some(FragmentState {
                    shader: render_shader,
                    shader_defs: vec![],
                    entry_point: "floater_fragment".into(),
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
            prepass_pipeline_id,
            render_pipeline_id,
        }
    }
}

#[derive(Default)]
struct FloaterPrepassNode;

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub(crate) struct FloaterPrepassLabel;

impl ViewNode for FloaterPrepassNode {
    type ViewQuery = (
        Read<ViewUniformOffset>,
        Read<DynamicUniformIndex<FloaterSettings>>,
    );

    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (view_uniform_offset, floater_settings_uniform_index): QueryItem<
            Self::ViewQuery,
        >,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let post_process_pipeline = world.resource::<FloaterPipeline>();
        let pipeline_cache = world.resource::<PipelineCache>();
        let view_uniforms = world.resource::<ViewUniforms>();

        let (Some(pipeline), Some(view_uniforms_binding)) = (
            pipeline_cache.get_compute_pipeline(
                post_process_pipeline.prepass_pipeline_id,
            ),
            view_uniforms.uniforms.binding(),
        ) else {
            return Ok(());
        };

        let settings_uniforms =
            world.resource::<ComponentUniforms<FloaterSettings>>();
        let Some(settings_binding) = settings_uniforms.uniforms().binding()
        else {
            return Ok(());
        };

        let view_bind_group = render_context.render_device().create_bind_group(
            "floater_prepass_view_bind_group",
            &post_process_pipeline.layout,
            &BindGroupEntries::sequential((
                view_uniforms_binding,
                settings_binding.clone(),
            )),
        );

        let mut command_encoder = render_context
            .command_encoder()
            .begin_compute_pass(&ComputePassDescriptor {
                label: Some("floater_compute_prepass"),
                timestamp_writes: None,
            });

        command_encoder.set_pipeline(pipeline);
        command_encoder.set_bind_group(
            0,
            &view_bind_group,
            &[
                view_uniform_offset.offset,
                floater_settings_uniform_index.index(),
            ],
        );

        command_encoder.dispatch_workgroups(1, 1, FLOATER_BUFFER_LAYERS as u32);

        Ok(())
    }
}
