use crate::prelude::{GameTickUpdate, HashMap, HashSet};
use bevy::prelude::*;
use rapier2d::prelude::*;

/// Opjects marked with this and a transform component will be updated in the
/// collision scene. Parenting *should* work as expected. TODO make parenting work
#[derive(Component)]
pub struct Collider(rapier2d::prelude::Collider);
pub struct ColliderHandle(rapier2d::prelude::ColliderHandle);

/// Todo: shape caster info; process in update queries pipeline
#[derive(Component)]
pub struct ShapeCaster();

/// Used to create queries on a physics world.
#[derive(Resource)]
pub struct QueryPipeline(rapier2d::prelude::QueryPipeline);

#[derive(Resource, Default)]
pub struct PhysicsWorld {
    pub col_set: ColliderSet,
    pub islands: IslandManager,
    pub rb_set: RigidBodySet,
    /// Used internally to track if any entities where removed.
    id_tracker: HashMap<Entity, rapier2d::prelude::ColliderHandle>,
}

/// A manual implementation of rapier to only use the features required by our project
///
/// It only supports setting colliders in the scene, and making shapecast queries on them.
pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(QueryPipeline(
            rapier2d::prelude::QueryPipeline::new(),
        ));
        app.insert_resource(PhysicsWorld::default());
        app.add_systems(Startup, init_physics_world);
        app.add_systems(GameTickUpdate, update_query_pipeline);
    }
}

fn init_physics_world(collider_set: ResMut<PhysicsWorld>) {
    let mut bodies = RigidBodySet::new();
    let mut colliders = ColliderSet::new();

    let mut physics_pipeline = PhysicsPipeline::new();
    let mut query_pipeline = rapier2d::prelude::QueryPipeline::new();
    query_pipeline.update(&bodies, &collider_set.col_set);
}

/// Updates the pipeline by reading all the positions/components with colliders
///
/// Make sure if you are reading from this in a system, you run after this finishes
///
/// TODO make sure this always runs after the GameTickUpdate; consider creating a seperate
/// [`ScheduleLabel`] for immediately after transform propagation
fn update_query_pipeline(
    mut query_pipeline: ResMut<QueryPipeline>,
    // Mutable reference because collider data is stored in an Arena that pipeline modifies
    mut world: ResMut<PhysicsWorld>,
    phys_obj_query: Query<(&GlobalTransform, Ref<Collider>)>,
    mut removed: RemovedComponents<Collider>,
) {
    let pipeline = &mut query_pipeline.0;
    let PhysicsWorld {
        col_set,
        islands,
        rb_set,
        id_tracker,
    } = &mut *world;
    let mut modified_colliders = vec![];
    for (transform, collider_info) in &phys_obj_query {
        if collider_info.is_added() {
            col_set.insert(collider_info.0.clone());
        };
    }
    let mut removed_colliders = vec![];
    // Go through the col_set, and see if any are there that aren't in our collider_set.
    for removed in removed.read() {
        let Some(removed_id) = id_tracker.get(&removed) else {
            continue;
        };
        removed_colliders.push(*removed_id);
        col_set.remove(*removed_id, islands, rb_set, false);
    }
    pipeline.update_incremental(
        &world.col_set,
        modified_colliders.as_slice(),
        removed_colliders.as_slice(),
        true,
    );
}
