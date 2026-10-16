use bevy::sprite::Anchor;
use crate::physics::LinearVelocity;
use crate::prelude::*;
use crate::time::GameTickUpdate;
use bevy::ecs::hierarchy::ChildOf;
use bevy::camera::visibility::RenderLayers;

/// Small Z offset to render ghosts slightly behind the source by default
const GHOST_Z_EPSILON: f32 = 0.000_000_1;

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
    /// Keep alpha constant for full-lifetime visibility
    Constant,
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
    /// Scale each axis independently to a target percentage of original
    Axes { x: f32, y: f32 },
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
    /// Render layers for spawned ghosts. If None, uses GhostingDefaults.
    pub render_layers: Option<RenderLayers>,

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
            render_layers: None,
            ticks_since_last_spawn: 0,
        }
    }
}

/// Global defaults for ghosting. Insert this resource to configure default render layers
/// for all ghosts that don't specify their own.
#[derive(Resource, Default, Clone)]
pub struct GhostingDefaults {
    /// Default render layers for ghosts when source doesn't specify.
    pub render_layers: Option<RenderLayers>,
}

/// Motion model for spawned ghosts
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GhostMovement {
    /// Ghost stays where it spawned
    Static,
    /// Follow the source velocity, retaining the spawn velocity if the source disappears
    CopyLinearVelocityFromSelf,
    /// Follow a specific entity velocity, retaining the spawn velocity if it disappears
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
    motion: GhostMotion,
}

pub struct GhostingPlugin;

impl Plugin for GhostingPlugin {
    fn build(&self, app: &mut App) {
        // Configure dedicated ghosting sets so gameplay systems can order around them
        app.configure_sets(
            GameTickUpdate,
            (
                GhostingSet::Spawn
                    .after(crate::animation::AnimationSet::Signals)
                    .after(
                        bevy::transform::TransformSystems::Propagate,
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
            update_ghosts.in_set(GhostingSet::Update),
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
        Option<&Anchor>,
        Option<&crate::gent::Gent>,
        Option<&LinearVelocity>,
    )>,
    gfx_query: Query<(&GlobalTransform, &Sprite, &Anchor), Without<GhostingSource>>,
    vel_query: Query<&LinearVelocity>,
    any_tf_query: Query<&GlobalTransform>,
    defaults: Option<Res<GhostingDefaults>>,
    mut commands: Commands,
) {
    for (
        source_entity,
        mut source,
        transform,
        sprite_opt,
        anchor_opt,
        gent_opt,
        self_vel_opt,
    ) in query.iter_mut()
    {
        if !source_ready_to_spawn(&mut source) {
            continue;
        }

        let Some(visual) =
            source_visual(transform, sprite_opt, anchor_opt, gent_opt, &gfx_query)
        else {
            continue;
        };

        let ghost_pos = ghost_spawn_position(visual.transform, source.offset);
        let base_color_rgb =
            ghost_base_color(visual.sprite, source.color_mode);
        let ghost_color = base_color_rgb.with_alpha(source.initial_alpha);
        let ghost_sprite = ghost_sprite_from_source(visual.sprite, ghost_color);
        let source_world_transform = visual.transform.compute_transform();
        let base_scale = source_world_transform.scale;
        let base_rotation = source_world_transform.rotation;
        let motion = ghost_spawn_motion(
            source_entity,
            &source,
            transform,
            ghost_pos,
            self_vel_opt,
            &vel_query,
            &any_tf_query,
        );
        let spawn_transform =
            ghost_spawn_transform(ghost_pos, base_rotation, base_scale, motion);

        spawn_ghost_entity(
            &mut commands,
            &source,
            GhostSpawnBundle {
                sprite: ghost_sprite,
                anchor: visual.anchor,
                transform: spawn_transform,
                base_scale,
                base_color: base_color_rgb,
                motion,
            },
            defaults.as_deref(),
        );
    }
}

struct SourceVisual<'a> {
    transform: &'a GlobalTransform,
    sprite: &'a Sprite,
    anchor: Anchor,
}

#[derive(Debug, Clone, Copy, Default)]
enum GhostMotion {
    #[default]
    Static,
    Velocity { source: Entity, fallback: Vec2 },
    Parent { entity: Entity, local: Vec3 },
}

struct GhostSpawnBundle {
    sprite: Sprite,
    anchor: Anchor,
    transform: Transform,
    base_scale: Vec3,
    base_color: Color,
    motion: GhostMotion,
}

fn source_ready_to_spawn(source: &mut GhostingSource) -> bool {
    source.ticks_since_last_spawn += 1;
    let interval = source.spawn_interval_ticks.max(1);
    if source.ticks_since_last_spawn < interval {
        return false;
    }
    source.ticks_since_last_spawn = 0;
    true
}

fn source_visual<'a>(
    transform: &'a GlobalTransform,
    sprite_opt: Option<&'a Sprite>,
    anchor_opt: Option<&Anchor>,
    gent_opt: Option<&crate::gent::Gent>,
    gfx_query: &'a Query<(&GlobalTransform, &Sprite, &Anchor), Without<GhostingSource>>,
) -> Option<SourceVisual<'a>> {
    if let Some(gent) = gent_opt {
        let (gfx_transform, gfx_sprite, gfx_anchor) = gfx_query.get(gent.e_gfx).ok()?;
        return Some(SourceVisual {
            transform: gfx_transform,
            sprite: gfx_sprite,
            anchor: *gfx_anchor,
        });
    }

    sprite_opt.map(|sprite| SourceVisual { transform, sprite, anchor: anchor_opt.copied().unwrap_or_default() })
}

fn ghost_spawn_position(transform: &GlobalTransform, offset: Vec2) -> Vec3 {
    let source_translation = transform.translation();
    Vec3::new(
        (source_translation.x + offset.x).round(),
        (source_translation.y + offset.y).round(),
        source_translation.z - GHOST_Z_EPSILON,
    )
}

fn ghost_base_color(sprite: &Sprite, color_mode: GhostColorMode) -> Color {
    let base = sprite.color.to_srgba();
    match color_mode {
        GhostColorMode::Original => Color::srgb(base.red, base.green, base.blue),
        GhostColorMode::Tint(tint) => {
            let tint = tint.to_srgba();
            Color::srgb(
                base.red * tint.red,
                base.green * tint.green,
                base.blue * tint.blue,
            )
        },
    }
}

fn ghost_sprite_from_source(source: &Sprite, color: Color) -> Sprite {
    Sprite {
        image: source.image.clone(),
        texture_atlas: source.texture_atlas.clone(),
        flip_x: source.flip_x,
        flip_y: source.flip_y,
        color,
        rect: source.rect,
        custom_size: source.custom_size,
        image_mode: source.image_mode.clone(),
    }
}

fn ghost_spawn_motion(
    source_entity: Entity,
    source: &GhostingSource,
    transform: &GlobalTransform,
    ghost_pos: Vec3,
    self_vel_opt: Option<&LinearVelocity>,
    vel_query: &Query<&LinearVelocity>,
    any_tf_query: &Query<&GlobalTransform>,
) -> GhostMotion {
    match source.movement {
        GhostMovement::Static => GhostMotion::default(),
        GhostMovement::CopyLinearVelocityFromSelf => GhostMotion::Velocity {
            source: source_entity,
            fallback: self_vel_opt.map(|v| v.0).unwrap_or(Vec2::ZERO),
        },
        GhostMovement::CopyLinearVelocityFrom(entity) => GhostMotion::Velocity {
            source: entity,
            fallback: vel_query.get(entity).map(|v| v.0).unwrap_or(Vec2::ZERO),
        },
        GhostMovement::CopyDisplacementFromSelf => {
            displacement_follow_motion(source_entity, transform.translation(), ghost_pos)
        },
        GhostMovement::CopyDisplacementFrom(entity) => {
            let parent_world = any_tf_query
                .get(entity)
                .map(bevy::prelude::GlobalTransform::translation)
                .unwrap_or_else(|_| transform.translation());
            displacement_follow_motion(entity, parent_world, ghost_pos)
        },
    }
}


fn displacement_follow_motion(parent: Entity, parent_world: Vec3, ghost_pos: Vec3) -> GhostMotion {
    GhostMotion::Parent { entity: parent, local: ghost_pos - parent_world }
}

fn ghost_spawn_transform(
    ghost_pos: Vec3,
    base_rotation: Quat,
    base_scale: Vec3,
    motion: GhostMotion,
) -> Transform {
    if let GhostMotion::Parent { local, .. } = motion {
        return Transform::from_translation(local).with_scale(base_scale);
    }

    Transform::from_translation(ghost_pos)
        .with_rotation(base_rotation)
        .with_scale(base_scale)
}

fn spawn_ghost_entity(
    commands: &mut Commands,
    source: &GhostingSource,
    spawn: GhostSpawnBundle,
    defaults: Option<&GhostingDefaults>,
) {
    let mut entity_commands = commands.spawn((
        spawn.sprite,
        spawn.anchor,
        crate::presentation::PresentationSource,
        spawn.transform,
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
            base_scale: spawn.base_scale,
            base_color: spawn.base_color,
            motion: spawn.motion,
        },
    ));

    let layers = source
        .render_layers
        .clone()
        .or_else(|| defaults.and_then(|d| d.render_layers.clone()));
    if let Some(layers) = layers {
        entity_commands.insert(layers);
    }

    if let GhostMotion::Parent { entity: parent, .. } = spawn.motion {
        entity_commands.insert((ChildOf(parent),));
    }
}

/// System that updates ghost appearance over time
fn update_ghosts(
    mut commands: Commands,
    mut query: Query<(Entity, &mut Ghost, &mut Sprite, &mut Transform)>,
    vel_query: Query<&LinearVelocity>,
) {
    for (entity, mut ghost, mut sprite, mut transform) in query.iter_mut() {
        // Update lifetime
        if ghost.ticks_remaining > 0 {
            ghost.ticks_remaining = ghost.ticks_remaining.saturating_sub(1);
        }

        apply_ghost_movement(&ghost, &vel_query, &mut transform);

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
            FadeCurve::Constant => ghost.initial_alpha,
        };

        // Update sprite alpha while preserving the base color
        sprite.color = ghost.base_color.with_alpha(alpha);

        if let Some(scale) = ghost_scale(ghost.scale_curve, progress) {
            transform.scale = ghost.base_scale * scale;
        }
        if ghost.ticks_remaining == 0 {
            commands.entity(entity).despawn();
        }
    }
}

fn ghost_scale(curve: ScaleCurve, progress: f32) -> Option<Vec3> {
    Some(match curve {
        ScaleCurve::Constant => return None,
        ScaleCurve::Shrink(target) => Vec3::splat(1.0 - (1.0 - target) * progress),
        ScaleCurve::Grow(target) => Vec3::splat(1.0 + (target - 1.0) * progress),
        ScaleCurve::ShrinkHorizontal(target) => Vec3::new(1.0 - (1.0 - target) * progress, 1.0, 1.0),
        ScaleCurve::Axes { x, y } => Vec3::new(1.0 - (1.0 - x) * progress, 1.0 - (1.0 - y) * progress, 1.0),
    })
}

fn apply_ghost_movement(
    ghost: &Ghost,
    vel_query: &Query<&LinearVelocity>,
    transform: &mut Transform,
) {
    let vel = ghost_velocity(ghost, vel_query);
    transform.translation.x += vel.x;
    transform.translation.y += vel.y;
}

fn ghost_velocity(ghost: &Ghost, vel_query: &Query<&LinearVelocity>) -> Vec2 {
    match ghost.motion {
        GhostMotion::Velocity { source, fallback } => vel_query.get(source).map_or(fallback, |velocity| velocity.0),
        GhostMotion::Static | GhostMotion::Parent { .. } => Vec2::ZERO,
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    fn ghost(ticks: u32, motion: GhostMotion) -> Ghost {
        Ghost { ticks_remaining: ticks, max_ticks: ticks, initial_alpha: 1.0,
            fade_curve: FadeCurve::Linear, scale_curve: ScaleCurve::Constant,
            base_scale: Vec3::ONE, base_color: Color::WHITE, motion }
    }
    #[test]
    fn velocity_tracks_live_source_then_reverts_to_spawn_velocity_without_resetting_scale() {
        let mut app = App::new();
        app.add_systems(Update, update_ghosts);
        let source = app.world_mut().spawn(LinearVelocity(Vec2::new(3.0, 4.0))).id();
        let entity = app.world_mut().spawn((ghost(4, GhostMotion::Velocity { source, fallback: Vec2::X }),
            Sprite::default(), Transform::from_scale(Vec3::splat(2.0)))).id();
        app.update();
        assert_eq!(app.world().get::<Transform>(entity).unwrap().translation.xy(), Vec2::new(3.0, 4.0));
        app.world_mut().get_mut::<LinearVelocity>(source).unwrap().0 = Vec2::new(2.0, 1.0);
        app.update();
        app.world_mut().despawn(source);
        app.update();
        let transform = app.world().get::<Transform>(entity).unwrap();
        assert_eq!(transform.translation.xy(), Vec2::new(6.0, 5.0));
        assert_eq!(transform.scale, Vec3::splat(2.0));
        app.update();
        assert!(app.world().get_entity(entity).is_err());
    }
    #[test]
    fn zero_and_one_tick_ghosts_expire_on_the_first_update() {
        let mut app = App::new();
        app.add_systems(Update, update_ghosts);
        let entities = [0, 1].map(|ticks| app.world_mut().spawn((ghost(ticks, GhostMotion::Static),
            Sprite::default(), Transform::default())).id());
        app.update();
        for entity in entities { assert!(app.world().get_entity(entity).is_err()); }
    }
}
