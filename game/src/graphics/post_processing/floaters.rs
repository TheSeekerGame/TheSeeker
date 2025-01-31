//! Post processing effect that adds randomly floating particles to the scene.
//! It works by first calculating the particle positions on a set grid relative
//! to the camera, then using that buffer directly in the vertex/fragment shader
//! for rendering, avoiding passing the data through CPU side. The grid and layers
//! are designed to correspond to the compute shader workgroup dimensions, which
//! should reduce the performance impact of the effect.

use std::path::Path;

use bevy::core_pipeline::core_3d::graph::Node3d;
use bevy::core_pipeline::core_3d::{self, CORE_3D_DEPTH_FORMAT};
use bevy::ecs::query::QueryItem;
use bevy::ecs::system::lifetimeless::Read;
use bevy::render::extract_component::{
    ComponentUniforms, DynamicUniformIndex, ExtractComponent,
    ExtractComponentPlugin, UniformComponentPlugin,
};
use bevy::render::globals::{GlobalsBuffer, GlobalsUniform};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_graph::{
    NodeRunError, RenderGraphApp, RenderGraphContext, RenderLabel, ViewNode,
    ViewNodeRunner,
};
use bevy::render::render_resource::binding_types::{
    sampler, storage_buffer, texture_2d, uniform_buffer,
};
use bevy::render::render_resource::{
    BindGroupEntries, BindGroupLayout, BindGroupLayoutEntries, Buffer,
    BufferDescriptor, BufferUsages, CachedComputePipelineId,
    CachedRenderPipelineId, ColorTargetState, ColorWrites, CompareFunction,
    ComputePassDescriptor, ComputePipelineDescriptor, DepthBiasState,
    DepthStencilState, FragmentState, LoadOp, MultisampleState, Operations,
    PipelineCache, PrimitiveState, RenderPassColorAttachment,
    RenderPassDescriptor, RenderPipelineDescriptor, SamplerBindingType,
    ShaderSize, ShaderStages, ShaderType, StencilState, StoreOp, TextureFormat,
    TextureSampleType, VertexState,
};
use bevy::render::renderer::{RenderContext, RenderDevice};
use bevy::render::texture::FallbackImageZero;
use bevy::render::view::{
    ViewDepthTexture, ViewTarget, ViewUniform, ViewUniformOffset, ViewUniforms,
};
use bevy::render::RenderApp;

use crate::graphics::dof::DepthOfFieldPostProcessLabel;
use crate::prelude::*;

/// Grid dimension for the camera-relative floater buffer; shader shared constant
const FLOATER_SAMPLES_X: usize = 32;

/// Grid dimension for the camera-relative floater buffer; shader shared constant
const FLOATER_SAMPLES_Y: usize = FLOATER_SAMPLES_X;

/// Total count of floater particles in the buffer
const FLOATER_BUFFER_SIZE: usize = FLOATER_SAMPLES_X * FLOATER_SAMPLES_Y;

/// Number of layers in the floater buffer and compute shader workgroup count (z)
const FLOATER_BUFFER_LAYERS: usize = 5;

const FLOATER_SHADER_HANDLE: Handle<Shader> =
    Handle::weak_from_u128(134392054226504942300212274996024942407);

/// File path to the floater sprite strip (x-axis)
const FLOATER_TEXTURE_FILE: &str = "fx/floater.png";

/// Plugin that adds the floater post processing effect to the render graph
pub(crate) struct FloaterPlugin;

impl Plugin for FloaterPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ExtractComponentPlugin::<FloaterSettings>::default(),
            UniformComponentPlugin::<FloaterSettings>::default(),
        ));

        // Preprocess the floater shader to set the grid dimension constants
        let shader_str = include_str!("floaters.wgsl")
            .replace(
                "{{FLOATER_SAMPLES_X}}",
                &FLOATER_SAMPLES_X.to_string(),
            )
            .replace(
                "{{FLOATER_SAMPLES_Y}}",
                &FLOATER_SAMPLES_Y.to_string(),
            );

        app.world.resource_mut::<Assets<_>>().insert(
            FLOATER_SHADER_HANDLE,
            Shader::from_wgsl(
                shader_str,
                Path::new(file!())
                    .parent()
                    .unwrap()
                    .join("floaters.wgsl")
                    .to_string_lossy(),
            ),
        );

        app.register_type::<FloaterSettings>();

        let Ok(render_app) = app.get_sub_app_mut(RenderApp) else {
            warn!("Failed to get render app for FloaterPlugin");
            return;
        };

        // Render graph nodes are split into foreground and background passes so that
        // DoF is consistent, as it doesn't apply to the foreground layer.
        render_app
            .add_render_graph_node::<ViewNodeRunner<FloaterPrepassNode>>(
                core_3d::graph::Core3d,
                FloaterPrepassLabel,
            )
            .add_render_graph_node::<ViewNodeRunner<FloaterPostProcessNode::<true>>>(
                core_3d::graph::Core3d,
                FloaterBgRenderLabel,
            )
            .add_render_graph_node::<ViewNodeRunner<FloaterPostProcessNode::<false>>>(
                core_3d::graph::Core3d,
                FloaterFgRenderLabel,
            )
            .add_render_graph_edges(
                core_3d::graph::Core3d,
                (
                    FloaterPrepassLabel,
                    Node3d::EndPrepasses,
                    Node3d::MainOpaquePass,
                    FloaterBgRenderLabel,
                    DepthOfFieldPostProcessLabel,
                    Node3d::MainTransparentPass,
                    FloaterFgRenderLabel,
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

/// Shader struct for the floater data - not used outside of shader definitions
#[derive(ShaderType)]
struct Floater {
    scale: f32,
    position: Vec2,
}

/// Shader struct for the floater buffer - not used outside of shader definitions
#[derive(ShaderType)]
struct FloaterBuffer {
    pub floaters: [[Floater; FLOATER_BUFFER_SIZE]; FLOATER_BUFFER_LAYERS],
}

/// Component that controls the floater post processing effect
#[derive(Component, Clone, Copy, ExtractComponent, ShaderType, Reflect)]
pub struct FloaterSettings {
    /// constant drift movement for all floaters
    pub static_drift: Vec2,

    /// Spacing between floaters in the camera relative grid.
    /// Also controls how far the floaters will move from their spawn point.
    pub spawn_spacing: Vec2,

    /// Scale of the floater particles. Since floater layers have different
    /// distances from the camera, this is the base size used for perspective
    /// scaling. The two components define min and max size.
    ///
    /// Floater size is this value at gameplay layer (z camera distance of 1.0)
    pub particle_size: Vec2,

    /// How fast the floater particle size randomizes over time
    pub particle_size_variance_speed: f32,

    /// Controls the random movement speed
    pub movement_speed: f32,

    /// Controls the random movement scale, AKA movement range relative to the
    /// floater "cell" size or spawn spacing.
    pub movement_strength: f32,

    /// Width of a single floater sprite in the spritesheet in pixels, used for UVs
    pub sprite_width: u32,

    /// Width of the whole spritesheet in pixels, used for UVs
    pub spritesheet_width: u32,

    /// Index of the floater sprite in the spritesheet
    pub sprite_index: u32,
}

impl Default for FloaterSettings {
    fn default() -> Self {
        Self {
            static_drift: Vec2::new(8.0, -4.0),
            spawn_spacing: Vec2::splat(35.0),
            particle_size: Vec2::new(5.0, 8.0),
            particle_size_variance_speed: 0.1,
            movement_speed: 0.1,
            movement_strength: 0.6,
            sprite_width: 256,
            spritesheet_width: 256,
            sprite_index: 0,
        }
    }
}

#[derive(Default)]
struct FloaterPostProcessNode<const BACKGROUND: bool>;

/// Render label for drawing background floater particles
#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub(crate) struct FloaterBgRenderLabel;

/// Render label for drawing foreground floater particles
#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub(crate) struct FloaterFgRenderLabel;

impl<const BACKGROUND: bool> ViewNode for FloaterPostProcessNode<BACKGROUND> {
    type ViewQuery = (
        Read<ViewTarget>,
        Read<ViewUniformOffset>,
        Read<ViewDepthTexture>,
        Read<DynamicUniformIndex<FloaterSettings>>,
    );

    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (
            view_target,
            view_uniform_offset,
            view_depth_texture,
            floater_settings_uniform_index,
        ): QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let postprocess_pipeline = world.resource::<FloaterPipeline>();

        let pipeline_cache = world.resource::<PipelineCache>();
        let view_uniforms = world.resource::<ViewUniforms>();
        let globals_buffer = world.resource::<GlobalsBuffer>();
        let render_assets = world.resource::<RenderAssets<Image>>();

        let (
            Some(pipeline),
            Some(view_uniforms_binding),
            Some(globals_buffer_binding),
        ) = (
            pipeline_cache
                .get_render_pipeline(postprocess_pipeline.render_pipeline_id),
            view_uniforms.uniforms.binding(),
            globals_buffer.buffer.binding(),
        )
        else {
            return Ok(());
        };

        let settings_uniforms =
            world.resource::<ComponentUniforms<FloaterSettings>>();
        let Some(settings_binding) = settings_uniforms.uniforms().binding()
        else {
            return Ok(());
        };

        let Some(gpu_image) =
            render_assets.get(postprocess_pipeline.floater_image.id())
        else {
            return Ok(());
        };

        let depth_stencil_attachment =
            Some(view_depth_texture.get_attachment(StoreOp::Store));

        let view_bind_group = render_context.render_device().create_bind_group(
            "floater_post_process_view_bind_group",
            &postprocess_pipeline.layout,
            &BindGroupEntries::sequential((
                view_uniforms_binding,
                globals_buffer_binding,
                settings_binding.clone(),
                postprocess_pipeline.buffer.as_entire_binding(),
                &gpu_image.texture_view,
                &gpu_image.sampler,
            )),
        );

        let mut render_pass =
            render_context.begin_tracked_render_pass(RenderPassDescriptor {
                label: Some("floater_post_process_pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: view_target.main_texture_view(),
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Load,
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment,
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

        // Selectively draw either foreground or background floaters, depending on which
        // pass it is. Foreground is one layer, background is the rest.
        if BACKGROUND {
            render_pass.draw(
                0..6,
                FLOATER_BUFFER_SIZE as u32
                    ..(FLOATER_BUFFER_SIZE * FLOATER_BUFFER_LAYERS) as u32,
            )
        } else {
            render_pass.draw(0..6, 0..FLOATER_BUFFER_SIZE as u32);
        }

        Ok(())
    }
}

#[derive(Resource)]
struct FloaterPipeline {
    layout: BindGroupLayout,
    buffer: Buffer,
    floater_image: Handle<Image>,
    prepass_pipeline_id: CachedComputePipelineId,
    render_pipeline_id: CachedRenderPipelineId,
}

impl FromWorld for FloaterPipeline {
    fn from_world(world: &mut World) -> Self {
        let render_device = world.resource::<RenderDevice>();

        let buffer = render_device.create_buffer(&BufferDescriptor {
            label: Some("floater_buffer"),
            size: FloaterBuffer::SHADER_SIZE.into(),
            usage: BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        let floater_image =
            world.resource::<AssetServer>().load(FLOATER_TEXTURE_FILE);

        // For simplicity's sake, using the same bind layout between all stages.
        let layout = render_device.create_bind_group_layout(
            "floater_postprocess_bind_group_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT
                    | ShaderStages::COMPUTE
                    | ShaderStages::VERTEX,
                (
                    uniform_buffer::<ViewUniform>(true),
                    uniform_buffer::<GlobalsUniform>(false),
                    uniform_buffer::<FloaterSettings>(true),
                    storage_buffer::<FloaterBuffer>(false),
                    texture_2d(TextureSampleType::Float { filterable: false }),
                    sampler(SamplerBindingType::NonFiltering),
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
                vertex: VertexState {
                    shader: render_shader.clone(),
                    shader_defs: vec![],
                    entry_point: "floater_vertex".into(),
                    buffers: vec![],
                },
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
                depth_stencil: Some(DepthStencilState {
                    format: CORE_3D_DEPTH_FORMAT,
                    depth_write_enabled: false,
                    depth_compare: CompareFunction::GreaterEqual,
                    stencil: StencilState::default(),
                    bias: DepthBiasState::default(),
                }),
                multisample: MultisampleState::default(),
                push_constant_ranges: vec![],
            });

        Self {
            layout,
            buffer,
            floater_image,
            prepass_pipeline_id,
            render_pipeline_id,
        }
    }
}

#[derive(Default)]
struct FloaterPrepassNode;

/// Render label for the floater prepass (compute)
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
        let postprocess_pipeline = world.resource::<FloaterPipeline>();
        let pipeline_cache = world.resource::<PipelineCache>();
        let view_uniforms = world.resource::<ViewUniforms>();
        let globals_buffer = world.resource::<GlobalsBuffer>();
        // We don't need to use the actual floater image here
        let fallback_image = world.resource::<FallbackImageZero>();

        let (
            Some(pipeline),
            Some(view_uniforms_binding),
            Some(globals_buffer_binding),
        ) = (
            pipeline_cache
                .get_compute_pipeline(postprocess_pipeline.prepass_pipeline_id),
            view_uniforms.uniforms.binding(),
            globals_buffer.buffer.binding(),
        )
        else {
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
            &postprocess_pipeline.layout,
            &BindGroupEntries::sequential((
                view_uniforms_binding,
                globals_buffer_binding,
                settings_binding.clone(),
                postprocess_pipeline.buffer.as_entire_binding(),
                &fallback_image.texture_view,
                &fallback_image.sampler,
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

        // Workgroup size already matches the grid size on the shader side, we just
        // dispatch the layer amount.
        command_encoder.dispatch_workgroups(1, 1, FLOATER_BUFFER_LAYERS as u32);

        Ok(())
    }
}
