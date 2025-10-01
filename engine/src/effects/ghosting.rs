use crate::physics::LinearVelocity;
use crate::prelude::*;
use crate::time::GameTickUpdate;
use bevy::ecs::hierarchy::ChildOf;

/// Small Z offset to render ghosts slightly behind the source by default
const GHOST_Z_EPSILON: f32 = 0.0000001;

/// Color mode for ghost sprites
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GhostColorMode {
    /// Keep original sprite colors
    Original,
    /// Tint by multiplying with the given color
    Tint(Color),
}

/// Fade curve for ghost opacity over time
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FadeCurve {
    /// Linear fade from initial_alpha to 0
    Linear,
    /// Ease-out curve (fast fade at start, slow at end)
    EaseOut,
    /// Exponential decay
    Exponential,
    /// Custom cubic bezier (t^3 interpolation)
    Cubic,
}

/// Scale curve for ghost size over time
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScaleCurve {
    /// Maintain original scale
    Constant,
    /// Shrink to a percentage of original (0.0 to 1.0)
    Shrink(f32),
    /// Grow to a percentage of original (> 1.0)
    Grow(f32),
    /// Shrink horizontally, maintain vertical
    ShrinkHorizontal(f32),
}

/// Component that marks an entity to produce ghost trail effects.
/// Attach to an entity with a `Sprite` (or a `Gent` with a sprite) to emit ghosts.
#[derive(Component, Debug)]
pub struct GhostingSource {
    /// How many ticks between spawning ghosts (1 = every tick, 2 = every other, etc.)
    pub spawn_interval_ticks: u32,
    /// How many ticks each ghost lives
    pub ghost_lifetime_ticks: u32,
    /// Starting opacity for ghosts (0.0 to 1.0)
    pub initial_alpha: f32,
    /// How the ghost fades over time
    pub fade_curve: FadeCurve,
    /// Color mode for the ghosts
    pub color_mode: GhostColorMode,
    /// How the ghost scales over time
    pub scale_over_time: ScaleCurve,
    /// Position offset from source entity
    pub offset: Vec2,
    /// Motion to apply to spawned ghosts over their lifetime
    pub movement: GhostMovement,

    /// Internal counter for spawn timing
    pub ticks_since_last_spawn: u32,
}

impl Default for GhostingSource {
    fn default() -> Self {
        Self {
            spawn_interval_ticks: 2,
            ghost_lifetime_ticks: 48, // 0.5 seconds at 96Hz
            initial_alpha: 0.5,
            fade_curve: FadeCurve::Linear,
            color_mode: GhostColorMode::Original,
            scale_over_time: ScaleCurve::Constant,
            offset: Vec2::ZERO,
            movement: GhostMovement::Static,
            ticks_since_last_spawn: 0,
        }
    }
}

/// Motion model for spawned ghosts
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GhostMovement {
    /// Ghost stays where it spawned
    Static,
    /// Copy linear velocity from the source entity at spawn time
    CopyLinearVelocityFromSelf,
    /// Copy linear velocity from a specific entity at spawn time
    CopyLinearVelocityFrom(Entity),
    /// Copy exact world displacement of the source each tick
    CopyDisplacementFromSelf,
    /// Copy exact world displacement of the specified entity each tick
    CopyDisplacementFrom(Entity),
}

/// Component attached to spawned ghost entities
#[derive(Component, Debug)]
pub struct Ghost {
    /// Ticks remaining before despawn
    pub ticks_remaining: u32,
    /// Total lifetime ticks for calculating progress
    pub max_ticks: u32,
    /// Starting alpha value
    pub initial_alpha: f32,
    /// Fade animation curve
    pub fade_curve: FadeCurve,
    /// Scale animation curve
    pub scale_curve: ScaleCurve,
    /// Base scale to apply curve to
    pub base_scale: Vec3,
    /// Base color (already tinted if applicable, without alpha)
    pub base_color: Color,
    /// Per-tick world displacement to apply while the ghost fades
    pub velocity_per_tick: Vec2,
    /// Optional entity to follow for live velocity updates
    pub follow_velocity_of: Option<Entity>,
    /// Optional entity to mirror world displacement from
    pub follow_displacement_of: Option<Entity>,
    /// Last known position of the tracked entity for displacement following
    pub last_follow_pos: Vec2,
}

pub struct GhostingPlugin;

impl Plugin for GhostingPlugin {
    fn build(&self, app: &mut App) {
        // Configure dedicated ghosting sets so gameplay systems can order around them
        app.configure_sets(
            GameTickUpdate,
            (
                GhostingSet::Spawn
                    .after(crate::animation::AnimationSet::LoopClear)
                    .after(
                        bevy::transform::TransformSystem::TransformPropagate,
                    ),
                GhostingSet::Update.after(GhostingSet::Spawn),
            ),
        );
        // Spawn ghosts from source entities
        app.add_systems(
            GameTickUpdate,
            spawn_ghosts.in_set(GhostingSet::Spawn),
        );

        // Update ghost opacity/scale and cleanup
        app.add_systems(
            GameTickUpdate,
            (update_ghosts, cleanup_ghosts)
                .chain()
                .in_set(GhostingSet::Update),
        );
    }
}

/// System set for ghosting to coordinate ordering with gameplay systems
#[derive(SystemSet, Clone, PartialEq, Eq, Debug, Hash)]
pub enum GhostingSet {
    Spawn,
    Update,
}

/// System that spawns ghost entities from sources
fn spawn_ghosts(
    mut query: Query<(
        Entity,
        &mut GhostingSource,
        &GlobalTransform,
        Option<&Sprite>,
        Option<&crate::gent::Gent>,
        Option<&LinearVelocity>,
    )>,
    gfx_query: Query<(&GlobalTransform, &Sprite), Without<GhostingSource>>,
    vel_query: Query<&LinearVelocity>,
    any_tf_query: Query<&GlobalTransform>,
    mut commands: Commands,
) {
    for (
        source_entity,
        mut source,
        transform,
        sprite_opt,
        gent_opt,
        self_vel_opt,
    ) in query.iter_mut()
    {
        // Check if it's time to spawn a ghost
        source.ticks_since_last_spawn += 1;

        // Check spawn interval (minimum interval is 1 to prevent overlap)
        let interval = source.spawn_interval_ticks.max(1);
        if source.ticks_since_last_spawn < interval {
            continue;
        }
        source.ticks_since_last_spawn = 0;

        // Get the visual entity's sprite and transform
        let (final_transform, final_sprite) = if let Some(gent) = gent_opt {
            // Entity uses Gent system - get sprite from graphics entity
            if let Ok((gfx_transform, gfx_sprite)) = gfx_query.get(gent.e_gfx) {
                (gfx_transform, gfx_sprite)
            } else {
                // No graphics entity found, skip
                continue;
            }
        } else if let Some(sprite) = sprite_opt {
            // Direct sprite on the entity
            (transform, sprite)
        } else {
            // No sprite available, skip
            continue;
        };

        // Calculate ghost position with offset; ensure Z is slightly behind the source
        let source_translation = final_transform.translation();
        let ghost_pos = Vec3::new(
            source_translation.x + source.offset.x,
            source_translation.y + source.offset.y,
            source_translation.z - GHOST_Z_EPSILON,
        );

        // Calculate the base color based on color mode
        let base_color_rgb = match source.color_mode {
            GhostColorMode::Original => {
                // Preserve original color without alpha
                let base = final_sprite.color.to_srgba();
                Color::srgb(base.red, base.green, base.blue)
            },
            GhostColorMode::Tint(tint) => {
                // Multiply colors for tinting effect
                let base = final_sprite.color.to_srgba();
                let tint = tint.to_srgba();
                Color::srgb(
                    base.red * tint.red,
                    base.green * tint.green,
                    base.blue * tint.blue,
                )
            },
        };

        // Create the ghost sprite with initial alpha
        let ghost_color = base_color_rgb.with_alpha(source.initial_alpha);

        // Clone the sprite with current animation frame
        let ghost_sprite = Sprite {
            image: final_sprite.image.clone(),
            texture_atlas: final_sprite.texture_atlas.clone(),
            flip_x: final_sprite.flip_x,
            flip_y: final_sprite.flip_y,
            color: ghost_color,
            anchor: final_sprite.anchor,
            rect: final_sprite.rect,
            custom_size: final_sprite.custom_size,
            image_mode: final_sprite.image_mode.clone(),
        };

        // Get base scale from transform
        let base_scale = final_transform.compute_transform().scale;

        // Determine ghost movement behaviour and parenting
        let mut velocity_per_tick = Vec2::ZERO;
        let mut follow_velocity_of: Option<Entity> = None;
        let mut add_child_of: Option<(Entity, Vec3)> = None; // (parent, local_translation)
        match source.movement {
            GhostMovement::Static => {},
            GhostMovement::CopyLinearVelocityFromSelf => {
                velocity_per_tick =
                    self_vel_opt.map(|v| v.0).unwrap_or(Vec2::ZERO);
                follow_velocity_of = Some(source_entity);
            },
            GhostMovement::CopyLinearVelocityFrom(entity) => {
                velocity_per_tick =
                    vel_query.get(entity).map(|v| v.0).unwrap_or(Vec2::ZERO);
                follow_velocity_of = Some(entity);
            },
            GhostMovement::CopyDisplacementFromSelf => {
                let parent_world = transform.translation();
                let local = Vec3::new(
                    ghost_pos.x - parent_world.x,
                    ghost_pos.y - parent_world.y,
                    ghost_pos.z - parent_world.z,
                );
                add_child_of = Some((source_entity, local));
            },
            GhostMovement::CopyDisplacementFrom(entity) => {
                let parent_world = any_tf_query
                    .get(entity)
                    .map(|xf| xf.translation())
                    .unwrap_or_else(|_| transform.translation());
                let local = Vec3::new(
                    ghost_pos.x - parent_world.x,
                    ghost_pos.y - parent_world.y,
                    ghost_pos.z - parent_world.z,
                );
                add_child_of = Some((entity, local));
            },
        }

        // Choose local or world transform depending on parenting
        let spawn_transform = if let Some((_parent, local)) = add_child_of {
            Transform::from_translation(local).with_scale(base_scale)
        } else {
            Transform::from_translation(ghost_pos).with_scale(base_scale)
        };

        // Compose ghost components
        let mut entity_commands = commands.spawn((
            ghost_sprite,
            spawn_transform,
            GlobalTransform::default(),
            Visibility::default(),
            InheritedVisibility::default(),
            ViewVisibility::default(),
            Ghost {
                ticks_remaining: source.ghost_lifetime_ticks,
                max_ticks: source.ghost_lifetime_ticks,
                initial_alpha: source.initial_alpha,
                fade_curve: source.fade_curve,
                scale_curve: source.scale_over_time,
                base_scale,
                base_color: base_color_rgb,
                velocity_per_tick,
                follow_velocity_of,
                follow_displacement_of: None,
                last_follow_pos: Vec2::ZERO,
            },
        ));
        // If parenting requested, set local transform and relation
        if let Some((parent, _local)) = add_child_of {
            entity_commands.insert((ChildOf(parent),));
        }
    }
}

/// System that updates ghost appearance over time
fn update_ghosts(
    mut query: Query<(&mut Ghost, &mut Sprite, &mut Transform)>,
    vel_query: Query<&LinearVelocity>,
) {
    for (mut ghost, mut sprite, mut transform) in query.iter_mut() {
        // Update lifetime
        if ghost.ticks_remaining > 0 {
            ghost.ticks_remaining = ghost.ticks_remaining.saturating_sub(1);
        }

        // Apply movement (world-space units per tick)
        {
            // Velocity following (live or captured)
            let vel = if let Some(e) = ghost.follow_velocity_of {
                vel_query
                    .get(e)
                    .map(|v| v.0)
                    .unwrap_or(ghost.velocity_per_tick)
            } else {
                ghost.velocity_per_tick
            };
            transform.translation.x += vel.x;
            transform.translation.y += vel.y;
        }

        // Calculate normalized progress (0 = just spawned, 1 = expired)
        let ticks_elapsed =
            ghost.max_ticks.saturating_sub(ghost.ticks_remaining) as f32;
        let progress = ticks_elapsed / ghost.max_ticks as f32;
        let progress = progress.clamp(0.0, 1.0);

        // Apply fade curve
        let alpha = match ghost.fade_curve {
            FadeCurve::Linear => ghost.initial_alpha * (1.0 - progress),
            FadeCurve::EaseOut => {
                // Quadratic ease out: 1 - (1 - x)^2
                let t = 1.0 - progress;
                ghost.initial_alpha * t * t
            },
            FadeCurve::Exponential => {
                // Exponential decay
                ghost.initial_alpha * (1.0 - progress).powf(2.5)
            },
            FadeCurve::Cubic => {
                // Cubic curve for smoother fade
                let t = 1.0 - progress;
                ghost.initial_alpha * t * t * t
            },
        };

        // Update sprite alpha while preserving the base color
        sprite.color = ghost.base_color.with_alpha(alpha);

        // Apply scale curve
        match ghost.scale_curve {
            ScaleCurve::Constant => {
                // Keep original scale
            },
            ScaleCurve::Shrink(target) => {
                let scale = 1.0 - (1.0 - target) * progress;
                transform.scale = ghost.base_scale * scale;
            },
            ScaleCurve::Grow(target) => {
                let scale = 1.0 + (target - 1.0) * progress;
                transform.scale = ghost.base_scale * scale;
            },
            ScaleCurve::ShrinkHorizontal(target) => {
                let h_scale = 1.0 - (1.0 - target) * progress;
                transform.scale.x = ghost.base_scale.x * h_scale;
                transform.scale.y = ghost.base_scale.y;
                transform.scale.z = ghost.base_scale.z;
            },
        }
    }
}

/// System that removes expired ghosts
fn cleanup_ghosts(query: Query<(Entity, &Ghost)>, mut commands: Commands) {
    for (entity, ghost) in query.iter() {
        if ghost.ticks_remaining == 0 {
            commands.entity(entity).despawn();
        }
    }
}
