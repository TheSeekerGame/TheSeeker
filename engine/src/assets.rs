use bevy::asset::Asset;
use bevy_common_assets::ron::RonAssetPlugin;

use crate::prelude::*;

pub struct AssetsPlugin<S: States> {
    pub loading_state: S,
}

impl<S: States> Plugin for AssetsPlugin<S> {
    fn build(&self, app: &mut App) {
        app.add_loading_state(LoadingState::new(
            self.loading_state.clone(),
        ));

        // asset preloading
        app.init_resource::<PreloadedAssets>();
        app.add_system(
            watch_preload_dynamic_collections
                .track_progress()
                .run_if(in_state(self.loading_state.clone()))
                // NOTE: this is "after" on purpose; we want to check readiness of assets
                // even though we might be adding more handles for tracking
                .after(AssetsTrackProgress),
        );
        app.add_system(
            finalize_preloaded_dynamic_assets.in_schedule(OnExit(self.loading_state.clone())),
        );
    }
}

/// Holds onto all handles for all "preloaded" assets.
///
/// That is, assets that should be loaded during the loading screen,
/// and kept loaded at all times, even when unused.
#[derive(Resource, Default)]
pub struct PreloadedAssets {
    handles: HashSet<HandleUntyped>,
    map: HashMap<String, Option<DynamicAssetType>>,
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

    pub fn get_multi_asset<T: Asset>(&self, key: &str) -> Option<&[HandleUntyped]> {
        if let Some(d) = self.get_asset(key) {
            if let DynamicAssetType::Collection(handles) = d {
                return Some(handles.as_slice());
            }
        }
        None
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
                Ok(handles) => {
                    *entry = Some(handles);
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
