

use bevy::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy::transform::TransformSystem::TransformPropagate;
use bevy_rapier2d::prelude::*;
use bevy_rapier2d::rapier::prelude::{SharedShape, Shape};
use bevy_rapier2d::plugin::context::systemparams::{ReadRapierContext, RapierContext};

use crate::prelude::{GameTickUpdate, HashMap};
use crate::script::ScriptSet;

/// Re-export bevy_rapier2d types for compatibility
pub use bevy_rapier2d::prelude::{
    Collider, CollisionGroups, Group, QueryFilter,
    RapierPhysicsPlugin as InternalPhysicsPlugin, Real, Rot, Vect,
    ExternalImpulse, ExternalForce, RigidBody, Velocity,
    ShapeCastHit, RayIntersection, PointProjection,
    ShapeCastOptions,
};
pub use bevy_rapier2d::rapier::geometry::ShapeType;
pub use bevy_rapier2d::rapier;

/// Re-export ShapeCastStatus from rapier
pub use bevy_rapier2d::rapier::parry::query::ShapeCastStatus;

/// Alias for backwards compatibility
pub use CollisionGroups as InteractionGroups;

/// Physics plugin that wraps bevy_rapier2d
pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        // Add the bevy_rapier2d plugin with pixels_per_meter scaling
        app.add_plugins(InternalPhysicsPlugin::<NoUserData>::pixels_per_meter(1.0));
        
        // Keep the sprite shape map for animation colliders
        app.init_resource::<SpriteShapeMap>();
        app.add_systems(Startup, init_physics_world);
        
        app.configure_sets(
            GameTickUpdate,
            PhysicsSet.after(TransformPropagate),
        );
        
        app.add_systems(
            GameTickUpdate,
            (
                update_sprite_colliders
                    .before(PhysicsSet)
                    .after(ScriptSet::Run),
            ),
        );
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PhysicsSet;

/// The player collision group
pub const PLAYER: Group = Group::GROUP_1;
/// The enemy collision group
pub const ENEMY: Group = Group::GROUP_2;
/// The player attack collision group
pub const PLAYER_ATTACK: Group = Group::GROUP_3;
/// The enemy attack collision group
pub const ENEMY_ATTACK: Group = Group::GROUP_4;
/// The ground collision group
pub const GROUND: Group = Group::GROUP_5;
/// Use when the other groups don't make sense,
/// and you just want to detect something
pub const SENSOR: Group = Group::GROUP_6;
/// Applied to an enemy when player is inside it
pub const ENEMY_INSIDE: Group = Group::GROUP_7;
/// Combination of ENEMY and ENEMY_INSIDE,
/// used for checking the players attacks
pub const ENEMY_HURT: Group = Group::from_bits_truncate(0b1000010);

/// Distance we keep the shape-cast above ground.
pub const GROUNDED_THRESHOLD: f32 = 1.0;
/// Offset applied after snapping to ground (negative to avoid penetration).
pub const GROUND_BUFFER: f32 = -1.0;

/// Centralised definitions for commonly-used collision-layer masks.
///
/// These should be used everywhere instead of hand-rolling
/// `CollisionGroups::new(...)` so that the two-way Rapier bit-test stays
/// consistent across the codebase.
pub mod groups {
    use super::*;

    // ---------- helpers (runtime) -----------------

    /// Derive the canonical *filter* mask for a collider whose `memberships`
    /// are `memberships`.  This encodes the handshake contract we want for
    /// each high-level category (player, enemy, etc.).  Centralising this
    /// logic guarantees that colliders and queries never diverge.
    pub fn default_filter(memberships: Group) -> Group {
        match memberships.bits() {
            bits if bits == super::PLAYER.bits() => super::ENEMY | super::ENEMY_ATTACK | super::GROUND,
            bits if bits == super::ENEMY.bits() => super::PLAYER | super::PLAYER_ATTACK | super::GROUND,
            bits if bits == super::PLAYER_ATTACK.bits() => super::ENEMY | super::GROUND,
            bits if bits == super::ENEMY_ATTACK.bits() => super::PLAYER | super::ENEMY_INSIDE,
            bits if bits == super::ENEMY_INSIDE.bits() => super::PLAYER,
            _ => Group::all(),
        }
    }

    /// Convenience: build the full `CollisionGroups` for a given membership.
    #[inline]
    pub fn groups(memberships: Group) -> CollisionGroups {
        CollisionGroups::new(memberships, default_filter(memberships))
    }

    // ----------- convenience builders -------------

    /// Standard body collider for the player (blocks enemy bodies & ground).
    #[inline]
    pub fn player_body() -> CollisionGroups {
        groups(super::PLAYER)
    }

    /// Standard body collider for enemies (blocks player body/attacks & ground).
    #[inline]
    pub fn enemy_body() -> CollisionGroups {
        groups(super::ENEMY)
    }

    /// Collider used by all player-owned hitboxes (sword, arrow, etc.).
    #[inline]
    pub fn player_attack() -> CollisionGroups {
        groups(super::PLAYER_ATTACK)
    }

    /// Collider for enemy-generated damage (melee hulls, projectiles, …)
    #[inline]
    pub fn enemy_attack() -> CollisionGroups {
        groups(super::ENEMY_ATTACK)
    }
}

#[derive(Resource, Default)]
pub struct SpriteShapeMap {
    /// Normal, flipped x, flipped y, and flipped x & y, respectively
    pub shapes: Vec<(
        SharedShape,
        SharedShape,
        SharedShape,
        SharedShape,
    )>,
    pub map: HashMap<AssetId<Image>, Vec<usize>>,
}

/// Caches which frame/flips have already been applied to a given collider so we
/// can avoid rewriting the Collider component every tick.  This greatly reduces
/// archetype churn and Rapier shape hashing without altering behaviour.
#[derive(Component, Default, Debug)]
pub struct StoredColliderFrame {
    pub frame_index: usize,
    pub flip_x: bool,
    pub flip_y: bool,
}

pub fn update_sprite_colliders(
    shape_map: Res<SpriteShapeMap>,
    q_sprite: Query<&Sprite>,
    mut q_collider: Query<
        (Entity, &mut Collider, &AnimationCollider, Option<&mut StoredColliderFrame>),
    >,
    mut commands: Commands,
) {
    for (entity, mut collider, anim_entity, cached_opt) in &mut q_collider {
        let sprite = match q_sprite.get(anim_entity.0) {
            Ok(sprite) => sprite,
            Err(_) => continue,
        };
        
        let texture_atlas = match &sprite.texture_atlas {
            Some(atlas) => atlas,
            None => continue,
        };
        
        let atlas_index = texture_atlas.index;
        let image_id = sprite.image.id();
        
        let shape_indices = match shape_map.map.get(&image_id) {
            Some(indices) => indices,
            None => continue,
        };
        
        let shape_index = match shape_indices.get(atlas_index) {
            Some(index) => *index,
            None => continue,
        };
        
        // Early-out: if the requested frame & flip is identical to what is already set, skip work.
        if let Some(cached) = cached_opt.as_ref() {
            if cached.frame_index == shape_index
                && cached.flip_x == sprite.flip_x
                && cached.flip_y == sprite.flip_y
            {
                continue;
            }
        }

        // Fetch the canonical (and pre-flipped) shapes.
        let convex_hull = match shape_map.shapes.get(shape_index) {
            Some(hull) => hull,
            None => continue,
        };

        // Select correct variant and write collider.
        let new_shape = match (sprite.flip_x, sprite.flip_y) {
            (false, false) => &convex_hull.0,
            (true, false) => &convex_hull.1,
            (false, true) => &convex_hull.2,
            (true, true) => &convex_hull.3,
        };
        *collider = Collider::from(new_shape.clone());

        // Update or insert cache component so future frames can early-out.
        match cached_opt {
            Some(mut cached) => {
                cached.frame_index = shape_index;
                cached.flip_x = sprite.flip_x;
                cached.flip_y = sprite.flip_y;
            },
            None => {
                commands.entity(entity).insert(StoredColliderFrame {
                    frame_index: shape_index,
                    flip_x: sprite.flip_x,
                    flip_y: sprite.flip_y,
                });
            },
        }
    }
}

/// An animation collider accepts a target entity that has the image, texture atlas,
/// and sprite animation components.
///
/// - You put the AnimationCollider component along with any collider type on an entity,
/// and it will use the shapes generated by the animation on the target entity as its
/// collider shape.
/// - If there are no shapes generated by the frame of the animation,
/// the shape will not collide with anything.
/// - Shapes are generated from the *convex_hull* of magenta pixels in the animation.
/// (ie: if you put a rubber band around the magenta pixel center points)
///
/// If you want to do a PhysicsWorld query on a ([`Collider`], [`AnimationCollider`]) entity,
/// make sure the query runs *after* [`update_sprite_colliders`]
#[derive(Component)]
pub struct AnimationCollider(pub Entity);

/// Component for shape casting
#[derive(Component)]
pub struct ShapeCaster {
    pub shape: SharedShape,
    /// Offsets the origin of the shape cast from the transform
    pub origin: Vec2,
    pub direction: Dir2,
    pub max_toi: f32,
    pub interaction: CollisionGroups,
}

impl ShapeCaster {
    pub fn cast(
        &self,
        rapier_context: &RapierContext,
        transform: &Transform,
        ignore: Option<Entity>,
    ) -> Option<(Entity, ShapeCastHit)> {
        let origin = transform.translation.xy() + self.origin;
        
        let mut filter = QueryFilter::new().groups(self.interaction);
        if let Some(entity) = ignore {
            filter = filter.exclude_collider(entity);
        }

        rapier_context.cast_shape(
            origin,
            transform.rotation.to_euler(EulerRot::XYZ).2,
            self.direction.xy(),
            &Collider::from(self.shape.clone()),
            ShapeCastOptions {
                max_time_of_impact: self.max_toi,
                target_distance: 0.0,
                stop_at_penetration: true,
                compute_impact_geometry_on_penetration: true,
            },
            filter,
        )
    }
}

/// System parameter wrapper for physics queries  
/// This provides a simplified interface to RapierContext that matches the old API
#[derive(SystemParam)]
pub struct PhysicsWorld<'w, 's> {
    rapier_context: ReadRapierContext<'w, 's>,
}

impl<'w, 's> PhysicsWorld<'w, 's> {
    /// Get the underlying RapierContext value (copied)
    pub fn context(&self) -> RapierContext {
        self
            .rapier_context
            .single()
            .expect("RapierContext resource missing")
    }
    
    /// Cast a shape and find the first collision
    pub fn shape_cast(
        &self,
        origin: Vec2,
        direction: Dir2,
        shape: &dyn Shape,
        max_toi: f32,
        interaction: CollisionGroups,
        exclude: Option<Entity>,
    ) -> Option<(Entity, ShapeCastHit)> {
        let context = self.context();
        let mut filter = QueryFilter::new().groups(interaction);
        if let Some(entity) = exclude {
            filter = filter.exclude_collider(entity);
        }
        
        // Convert the shape to a Collider
        let collider = shape_to_collider(shape);
        
        context.cast_shape(
            origin,
            0.0, // rotation
            direction.xy(),
            &collider,
            ShapeCastOptions {
                max_time_of_impact: max_toi,
                target_distance: 0.0,
                stop_at_penetration: true,
                compute_impact_geometry_on_penetration: true,
            },
            filter,
        )
    }
    
    /// Cast a ray and find the first collision
    pub fn ray_cast(
        &self,
        origin: Vec2,
        cast: Vec2,
        max_toi: f32,
        solid: bool,
        interaction: CollisionGroups,
        exclude: Option<Entity>,
    ) -> Option<(Entity, RayIntersection)> {
        let context = self.context();
        let mut filter = QueryFilter::new().groups(interaction);
        if let Some(entity) = exclude {
            filter = filter.exclude_collider(entity);
        }
        
        context.cast_ray_and_get_normal(
            origin,
            cast.normalize_or_zero(),
            max_toi,
            solid,
            filter,
        )
    }
    
    /// Find all entities with shapes intersecting the given shape
    pub fn intersect(
        &self,
        origin: Vec2,
        shape: &dyn Shape,
        interaction: CollisionGroups,
        exclude: Option<Entity>,
    ) -> Vec<Entity> {
        let context = self.context();
        let mut intersections = Vec::new();
        let mut filter = QueryFilter::new().groups(interaction);
        if let Some(entity) = exclude {
            filter = filter.exclude_collider(entity);
        }
        
        // Convert the shape to a Collider
        let collider = shape_to_collider(shape);
        
        context.intersections_with_shape(
            origin,
            0.0,
            &collider,
            filter,
            |entity| {
                intersections.push(entity);
                true
            },
        );
        
        intersections
    }
    
    /// Project a point onto the nearest collider
    pub fn point_project(
        &self,
        point: Vec2,
        interaction: CollisionGroups,
        exclude: Option<Entity>,
    ) -> Option<(Entity, PointProjection)> {
        let context = self.context();
        let mut filter = QueryFilter::new().groups(interaction);
        if let Some(entity) = exclude {
            filter = filter.exclude_collider(entity);
        }
        
        context.project_point(point, true, filter)
    }
}

fn init_physics_world() {
    // Initialization is handled by RapierPhysicsPlugin
    // This hook is kept for future custom physics initialization if needed
}

/// Helper function to convert a Shape to a Collider
fn shape_to_collider(shape: &dyn Shape) -> Collider {
    if let Some(ball) = shape.as_ball() {
        Collider::ball(ball.radius)
    } else if let Some(cuboid) = shape.as_cuboid() {
        Collider::cuboid(cuboid.half_extents.x, cuboid.half_extents.y)
    } else if let Some(capsule) = shape.as_capsule() {
        // Calculate half-height from segment endpoints
        let half_height = (capsule.segment.a.y - capsule.segment.b.y).abs() * 0.5;
        Collider::capsule_y(half_height, capsule.radius)
    } else if let Some(convex) = shape.as_convex_polygon() {
        // Collect the vertices and build a new convex-hull collider
        // If the hull construction fails (shouldn't), fall back to a small ball so the query still works.
        let verts: Vec<Vec2> = convex
            .points()
            .iter()
            .map(|p| Vec2::new(p.x as f32, p.y as f32))
            .collect();
        Collider::convex_hull(&verts).unwrap_or_else(|| Collider::ball(1.0))
    } else {
        // Default to a small ball if we can't determine the shape type
        Collider::ball(1.0)
    }
}

pub fn into_vec(vec: Vec2) -> Real {
    vec.length()
}

pub fn into_vec2(vec: Vec2) -> Vec2 {
    vec
}

/// Linear velocity is now represented by the Velocity component
#[derive(Component, Default, Clone, Copy, Debug, PartialEq)]
pub struct LinearVelocity(pub Vec2);

// Add conversion implementations for LinearVelocity
impl From<LinearVelocity> for Velocity {
    fn from(linear: LinearVelocity) -> Self {
        Velocity {
            linvel: linear.0,
            angvel: 0.0,
        }
    }
}

impl From<&LinearVelocity> for Velocity {
    fn from(linear: &LinearVelocity) -> Self {
        Velocity {
            linvel: linear.0,
            angvel: 0.0,
        }
    }
}

// Extension methods for easy migration
pub trait ColliderExt {
    fn cuboid(x_length: f32, y_length: f32, interaction: CollisionGroups) -> Collider;
    fn empty(interaction: CollisionGroups) -> Collider;
}

impl ColliderExt for Collider {
    fn cuboid(x_length: f32, y_length: f32, _interaction: CollisionGroups) -> Collider {
        // Note: bevy_rapier2d uses half-extents for cuboids
        // Collision groups are now handled separately as components
        Collider::cuboid(x_length * 0.5, y_length * 0.5)
    }
    
    fn empty(_interaction: CollisionGroups) -> Collider {
        // Create a small sensor collider
        Collider::cuboid(2.5, 2.5)
    }
}

// Helper trait to get the shape from a Collider
pub trait ColliderShapeAccess {
    fn shape(&self) -> &dyn Shape;
    fn shared_shape(&self) -> &SharedShape;
}

impl ColliderShapeAccess for Collider {
    fn shape(&self) -> &dyn Shape {
        self.into()
    }
    
    fn shared_shape(&self) -> &SharedShape {
        &self.raw
    }
}

// INSERT helper module for "inside" relationship
pub mod inside {
    use super::{groups, ENEMY_INSIDE, CollisionGroups, Group};
    use bevy::prelude::*;

    /// Marker put on the player while clipped inside an enemy.
    #[derive(Component)]
    pub struct PlayerInsideEnemy;

    /// Marker put on the enemy while the player is inside it.
    #[derive(Component)]
    pub struct EnemyInsidePlayer;

    /// Attach ENEMY_INSIDE group to `player`, add marker to `enemy`.
    pub fn set(commands: &mut Commands, player: Entity, enemy: Entity) {
        commands
            .entity(player)
            .insert(CollisionGroups::new(ENEMY_INSIDE, Group::all()))
            .insert(PlayerInsideEnemy);
        commands.entity(enemy).insert(EnemyInsidePlayer);
    }

    /// Revert both sides to their default bodies.
    pub fn clear(commands: &mut Commands, enemy: Entity, player: Entity) {
        commands
            .entity(player)
            .remove::<PlayerInsideEnemy>()
            .insert(groups::player_body());
        commands.entity(enemy).remove::<EnemyInsidePlayer>();
    }
}