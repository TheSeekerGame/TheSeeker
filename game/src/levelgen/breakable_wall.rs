//! Breakable walls close secret rooms until player attacks destroy them.

use crate::camera::CameraShake;
use crate::game::combat::{effects::DamageFlash, DamageInfo};
use crate::prelude::*;
use theseeker_engine::effects::{SpriteScale, SpriteStretch};
use theseeker_engine::physics::{CoverCollisionTiles, TileCollisionWorld};
use theseeker_engine::time::GameTickUpdate;

/// Number of hits required to break a breakable wall.
pub const BREAKABLE_WALL_HITS: u32 = 5;

// Match enemy hit/killed shakes from the damage pipeline.
const ON_HIT_SHAKE_STRENGTH: f32 = 0.9;
const ON_HIT_SHAKE_DURATION: f32 = 0.1;
const ON_HIT_SHAKE_FREQ: f32 = 2.0;

const ON_BREAK_SHAKE_STRENGTH: f32 = 2.1;
const ON_BREAK_SHAKE_DURATION: f32 = 0.24;
const ON_BREAK_SHAKE_FREQ: f32 = 3.0;

#[derive(Component)]
pub struct BreakableWall {
    pub hits_remaining: u32,
    /// The room's cover, despawned when the wall breaks.
    pub cover: Entity,
    /// What grows in the room, revealed when the wall breaks.
    pub contents: Option<Entity>,
}

/// Component marking a secret room cover sprite; its `Name` identifies the room.
#[derive(Component)]
pub struct SecretRoomCover;

pub struct BreakableWallPlugin;

impl Plugin for BreakableWallPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            breakable_wall_damage_handler.after(crate::game::combat::damage_source::apply_damage),
        );
    }
}

/// Flashes and squashes a wall on each hit, and opens its room on the last one.
fn breakable_wall_damage_handler(
    mut commands: Commands,
    mut damage_events: MessageReader<DamageInfo>,
    mut walls: Query<&mut BreakableWall>,
    cover_tiles: Query<&CoverCollisionTiles>,
    mut collision_world: ResMut<TileCollisionWorld>,
) {
    for target in damage_events.read().map(|damage| damage.target) {
        // A wall broken earlier this tick is only waiting to despawn.
        let Some(mut wall) = walls.get_mut(target).ok().filter(|wall| wall.hits_remaining > 0) else {
            continue;
        };
        commands.entity(target).insert((DamageFlash { current_ticks: 0, max_ticks: 8 }, SpriteScale::new(), SpriteStretch::default()));
        wall.hits_remaining -= 1;
        info!("Breakable wall hit: {} hits remaining", wall.hits_remaining);
        commands.insert_resource(CameraShake::new(ON_HIT_SHAKE_STRENGTH, ON_HIT_SHAKE_DURATION, ON_HIT_SHAKE_FREQ));
        if wall.hits_remaining > 0 {
            continue;
        }
        info!("Breakable wall destroyed, revealing secret room");
        commands.insert_resource(CameraShake::new(ON_BREAK_SHAKE_STRENGTH, ON_BREAK_SHAKE_DURATION, ON_BREAK_SHAKE_FREQ));
        commands.entity(target).despawn();
        commands.entity(wall.cover).despawn();
        if let Some(contents) = wall.contents {
            commands.entity(contents).insert(Visibility::Inherited);
        }
        if let Some(tiles) = cover_tiles.get(wall.cover).ok().filter(|_| collision_world.is_ready()) {
            collision_world.remove_dynamic_tiles(&tiles.tiles);
        }
    }
}
