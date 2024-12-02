use std::marker::PhantomData;

use bevy::asset::{Asset, UntypedAssetId};
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy_common_assets::toml::TomlAssetPlugin;
use rapier2d::geometry::SharedShape;
use rapier2d::prelude::Point;

use crate::physics::SpriteShapeMap;
use crate::prelude::*;

pub mod animation;
pub mod config;
pub mod script;

pub struct AssetsPlugin<S: States> {
    pub loading_state: S,
}

impl<S: States> Plugin for AssetsPlugin<S> {
    fn build(&self, app: &mut App) {
        // add custom asset types
        app.add_plugins((
            TomlAssetPlugin::<self::script::Script>::new(&["script.toml"]),
            TomlAssetPlugin::<self::animation::SpriteAnimation>::new(&[
                "anim.toml",
            ]),
            TomlAssetPlugin::<self::config::DynamicConfig>::new(&["cfg.toml"]),
        ));
        // dynamic key resolvers for whatever we need
        // we want to be able to do things per-game-tick, so put this in `GameTickUpdate`
        app.add_systems(
            GameTickUpdate,
            (
                resolve_asset_keys::<self::script::Script>,
                resolve_asset_keys::<self::animation::SpriteAnimation>,
            )
                .in_set(AssetsSet::ResolveKeys),
        );

        // asset preloading
        app.init_resource::<PreloadedAssets>();
        app.add_systems(
            Update,
            watch_preload_dynamic_collections
                .track_progress()
                .run_if(in_state(self.loading_state.clone()))
                // NOTE: this is "after" on purpose; we want to check readiness of assets
                // even though we might be adding more handles for tracking
                .after(AssetsTrackProgress),
        );
        app.add_systems(
            OnExit(self.loading_state.clone()),
            (
                finalize_preloaded_dynamic_assets,
                populate_collider_map.after(finalize_preloaded_dynamic_assets),
            ),
        );
    }
}

/// Use this for system ordering relative to assets
/// (within the `GameTickUpdate` schedule)
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssetsSet {
    /// This is when `AssetKey` gets resolved to `Handle`
    ResolveKeys,
}

/// Component for when we want to use a dynamic asset key string to refer to an asset
/// on an entity, instead of a handle.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct AssetKey<T: Asset> {
    pub key: String,
    pub _pd: PhantomData<T>,
}

impl<T: Asset> AssetKey<T> {
    pub fn new(key: &str) -> Self {
        Self {
            key: key.to_owned(),
            _pd: PhantomData,
        }
    }
}

impl<T: Asset> From<&str> for AssetKey<T> {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl<T: Asset> From<&String> for AssetKey<T> {
    fn from(value: &String) -> Self {
        Self::new(value.as_str())
    }
}

/// System that detects `AssetKey` components on entities and inserts `Handle`.
///
/// This happens in `GameTickUpdate`, so it is possible to spawn and resolve
/// things every game tick.
pub fn resolve_asset_keys<T: Asset>(
    mut commands: Commands,
    preloaded: Res<PreloadedAssets>,
    q_key: Query<(Entity, &AssetKey<T>), Without<Handle<T>>>,
) {
    for (e, key) in &q_key {
        if let Some(handle) = preloaded.get_single_asset::<T>(&key.key) {
            commands.entity(e).insert(handle);
        }
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
fn watch_preload_dynamic_collections(
    dynamic_ass: Res<DynamicAssets>,
    mut assets_progress: ResMut<AssetsLoading>,
    mut assets_preloaded: ResMut<PreloadedAssets>,
    ass: Res<AssetServer>,
    progress: Res<ProgressCounter>,
    mut done: Local<bool>,
) -> HiddenProgress {
    if dynamic_ass.is_changed() {
        for (key, asset) in dynamic_ass.iter_assets() {
            // TODO: uncomment this when we have per-level asset management
            // skip preloading level-specific assets
            // if key.starts_with("level.") {
            //     continue;
            // }

            for handle in asset.load(&ass) {
                assets_preloaded.handles.insert(handle.clone());
                assets_progress.add(handle);
            }

            // reserve an entry in our map for later
            assets_preloaded.map.insert(key.to_owned(), None);
        }
    }

    // give one frame grace, just in case
    let r = HiddenProgress(Progress::from(*done));

    // hold on until everything else (non-hidden progress) is done,
    // and then complete ourselves to allow the iyes_progress to transition
    let progress = progress.progress(); // NOTE: not including hidden
    if progress.done >= progress.total {
        *done = true;
    }

    r
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
    let null_shape = SharedShape::convex_hull(&*vec![
        Point::new(1000000.0f32, 1000000.0f32),
        Point::new(1000001.0f32, 1000000.0f32),
    ])
    .unwrap();
    // fun thing about shared shapes is the are arc, so clones don't use more memory.
    collider_map.shapes.push((
        null_shape.clone(),
        null_shape.clone(),
        null_shape.clone(),
        null_shape,
    ));
    for (h_image, h_layout) in iter_assets {
        let Some(image_origin) = images.get_mut(&h_image) else {
            continue;
        };
        let Some(layout) = layouts.get(&h_layout) else {
            continue;
        };

        let mut collider_ids = vec![];
        let mut image =
            image_origin.convert(TextureFormat::Rgba8UnormSrgb).unwrap();
        let width = image.width() as usize;
        let mut data = &mut image.data;
        for (i, anim_frame_rect) in layout.textures.iter().enumerate() {
            let min = anim_frame_rect.min;
            let max = anim_frame_rect.max;
            let size = anim_frame_rect.size();
            let mut collider_points = vec![];
            for y in min.y as usize..max.y as usize {
                for x in min.x as usize..max.x as usize {
                    let pixel_index = (y * width + x) * 4;

                    // Read the pixel values (assuming RGBA format)
                    let pixel = &mut data[pixel_index..pixel_index + 4];

                    // Any pixels with the bright Magenta color will be used for
                    // building the collider
                    if pixel[0] == 255
                        && pixel[1] == 0
                        && pixel[2] == 255
                        && pixel[3] == 255
                    {
                        collider_points.push(Point::new(
                            (0.5 + x as f32 - min.x) - size.x * 0.5,
                            // the siz.y - flips it on y since texture coords are inverted y
                            size.y - ((0.5 + y as f32 - min.y) + size.y * 0.5),
                        ));
                        // Overwrites it with an empty color
                        pixel.copy_from_slice(&[0, 0, 0, 0]);
                    }
                }
            }
            if collider_points.len() < 2 {
                collider_ids.push(0);
                continue;
            }
            let shape = SharedShape::convex_hull(&*collider_points)
                .expect("Cannot build convex hull");
            collider_points.iter_mut().for_each(|p| p.x = -p.x);
            let shape_flipped_x =
                SharedShape::convex_hull(&*collider_points).unwrap();
            collider_points.iter_mut().for_each(|p| {
                p.x = -p.x;
                p.y = -p.y;
            });
            let shape_flipped_y =
                SharedShape::convex_hull(&*collider_points).unwrap();
            collider_points.iter_mut().for_each(|p| p.y = -p.y);
            let shape_flipped_xy =
                SharedShape::convex_hull(&*collider_points).unwrap();

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
