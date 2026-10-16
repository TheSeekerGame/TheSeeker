//! Prepared local world states. Activation only publishes existing assets and bounded deltas.
use super::*;
use bevy::sprite_render::MeshMaterial2d;

#[derive(Clone)]
pub(super) struct TileDelta {
    pub index: usize,
    pub terrain: bool,
    pub hazard: bool,
}

#[derive(Clone, Default)]
pub(super) struct PreparedState {
    pub visuals: Vec<(Entity, Handle<ColorMaterial>)>,
    pub tiles: Vec<TileDelta>,
    pub entities: Vec<Entity>,
}

#[derive(Clone)]
pub(super) struct PreparedAdaptation {
    pub id: u64,
    pub affected_chunks: Vec<IVec2>,
    pub states: [PreparedState; 2],
    active_state: usize,
}

impl Default for PreparedAdaptation {
    fn default() -> Self {
        Self { id: 0, affected_chunks: Vec::new(), states: std::array::from_fn(|_| PreparedState::default()), active_state: 0 }
    }
}

impl PreparedAdaptation {
    pub fn is_active(&self) -> bool { self.active_state != 0 }

    pub fn activate(&mut self, commands: &mut Commands, level: &mut CompiledLevel, collision: &mut TileCollisionWorld,
        visuals: &mut Query<&mut MeshMaterial2d<ColorMaterial>>,
    ) -> usize {
        if self.is_active() { return 0; }
        let state = &self.states[1];
        if state.visuals.iter().any(|(entity, _)| visuals.get(*entity).is_err())
            || state.entities.iter().any(|&entity| commands.get_entity(entity).is_err()) { return 0; }
        apply_tiles(level, &state.tiles);
        collision.patch_terrain(state.tiles.iter().map(|tile| (tile.index, tile.terrain)));
        apply_visuals(visuals, &state.visuals);
        for &entity in &state.entities { commands.entity(entity).insert(hazard_groups(true)); }
        self.active_state = 1;
        state.tiles.len()
    }
}

fn apply_tiles(level: &mut CompiledLevel, tiles: &[TileDelta]) {
    for tile in tiles {
        level.terrain[tile.index] = tile.terrain;
        level.spike_tiles[tile.index] = tile.hazard;
    }
}

fn apply_visuals(query: &mut Query<&mut MeshMaterial2d<ColorMaterial>>, visuals: &[(Entity, Handle<ColorMaterial>)]) {
    for (entity, handle) in visuals {
        if let Ok(mut material) = query.get_mut(*entity) { material.0 = handle.clone(); }
    }
}

pub(super) fn hazard_groups(active: bool) -> theseeker_engine::physics::CollisionGroups {
    use theseeker_engine::physics::{CollisionGroups, Group, groups};
    if active { groups::enemy_attack() } else { CollisionGroups::new(Group::NONE, Group::NONE) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use theseeker_engine::physics::{Collider, CollisionGroups, EntityCollisionWorld, Group,
        PLAYER, ENEMY_INSIDE, rebuild_entity_collision_world};

    #[test]
    fn active_spikes_interact_with_players_inside_and_outside_enemy_bodies() {
        let mut world = World::new();
        world.insert_resource(GameTime::default());
        world.insert_resource(EntityCollisionWorld::default());
        let spike = world.spawn((GlobalTransform::default(), Collider::cuboid(1.0, 1.0),
            hazard_groups(true))).id();
        let mut index = Schedule::default();
        index.add_systems(rebuild_entity_collision_world);
        index.run(&mut world);
        for membership in [PLAYER, ENEMY_INSIDE] {
            assert_eq!(world.resource::<EntityCollisionWorld>().intersect(Vec2::ZERO,
                &Collider::cuboid(1.0, 1.0), CollisionGroups::new(membership, Group::all()), None), vec![spike]);
        }
        world.entity_mut(spike).insert(hazard_groups(false));
        index.run(&mut world);
        assert!(world.resource::<EntityCollisionWorld>().intersect(Vec2::ZERO,
            &Collider::cuboid(1.0, 1.0), CollisionGroups::new(PLAYER, Group::all()), None).is_empty());
    }
}
