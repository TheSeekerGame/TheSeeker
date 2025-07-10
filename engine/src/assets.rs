use bevy::asset::{Asset, UntypedAssetId};
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::prelude::States;
use bevy_common_assets::toml::TomlAssetPlugin;


use crate::physics::{SpriteShapeMap, Collider};
use crate::prelude::*;

pub mod animation;
pub mod config;
pub mod script;

pub struct AssetsPlugin<S: States> {
    pub loading_state: S,
}

impl<S: States> Plugin for AssetsPlugin<S> {
    fn build(&self, app: &mut App) {
        // Register AI asset loaders (must run before LoadingState)
        app.add_plugins((
            TomlAssetPlugin::<crate::ai::asset::EnemyArchetype>::new(&["arch.toml"]),
            TomlAssetPlugin::<crate::ai::asset::EnemyFsm>::new(&["fsm.toml"]),
        ));
        
        // add custom asset types
        app.add_plugins((
            TomlAssetPlugin::<self::script::Script>::new(&["script.toml"]),
            TomlAssetPlugin::<self::animation::SpriteAnimation>::new(&[
                "anim.toml",
            ]),
            TomlAssetPlugin::<self::config::DynamicConfig>::new(&["cfg.toml"]),
        ));

        // asset preloading
        app.init_resource::<PreloadedAssets>();
        app.add_systems(
            Update,
            watch_preload_dynamic_collections::<S>
                .run_if(in_state(self.loading_state.clone())),
        );
        app.add_systems(
            OnExit(self.loading_state.clone()),
            (
                finalize_preloaded_dynamic_assets,
                merge_archetype_overrides.after(finalize_preloaded_dynamic_assets),
                compile_ai_fsm_assets.after(merge_archetype_overrides),
                populate_collider_map.after(compile_ai_fsm_assets),
            ),
        );

        // Add hot-reload support for archetype changes
        app.add_systems(
            Update,
            hot_reload_archetype_fsms.run_if(on_event::<AssetEvent<crate::ai::EnemyArchetype>>),
        );
    }
}

/// Holds onto all handles for all "preloaded" assets.
///
/// That is, assets that should be loaded during the loading screen,
/// and kept loaded at all times, even when unused.
#[derive(Resource, Default)]
pub struct PreloadedAssets {
    handles: HashSet<UntypedHandle>,
    map: HashMap<String, Option<DynamicAssetType>>,
    map_reverse: HashMap<UntypedAssetId, String>,
}

impl PreloadedAssets {
    pub fn get_asset(&self, key: &str) -> Option<&DynamicAssetType> {
        if let Some(v) = self.map.get(key) {
            v.as_ref()
        } else {
            None
        }
    }

    pub fn get_single_asset<T: Asset>(&self, key: &str) -> Option<Handle<T>> {
        if let Some(d) = self.get_asset(key) {
            if let DynamicAssetType::Single(handle) = d {
                return Some(handle.clone().typed::<T>());
            }
        }
        None
    }

    pub fn get_single_assetid<T: Asset>(
        &self,
        key: &str,
    ) -> Option<AssetId<T>> {
        if let Some(d) = self.get_asset(key) {
            if let DynamicAssetType::Single(handle) = d {
                return Some(handle.id().typed::<T>());
            }
        }
        None
    }

    pub fn get_multi_asset(&self, key: &str) -> Option<&[UntypedHandle]> {
        if let Some(d) = self.get_asset(key) {
            if let DynamicAssetType::Collection(handles) = d {
                return Some(handles.as_slice());
            }
        }
        None
    }

    pub fn get_key_for_asset(
        &self,
        assid: impl Into<UntypedAssetId>,
    ) -> Option<&str> {
        let assid = assid.into();
        self.map_reverse.get(&assid).map(|x| x.as_str())
    }
}

/// Detects any "dynamic assets", as they get discovered by `bevy_asset_loader`,
/// and preloads the things we want preloaded.
fn watch_preload_dynamic_collections<S: States>(
    dynamic_ass: Res<DynamicAssets>,
    mut assets_preloaded: ResMut<PreloadedAssets>,
    ass: Res<AssetServer>,
) {
    if dynamic_ass.is_changed() {
        for (key, asset) in dynamic_ass.iter_assets() {
            // TODO: per-level asset management
            // skip preloading level-specific assets
            // if key.starts_with("level.") {
            //     continue;
            // }

            for handle in asset.load(&ass) {
                assets_preloaded.handles.insert(handle.clone());
            }

            // reserve an entry in our map for later
            assets_preloaded.map.insert(key.to_owned(), None);
        }
    }
}

/// At the end of the loading state, "build" any preloaded dynamic assets
/// and populate a cache/map of handles
fn finalize_preloaded_dynamic_assets(world: &mut World) {
    // take `DynamicAssets` and `PreloadedAssets` out of the world,
    // to avoid mut conflicts
    let dynamic_ass = world
        .remove_resource::<DynamicAssets>()
        .expect("DynamicAssets resource must exist!");
    let mut preloaded_ass = world
        .remove_resource::<PreloadedAssets>()
        .expect("PreloadedAssets resource must exist!");

    for (key, entry) in preloaded_ass.map.iter_mut() {
        if let Some(dynass) = dynamic_ass.get_asset(key.as_str()) {
            match dynass.build(world) {
                Ok(dat) => {
                    match &dat {
                        DynamicAssetType::Single(handle) => {
                            preloaded_ass
                                .map_reverse
                                .insert(handle.id(), key.clone());
                        },
                        DynamicAssetType::Collection(handles) => {
                            for handle in handles {
                                preloaded_ass
                                    .map_reverse
                                    .insert(handle.id(), key.clone());
                            }
                        },
                    }
                    *entry = Some(dat);
                },
                Err(e) => {
                    error!(
                        "Failed to build dynamic asset for key {:?}: {:#}",
                        key, e
                    );
                },
            }
        } else {
            error!(
                "Dynamic asset for key {:?} does not exist!",
                key
            );
        }
    }

    // put them back
    world.insert_resource(dynamic_ass);
    world.insert_resource(preloaded_ass);
}

/// Merge archetype override assets with their base archetypes
fn merge_archetype_overrides(
    mut arch_assets: ResMut<Assets<crate::ai::EnemyArchetype>>,
    preloaded: Res<PreloadedAssets>,
) {
    // Collect archetypes that need merging
    let mut to_merge = Vec::new();
    for (id, archetype) in arch_assets.iter() {
        if archetype.base.is_some() {
            to_merge.push((id, archetype.clone()));
        }
    }
    
    // Merge each override archetype with its base
    for (id, mut archetype) in to_merge {
        if let Some(base_name) = &archetype.base {
            // Find the base archetype
            let base_key = format!("arch.{}", base_name);
            if let Some(base_handle) = preloaded.get_single_asset::<crate::ai::EnemyArchetype>(&base_key) {
                if let Some(base_arch) = arch_assets.get(&base_handle) {
                    archetype.merge_with_base(base_arch);
                    // Update the archetype in assets
                    arch_assets.insert(id, archetype);
                } else {
                    error!("Base archetype {} not found for override", base_name);
                }
            } else {
                error!("Base archetype {} not in preloaded assets", base_name);
            }
        }
    }
}

/// Compile all loaded FSM assets into CompiledFsm assets
fn compile_ai_fsm_assets(
    fsm_assets: Res<Assets<crate::ai::EnemyFsm>>,
    arch_assets: Res<Assets<crate::ai::EnemyArchetype>>,
    mut compiled_assets: ResMut<Assets<crate::ai::CompiledFsm>>,
    mut preloaded: ResMut<PreloadedAssets>,
) {
    // First validate all FSM assets
    for (handle, fsm) in fsm_assets.iter() {
        if let Err(e) = fsm.validate() {
            error!("FSM validation failed for {:?}: {}", handle, e);
        }
    }
    
    // Then validate all archetype assets
    for (handle, archetype) in arch_assets.iter() {
        if let Err(e) = archetype.validate() {
            error!("Archetype validation failed for {:?}: {}", handle, e);
        }
    }
    
    let compiled_map = crate::ai::compile_all_fsms(
        &fsm_assets,
        &arch_assets,
        &mut compiled_assets,
        &preloaded,
    );
    
    // Add compiled FSM handles to preloaded assets
    for (key, handle) in compiled_map {
        let untyped_handle = handle.untyped();
        preloaded.handles.insert(untyped_handle.clone());
        preloaded.map.insert(
            key.clone(),
            Some(DynamicAssetType::Single(untyped_handle.clone())),
        );
        preloaded.map_reverse.insert(untyped_handle.id(), key);
    }
}

fn populate_collider_map(
    preloaded: Res<PreloadedAssets>,
    animations: Res<Assets<animation::SpriteAnimation>>,
    mut images: ResMut<Assets<Image>>,
    layouts: Res<Assets<TextureAtlasLayout>>,
    mut collider_map: ResMut<SpriteShapeMap>,
) {
    // we only want to process images that are actually used
    // by animations, so first we need to collect a list of
    // relevant image assets by going through all loaded
    // animations and resolving their image and layout asset keys
    let iter_assets = animations.iter().filter_map(|(anim_id, anim)| {
        anim.resolve_image_atlas(
            &preloaded,
            preloaded.get_key_for_asset(anim_id),
        )
    });

    // A dummy collider that gets used when the image has no shape generated.
    // (need to do it this way because it removes any requirements for tracking the collider component
    // properties, so everything works as expected.)
    let null_collider = Collider::cuboid(0.1, 0.1); // Use a tiny cuboid as null collider
    let null_shape = null_collider.raw.clone();
    // fun thing about shared shapes is the are arc, so clones don't use more memory.
    collider_map.shapes.push((
        null_shape.clone(),
        null_shape.clone(),
        null_shape.clone(),
        null_shape,
    ));
    for (h_image, h_layout) in iter_assets {
        let Some(image_origin) = images.get_mut(&h_image) else {
            warn!("Image asset {:?} not found in assets, skipping collider generation", h_image);
            continue;
        };
        let Some(layout) = layouts.get(&h_layout) else {
            warn!("TextureAtlasLayout asset {:?} not found in assets, skipping collider generation", h_layout);
            continue;
        };
        
        debug!("Processing colliders for image {:?} with layout {:?}, format: {:?}", 
               h_image, h_layout, image_origin.texture_descriptor.format);

        let mut collider_ids = vec![];
        let mut image = match image_origin.convert(TextureFormat::Rgba8UnormSrgb) {
            Some(converted_image) => converted_image,
            None => {
                warn!("Failed to convert image {:?} to Rgba8UnormSrgb format, skipping collider generation", h_image);
                // Insert null colliders for all frames in this texture atlas
                for _ in &layout.textures {
                    collider_ids.push(0);
                }
                collider_map.map.insert(h_image.id(), collider_ids);
                continue;
            }
        };
        let width = image.width() as usize;
        let Some(data) = image.data.as_mut() else {
            warn!(
                "Image {:?} has no pixel data after conversion, skipping collider generation", h_image
            );
            continue;
        };
        for (i, anim_frame_rect) in layout.textures.iter().enumerate() {
            let min = anim_frame_rect.min;
            let max = anim_frame_rect.max;
            let size = anim_frame_rect.size();
            let mut collider_points: Vec<Vec2> = vec![];
            for y in min.y as usize..max.y as usize {
                for x in min.x as usize..max.x as usize {
                    let pixel_index = (y * width + x) * 4;

                    // Read the pixel values (assuming RGBA format)
                    let pixel = &mut data[pixel_index..pixel_index + 4];

                    // Detect magenta pixels (RGB: 255, 0, 255, alpha: 255)
                    // These pixels define the collision shape for this animation frame
                    if pixel[0] == 255
                        && pixel[1] == 0
                        && pixel[2] == 255
                        && pixel[3] == 255
                    {
                        // Convert texture coordinates to physics coordinates
                        // Texture space: origin at top-left, Y increases downward
                        // Physics space: origin at sprite center, Y increases upward
                        let texture_x = x as f32 - min.x as f32; // Relative to frame origin
                        let texture_y = y as f32 - min.y as f32; // Relative to frame origin
                        
                        // Transform to physics coordinates:
                        // - Center around sprite center (subtract half-width/height)
                        // - Add 0.5 to move from pixel corner to pixel center
                        let local_x = texture_x + 0.5 - size.x as f32 * 0.5;
                        // - Flip Y axis (texture Y down -> physics Y up)
                        // - Center vertically
                        let local_y = size.y as f32 * 0.5 - (texture_y + 0.5);
                        

                        
                        collider_points.push(Vec2::new(local_x, local_y));
                        // Clear the magenta pixel to transparent (prevents it from being visible in-game)
                        pixel.copy_from_slice(&[0, 0, 0, 0]);
                    }
                }
            }
            if collider_points.len() < 2 {
                collider_ids.push(0);
                continue;
            }
            // Use the Vec2 points directly
            let verts = collider_points;
            

            
            let shape = match Collider::convex_hull(&verts) {
                Some(collider) => collider.raw,
                None => {
                    warn!("Failed to build convex hull for frame {} in image {:?}, using null collider", i, h_image);
                    collider_ids.push(0);
                    continue;
                }
            };
            
            // Flip X
            let verts_flipped_x: Vec<Vec2> = verts.iter().map(|p| Vec2::new(-p.x, p.y)).collect();
            let shape_flipped_x = match Collider::convex_hull(&verts_flipped_x) {
                Some(collider) => collider.raw,
                None => {
                    warn!("Failed to build flipped-x convex hull for frame {} in image {:?}, using null collider", i, h_image);
                    collider_ids.push(0);
                    continue;
                }
            };
            
            // Flip Y
            let verts_flipped_y: Vec<Vec2> = verts.iter().map(|p| Vec2::new(p.x, -p.y)).collect();
            let shape_flipped_y = match Collider::convex_hull(&verts_flipped_y) {
                Some(collider) => collider.raw,
                None => {
                    warn!("Failed to build flipped-y convex hull for frame {} in image {:?}, using null collider", i, h_image);
                    collider_ids.push(0);
                    continue;
                }
            };
            
            // Flip both X and Y
            let verts_flipped_xy: Vec<Vec2> = verts.iter().map(|p| Vec2::new(-p.x, -p.y)).collect();
            let shape_flipped_xy = match Collider::convex_hull(&verts_flipped_xy) {
                Some(collider) => collider.raw,
                None => {
                    warn!("Failed to build flipped-xy convex hull for frame {} in image {:?}, using null collider", i, h_image);
                    collider_ids.push(0);
                    continue;
                }
            };

            let i_new = collider_map.shapes.len();
            collider_map.shapes.push((
                shape,
                shape_flipped_x,
                shape_flipped_y,
                shape_flipped_xy,
            ));
            collider_ids.push(i_new);
        }
        *image_origin = image.into();
        // collider ids is either 0 or an index into shapes?
        // could it be option instead..
        collider_map.map.insert(h_image.id(), collider_ids);
    }
}

/// System that recompiles FSMs when archetypes are modified at runtime
fn hot_reload_archetype_fsms(
    mut events: EventReader<AssetEvent<crate::ai::EnemyArchetype>>,
    fsm_assets: Res<Assets<crate::ai::EnemyFsm>>,
    mut arch_assets: ResMut<Assets<crate::ai::EnemyArchetype>>,
    mut compiled_assets: ResMut<Assets<crate::ai::CompiledFsm>>,
    preloaded: Res<PreloadedAssets>,
    mut fsm_instances: Query<&mut crate::ai::FsmInstance>,
) {
    for event in events.read() {
        match event {
            AssetEvent::Modified { id } => {
                // Find which archetype was modified
                if let Some(archetype) = arch_assets.get(*id).cloned() {
                    // First, check if this is an override archetype that needs merging
                    if let Some(base_name) = &archetype.base {
                        // Find and merge with base
                        let base_key = format!("arch.{}", base_name);
                        if let Some(base_handle) = preloaded.get_single_asset::<crate::ai::EnemyArchetype>(&base_key) {
                            if let Some(base_arch) = arch_assets.get(&base_handle) {
                                let mut merged_archetype = archetype.clone();
                                merged_archetype.merge_with_base(base_arch);
                                // Update the archetype in assets
                                arch_assets.insert(*id, merged_archetype);
                            }
                        }
                    }
                    
                    // Additionally, if the modified archetype is a base archetype (no base field),
                    // find all override archetypes that inherit from it and re-merge them
                    if archetype.base.is_none() {
                        let base_id = &archetype.id;
                        let mut to_remerge = Vec::new();
                        
                        // Find all archetypes that use this as a base
                        for (override_handle, override_arch) in arch_assets.iter() {
                            if let Some(ref override_base) = override_arch.base {
                                if override_base == base_id {
                                    to_remerge.push((override_handle, override_arch.clone()));
                                }
                            }
                        }
                        
                        // Re-merge each override archetype with the updated base
                        for (override_handle, mut override_arch) in to_remerge {
                            override_arch.merge_with_base(&archetype);
                            arch_assets.insert(override_handle, override_arch);
                        }
                    }
                    
                    // Now recompile all FSMs with the updated archetypes
                    let compiled_map = crate::ai::compile_all_fsms(
                        &fsm_assets,
                        &arch_assets,
                        &mut compiled_assets,
                        &preloaded,
                    );

                    // Hot-patch: Replace CompiledFsm data in existing handles.
                    // This preserves handle references in active FsmInstance components
                    // while updating the FSM logic. Keeps temporary handle to avoid
                    // Bevy asset reuse conflicts during rapid edits.
                    for (key, new_handle) in compiled_map {
                        if let Some(old_handle) = preloaded.get_single_asset::<crate::ai::CompiledFsm>(&key) {
                            if old_handle != new_handle {
                                // Clone the new FSM data first to avoid borrowing conflicts
                                if let Some(new_fsm) = compiled_assets.get(&new_handle) {
                                    let new_fsm_data = new_fsm.clone();
                                    if let Some(old_fsm) = compiled_assets.get_mut(&old_handle) {
                                        *old_fsm = new_fsm_data;
                                    }
                                }
                                // Don't remove the temporary handle immediately - let Bevy's 
                                // Assets GC clean it up to avoid reuse panic
                            }
                        }
                    }
                    
                    // Extend cooldown vectors on active enemies if needed
                    for mut fsm in fsm_instances.iter_mut() {
                        if let Some(compiled) = compiled_assets.get(&fsm.brain) {
                            let expected_len = compiled.inner.cooldown_names.len();
                            if fsm.cooldowns.len() < expected_len {
                                // Extend with zeros for new cooldowns
                                fsm.cooldowns.resize(expected_len, 0);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
