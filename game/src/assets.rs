//! Everything to do with loading assets

use crate::prelude::*;

pub struct AssetsPlugin;

impl Plugin for AssetsPlugin {
    fn build(&self, app: &mut App) {
        // add custom asset infra
        app.add_plugin(theseeker_engine::assets::AssetsPlugin {
            loading_state: AppState::AssetsLoading,
        });

        // All game assets are to be defined in dynamic collections files
        // See those files for details on each one, there be comments. :)
        app.add_dynamic_collection_to_loading_state::<_, StandardDynamicAssetCollection>(
            AppState::AssetsLoading,
            "gfx.assets.ron",
        );
        app.add_dynamic_collection_to_loading_state::<_, StandardDynamicAssetCollection>(
            AppState::AssetsLoading,
            "audio.assets.ron",
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
            "ui.assets.ron",
        );
        app.add_dynamic_collection_to_loading_state::<_, StandardDynamicAssetCollection>(
            AppState::AssetsLoading,
            "locale.assets.ron",
        );

        // Some assets can be defined statically in Resource structs for ease of access
        app.add_collection_to_loading_state::<_, UiAssets>(AppState::AssetsLoading);
        app.add_collection_to_loading_state::<_, LocaleAssets>(AppState::AssetsLoading);
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
