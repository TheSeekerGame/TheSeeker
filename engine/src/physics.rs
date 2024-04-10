use crate::prelude::{GameTickUpdate, HashMap, HashSet};
use bevy::prelude::*;
use bevy::transform::TransformSystem::TransformPropagate;
use rapier2d::na::{Unit, UnitComplex};
use rapier2d::parry;
use rapier2d::prelude::*;

/// The player collision group
pub const PLAYER: Group = Group::from_bits_truncate(0b0001);
/// The enemy collision group
pub const ENEMY: Group = Group::from_bits_truncate(0b0010);
/// The ground collision group
pub const GROUND: Group = Group::from_bits_truncate(0b0100);
/// The for when the other two groups don't make sense,
/// and you just want to detect something
pub const SENSOR: Group = Group::from_bits_truncate(0b1000);

/// Objects marked with this and a transform component will be updated in the
/// collision scene. Parenting is not currently kept in sync; global transforms are used instead.
/// Colliders ignore all scaling!
#[derive(Component)]
pub struct Collider(pub rapier2d::prelude::Collider);

impl Collider {
    pub fn cuboid(x_length: f32, y_length: f32, interaction: InteractionGroups) -> Self {
        // Rapiers cuboid is subtely different from xpbd, as rapier is defined by its
        // half extents, and xpbd is by its extents.
        Self(
            rapier2d::prelude::ColliderBuilder::cuboid(x_length * 0.5, y_length * 0.5)
                .collision_groups(interaction)
                .build(),
        )
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
    /// Offsets the origin of the shape cast from the transform
    pub origin: Vec2,
    pub direction: Direction2d,
    pub max_toi: f32,
    pub interaction: InteractionGroups,
}

impl ShapeCaster {
    pub fn cast(
        &self,
        physics_world: &PhysicsWorld,
        transform: &Transform,
        ignore: Option<Entity>,
    ) -> Option<(Entity, parry::query::TOI)> {
        let origin = transform.translation.xy() + self.origin;
        let shape = &*self.shape;

        physics_world.shape_cast(
            origin,
            self.direction,
            shape,
            self.max_toi,
            self.interaction,
            ignore,
        )
    }
}

/*#[derive(Component)]
pub struct ShapeHit(Option<Entity>);*/

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
        direction: Direction2d,
        shape: &dyn Shape,
        max_toi: f32,
        interaction: InteractionGroups,
        exclude: Option<Entity>,
    ) -> Option<(Entity, parry::query::TOI)> {
        let mut filter = QueryFilter::new().groups(interaction);
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
            &into_vec(direction.xy()).into(),
            shape,
            max_toi,
            true,
            filter,
        );
        if let Some((collider, toi)) = result {
            let entity: Entity = self.collider2entity(collider)?;
            Some((entity, toi))
        } else {
            None
        }
    }

    pub fn ray_cast(
        &self,
        origin: Vec2,
        cast: Vec2,
        max_toi: f32,
        interaction: InteractionGroups,
        exclude: Option<Entity>,
    ) -> Option<(Entity, parry::query::RayIntersection)> {
        let mut filter = QueryFilter::new().groups(interaction);
        if let Some(exclude) = exclude {
            if let Some(col_id) = self.id_tracker.get(&exclude) {
                filter = filter.exclude_collider(*col_id)
            }
        }
        let ray = Ray::new(
            into_vec(origin).into(),
            into_vec(cast).into(),
        );
        let result = self.query_pipeline.cast_ray_and_get_normal(
            &self.rb_set,
            &self.col_set,
            &ray,
            max_toi,
            true,
            filter,
        );
        if let Some((collider, intersection)) = result {
            let entity: Entity = self.collider2entity(collider)?;
            Some((entity, intersection))
        } else {
            None
        }
    }

    pub fn intersect(
        &self,
        origin: Vec2,
        shape: &dyn Shape,
        interaction: InteractionGroups,
        exclude: Option<Entity>,
    ) -> Vec<Entity> {
        let mut filter = QueryFilter::new().groups(interaction);
        if let Some(exclude) = exclude {
            if let Some(col_id) = self.id_tracker.get(&exclude) {
                filter = filter.exclude_collider(*col_id)
            }
        }
        let mut intersections = Vec::new();
        self.query_pipeline.intersections_with_shape(
            &self.rb_set,
            &self.col_set,
            &into_vec(origin).into(),
            shape,
            filter,
            |collider| {
                let entity: Entity = self.collider2entity(collider).unwrap();
                intersections.push(entity);
                true
            },
        );
        intersections
    }

    /// Small utility function that gets the entity associated with the collider;
    /// panics if entity does not exist.
    pub fn collider2entity(&self, handle: rapier2d::prelude::ColliderHandle) -> Option<Entity> {
        if let Some(result) = self.col_set.get(handle) {
            match Entity::try_from_bits(result.user_data as u64) {
                Ok(e) => Some(e),
                Err(e) => {
                    if result.user_data == 0 {
                        println!("Warning! detected colider with no associated entity/user data!");
                    }
                    println!("Warning Failed to find entity for collider!: tried entity: {} with col: {handle:?}", result.user_data);
                    None
                },
            }
        } else {
            None
        }
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
struct PhysicsSet;

/// A manual implementation of rapier to only use the features required by our project
///
/// It only supports setting colliders in the scene, and making shapecast queries on them.
pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PhysicsWorld::default());
        app.add_systems(Startup, init_physics_world);
        app.configure_sets(
            GameTickUpdate,
            PhysicsSet.after(TransformPropagate),
        );
        app.add_systems(
            GameTickUpdate,
            update_query_pipeline.in_set(PhysicsSet),
        );
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
pub fn update_query_pipeline(
    // Mutable reference because collider data is stored in an Arena that pipeline modifies
    mut world: ResMut<PhysicsWorld>,
    phys_obj_query: Query<(
        Entity,
        &Transform,
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
    for (entity, trnsfm, transform, collider_info, handle) in &phys_obj_query {
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
            //println!("new collider added: {col_id:?}");
            col_id
        } else {
            handle.unwrap().0
        };

        if collider_info.is_changed() {
            let old_entity = col_set.get(col_id).unwrap().user_data;
            *col_set.get_mut(col_id).unwrap() = collider_info.0.clone();
            col_set.get_mut(col_id).unwrap().user_data = old_entity;
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

/// Utility to convert from [`Vec2`] to rapier compatible structure
pub fn into_vec(vec: Vec2) -> Vector<f32> {
    vector![vec.x, vec.y]
}

/// Utility to convert from rapier type to [`Vec2`]
pub fn into_vec2(vec: Unit<Vector<f32>>) -> Vec2 {
    Vec2::new(vec.x, vec.y)
}

/// A convenient component type for referring to velocity of an entity.
///
/// Doesn't do anything on its own, but character controllers use it.
#[derive(Component, Deref, DerefMut)]
pub struct LinearVelocity(pub Vec2);
