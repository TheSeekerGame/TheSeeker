use crate::prelude::{GameTickUpdate, HashMap, HashSet};
use bevy::prelude::*;
use rapier2d::na::UnitComplex;
use rapier2d::parry;
use rapier2d::prelude::*;

// Layers used for collision checks.

pub struct Layer(Group);
impl Layer {
    // Todo impl consts
    // Sets it up so that the character and enemies layers only
    // interact with the world
    // Emissions on world layer will ignore colliders in world
    // const WORLD: Layer = Layer(Group::GROUP_2 | Group::GROUP_3);
    // Emissions on character layer will ignore colliders in enemies and character
    //   const CHARACTER: Layer = Layer(Group::GROUP_1);
    // Emissions on enemies layer will ignore colliders in enemies and character
    // const ENEMIES: Layer = Layer(Group::GROUP_1);
}

/// Objects marked with this and a transform component will be updated in the
/// collision scene. Parenting is not currently kept in sync; global transforms are used instead.
#[derive(Component)]
pub struct Collider(rapier2d::prelude::Collider);

impl Collider {
    pub fn cuboid(x_length: f32, y_length: f32) -> Self {
        Self(rapier2d::prelude::ColliderBuilder::cuboid(x_length, y_length).build())
    }
}

/// Just a wrapper that lets us treat rapiers collider handle as a component
#[derive(Component)]
pub struct ColliderHandle(pub rapier2d::prelude::ColliderHandle);

// Todo: shape caster info; process in update queries pipeline
//  or maybe seoperate system
#[derive(Component)]
pub struct ShapeCaster {
    pub shape: SharedShape,
    pub vec: Vec2,
    pub offset: Vec2,
    pub max_toi: f32,
    //layer: Layer,
}

#[derive(Component)]
pub struct ShapeHit(Option<Entity>);

/// Used to create queries on a physics world.
///
/// To add a collider, you don't need this Resource, instead
/// add the [`Collider`] component with the local position relative to the entity transform
///
/// If you need the collider id (for building queries etc)
#[derive(Resource, Default)]
pub struct PhysicsWorld {
    // Can't make query's on this without most of the other structures,
    // so it makes sense to group them.
    pub query_pipeline: rapier2d::prelude::QueryPipeline,
    pub col_set: ColliderSet,
    pub islands: IslandManager,
    pub rb_set: RigidBodySet,
    /// Used internally to track if any entities where removed.
    id_tracker: HashMap<Entity, rapier2d::prelude::ColliderHandle>,
}

impl PhysicsWorld {
    pub fn shape_cast(
        &self,
        origin: Vec2,
        cast: Vec2,
        shape: &dyn Shape,
        max_toi: f32,
        // todo
        // layer: Group,
        exclude: Option<Entity>,
    ) -> Option<(Entity, parry::query::TOI)> {
        let mut filter = QueryFilter::new(); /*.groups(InteractionGroups {
                                                 // I *think* this is setup properly... needs testing though to verify
                                                 memberships: layer.0,
                                                 filter: Group::all(),
                                             });*/
        if let Some(exclude) = exclude {
            // Entity might not be added yet; or even exist.
            if let Some(col_id) = self.id_tracker.get(&exclude) {
                filter = filter.exclude_collider(*col_id)
            }
        }
        let result = self.query_pipeline.cast_shape(
            &self.rb_set,
            &self.col_set,
            &into_vec(origin).into(),
            &into_vec(cast).into(),
            shape,
            max_toi,
            true,
            filter,
        );
        if let Some((collider, toi)) = result {
            let entity: Entity = self.collider2entity(collider);
            Some((entity, toi))
        } else {
            None
        }
    }

    /// Small utility function that gets the entity associated with the collider;
    /// panics if entity does not exist.
    pub fn collider2entity(&self, handle: rapier2d::prelude::ColliderHandle) -> Entity {
        self.col_set
            .get(handle)
            .map(|co| Entity::from_bits(co.user_data as u64))
            .expect("Internal error: entity not found for collider.")
    }
}

/// A manual implementation of rapier to only use the features required by our project
///
/// It only supports setting colliders in the scene, and making shapecast queries on them.
pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PhysicsWorld::default());
        app.add_systems(Startup, init_physics_world);
        app.add_systems(GameTickUpdate, update_query_pipeline);
    }
}

fn init_physics_world(mut world: ResMut<PhysicsWorld>) {
    let PhysicsWorld {
        query_pipeline,
        col_set,
        islands,
        rb_set,
        id_tracker,
    } = &mut *world;
    query_pipeline.update(&rb_set, &col_set);
}

/// Updates the pipeline by reading all the positions/components with colliders
///
/// Make sure if you are reading from this in a system, you run after this finishes
///
/// TODO make sure this always runs after the GameTickUpdate; consider creating a seperate
/// [`ScheduleLabel`] for immediately after transform propagation
fn update_query_pipeline(
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
    let PhysicsWorld {
        query_pipeline,
        col_set,
        islands,
        rb_set,
        id_tracker,
    } = &mut *world;
    //query_pipeline.cast_shape()
    let mut modified_colliders = vec![];
    for (entity, transform, collider_info, handle) in &phys_obj_query {
        let col_id = if collider_info.is_added() && handle.is_none() {
            let col_id = col_set.insert(collider_info.0.clone());
            modified_colliders.push(col_id);
            id_tracker.insert(entity, col_id);
            commands
                .get_entity(entity)
                .unwrap()
                .insert(ColliderHandle(col_id));
            // Sets the user associated data on the collider to the entity id
            // so that when we get a query result with a collider id we can lookup
            // what entity its associated with.
            col_set.get_mut(col_id).unwrap().user_data = entity.to_bits() as u128;
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
    query_pipeline.update_incremental(
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

/// A convenient component type for referring to velocity of an entity.
///
/// Doesn't do anything on its own, but character controllers use it.
#[derive(Component, Deref, DerefMut)]
pub struct LinearVelocity(pub Vec2);
