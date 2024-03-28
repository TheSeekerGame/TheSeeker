use crate::camera::MainCamera;
use crate::game::player::PlayerGent;
use crate::parallax::Parallax;
use bevy::core_pipeline::core_2d;
use bevy::core_pipeline::fullscreen_vertex_shader::fullscreen_shader_vertex_state;
use bevy::ecs::query::QueryItem;
use bevy::prelude::*;
use bevy::render::extract_component::{
    ComponentUniforms, ExtractComponent, ExtractComponentPlugin, UniformComponentPlugin,
};
use bevy::render::render_graph::{
    NodeRunError, RenderGraphApp, RenderGraphContext, ViewNode, ViewNodeRunner, RenderLabel
};
use bevy::render::render_resource::{
    BindGroupEntries, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
    BindingType, CachedRenderPipelineId, ColorTargetState, ColorWrites, FragmentState,
    MultisampleState, Operations, PipelineCache, PrimitiveState, RenderPassColorAttachment,
    RenderPassDescriptor, RenderPipelineDescriptor, Sampler, SamplerBindingType, SamplerDescriptor,
    ShaderStages, ShaderType, TextureFormat, TextureSampleType, TextureViewDimension,
};
use bevy::render::renderer::{RenderContext, RenderDevice};
use bevy::render::texture::BevyDefault;
use bevy::render::view::ViewTarget;
use bevy::render::RenderApp;
use glam::FloatExt;
use std::f32::consts::PI;

/// To use this plugin add it to your app, and make sure the [`DarknessSettings`] component is added
/// to the camera:
/// ```
/// commands.spawn((
///     // Whatever your camera bundle is
///     Camera3dBundle {
///         transform: Transform::from_translation(Vec3::new(0.0, 0.0, 5.0))
///             .looking_at(Vec3::default(), Vec3::Y),
///         camera_3d: Camera3d {
///             clear_color: ClearColorConfig::Custom(Color::WHITE),
///             ..default()
///         },
///         ..default()
///     },
///     // Add the setting to the camera.
///     // This component is also used to determine on which camera to run the post processing effect.
///    DarknessSettings {
///        bg_light_level: 1.0,
///        lantern_position: Default::default(),
///        lantern: 1.0,
///        lantern_color: Vec3::new(0.965, 0.882, 0.678),
///        bg_light_color: Vec3::new(0.761, 0.773, 0.8),
///    },
/// ));
/// ```
pub struct DarknessPlugin;

impl Plugin for DarknessPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, darkness_dynamics);
        app.add_systems(
            Update,
            darkness_parallax.after(darkness_dynamics),
        );
        app.add_plugins((
            // The settings will be a component that lives in the main world but will
            // be extracted to the render world every frame.
            // This makes it possible to control the effect from the main world.
            // This plugin will take care of extracting it automatically.
            // It's important to derive [`ExtractComponent`] on [`PostProcessingSettings`]
            // for this plugin to work correctly.
            ExtractComponentPlugin::<DarknessSettings>::default(),
            // The settings will also be the data used in the shader.
            // This plugin will prepare the component for the GPU by creating a uniform buffer
            // and writing the data to that buffer every frame.
            UniformComponentPlugin::<DarknessSettings>::default(),
        ));

        // We need to get the render app from the main app
        let Ok(render_app) = app.get_sub_app_mut(RenderApp) else {
            warn!(
                "Failed to get render app for DarknessPlugin {}:{}",
                file!(),
                line!()
            );
            return;
        };

        render_app
            // Bevy's renderer uses a render graph which is a collection of nodes in a directed acyclic graph.
            // It currently runs on each view/camera and executes each node in the specified order.
            // It will make sure that any node that needs a dependency from another node
            // only runs when that dependency is done.
            //
            // Each node can execute arbitrary work, but it generally runs at least one render pass.
            // A node only has access to the render world, so if you need data from the main world
            // you need to extract it manually or with the plugin like above.
            // Add a [`Node`] to the [`RenderGraph`]
            // The Node needs to impl FromWorld
            //
            // The [`ViewNodeRunner`] is a special [`Node`] that will automatically run the node for each view
            // matching the [`ViewQuery`]
            .add_render_graph_node::<ViewNodeRunner<DarknessPostProcessNode>>(
                // Specify the name of the graph, in this case we want the graph for 3d
                core_2d::graph::Core2d,
                // It also needs the name of the node
                DarknessPostProcessLabel,
                // DarknessPostProcessNode::NAME,
            )
            .add_render_graph_edges(
                core_2d::graph::Core2d,
                // core_2d::graph::NAME,
                // Specify the node ordering.
                // This will automatically create all required node edges to enforce the given ordering.
                // Currently runs after ToneMapping, which seems to give best appearance... might need to revisit
                // to handle bloom/ other glowing objects.
                (
                    core_2d::graph::Node2d::Tonemapping,
                    // core_2d::graph::node::TONEMAPPING,
                    DarknessPostProcessLabel,
                    // DarknessPostProcessNode::NAME,
                    core_2d::graph::Node2d::EndMainPassPostProcessing,
                    // core_2d::graph::node::END_MAIN_PASS_POST_PROCESSING,
                ),
            );
    }

    fn finish(&self, app: &mut App) {
        // We need to get the render app from the main app
        let Ok(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            // Initialize the pipeline
            .init_resource::<DarknessPostProcessPipeline>();
    }
}

#[derive(Component, Default, Clone, Copy, ExtractComponent, ShaderType)]
/// Darkness Post Process Settings
pub struct DarknessSettings {
    /// 0.0 is pitch black, and 1.0 is normal brightness
    pub bg_light_level: f32,
    /// Position is currently relative to the camera center
    pub lantern_position: Vec2,
    /// Output of the light source; 0.0 is off and 1.0 is normal brightness.
    pub lantern: f32,
    /// RGB
    pub lantern_color: Vec3,
    /// RGB
    pub bg_light_color: Vec3,
    // WebGL2 structs must be 16 byte aligned.
    #[cfg(feature = "webgl2")]
    _webgl2_padding: Vec2,
}

/// Changes the intensity over time to show that the effect is controlled from the main world
fn darkness_dynamics(
    mut settings: Query<&mut DarknessSettings>,
    time: Res<Time>,
    camera: Query<&Transform, (With<MainCamera>, Without<PlayerGent>)>,
    player: Query<&Transform, (With<PlayerGent>, Without<MainCamera>)>,
) {
    let mut offest = Vec2::new(0.0, 0.0);
    // make sure the lantern is centered on the player even if the camera isn't
    if let Ok(cam_transform) = camera.get_single() {
        if let Ok(player_transform) = player.get_single() {
            offest = player_transform.translation.xy() - cam_transform.translation.xy()
        };
    };

    for mut setting in &mut settings {
        let seconds_per_day_cycle = 30.0;

        let mut intensity = (time.elapsed_seconds() * PI / seconds_per_day_cycle).sin();
        // remaps sines normal output to the 0-1 range
        let intensity = intensity * 0.5 + 0.5;

        // uses the lerp trick to easily add smooth transition; maybe use different
        // curve/tweening in the future.
        if intensity < 0.3 {
            setting.lantern = setting.lantern.lerp(1.0, time.delta_seconds() * 0.9);
        } else if intensity > 0.3 {
            setting.lantern = setting.lantern.lerp(0.0, time.delta_seconds() * 0.9);
        }

        // Set the intensity.
        // This will then be extracted to the render world and uploaded to the gpu automatically by the [`UniformComponentPlugin`]
        setting.bg_light_level = intensity;
        setting.lantern_position = offest;
    }
}

/// Applies a dark color tint to parallaxed backrounds, to account for the fact that
/// they are "farther away" and should be dimmer under lantern light
///
/// Currently only applies to [`bevy_ecs_tilemap::tiles::TileColor`]'s
/// Might need modifications if we draw other things in the backround and
/// want it to work with darkness properly.
fn darkness_parallax(
    settings: Query<&DarknessSettings>,
    parallaxed_bgs: Query<(Entity, &Parallax)>,
    children: Query<&Children>,
    mut sprites: Query<&mut bevy_ecs_tilemap::tiles::TileColor>,
) {
    let Some(settings) = settings.iter().next() else {
        return;
    };
    for (entity, paralax) in parallaxed_bgs.iter() {
        let light_multiplier = 1.0 / (paralax.depth * 1.25).powi(2);
        let final_mult = light_multiplier.lerp(1.0, settings.bg_light_level);
        let color = Color::rgb(1.0, 1.0, 1.0) * final_mult;
        for descendant in children.iter_descendants(entity) {
            if let Ok(mut sprite) = sprites.get_mut(descendant) {
                sprite.0 = color;
            };
        }
    }
}

// Below is all boilerplate for setting up the post process.

// The post process node used for the render graph
#[derive(Default)]
struct DarknessPostProcessNode;
// impl DarknessPostProcessNode {
//     pub const NAME: &'static str = "darkness_post_process";
// }
#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct DarknessPostProcessLabel;

// The ViewNode trait is required by the ViewNodeRunner
impl ViewNode for DarknessPostProcessNode {
    // The node needs a query to gather data from the ECS in order to do its rendering,
    // but it's not a normal system so we need to define it manually.
    //
    // This query will only run on the view entity
    type ViewQuery = &'static ViewTarget;

    // Runs the node logic
    // This is where you encode draw commands.
    //
    // This will run on every view on which the graph is running.
    // If you don't want your effect to run on every camera,
    // you'll need to make sure you have a marker component as part of [`ViewQuery`]
    // to identify which camera(s) should run the effect.
    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        view_target: QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        // Get the pipeline resource that contains the global data we need
        // to create the render pipeline
        let post_process_pipeline = world.resource::<DarknessPostProcessPipeline>();

        // The pipeline cache is a cache of all previously created pipelines.
        // It is required to avoid creating a new pipeline each frame,
        // which is expensive due to shader compilation.
        let pipeline_cache = world.resource::<PipelineCache>();

        // Get the pipeline from the cache
        let Some(pipeline) = pipeline_cache.get_render_pipeline(post_process_pipeline.pipeline_id)
        else {
            return Ok(());
        };

        // Get the settings uniform binding
        let settings_uniforms = world.resource::<ComponentUniforms<DarknessSettings>>();
        let Some(settings_binding) = settings_uniforms.uniforms().binding() else {
            return Ok(());
        };

        // This will start a new "post process write", obtaining two texture
        // views from the view target - a `source` and a `destination`.
        // `source` is the "current" main texture and you _must_ write into
        // `destination` because calling `post_process_write()` on the
        // [`ViewTarget`] will internally flip the [`ViewTarget`]'s main
        // texture to the `destination` texture. Failing to do so will cause
        // the current main texture information to be lost.
        let post_process = view_target.post_process_write();

        // The bind_group gets created each frame.
        //
        // Normally, you would create a bind_group in the Queue set,
        // but this doesn't work with the post_process_write().
        // The reason it doesn't work is because each post_process_write will alternate the source/destination.
        // The only way to have the correct source/destination for the bind_group
        // is to make sure you get it during the node execution.
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

        // Begin the render pass
        let mut render_pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
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

        // This is mostly just wgpu boilerplate for drawing a fullscreen triangle,
        // using the pipeline/bind_group created above
        render_pass.set_render_pipeline(pipeline);
        render_pass.set_bind_group(0, &bind_group, &[]);
        render_pass.draw(0..3, 0..1);

        Ok(())
    }
}

// This contains global data used by the render pipeline. This will be created once on startup.
#[derive(Resource)]
struct DarknessPostProcessPipeline {
    layout: BindGroupLayout,
    sampler: Sampler,
    pipeline_id: CachedRenderPipelineId,
}

impl FromWorld for DarknessPostProcessPipeline {
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
                        min_binding_size: Some(DarknessSettings::min_size()),
                    },
                    count: None,
                },
            ],
        );

        // We can create the sampler here since it won't change at runtime and doesn't depend on the view
        let sampler = render_device.create_sampler(&SamplerDescriptor::default());

        // Get the shader handle
        let shader = world
            .resource::<AssetServer>()
            .load("shaders/darkness_post_processing.wgsl");

        let pipeline_id = world
            .resource_mut::<PipelineCache>()
            // This will add the pipeline to the cache and queue it's creation
            .queue_render_pipeline(RenderPipelineDescriptor {
                label: Some("darkness_post_process_pipeline".into()),
                layout: vec![layout.clone()],
                // This will setup a fullscreen triangle for the vertex state
                vertex: fullscreen_shader_vertex_state(),
                fragment: Some(FragmentState {
                    shader,
                    shader_defs: vec![],
                    // Make sure this matches the entry point of your shader.
                    // It can be anything as long as it matches here and in the shader.
                    entry_point: "fragment".into(),
                    targets: vec![Some(ColorTargetState {
                        // Since the post process is running after tone mapping/bloom, the input texture
                        // format is Rgba16Float instead of Rgba8UnormSrgb (TextureFormat::bevy_default())
                        format: TextureFormat::Rgba16Float,
                        blend: None,
                        write_mask: ColorWrites::ALL,
                    })],
                }),
                // All of the following properties are not important for this effect so just use the default values.
                // This struct doesn't have the Default trait implemented because not all field can have a default value.
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
