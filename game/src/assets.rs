//! Everything to do with loading assets

use crate::prelude::*;
use std::path::{Path, PathBuf};
use theseeker_engine::assets::animation::parse_animation_png_metadata;

pub struct AssetsPlugin;

impl Plugin for AssetsPlugin {
    fn build(&self, app: &mut App) {
        // add custom asset infra
        app.add_plugins(theseeker_engine::assets::AssetsPlugin {
            loading_state: AppState::AssetsLoading,
        });
        app.add_systems(Startup, register_generated_gameplay_assets);

        // bevy_asset_loader
        app.add_loading_state(
            LoadingState::new(AppState::AssetsLoading)
                .with_dynamic_assets_file::<StandardDynamicAssetCollection>(
                    "audio.assets.ron",
                )
                .with_dynamic_assets_file::<StandardDynamicAssetCollection>(
                    "gameplay.assets.ron",
                )
                .with_dynamic_assets_file::<StandardDynamicAssetCollection>(
                    "ui.assets.ron",
                )
                .load_collection::<GameUiAssets>(),
        );
    }
}

const GENERATED_GAMEPLAY_ASSET_DIRS: [(&str, &str, &str); 3] = [
    ("arch", "enemies/archetypes", ".arch.toml"),
    ("action", "actions", ".action.toml"),
    ("projectile", "projectiles", ".projectile.toml"),
];

fn register_generated_gameplay_assets(mut dynamic_assets: ResMut<DynamicAssets>) {
    register_animation_assets(&mut dynamic_assets);
    for (key_prefix, asset_dir, suffix) in GENERATED_GAMEPLAY_ASSET_DIRS {
        register_gameplay_assets(&mut dynamic_assets, key_prefix, asset_dir, suffix);
    }
}

fn register_animation_assets(dynamic_assets: &mut DynamicAssets) {
    register_animation_assets_in(dynamic_assets, Path::new("assets/animations"));
}

fn register_animation_assets_in(
    dynamic_assets: &mut DynamicAssets,
    asset_dir: &Path,
) {
    let Ok(entries) = std::fs::read_dir(asset_dir) else {
        warn!("Could not scan animation asset dir '{}'", asset_dir.display());
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            register_animation_assets_in(dynamic_assets, &path);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("toml")
            && path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.ends_with(".anim.toml"))
        {
            register_animation_asset(dynamic_assets, &path);
        }
    }
}

fn register_animation_asset(dynamic_assets: &mut DynamicAssets, path: &Path) {
    let Ok(source) = std::fs::read_to_string(path) else {
        warn!("Could not read animation asset metadata '{}'", path.display());
        return;
    };
    let Ok(value) = toml::from_str::<toml::Value>(&source) else {
        warn!("Could not parse animation asset metadata '{}'", path.display());
        return;
    };
    let Some(asset) = value.get("asset").and_then(|asset| asset.as_table()) else {
        return;
    };
    let keys = animation_asset_keys(asset);
    if keys.is_empty() {
        return;
    }
    let Some(image_path) = asset
        .get("image")
        .and_then(|image| image.as_str())
        .map(PathBuf::from)
    else {
        warn!("Animation '{}' has keys but no asset image", path.display());
        return;
    };
    let Some(metadata) = image_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(parse_animation_png_metadata)
    else {
        warn!("Animation image '{}' lacks atlas metadata", image_path.display());
        return;
    };
    let Some(anim_path) = path.strip_prefix("assets").ok().and_then(path_string)
    else {
        warn!("Animation path '{}' is outside assets", path.display());
        return;
    };
    let Some(image_path) = path_string(&image_path) else {
        warn!("Animation image path '{}' is not valid UTF-8", image_path.display());
        return;
    };

    for key in keys {
        dynamic_assets.register_asset(
            key.clone(),
            Box::new(StandardDynamicAsset::File {
                path: anim_path.clone(),
            }),
        );
        dynamic_assets.register_asset(
            format!("{key}.image"),
            Box::new(StandardDynamicAsset::File {
                path: image_path.clone(),
            }),
        );
        dynamic_assets.register_asset(
            format!("{key}.atlas"),
            Box::new(StandardDynamicAsset::TextureAtlasLayout {
                tile_size_x: metadata.cell_width,
                tile_size_y: metadata.cell_height,
                columns: metadata.columns,
                rows: metadata.rows,
                padding_x: None,
                padding_y: None,
                offset_x: None,
                offset_y: None,
            }),
        );
    }
}

fn animation_asset_keys(asset: &toml::map::Map<String, toml::Value>) -> Vec<String> {
    if let Some(key) = asset.get("key").and_then(|key| key.as_str()) {
        return vec![key.to_owned()];
    }
    asset
        .get("keys")
        .and_then(|keys| keys.as_array())
        .into_iter()
        .flatten()
        .filter_map(|key| key.as_str().map(ToOwned::to_owned))
        .collect()
}

fn path_string(path: &Path) -> Option<String> {
    Some(path.to_str()?.replace('\\', "/"))
}

fn register_gameplay_assets(
    dynamic_assets: &mut DynamicAssets,
    key_prefix: &str,
    asset_dir: &str,
    suffix: &str,
) {
    let Ok(entries) = std::fs::read_dir(format!("assets/{asset_dir}")) else {
        warn!("Could not scan generated gameplay asset dir '{asset_dir}'");
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|name| name.to_str())
        else {
            continue;
        };
        let Some(stem) = file_name.strip_suffix(suffix) else {
            continue;
        };
        dynamic_assets.register_asset(
            format!("{key_prefix}.{stem}"),
            Box::new(StandardDynamicAsset::File {
                path: format!("{asset_dir}/{file_name}"),
            }),
        );
    }
}

#[derive(AssetCollection, Resource)]
pub struct GameUiAssets {
    #[asset(key = "ui.game.frame_corner_bottom_left")]
    pub frame_corner_bottom_left: Handle<Image>,
}
