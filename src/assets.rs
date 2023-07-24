//! Everything to do with loading assets

use bevy_asset_loader::prelude::*;

use crate::prelude::*;

pub struct AssetsPlugin;

impl Plugin for AssetsPlugin {
    fn build(&self, app: &mut App) {
        app.add_loading_state(LoadingState::new(
            AppState::AssetsLoading,
        ));

        // All game assets are to be defined in dynamic collections files
        // See those files for details on each one, there be comments. :)
        app.add_dynamic_collection_to_loading_state::<_, StandardDynamicAssetCollection>(
            AppState::AssetsLoading,
            "ui.assets.ron",
        );
        app.add_dynamic_collection_to_loading_state::<_, StandardDynamicAssetCollection>(
            AppState::AssetsLoading,
            "locale.assets.ron",
        );
        app.add_dynamic_collection_to_loading_state::<_, StandardDynamicAssetCollection>(
            AppState::AssetsLoading,
            "levels.assets.ron",
        );
        app.add_dynamic_collection_to_loading_state::<_, StandardDynamicAssetCollection>(
            AppState::AssetsLoading,
            "gameplay.assets.ron",
        );
        app.add_dynamic_collection_to_loading_state::<_, StandardDynamicAssetCollection>(
            AppState::AssetsLoading,
            "gfx.assets.ron",
        );
        app.add_dynamic_collection_to_loading_state::<_, StandardDynamicAssetCollection>(
            AppState::AssetsLoading,
            "audio.assets.ron",
        );

        // Some assets can be defined statically in Resource structs for ease of access
        app.add_collection_to_loading_state::<_, UiAssets>(AppState::AssetsLoading);
        app.add_collection_to_loading_state::<_, LocaleAssets>(AppState::AssetsLoading);

        app.init_resource::<PreloadedAssets>();
        app.add_system(
            watch_preload_dynamic_collections
                .track_progress()
                .run_if(in_state(AppState::AssetsLoading))
                // NOTE: this is "after" on purpose; we want to check readiness of assets
                // even though we might be adding more handles for tracking
                .after(AssetsTrackProgress),
        );
    }
}

#[derive(AssetCollection, Resource)]
pub struct UiAssets {
    #[asset(key = "ui.font.regular")]
    pub font_regular: Handle<Font>,
    #[asset(key = "ui.font.italic")]
    pub font_italic: Handle<Font>,
    #[asset(key = "ui.font.bold")]
    pub font_bold: Handle<Font>,
    #[asset(key = "ui.font.bolditalic")]
    pub font_bolditalic: Handle<Font>,
    #[asset(key = "ui.font.light")]
    pub font_light: Handle<Font>,
    #[asset(key = "ui.font.lightitalic")]
    pub font_lightitalic: Handle<Font>,
}

impl UiAssets {
    pub fn get_font(&self, light: bool, bold: bool, italic: bool) -> Handle<Font> {
        match (light, bold, italic) {
            (false, false, false) => &self.font_regular,
            (false, false, true) => &self.font_italic,
            (false, true, false) => &self.font_bold,
            (false, true, true) => &self.font_bolditalic,
            (true, false, false) => &self.font_light,
            (true, false, true) => &self.font_lightitalic,
            // TODO: should these be something else?
            (true, true, false) => &self.font_regular,
            (true, true, true) => &self.font_italic,
        }
        .clone()
    }
}

#[derive(AssetCollection, Resource)]
pub struct LocaleAssets {
    #[asset(key = "locale.bundles", collection(typed))]
    pub bundles: Vec<Handle<bevy_fluent::BundleAsset>>,
}

/// Holds onto all handles for all "preloaded" assets.
///
/// That is, assets that should be loaded during the loading screen,
/// and kept loaded at all times, even when unused.
#[derive(Resource, Default)]
struct PreloadedAssets {
    handles: HashSet<HandleUntyped>,
}

/// Detects any "dynamic assets", as they get discovered by `bevy_asset_loader`,
/// and preloads the things we want preloaded.
fn watch_preload_dynamic_collections(
    dynass: Res<DynamicAssets>,
    mut assets_progress: ResMut<AssetsLoading>,
    mut assets_preloaded: ResMut<PreloadedAssets>,
    ass: Res<AssetServer>,
    progress: Res<ProgressCounter>,
    mut done: Local<bool>,
) -> HiddenProgress {
    if dynass.is_changed() {
        for (_key, asset) in dynass.iter_assets() {
            // TODO: uncomment this when we have per-level asset management
            // skip preloading level-specific assets
            // if key.starts_with("level.") {
            //     continue;
            // }

            for handle in asset.load(&ass) {
                assets_preloaded.handles.insert(handle.clone());
                assets_progress.add(handle);
            }
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
