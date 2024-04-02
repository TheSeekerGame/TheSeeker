use crate::prelude::{GameTickUpdate, HashMap, HashSet};
use bevy::prelude::*;
use rapier2d::na::UnitComplex;
use rapier2d::prelude::*;

/// Opjects marked with this and a transform component will be updated in the
/// collision scene. Parenting is not currently kept in sync; global transforms are used instead.
#[derive(Component)]
pub struct Collider(rapier2d::prelude::Collider);

#[derive(Component)]
pub struct ColliderHandle(rapier2d::prelude::ColliderHandle);

// Todo: shape caster info; process in update queries pipeline
//#[derive(Component)]
//pub struct ShapeCaster(rapier2d::prelude::Collider);

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

fn init_physics_world(world: Res<PhysicsWorld>, mut query_pipeline: ResMut<QueryPipeline>) {
    let PhysicsWorld {
        col_set,
        islands,
        rb_set,
        id_tracker,
    } = &*world;
    query_pipeline.0.update(&rb_set, &col_set);
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
    phys_obj_query: Query<(
        Entity,
        Ref<GlobalTransform>,
        Ref<Collider>,
        Option<&ColliderHandle>,
    )>,
    mut removed: RemovedComponents<Collider>,
    mut commands: Commands,
) {
    let pipeline = &mut query_pipeline.0;
    let PhysicsWorld {
        col_set,
        islands,
        rb_set,
        id_tracker,
    } = &mut *world;

    let mut modified_colliders = vec![];
    for (entity, transform, collider_info, handle) in &phys_obj_query {
        let col_id = if collider_info.is_added() {
            let col_id = col_set.insert(collider_info.0.clone());
            modified_colliders.push(col_id);
            id_tracker.insert(entity, col_id);
            commands
                .get_entity(entity)
                .unwrap()
                .insert(ColliderHandle(col_id));
            col_id
        } else {
            handle.unwrap().0
        };

        if collider_info.is_changed() {
            *col_set.get_mut(col_id).unwrap() = collider_info.0.clone();
            modified_colliders.push(col_id);
        }
        if transform.is_changed() {
            col_set
                .get_mut(col_id)
                .unwrap()
                .set_translation(into_vec(transform.translation().xy()));
            col_set
                .get_mut(col_id)
                .unwrap()
                .set_rotation(UnitComplex::new(
                    transform
                        .compute_transform()
                        .rotation
                        .to_euler(EulerRot::XYZ)
                        .2,
                ));
            modified_colliders.push(col_id);
        }
    }
    let mut removed_colliders = vec![];
    // Go through the col_set, and see if any are there that aren't in our collider_set.
    for removed in removed.read() {
        let Some(removed_id) = id_tracker.get(&removed).copied() else {
            continue;
        };
        id_tracker.remove(&removed);
        removed_colliders.push(removed_id);
        col_set.remove(removed_id, islands, rb_set, false);
    }
    pipeline.update_incremental(
        &col_set,
        modified_colliders.as_slice(),
        removed_colliders.as_slice(),
        true,
    );
}

/// Utility to convert from vec to rapier compatible structure
pub fn into_vec(vec: Vec2) -> Vector<f32> {
    vector![vec.x, vec.y]
}
