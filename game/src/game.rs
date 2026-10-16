//! Everything related to gameplay and game mechanics.
//!
//! Create sub-modules for different aspects of the gameplay.

use crate::prelude::*;

pub mod actors;
pub mod combat;
pub(crate) mod capsule_commerce;
pub mod effects;
pub mod enemy;
mod game_over;
pub(crate) mod intro;
pub mod gentstate;
pub(crate) mod interaction;
pub(crate) mod npc;
pub mod physics;
pub mod player;
pub mod spikes;
mod tick_profile;
pub(crate) mod world_objects;
pub(crate) mod yak;

pub use game_over::GameOver;

pub struct GameplayPlugin;

#[cfg(test)]
mod schedule_tests;

impl Plugin for GameplayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnExit(AppState::InGame), theseeker_engine::presentation::clear_presentation);
        app.insert_resource(theseeker_engine::assets::animation::AnimationClock(
            theseeker_engine::assets::animation::FrameClock::Global,
        ));
        // LDtk registration removed; entities are spawned by PNG level pipeline

        // Add the plugins for each game mechanic
        app.add_plugins(actors::ActorPlugin);
        app.add_plugins((
            combat::CombatPlugin,
            interaction::InteractionPlugin,
            physics::PhysicsPlugin,
            effects::EffectsPlugin,
            player::PlayerPlugin,
            enemy::EnemyPlugin,
            tick_profile::TickProfilePlugin,
            yak::YakPlugin,
            world_objects::WorldObjectsPlugin,
            spikes::SpikeHazardPlugin,
            game_over::GameOverPlugin,
            intro::IntroPlugin,
        ));
    }
}
