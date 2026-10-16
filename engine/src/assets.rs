use bevy::asset::{Asset, UntypedAssetId};
use bevy::prelude::States;
use bevy_common_assets::toml::TomlAssetPlugin;

use crate::prelude::*;

pub mod animation;
pub mod attack;
pub mod config;
pub mod projectile;
pub mod script;
mod sprite_colliders;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssetsSet { ResolveKeys, Colliders }

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
            TomlAssetPlugin::<self::attack::AttackDescriptor>::new(&[
                "attack.toml",
            ]),
            TomlAssetPlugin::<self::projectile::ProjectileDescriptor>::new(&[
                "projectile.toml",
            ]),
            TomlAssetPlugin::<self::config::DynamicConfig>::new(&["cfg.toml"]),
        ));

        // asset preloading
        app.init_resource::<PreloadedAssets>();
        app.add_systems(
            Update,
            watch_preload_dynamic_collections
                .run_if(in_state(self.loading_state.clone())),
        );
        app.add_systems(
            OnExit(self.loading_state.clone()),
            (
                finalize_preloaded_dynamic_assets.in_set(AssetsSet::ResolveKeys),
                validate_attack_descriptors.after(finalize_preloaded_dynamic_assets),
                validate_projectile_descriptors.after(validate_attack_descriptors).before(AssetsSet::Colliders),
                sprite_colliders::populate_collider_map.in_set(AssetsSet::Colliders),
            ),
        );

        app.configure_sets(OnExit(self.loading_state.clone()), (AssetsSet::ResolveKeys, AssetsSet::Colliders).chain());

        // Descriptor changes are validated before gameplay consumes them.
        app.add_systems(
            Update,
            (
                validate_modified_attack_descriptors.run_if(on_message::<
                    AssetEvent<self::attack::AttackDescriptor>,
                >),
                validate_modified_projectile_descriptors.run_if(on_message::<
                    AssetEvent<self::projectile::ProjectileDescriptor>,
                >),
            ),
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
    /// Handles requested during loading, including assets whose bytes are still in flight.
    pub fn handles(&self) -> impl Iterator<Item = &UntypedHandle> { self.handles.iter() }

    pub fn register_runtime_asset<T: Asset>(&mut self, key: String, handle: Handle<T>) {
        let untyped = handle.untyped();
        self.handles.insert(untyped.clone());
        self.map_reverse.insert(untyped.id(), key.clone());
        let previous = self.map.insert(key, Some(DynamicAssetType::Single(untyped.clone())));
        if let Some(Some(DynamicAssetType::Single(previous))) = previous {
            self.release_replaced_handle(&previous, &untyped);
        }
    }

    fn release_replaced_handle(&mut self, previous: &UntypedHandle, current: &UntypedHandle) {
        let still_referenced = self.map.values().flatten().any(|value| matches!(value, DynamicAssetType::Single(handle) if handle == previous));
        if previous != current && !still_referenced {
            self.handles.remove(previous);
            self.map_reverse.remove(&previous.id());
        }
    }

    pub fn get_asset(&self, key: &str) -> Option<&DynamicAssetType> {
        if let Some(v) = self.map.get(key) {
            v.as_ref()
        } else {
            None
        }
    }

    pub fn get_single_asset<T: Asset>(&self, key: &str) -> Option<Handle<T>> {
        if let Some(DynamicAssetType::Single(handle)) = self.get_asset(key) {
            return Some(handle.clone().typed::<T>());
        }
        None
    }

    pub fn get_multi_asset(&self, key: &str) -> Option<&[UntypedHandle]> {
        if let Some(DynamicAssetType::Collection(handles)) = self.get_asset(key)
        {
            return Some(handles.as_slice());
        }
        None
    }

    pub fn get_key_for_asset(
        &self,
        assid: impl Into<UntypedAssetId>,
    ) -> Option<&str> {
        let assid = assid.into();
        self.map_reverse.get(&assid).map(std::string::String::as_str)
    }
}

/// Detects any "dynamic assets", as they get discovered by `bevy_asset_loader`,
/// and preloads the things we want preloaded.
fn watch_preload_dynamic_collections(
    dynamic_ass: Res<DynamicAssets>,
    mut assets_preloaded: ResMut<PreloadedAssets>,
    ass: Res<AssetServer>,
) {
    if !dynamic_ass.is_changed() {
        return;
    }

    for (key, asset) in dynamic_ass.iter_assets() {
        // TODO: per-level asset management

        for handle in asset.load(&ass) {
            assets_preloaded.handles.insert(handle.clone());
        }

        // reserve an entry in our map for later
        assets_preloaded.map.insert(key.to_owned(), None);
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

    let map_reverse = &mut preloaded_ass.map_reverse;
    for (key, entry) in preloaded_ass.map.iter_mut() {
        finalize_dynamic_asset_entry(
            world,
            &dynamic_ass,
            map_reverse,
            key,
            entry,
        );
    }

    // put them back
    world.insert_resource(dynamic_ass);
    world.insert_resource(preloaded_ass);
}

fn finalize_dynamic_asset_entry(
    world: &mut World,
    dynamic_ass: &DynamicAssets,
    map_reverse: &mut HashMap<UntypedAssetId, String>,
    key: &str,
    entry: &mut Option<DynamicAssetType>,
) {
    let Some(dynass) = dynamic_ass.get_asset(key) else {
        error!("Dynamic asset for key {:?} does not exist!", key);
        return;
    };

    let dat = match dynass.build(world) {
        Ok(dat) => dat,
        Err(e) => {
            error!("Failed to build dynamic asset for key {:?}: {:#}", key, e);
            return;
        },
    };

    register_dynamic_asset_handles(map_reverse, key, &dat);
    *entry = Some(dat);
}

fn register_dynamic_asset_handles(
    map_reverse: &mut HashMap<UntypedAssetId, String>,
    key: &str,
    dat: &DynamicAssetType,
) {
    match dat {
        DynamicAssetType::Single(handle) => {
            map_reverse.insert(handle.id(), key.to_owned());
        },
        DynamicAssetType::Collection(handles) => {
            for handle in handles {
                map_reverse.insert(handle.id(), key.to_owned());
            }
        },
    }
}

/// Merge archetype override assets with their base archetypes
fn validate_projectile_descriptors(
    projectile_assets: Res<Assets<projectile::ProjectileDescriptor>>,
) {
    for (handle, descriptor) in projectile_assets.iter() {
        if let Err(e) = descriptor.validate() {
            error!(
                "Projectile descriptor validation failed for {:?}: {}",
                handle, e
            );
        }
    }
}

fn validate_modified_projectile_descriptors(
    mut events: MessageReader<AssetEvent<projectile::ProjectileDescriptor>>,
    projectile_assets: Res<Assets<projectile::ProjectileDescriptor>>,
) {
    for event in events.read() {
        let AssetEvent::Modified { id } = event else {
            continue;
        };
        validate_modified_projectile_descriptor(*id, &projectile_assets);
    }
}

fn validate_modified_projectile_descriptor(
    id: AssetId<projectile::ProjectileDescriptor>,
    projectile_assets: &Assets<projectile::ProjectileDescriptor>,
) {
    let Some(descriptor) = projectile_assets.get(id) else {
        return;
    };
    if let Err(e) = descriptor.validate() {
        error!(
            "Projectile descriptor validation failed for {:?}: {}",
            id, e
        );
    }
}

fn validate_attack_descriptors(
    attack_assets: Res<Assets<attack::AttackDescriptor>>,
) {
    for (handle, descriptor) in attack_assets.iter() {
        if let Err(e) = descriptor.validate() {
            error!(
                "Attack descriptor validation failed for {:?}: {}",
                handle, e
            );
        }
    }
}

fn validate_modified_attack_descriptors(
    mut events: MessageReader<AssetEvent<attack::AttackDescriptor>>,
    attack_assets: Res<Assets<attack::AttackDescriptor>>,
) {
    for event in events.read() {
        let AssetEvent::Modified { id } = event else {
            continue;
        };
        validate_modified_attack_descriptor(*id, &attack_assets);
    }
}

fn validate_modified_attack_descriptor(
    id: AssetId<attack::AttackDescriptor>,
    attack_assets: &Assets<attack::AttackDescriptor>,
) {
    let Some(descriptor) = attack_assets.get(id) else {
        return;
    };
    if let Err(e) = descriptor.validate() {
        error!("Attack descriptor validation failed for {:?}: {}", id, e);
    }
}
