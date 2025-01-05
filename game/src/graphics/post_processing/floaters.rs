use bevy::core_pipeline::core_3d;
use bevy::core_pipeline::core_3d::graph::Node3d;
use bevy::core_pipeline::fullscreen_vertex_shader::fullscreen_shader_vertex_state;
use bevy::ecs::query::QueryItem;
use bevy::ecs::system::lifetimeless::Read;
use bevy::prelude::*;
use bevy::render::camera::CameraMainTextureUsages;
use bevy::render::extract_component::{
    ComponentUniforms, DynamicUniformIndex, ExtractComponent,
    ExtractComponentPlugin, UniformComponentPlugin,
};
use bevy::render::render_graph::{
    NodeRunError, RenderGraphApp, RenderGraphContext, RenderLabel, ViewNode,
    ViewNodeRunner,
};
use bevy::render::render_resource::binding_types::{
    sampler, texture_2d, texture_depth_2d, texture_depth_2d_multisampled,
    uniform_buffer,
};
use bevy::render::render_resource::{
    BindGroupEntries, BindGroupLayout, BindGroupLayoutEntries,
    CachedRenderPipelineId, ColorTargetState, ColorWrites, FragmentState,
    MultisampleState, Operations, PipelineCache, PrimitiveState,
    RenderPassColorAttachment, RenderPassDescriptor, RenderPipelineDescriptor,
    Sampler, SamplerBindingType, SamplerDescriptor, ShaderStages, ShaderType,
    TextureFormat, TextureSampleType, TextureUsages,
};
use bevy::render::renderer::{RenderContext, RenderDevice};
use bevy::render::view::{
    prepare_view_targets, ViewDepthTexture, ViewTarget, ViewUniform,
    ViewUniformOffset, ViewUniforms,
};
use bevy::render::{Render, RenderApp, RenderSet};

pub(crate) struct FloaterPlugin;

impl Plugin for FloaterPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ExtractComponentPlugin::<FloaterSettings>::default(),
            UniformComponentPlugin::<FloaterSettings>::default(),
        ));

        app.register_type::<FloaterSettings>();

        let Ok(render_app) = app.get_sub_app_mut(RenderApp) else {
            warn!("Failed to get render app for FloaterPlugin");
            return;
        };

        render_app
            .add_systems(
                Render,
                configure_floater_view_targets
                    .after(prepare_view_targets)
                    .in_set(RenderSet::ManageViews),
            )
            .add_render_graph_node::<ViewNodeRunner<FloaterPostProcessNode>>(
                core_3d::graph::Core3d,
                FloaterPostProcessLabel,
            )
            .add_render_graph_edges(
                core_3d::graph::Core3d,
                (
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

        render_app.init_resource::<FloaterPostProcessPipeline>();
    }
}

pub fn configure_floater_view_targets(
    mut view_targets: Query<
        (
            &mut Camera3d,
            &mut CameraMainTextureUsages,
        ),
        With<FloaterSettings>,
    >,
) {
    for (mut camera_3d, mut texture_usages) in view_targets.iter_mut() {
        let mut depth_texture_usages =
            TextureUsages::from(camera_3d.depth_texture_usages);
        depth_texture_usages |= TextureUsages::TEXTURE_BINDING;
        camera_3d.depth_texture_usages = depth_texture_usages.into();

        texture_usages.0 = texture_usages.0.union(TextureUsages::COPY_DST);
    }
}

/// Component that controls the vignette post processing effect
#[derive(Component, Clone, Copy, ExtractComponent, ShaderType, Reflect)]
pub struct FloaterSettings {
    pub static_drift: Vec2,
    // TODO
}

impl Default for FloaterSettings {
    fn default() -> Self {
        Self {
            static_drift: Vec2::ZERO,
        }
    }
}

#[derive(Default)]
struct FloaterPostProcessNode;

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub(crate) struct FloaterPostProcessLabel;

impl ViewNode for FloaterPostProcessNode {
    type ViewQuery = (
        Read<ViewTarget>,
        Read<ViewDepthTexture>,
        Read<ViewUniformOffset>,
        Read<DynamicUniformIndex<FloaterSettings>>,
    );

    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (
            view_target,
            view_depth_texture,
            view_uniform_offset,
            floater_settings_uniform_index,
        ): QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let post_process_pipeline =
            world.resource::<FloaterPostProcessPipeline>();

        let pipeline_cache = world.resource::<PipelineCache>();
        let view_uniforms = world.resource::<ViewUniforms>();

        let (Some(pipeline), Some(view_uniforms_binding)) = (
            pipeline_cache
                .get_render_pipeline(post_process_pipeline.pipeline_id),
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

        let post_process = view_target.post_process_write();

        let view_bind_group = render_context.render_device().create_bind_group(
            "floater_post_process_view_bind_group",
            &post_process_pipeline.layout,
            &BindGroupEntries::sequential((
                view_uniforms_binding,
                view_depth_texture.view(),
                post_process.source,
                &post_process_pipeline.sampler,
                settings_binding.clone(),
            )),
        );

        let mut render_pass =
            render_context.begin_tracked_render_pass(RenderPassDescriptor {
                label: Some("floater_post_process_pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: post_process.destination,
                    resolve_target: None,
                    ops: Operations::default(),
                })],
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
        render_pass.draw(0..3, 0..1);

        Ok(())
    }
}

#[derive(Resource)]
struct FloaterPostProcessPipeline {
    layout: BindGroupLayout,
    sampler: Sampler,
    pipeline_id: CachedRenderPipelineId,
}

impl FromWorld for FloaterPostProcessPipeline {
    fn from_world(world: &mut World) -> Self {
        let render_device = world.resource::<RenderDevice>();
        let msaa = world.get_resource::<Msaa>().unwrap_or(&Msaa::Off);
        let layout = render_device.create_bind_group_layout(
            "floater_post_process_bind_group_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (
                    uniform_buffer::<ViewUniform>(true),
                    if *msaa != Msaa::Off {
                        texture_depth_2d_multisampled()
                    } else {
                        texture_depth_2d()
                    },
                    texture_2d(TextureSampleType::Float { filterable: true }),
                    sampler(SamplerBindingType::Filtering),
                    uniform_buffer::<FloaterSettings>(true),
                ),
            ),
        );

        let sampler =
            render_device.create_sampler(&SamplerDescriptor::default());

        let shader = world
            .resource::<AssetServer>()
            .load("shaders/post_processing/floaters.wgsl");

        let pipeline_id = world
            .resource_mut::<PipelineCache>()
            .queue_render_pipeline(RenderPipelineDescriptor {
                label: Some("floater_post_process_pipeline".into()),
                layout: vec![layout.clone()],
                vertex: fullscreen_shader_vertex_state(),
                fragment: Some(FragmentState {
                    shader,
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
            sampler,
            pipeline_id,
        }
    }
}
