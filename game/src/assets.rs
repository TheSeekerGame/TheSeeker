//! Everything to do with loading assets

use crate::prelude::*;

pub struct AssetsPlugin;

impl Plugin for AssetsPlugin {
    fn build(&self, app: &mut App) {
        // add custom asset infra
        app.add_plugins(theseeker_engine::assets::AssetsPlugin {
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

        // Some assets can be defined statically in Resource structs for ease of access
        app.add_collection_to_loading_state::<_, UiAssets>(AppState::AssetsLoading);
        app.add_collection_to_loading_state::<_, MainMenuAssets>(AppState::AssetsLoading);
    }
}

#[derive(AssetCollection, Resource)]
pub struct UiAssets {
    #[asset(key = "ui.font.regular")]
    pub font_regular: Handle<Font>,
    #[asset(key = "ui.font.bold")]
    pub font_bold: Handle<Font>,
    #[asset(key = "ui.font.light")]
    pub font_light: Handle<Font>,
}

#[derive(AssetCollection, Resource)]
pub struct MainMenuAssets {
    #[asset(key = "ui.mainmenu.background")]
    pub background: Handle<Image>,
    #[asset(key = "ui.mainmenu.logo")]
    pub logo: Handle<Image>,
}
