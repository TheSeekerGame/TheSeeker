//! Everything to do with loading assets

use crate::prelude::*;

pub struct AssetsPlugin;

impl Plugin for AssetsPlugin {
    fn build(&self, app: &mut App) {
        // add custom asset infra
        app.add_plugins(theseeker_engine::assets::AssetsPlugin {
            loading_state: AppState::AssetsLoading,
        });

        // bevy_asset_loader
        app.add_loading_state(
            LoadingState::new(AppState::AssetsLoading)
                .with_dynamic_assets_file::<StandardDynamicAssetCollection>(
                    "animations.assets.ron",
                )
                .with_dynamic_assets_file::<StandardDynamicAssetCollection>(
                    "sprites.assets.ron",
                )
                .with_dynamic_assets_file::<StandardDynamicAssetCollection>(
                    "audio.assets.ron",
                )
                .with_dynamic_assets_file::<StandardDynamicAssetCollection>(
                    "levels.assets.ron",
                )
                .with_dynamic_assets_file::<StandardDynamicAssetCollection>(
                    "gameplay.assets.ron",
                )
                .with_dynamic_assets_file::<StandardDynamicAssetCollection>(
                    "ui.assets.ron",
                )
                .load_collection::<UiAssets>()
                .load_collection::<MainMenuAssets>()
                .load_collection::<DialogAssets>()
                .load_collection::<GameUiAssets>(),
        );
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
}

#[derive(AssetCollection, Resource)]
pub struct DialogAssets {
    #[asset(key = "dialog.MrSnafflesBackground")]
    pub mr_snaffles_background: Handle<Image>,
    #[asset(key = "dialog.VagrantBackground")]
    pub vagrant_background: Handle<Image>,
}

#[derive(AssetCollection, Resource)]
pub struct GameUiAssets {
    #[asset(key = "ui.game.toolbar_frame")]
    pub toolbar_frame: Handle<Image>,
    #[asset(key = "ui.game.attack_skill_icon")]
    pub attack_skill_icon: Handle<Image>,
    #[asset(key = "ui.game.dash_skill_icon")]
    pub dash_skill_icon: Handle<Image>,
    #[asset(key = "ui.game.whirl_skill_icon")]
    pub whirl_skill_icon: Handle<Image>,
    #[asset(key = "ui.game.stealth_skill_icon")]
    pub stealth_skill_icon: Handle<Image>,
}
