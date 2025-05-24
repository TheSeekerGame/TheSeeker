mod texture_array;

use bevy::asset::load_internal_asset;
use bevy::core_pipeline::core_2d::{Transparent2d, CORE_2D_DEPTH_FORMAT};
use bevy::core_pipeline::core_3d::{Opaque3d, Transparent3d, CORE_3D_DEPTH_FORMAT};
use bevy::core_pipeline::prepass::Opaque3dPrepass;
use bevy::core_pipeline::tonemapping::{
    get_lut_bind_group_layout_entries, DebandDither, Tonemapping,
};
use bevy::ecs::entity::EntityHashMap;
use bevy::ecs::query::{QueryItem, ROQueryItem};
use bevy::ecs::system::lifetimeless::{Read, SRes};
use bevy::ecs::system::{SystemParamItem, SystemState};
use bevy::math::{Affine3, FloatOrd};
use bevy::prelude::*;
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin};
use bevy::render::mesh::{PrimitiveTopology};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_graph::{Node, NodeRunError, RenderGraphContext, SlotInfo};
use bevy::render::render_phase::{
    AddRenderCommand, DrawFunctions, PhaseItem, PhaseItemExtraIndex, RenderCommand, RenderCommandResult, SetItemPipeline, TrackedRenderPass, ViewBinnedRenderPhases, ViewSortedRenderPhases
};
use bevy::render::render_resource::binding_types::{sampler, texture_2d, uniform_buffer};
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue};
use bevy::render::sync_world::{MainEntity, RenderEntity};
use bevy::render::texture::{
    DefaultImageSampler, GpuImage
};
use bevy::render::view::{
    ExtractedView, NoFrustumCulling, ViewTarget, ViewUniform, ViewUniformOffset, ViewUniforms,
    VisibleEntities,
};
use bevy::render::{Extract, Render, RenderApp, RenderSet};
use bevy::utils::hashbrown::hash_map::Entry;
use bevy::utils::HashMap;
use binding_types::texture_2d_array;
use std::borrow::Cow;
use std::thread::sleep;
use std::time::Duration;

use crate::render::texture_array::{create_texture_array, update_texture_array};
use crate::{map::*, tiles::*};

#[cfg(feature = "use_3d_pipeline")]
type Transparent = Transparent3d;
#[cfg(not(feature = "use_3d_pipeline"))]
type Transparent = Transparent2d;

pub struct TileMapRendererPlugin;
impl Plugin for TileMapRendererPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, update_tilemap_chunks.run_if(has_dirty_tiles));
        load_internal_asset!(
            app,
            TILE_MAP_SHADER_HANDLE,
            "tile_map_render.wgsl",
            Shader::from_wgsl
        );

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            warn!("Failed to get render app for tilemap_renderer");
            return;
        };

        render_app
            .init_resource::<SpecializedRenderPipelines<TilemapPipeline>>()
            .add_render_command::<Transparent, DrawTilemap>();

        #[cfg(feature = "background_tiles")]
        render_app
            .add_render_command::<Opaque3d, DrawTilemap>();

        render_app
            .add_systems(ExtractSchedule, extract_tilemaps)
            .add_systems(ExtractSchedule, extract_tilemap_textures)
            .add_systems(
                Render,
                (
                    prepare_tilemaps.in_set(RenderSet::Prepare),
                    queue_tilemaps.in_set(RenderSet::Queue),
                ),
            );
    }

    fn finish(&self, app: &mut App) {
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.init_resource::<TilemapPipeline>();
        }
    }
}

pub const TILE_MAP_SHADER_HANDLE: Handle<Shader> = Handle::weak_from_u128(11575316803102594335);

/// GPU representation of TilemapChunks
struct GpuTilemapChunks {
    texture: Texture,
    texture_view: TextureView,
}

/// Minimal representation needed for rendering.
#[derive(Component)]
struct ExtractedTilemap {
    transform: GlobalTransform,
    chunks: TilemapChunks,
    tile_size: TilemapTileSize,
    grid_size: TilemapGridSize,
    texture: Option<ExtractedTileset>,
}

pub(crate) struct ExtractedTileset {
    pub tilemap_id: TilemapId,
    pub texture_size: TilemapTextureSize,
    pub tile_size: TilemapTileSize,
    pub tile_spacing: TilemapSpacing,
    pub tile_count: u32,
    pub texture: ExtractedTilesetTexture,
    pub filtering: FilterMode,
    pub format: TextureFormat,
    pub bg_uploaded: bool,
}

impl ExtractedTileset {
    pub fn new(
        tilemap_entity: Entity,
        texture: TilesetTexture,
        tile_size: TilemapTileSize,
        tile_spacing: TilemapSpacing,
        filtering: FilterMode,
        image_assets: &Res<Assets<Image>>,
    ) -> ExtractedTileset {
        let (texture, tile_count, texture_size, format) = match &texture {
            TilesetTexture::Single(handle) => {
                let image = image_assets.get(handle).expect(
                    "Expected image to have finished loading if \
                    it is being extracted as a texture!",
                );
                let texture_size: TilemapTextureSize = image.size_f32().into();
                let tile_count_x = ((texture_size.x) / (tile_size.x + tile_spacing.x)).floor();
                let tile_count_y = ((texture_size.y) / (tile_size.y + tile_spacing.y)).floor();
                (
                    ExtractedTilesetTexture::Single(image.clone()),
                    (tile_count_x * tile_count_y) as u32,
                    texture_size,
                    image.texture_descriptor.format,
                )
            }
            TilesetTexture::Vector(handles) => {
                let mut images = vec![];
                for handle in handles {
                    let image = image_assets.get(handle).expect(
                        "Expected image to have finished loading if \
                        it is being extracted as a texture!",
                    );
                    let this_tile_size: TilemapTileSize = image.size_f32().into();
                    if this_tile_size != tile_size {
                        panic!(
                            "Expected all provided image assets to have size {tile_size:?}, \
                                    but found image with size: {this_tile_size:?}",
                        );
                    }
                }
                let first_format = image_assets
                    .get(handles.first().unwrap())
                    .unwrap()
                    .texture_descriptor
                    .format;

                for handle in handles {
                    let image = image_assets.get(handle).unwrap();
                    if image.texture_descriptor.format != first_format {
                        panic!("Expected all provided image assets to have the same format of: {:?} but found image with format: {:?}", first_format, image.texture_descriptor.format);
                    }
                    images.push(image.clone())
                }

                (
                    ExtractedTilesetTexture::Vector(images),
                    handles.len() as u32,
                    tile_size.into(),
                    first_format,
                )
            }
            TilesetTexture::TextureContainer(image_handle) => {
                let image = image_assets.get(image_handle).expect(
                    "Expected image to have finished loading if \
                        it is being extracted as a texture!",
                );
                let tile_size: TilemapTileSize = image.size_f32().into();
                (
                    ExtractedTilesetTexture::TextureContainer(image.clone()),
                    image.texture_descriptor.array_layer_count(),
                    tile_size.into(),
                    image.texture_descriptor.format,
                )
            }
        };

        ExtractedTileset {
            tilemap_id: TilemapId(tilemap_entity),
            texture,
            tile_spacing,
            filtering,
            tile_count,
            texture_size,
            tile_size,
            format,
            bg_uploaded: false,
        }
    }
}

/// The raw data for the Texture
#[derive(Clone, Debug)]
pub enum ExtractedTilesetTexture {
    /// All textures for tiles are inside a single image asset directly next to each other
    Single(Image),
    /// Each tile's texture has its own image asset (each asset must have the same size), so there
    /// is a vector of image assets.
    ///
    /// Each image should have the same size, identical to the provided `TilemapTileSize`. If this
    /// is not the case, a panic will be thrown during the verification when images are being
    /// extracted to the render world.
    Vector(Vec<Image>),
    /// The tiles are provided as array layers inside a KTX2 or DDS container.
    TextureContainer(Image),
}

#[derive(Component)]
struct GpuTilemap {
    gpu_chunks: GpuTilemapChunks,
    tilemap_uniform: UniformBuffer<TilemapInfo>,
    view_bind_group: BindGroup,
    tilemap_bind_group: BindGroup,
    tileset_bind_group: Option<BindGroup>,
}

#[derive(ShaderType, Clone)]
struct TilemapInfo {
    transform: [Vec4; 3],
    tile_size: Vec2,
    grid_size: Vec2,
    n_tiles_per_chunk: UVec2,
    n_chunks: UVec2,
}

#[derive(Resource)]
struct TilemapPipeline {
    view_layout: BindGroupLayout,
    tilemap_layout: BindGroupLayout,
    tiles_layout: BindGroupLayout,
}

// Initialize the pipelines data
impl FromWorld for TilemapPipeline {
    fn from_world(world: &mut World) -> Self {
        let mut system_state: SystemState<(
            Res<RenderDevice>,
            Res<DefaultImageSampler>,
            Res<RenderQueue>,
        )> = SystemState::new(world);
        let (render_device, default_sampler, render_queue) = system_state.get_mut(world);

        let view_layout = render_device.create_bind_group_layout(
            "tilemap_view_layout",
            &BindGroupLayoutEntries::single(
                ShaderStages::VERTEX_FRAGMENT,
                uniform_buffer::<ViewUniform>(true),
            ),
        );
        let tilemap_layout = render_device.create_bind_group_layout(
            "tilemap_data_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::VERTEX_FRAGMENT,
                (
                    uniform_buffer::<TilemapInfo>(false),
                    texture_2d_array(TextureSampleType::Uint),
                ),
            ),
        );
        let tiles_layout = render_device.create_bind_group_layout(
            "tilemap_tiledata_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (
                    texture_2d_array(TextureSampleType::Float { filterable: true }),
                    sampler(SamplerBindingType::Filtering),
                ),
            ),
        );

        TilemapPipeline {
            view_layout,
            tilemap_layout,
            tiles_layout,
        }
    }
}

// Specialize the pipeline with size/runtime configurable data. I think.
#[derive(Clone, Hash, PartialEq, Eq)]
pub struct TilemapPipelineKey {
    msaa_samples: u8,
    has_tiles_texture: bool,
    hdr: bool,
}

impl SpecializedRenderPipeline for TilemapPipeline {
    type Key = TilemapPipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        let mut shader_defs: Vec<ShaderDefVal> = vec![];
        let mut layout: Vec<BindGroupLayout> =
            vec![self.view_layout.clone(), self.tilemap_layout.clone()];

        if key.has_tiles_texture {
            shader_defs.push("TILEMAP_HAS_TILE_TEXTURE".into());
            layout.push(self.tiles_layout.clone())
        }

        let format = match key.hdr {
            true => ViewTarget::TEXTURE_FORMAT_HDR,
            false => TextureFormat::bevy_default(),
        };

        #[cfg(feature = "use_3d_pipeline")]
        let depth =  Some(DepthStencilState {
            format: CORE_3D_DEPTH_FORMAT,
            depth_write_enabled: true,
            depth_compare: CompareFunction::GreaterEqual,
            stencil: StencilState {
                front: StencilFaceState::IGNORE,
                back: StencilFaceState::IGNORE,
                read_mask: 0,
                write_mask: 0,
            },
            bias: DepthBiasState {
                constant: 0,
                slope_scale: 0.0,
                clamp: 0.0,
            },
        });
        #[cfg(not(feature = "use_3d_pipeline"))]
        let depth = Some(DepthStencilState {
            format: CORE_2D_DEPTH_FORMAT,
            depth_write_enabled: true,
            depth_compare: CompareFunction::GreaterEqual,
            stencil: StencilState {
                front: StencilFaceState::IGNORE,
                back: StencilFaceState::IGNORE,
                read_mask: 0,
                write_mask: 0,
            },
            bias: DepthBiasState {
                constant: 0,
                slope_scale: 0.0,
                clamp: 0.0,
            },
        });

        RenderPipelineDescriptor {
            zero_initialize_workgroup_memory: false,
            label: Some("tilemap_pipeline".into()),
            layout,
            push_constant_ranges: vec![],
            vertex: VertexState {
                shader: TILE_MAP_SHADER_HANDLE,
                shader_defs: shader_defs.clone(),
                entry_point: "vertex".into(),
                buffers: vec![],
            },
            fragment: Some(FragmentState {
                shader: TILE_MAP_SHADER_HANDLE,
                shader_defs: shader_defs,
                entry_point: "fragment".into(),
                targets: vec![Some(ColorTargetState {
                    // todo: make this support HDR at some point?
                    format: format,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState {
                front_face: FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: PolygonMode::Fill,
                conservative: false,
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
            },
            depth_stencil: depth,
            multisample: MultisampleState {
                count: key.msaa_samples as u32,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
        }
    }
}

/// Run-criteria that returns `true` when any tile entity had a relevant component **added** or
/// **changed** this frame.  When `false`, the expensive `update_tilemap_chunks` system is skipped
/// entirely, eliminating per-frame overhead for static maps.
fn has_dirty_tiles(
    q_dirty: Query<(), Or<(
        Added<TileTextureIndex>,
        Changed<TileTextureIndex>,
        Changed<TileColor>,
        Changed<TileFlip>,
        Changed<TileVisible>,
    )>>,
) -> bool {
    !q_dirty.is_empty()
}

fn update_tilemap_chunks(
    mut q_map: Query<(&mut TilemapChunks, &TilemapSize)>,
    q_tile: Query<(
        &TilemapId,
        &TilePos,
        &TileTextureIndex,
        &TileColor,
        &TileFlip,
        &TileVisible,
    ), Or<(
        Added<TileTextureIndex>,
        Changed<TileTextureIndex>,
        Changed<TileColor>,
        Changed<TileFlip>,
        Changed<TileVisible>,
    )>>,
) {
    // first, init things if necessary
    for (mut chunks, size) in &mut q_map {
        if !chunks.is_added() {
            continue;
        }
        // TODO: don't hardcode 2048x2048, this can be optimized
        // if map size is >2048, divide into roughly equal chunks
        // that have minimal wastage when padded to 64x64
        chunks.init((*size).into(), UVec2::new(2048, 2048));
    }
    // now, update any changed tiles
    // memoize tilemap lookup for perf
    let mut last_map_id = None;
    let mut last_map = None;
    for (tid, pos, index, color, flip, vis) in &q_tile {
        if last_map_id != Some(*tid) {
            last_map_id = None;
            last_map = None;
            let Ok(map) = q_map.get_mut(tid.0) else {
                continue;
            };
            last_map_id = Some(*tid);
            last_map = Some(map);
        }
        let Some((ref mut chunks, _)) = last_map else {
            unreachable!()
        };
        // Because the query filter only yields tiles whose components were **added** or **changed**,
        // we can unconditionally write the new data to the chunk without further checks.
        chunks.set_tiledata_at(pos, index, color, flip, vis);
    }
}

fn extract_tilemaps(
    tilemap_query: Extract<
        Query<(
            RenderEntity,
            &ViewVisibility,
            &GlobalTransform,
            &TilemapChunks,
            &TilemapTileSize,
            &TilemapGridSize,
        )>,
    >,
    mut render_query: Query<&mut ExtractedTilemap>,
    mut removed: Extract<RemovedComponents<TilemapChunks>>,
    mut commands: Commands,
) {
    for removed in removed.read() {
        commands.entity(removed).remove::<ExtractedTilemap>();
    }
    for (entity, view_visibility, transform, chunks, tile_size, grid_size) in tilemap_query.iter() {
        // TODO: in order for this to actually work, we need a system in the
        // main world that knows how to do frustum culling for tilemaps
        if !view_visibility.get() {
            // continue;
        }

        if let Ok(mut extracted) = render_query.get_mut(entity) {
            // Transfer all "dirty" parts of the chunks here.
            extracted.transform = transform.clone();
            extracted.chunks.copy_dirty(chunks);
        } else {
            // otherwise copy all chunks, since it's the first time.
            commands.entity(entity).insert(ExtractedTilemap {
                transform: transform.clone(),
                chunks: chunks.clone(),
                tile_size: *tile_size,
                grid_size: *grid_size,
                texture: None,
            });
        }
    }
}

fn extract_tilemap_textures(
    tilemap_query: Extract<Query<(RenderEntity, &TilemapTileSize, &TilemapSpacing, &TilesetTexture)>>,
    mut render_query: Query<&mut ExtractedTilemap>,
    images: Extract<Res<Assets<Image>>>,
) {
    for (entity, size, spacing, texture) in tilemap_query.iter() {
        let Ok(mut tilemap) = render_query.get_mut(entity) else {
            return;
        };
        if tilemap.texture.is_none() && texture.verify_ready(&images) {
            tilemap.texture = Some(ExtractedTileset::new(
                entity,
                texture.clone_weak(),
                *size,
                *spacing,
                FilterMode::Nearest,
                &images,
            ))
        }
    }
}

fn prepare_tilemaps(
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    mut q_tilemap: Query<(Entity, &mut ExtractedTilemap, Option<&mut GpuTilemap>)>,
    tilemap_pipeline: Res<TilemapPipeline>,
    view_uniforms: Res<ViewUniforms>,
    mut commands: Commands,
) {
    for (e, mut extracted, mut prepared) in q_tilemap.iter_mut() {
        if let Some(tileset) = &mut extracted.texture {
            tileset.bg_uploaded = true;
        }

        if let Some(mut prepared) = prepared {
            // Texture already exists in GPU memory.
            // Update it with any dirty data!
            prepared.gpu_chunks.copy_dirty(&queue, &extracted.chunks);
            // Tilemap Uniform already exists,
            // but we need to update the data in the buffer.
            prepared.tilemap_uniform.set(TilemapInfo {
                transform: Affine3::from(&extracted.transform.affine()).to_transpose(),
                tile_size: extracted.tile_size.into(),
                grid_size: extracted.grid_size.into(),
                n_tiles_per_chunk: extracted.chunks.chunk_size,
                n_chunks: extracted.chunks.n_chunks,
            });
            prepared.tilemap_uniform.write_buffer(&device, &queue);
            // Bind Groups already exist and don't need changing.

            if prepared.tileset_bind_group.is_none() && extracted.texture.is_some() {
                let tileset_bind_group = extracted.texture.as_ref().map(|texture| {
                    let texture_array = create_texture_array(&device, &queue, texture);
                    let bg = device.create_bind_group(
                        "tile_bind_group",
                        &tilemap_pipeline.tiles_layout,
                        &BindGroupEntries::sequential((
                            &texture_array.view,
                            &texture_array.sampler,
                        )),
                    );
                    update_texture_array(&device, &queue, &texture_array, &texture);
                    bg
                });
                prepared.tileset_bind_group = tileset_bind_group;
            }
        } else {
            let Some(view_binding) = view_uniforms.uniforms.binding() else {
                continue;
            };
            // Setup the GPU texture.
            let gpu_chunks = GpuTilemapChunks::new(&device, &extracted.chunks);
            gpu_chunks.copy_all(&queue, &extracted.chunks);
            extracted.chunks.clear_all_dirty_bitmaps();
            // Setup the tilemap uniform
            let tilemap_info = TilemapInfo {
                transform: Affine3::from(&extracted.transform.affine()).to_transpose(),
                tile_size: extracted.tile_size.into(),
                grid_size: extracted.grid_size.into(),
                n_tiles_per_chunk: extracted.chunks.chunk_size,
                n_chunks: extracted.chunks.n_chunks,
            };
            let mut tilemap_uniform = UniformBuffer::from(tilemap_info);
            tilemap_uniform.set_label(Some("tilemap_uniform"));
            tilemap_uniform.write_buffer(&device, &queue);
            // Setup the bind groups
            let view_bind_group = device.create_bind_group(
                "tilemap_view_bind_group",
                &tilemap_pipeline.view_layout,
                &BindGroupEntries::single(view_binding),
            );
            let tilemap_bind_group = device.create_bind_group(
                "tilemap_bind_group",
                &tilemap_pipeline.tilemap_layout,
                &BindGroupEntries::sequential((&tilemap_uniform, &gpu_chunks.texture_view)),
            );

            let tileset_bind_group = extracted.texture.as_ref().map(|texture| {
                let texture_array = create_texture_array(&device, &queue, texture);
                let bg = device.create_bind_group(
                    "tile_bind_group",
                    &tilemap_pipeline.tiles_layout,
                    &BindGroupEntries::sequential((&texture_array.view, &texture_array.sampler)),
                );
                update_texture_array(&device, &queue, &texture_array, &texture);
                bg
            });

            commands.entity(e).insert(
                GpuTilemap {
                    gpu_chunks,
                    tilemap_uniform,
                    view_bind_group,
                    tilemap_bind_group,
                    tileset_bind_group,
                },
            );
        }
    }
}

fn queue_tilemaps(
    draw_functions: Res<DrawFunctions<Transparent>>,
    op_draw_functions: Res<DrawFunctions<Opaque3d>>,
    q_tilemap: Query<(Entity, &MainEntity, &ExtractedTilemap)>,
    tilemap_pipeline: Res<TilemapPipeline>,
    mut pipelines: ResMut<SpecializedRenderPipelines<TilemapPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    mut transparent_render_phases: ResMut<ViewSortedRenderPhases<Transparent>>,
    mut opaque_render_phases: ResMut<ViewBinnedRenderPhases<Opaque3d>>,
    mut views: Query<(
        Entity,
        &VisibleEntities,
        &ExtractedView,
        &Msaa,
        Option<&Tonemapping>,
        Option<&DebandDither>,
    )>,
) {
    for (
        view_entity,
        visible_entities,
        view,
        msaa,
        tonemapping,
        dither,
    ) in &mut views
    {
        for (entity, main_entity, extracted_tilemap) in q_tilemap.iter() {
            // These items will be sorted by depth with other phase items
            let z = extracted_tilemap.transform.translation().z;
            let sort_key = FloatOrd(z);
            // let mut use_opaque = false;
            // #[cfg(feature = "background_tiles")]
            // {
            //     if z < -0.0 {
            //         use_opaque = true;
            //     }
            // }
            let pipeline = pipelines.specialize(
                &pipeline_cache,
                &tilemap_pipeline,
                TilemapPipelineKey {
                    msaa_samples: msaa.samples() as u8,
                    has_tiles_texture: extracted_tilemap.texture.is_some(),
                    hdr: view.hdr,
                },
            );

            /*let index = extracted_tilemap.unwrap_or(*entity).index();

            if !view_entities.contains(index as usize) {
                continue;
            }*/

            #[cfg(feature = "use_3d_pipeline")]
            {
                // if use_opaque {
                //     let opaque_phase = opaque_render_phases.get_mut(&view_entity).unwrap();
                //     opaque_phase.add(Opaque3d {
                //         draw_function: op_draw_functions.read().id::<DrawTilemap>(),
                //         entity: *entity,
                //         asset_id: Default::default(),
                //         batch_range: 0..1,
                //         dynamic_offset: None,
                //         pipeline,
                //     });
                // } else {
                    let transparent_phase = transparent_render_phases.get_mut(&view_entity).unwrap();
                    transparent_phase.add(Transparent {
                        distance: extracted_tilemap.transform.translation().z,
                        draw_function: draw_functions.read().id::<DrawTilemap>(),
                        pipeline,
                        entity: (entity, *main_entity),
                        batch_range: 0..1,
                        extra_index: PhaseItemExtraIndex(0),
                    });
                // }
            }
            #[cfg(not(feature = "use_3d_pipeline"))]
            {
                // Add the item to the render phase
                let transparent_phase = transparent_render_phases.get_mut(&view_entity).unwrap();
                transparent_phase.add(Transparent2d {
                    draw_function: draw_functions.read().id::<DrawTilemap>(),
                    pipeline,
                    entity: (entity, *main_entity),
                    sort_key,
                    // I think this needs to be at least 1
                    batch_range: 0..1,
                    extra_index: PhaseItemExtraIndex::NONE,
                });
            }
        }
    }
}

impl GpuTilemapChunks {
    fn new(device: &RenderDevice, chunks: &TilemapChunks) -> Self {
        let desc_texture = TextureDescriptor {
            label: Some("seek_ecs_tilemap_chunks"),
            size: chunks.texture_size(),
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rg32Uint,
            usage: TextureUsages::COPY_DST | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };
        let texture = device.create_texture(&desc_texture);
        let desc_view = TextureViewDescriptor {
            label: Some("seek_ecs_tilemap_chunks"),
            format: Some(TextureFormat::Rg32Uint),
            dimension: Some(TextureViewDimension::D2Array),
            aspect: TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
        };
        let texture_view = texture.create_view(&desc_view);
        Self {
            texture,
            texture_view,
        }
    }
    fn copy_all(&self, queue: &RenderQueue, chunks: &TilemapChunks) {
        for (z, chunk) in chunks.chunks.iter().enumerate() {
            let mut data_start = 0;
            for sc_y in (0..32).take(chunks.n_subchunks.y as usize) {
                for sc_x in (0..32).take(chunks.n_subchunks.x as usize) {
                    let data_end = data_start + TilemapChunks::SUBCHUNK_DATA_LEN;
                    let data = &chunk.data[data_start..data_end];
                    self.copy_subchunk_data(queue, sc_x, sc_y, z as u32, data);
                    data_start = data_end;
                }
            }
        }
    }
    fn copy_dirty(&self, queue: &RenderQueue, chunks: &TilemapChunks) {
        for (z, chunk) in chunks.chunks.iter().enumerate() {
            let mut data_start = 0;
            for (sc_y, row_bitmap) in chunk
                .dirty_bitmap
                .iter()
                .copied()
                .take(chunks.n_subchunks.y as usize)
                .enumerate()
            {
                for sc_x in (0..32).take(chunks.n_subchunks.x as usize) {
                    let data_end = data_start + TilemapChunks::SUBCHUNK_DATA_LEN;
                    if row_bitmap & (1 << sc_x) != 0 {
                        let data = &chunk.data[data_start..data_end];
                        self.copy_subchunk_data(queue, sc_x, sc_y as u32, z as u32, data);
                    }
                    data_start = data_end;
                }
            }
        }
    }
    fn copy_subchunk_data(&self, queue: &RenderQueue, x: u32, y: u32, z: u32, data: &[u8]) {
        let texture = ImageCopyTexture {
            texture: &self.texture,
            mip_level: 0,
            origin: Origin3d {
                x: x * 64,
                y: y * 64,
                z,
            },
            aspect: TextureAspect::All,
        };
        let data_layout = ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(64 * 8),
            rows_per_image: Some(64),
        };
        let size = Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        };
        queue.write_texture(texture, data, data_layout, size);
    }
}

/// [`RenderCommand`]s for TileMap rendering.
type DrawTilemap = (
    SetItemPipeline,
    SetTilemapViewBindGroup<0>,
    SetTilemapBindGroup<1>,
    SetTilesetBindGroup<2>,
    DrawTileMap,
);

struct SetTilemapViewBindGroup<const I: usize>;
impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetTilemapViewBindGroup<I> {
    type Param = ();
    type ViewQuery = Read<ViewUniformOffset>;
    type ItemQuery = Read<GpuTilemap>;

    fn render<'w>(
        item: &P,
        view_uniform: ROQueryItem<'w, Self::ViewQuery>,
        tilemap: Option<ROQueryItem<'w, Self::ItemQuery>>,
        _param: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let Some(tilemap) = tilemap else {
            return RenderCommandResult::Failure("no tilemap");
        };
        pass.set_bind_group(I, &tilemap.view_bind_group, &[view_uniform.offset]);
        RenderCommandResult::Success
    }
}

struct SetTilemapBindGroup<const I: usize>;
impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetTilemapBindGroup<I> {
    type Param = ();
    type ViewQuery = ();
    type ItemQuery = Read<GpuTilemap>;

    fn render<'w>(
        item: &P,
        _view: (),
        tilemap: Option<ROQueryItem<'w, Self::ItemQuery>>,
        _param: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let Some(tilemap) = tilemap else {
            return RenderCommandResult::Failure("no tilemap");
        };
        pass.set_bind_group(I, &tilemap.tilemap_bind_group, &[]);
        RenderCommandResult::Success
    }
}

struct SetTilesetBindGroup<const I: usize>;

impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetTilesetBindGroup<I> {
    type Param = ();
    type ViewQuery = ();
    type ItemQuery = Read<GpuTilemap>;

    fn render<'w>(
        item: &P,
        _view: (),
        tilemap: Option<ROQueryItem<'w, Self::ItemQuery>>,
        _param: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let Some(tilemap) = tilemap else {
            return RenderCommandResult::Failure("no tilemap");
        };
        if let Some(tileset) = &tilemap.tileset_bind_group {
            pass.set_bind_group(I, &tileset, &[]);
        } else {
            return RenderCommandResult::Failure("no tilemap bind group");
        }
        RenderCommandResult::Success
    }
}

struct DrawTileMap {}
impl<P: PhaseItem> RenderCommand<P> for DrawTileMap {
    type Param = ();
    type ViewQuery = ();
    type ItemQuery = Read<ExtractedTilemap>;

    fn render<'w>(
        item: &P,
        _view: (),
        tilemap: Option<ROQueryItem<'w, Self::ItemQuery>>,
        _param: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let Some(tilemap) = tilemap else {
            return RenderCommandResult::Failure("no tilemap");
        };
        let chunk_size = tilemap.chunks.chunk_size;
        let chunks = tilemap.chunks.n_chunks;

        let n_verts = chunk_size.x * chunk_size.y * 6;
        let n_insts = chunks.x * chunks.y;
        pass.draw(0..n_verts, 0..n_insts);
        RenderCommandResult::Success
    }
}
