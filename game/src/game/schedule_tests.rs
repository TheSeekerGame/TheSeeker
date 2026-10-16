//! Build the gameplay schedule without opening a window or running audio.

use bevy::prelude::*;
use theseeker_engine::time::GameTickUpdate;

#[test]
fn gameplay_playback_actions_and_damage_have_an_acyclic_schedule() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default(), bevy::state::app::StatesPlugin));
    app.init_asset::<Image>();
    app.init_asset::<Shader>();
    app.init_asset::<Mesh>();
    app.init_asset::<TextureAtlasLayout>();
    app.init_asset::<theseeker_engine::assets::animation::SpriteAnimation>();
    app.add_plugins((theseeker_engine::script::ScriptPlugin,
        theseeker_engine::animation::SpriteAnimationPlugin,
        theseeker_engine::physics::PhysicsPlugin, super::GameplayPlugin));
    app.world_mut().schedule_scope(GameTickUpdate, |world, schedule| {
        schedule.initialize(world).expect("playback, actions, motion, and damage schedule");
    });
}
