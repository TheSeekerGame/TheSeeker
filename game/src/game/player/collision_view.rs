//! The player's collision spaces, with capsule policy kept out of global terrain.
use bevy::{ecs::query::QueryFilter, prelude::*};
use theseeker_engine::physics::{TileCollisionWorld, geometry::{Axis, ProbeDirection}};
use super::skills::portable_tent::{collision_space, TentInteriorAssets, TentInteriorOccupant};
use crate::game::world_objects::{self, CapsuleCollisionMask, CapsuleDropPlatformState};

pub(super) type CapsuleCollisionQuery<'w, 's, F> = Query<'w, 's, (Entity, &'static Transform, &'static CapsuleCollisionMask), F>;

pub(super) struct PlayerCollisionView<'a, 'w, 's, F: QueryFilter> {
    pub origin: Vec2,
    tiles: &'a TileCollisionWorld,
    capsules: &'a CapsuleCollisionQuery<'w, 's, F>,
    include_capsules: bool,
}

impl<'a, 'w, 's, F: QueryFilter> PlayerCollisionView<'a, 'w, 's, F> {
    pub fn new(
        occupant: Option<&TentInteriorOccupant>,
        interiors: Option<&'a TentInteriorAssets>,
        terrain: Option<&'a TileCollisionWorld>,
        capsules: &'a CapsuleCollisionQuery<'w, 's, F>,
    ) -> Option<Self> {
        let (tiles, origin) = collision_space(occupant.map(|occupant| occupant.kind), interiors, terrain)?;
        tiles.is_ready().then_some(Self { tiles, origin, capsules, include_capsules: occupant.is_none() })
    }

    /// Axis results use the selected terrain's coordinates, as does actor movement.
    pub fn resolve_axis(
        &self,
        axis: Axis,
        local: Vec2,
        delta: f32,
        half: Vec2,
        platforms: Option<&CapsuleDropPlatformState>,
    ) -> (f32, bool) {
        let terrain = self.tiles.resolve_axis(axis, local, delta, half);
        if !self.include_capsules { return terrain; }
        let capsule = world_objects::resolve_capsule_axis(axis, local, delta, half, platforms, self.capsules);
        let position = if delta < 0.0 { terrain.0.max(capsule.0) } else { terrain.0.min(capsule.0) };
        (position, terrain.1 || capsule.1)
    }

    pub fn distance(
        &self,
        direction: ProbeDirection,
        world: Vec2,
        half: Vec2,
        max: f32,
        platforms: Option<&CapsuleDropPlatformState>,
    ) -> f32 {
        self.probe(direction, world - self.origin, world, half, max, platforms)
    }

    pub fn distance_local(&self, direction: ProbeDirection, local: Vec2, half: Vec2, max: f32) -> f32 {
        self.probe(direction, local, local, half, max, None)
    }

    pub fn overlaps_solid(&self, world: Vec2, half: Vec2) -> bool {
        self.tiles.overlaps_solid(world - self.origin, half)
            || (self.include_capsules
                && world_objects::capsule_overlaps_solid(world, half, self.capsules))
    }

    pub fn tile_size(&self) -> f32 { self.tiles.tile_size }

    fn probe(
        &self,
        direction: ProbeDirection,
        local: Vec2,
        world: Vec2,
        half: Vec2,
        max: f32,
        platforms: Option<&CapsuleDropPlatformState>,
    ) -> f32 {
        let terrain = self.tiles.distance(direction, local, half, max);
        if !self.include_capsules { return terrain; }
        terrain.min(world_objects::capsule_distance(direction, world, half, max, platforms, self.capsules))
    }
}
