use bevy::math::Isometry2d;
use bevy::prelude::*;
use bevy::camera::Projection;
use bevy::transform::TransformSystems::Propagate;

use crate::camera::MainCamera;
use crate::game::combat::DamageSource;
use crate::game::enemy::Enemy;
use theseeker_engine::physics::{
    Collider, CollisionGroups, Group, TileCollisionWorld, TileRange, TileSolidInfo, ENEMY_ATTACK,
    PLAYER_ATTACK,
};
use crate::game::player::Player;
use crate::game::world_objects::{Capsule, CapsuleCollisionMask};

#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct TileCollisionGizmos;

#[derive(Clone, Reflect)]
#[reflect(Default)]
pub struct TileCollisionDebugColors {
    pub static_tile: Color,
    pub dynamic_tile: Color,
    pub overlap_tile: Color,
    pub capsule_wall: Color,
    pub capsule_outside_trigger: Color,
    pub capsule_inside_trigger: Color,
    pub capsule_vertical_transit_up_trigger: Color,
    pub capsule_vertical_transit_down_trigger: Color,
    pub capsule_vertical_transit_up_stop: Color,
    pub capsule_vertical_transit_down_stop: Color,
    pub capsule_platform: Color,
    pub player: Color,
    pub enemy: Color,
    pub player_hitbox: Color,
    pub enemy_hitbox: Color,
    pub bounds: Color,
}

impl Default for TileCollisionDebugColors {
    fn default() -> Self {
        Self {
            static_tile: Color::srgb(0.2, 0.7, 0.9),
            dynamic_tile: Color::srgb(0.95, 0.6, 0.2),
            overlap_tile: Color::srgb(0.95, 0.2, 0.8),
            capsule_wall: Color::srgba(1.0, 0.2, 0.2, 0.9),
            capsule_outside_trigger: Color::srgba(1.0, 0.9, 0.1, 0.95),
            capsule_inside_trigger: Color::srgba(0.75, 0.62, 0.08, 0.95),
            capsule_vertical_transit_up_trigger: Color::srgba(
                0.73, 0.96, 0.81, 0.95,
            ),
            capsule_vertical_transit_down_trigger: Color::srgba(
                0.25, 0.96, 0.49, 0.95,
            ),
            capsule_vertical_transit_up_stop: Color::srgba(
                0.0, 1.0, 0.33, 0.95,
            ),
            capsule_vertical_transit_down_stop: Color::srgba(
                0.04, 1.0, 0.36, 0.95,
            ),
            capsule_platform: Color::srgba(0.45, 0.8, 0.95, 0.95),
            player: Color::srgb(0.3, 1.0, 0.4),
            enemy: Color::srgb(1.0, 0.6, 0.1),
            player_hitbox: Color::srgb(0.2, 0.9, 1.0),
            enemy_hitbox: Color::srgb(1.0, 0.2, 0.2),
            bounds: Color::srgb(1.0, 0.2, 0.3),
        }
    }
}

#[derive(Resource, Reflect, Clone)]
#[reflect(Resource, Default)]
pub struct TileCollisionDebugContext {
    pub enabled: bool,
    pub draw_player: bool,
    pub draw_enemies: bool,
    pub draw_damage_hitboxes: bool,
    pub draw_world_bounds: bool,
    pub draw_static_tiles: bool,
    pub draw_dynamic_tiles: bool,
    pub draw_capsule_colliders: bool,
    pub cull_to_camera: bool,
    pub camera_padding: f32,
    pub line_width: f32,
    pub colors: TileCollisionDebugColors,
}

impl Default for TileCollisionDebugContext {
    fn default() -> Self {
        Self {
            enabled: cfg!(debug_assertions),
            draw_player: true,
            draw_enemies: true,
            draw_damage_hitboxes: true,
            draw_world_bounds: true,
            draw_static_tiles: false,
            draw_dynamic_tiles: false,
            draw_capsule_colliders: false,
            cull_to_camera: true,
            camera_padding: 32.0,
            line_width: 1.5,
            colors: TileCollisionDebugColors::default(),
        }
    }
}

pub struct TileCollisionDebugPlugin {
    pub enabled: bool,
}

impl Default for TileCollisionDebugPlugin {
    fn default() -> Self {
        Self {
            enabled: cfg!(debug_assertions),
        }
    }
}

impl TileCollisionDebugPlugin {
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

impl Plugin for TileCollisionDebugPlugin {
    fn build(&self, app: &mut App) {
        app.init_gizmo_group::<TileCollisionGizmos>();
        app.register_type::<TileCollisionDebugContext>();
        app.register_type::<TileCollisionDebugColors>();
        app.insert_resource(TileCollisionDebugContext {
            enabled: self.enabled,
            ..Default::default()
        });
        app.add_systems(
            PostUpdate,
            sync_tile_collision_gizmo_config,
        );
        app.add_systems(
            PostUpdate,
            draw_tile_collision_debug.after(Propagate),
        );
    }
}

fn sync_tile_collision_gizmo_config(
    mut config_store: ResMut<GizmoConfigStore>,
    context: Res<TileCollisionDebugContext>,
) {
    if !context.is_changed() {
        return;
    }
    let (config, _) = config_store.config_mut::<TileCollisionGizmos>();
    config.enabled = context.enabled;
    config.line.width = context.line_width;
}

#[expect(clippy::too_many_arguments, reason = "Bevy system dependencies are expressed as explicit system parameters.")]
fn draw_tile_collision_debug(
    collision_world: Option<Res<TileCollisionWorld>>,
    context: Res<TileCollisionDebugContext>,
    q_player: Query<(&GlobalTransform, &Collider), With<Player>>,
    q_enemy: Query<(&GlobalTransform, &Collider), With<Enemy>>,
    q_capsule: Query<(&GlobalTransform, &CapsuleCollisionMask), With<Capsule>>,
    q_damage_hitboxes: Query<
        (
            &GlobalTransform,
            &Collider,
            Option<&CollisionGroups>,
        ),
        With<DamageSource>,
    >,
    q_camera: Query<(&GlobalTransform, &Projection), With<MainCamera>>,
    mut gizmos: Gizmos<TileCollisionGizmos>,
) {
    let Some(collision_world) = collision_world.as_deref() else {
        return;
    };
    if !context.enabled || !collision_world.is_ready() {
        return;
    }

    if context.draw_world_bounds {
        let bounds = collision_world.bounds_rect();
        draw_rect(
            &mut gizmos,
            bounds,
            context.colors.bounds,
        );
    }

    if context.draw_player {
        for (transform, collider) in q_player.iter() {
            draw_collider(
                &mut gizmos,
                transform.translation().truncate(),
                collider,
                context.colors.player,
            );
        }
    }

    if context.draw_enemies {
        for (transform, collider) in q_enemy.iter() {
            draw_collider(
                &mut gizmos,
                transform.translation().truncate(),
                collider,
                context.colors.enemy,
            );
        }
    }

    if context.draw_damage_hitboxes {
        for (transform, collider, groups) in q_damage_hitboxes.iter() {
            draw_damage_hitbox_debug(
                &mut gizmos,
                transform,
                collider,
                groups,
                &context.colors,
            );
        }
    }

    if context.draw_capsule_colliders {
        draw_capsule_collision_debug(
            &q_capsule,
            &q_camera,
            &context,
            &mut gizmos,
        );
    }

    if !context.draw_static_tiles && !context.draw_dynamic_tiles {
        return;
    }

    let tile_range = if context.cull_to_camera {
        camera_tile_range(
            collision_world,
            &q_camera,
            context.camera_padding,
        )
    } else {
        Some(full_tile_range(collision_world))
    };

    let Some(range) = tile_range else {
        return;
    };

    for ty in range.min_y..=range.max_y {
        for tx in range.min_x..=range.max_x {
            draw_tile_debug_cell(collision_world, &context, &mut gizmos, tx, ty);
        }
    }
}

fn draw_damage_hitbox_debug(
    gizmos: &mut Gizmos<TileCollisionGizmos>,
    transform: &GlobalTransform,
    collider: &Collider,
    groups: Option<&CollisionGroups>,
    colors: &TileCollisionDebugColors,
) {
    let Some(color) = hitbox_color(groups, colors) else {
        return;
    };

    draw_collider(
        gizmos,
        transform.translation().truncate(),
        collider,
        color,
    );
}

fn draw_tile_debug_cell(
    collision_world: &TileCollisionWorld,
    context: &TileCollisionDebugContext,
    gizmos: &mut Gizmos<TileCollisionGizmos>,
    tx: i32,
    ty: i32,
) {
    let Some(info) = collision_world.tile_solid_info(tx, ty) else {
        return;
    };
    let Some(color) = tile_color(context, info) else {
        return;
    };

    let (min, max) = collision_world.tile_bounds(tx, ty);
    draw_rect(gizmos, Rect::from_corners(min, max), color);
}

fn full_tile_range(collision_world: &TileCollisionWorld) -> TileRange {
    TileRange {
        min_x: 0,
        max_x: collision_world.width as i32 - 1,
        min_y: 0,
        max_y: collision_world.height as i32 - 1,
    }
}

fn camera_tile_range(
    collision_world: &TileCollisionWorld,
    q_camera: &Query<(&GlobalTransform, &Projection), With<MainCamera>>,
    padding: f32,
) -> Option<TileRange> {
    let Ok((transform, projection)) = q_camera.single() else {
        return Some(full_tile_range(collision_world));
    };
    let Projection::Orthographic(projection) = projection else {
        return Some(full_tile_range(collision_world));
    };
    let padding = padding.max(0.0);
    let offset = transform.translation().truncate();
    let min = projection.area.min + offset - Vec2::splat(padding);
    let max = projection.area.max + offset + Vec2::splat(padding);
    collision_world.tile_range_for_rect(Rect::from_corners(min, max))
}

fn tile_color(
    context: &TileCollisionDebugContext,
    info: TileSolidInfo,
) -> Option<Color> {
    let show_static = context.draw_static_tiles && info.terrain;
    let show_dynamic = context.draw_dynamic_tiles && info.dynamic;
    match (show_static, show_dynamic) {
        (true, true) => Some(context.colors.overlap_tile),
        (true, false) => Some(context.colors.static_tile),
        (false, true) => Some(context.colors.dynamic_tile),
        (false, false) => None,
    }
}

fn draw_capsule_collision_debug(
    q_capsule: &Query<(&GlobalTransform, &CapsuleCollisionMask), With<Capsule>>,
    q_camera: &Query<(&GlobalTransform, &Projection), With<MainCamera>>,
    context: &TileCollisionDebugContext,
    gizmos: &mut Gizmos<TileCollisionGizmos>,
) {
    let camera_rect = if context.cull_to_camera {
        camera_world_rect(q_camera, context.camera_padding)
    } else {
        None
    };

    for (transform, layout) in q_capsule.iter() {
        let world_origin = layout.world_origin(transform.translation().truncate());

        draw_capsule_rects(
            gizmos,
            camera_rect,
            world_origin,
            &layout.wall_rects,
            context.colors.capsule_wall,
        );
        draw_capsule_rects(
            gizmos,
            camera_rect,
            world_origin,
            &layout.outside_trigger_rects,
            context.colors.capsule_outside_trigger,
        );
        draw_capsule_rects(
            gizmos,
            camera_rect,
            world_origin,
            &layout.inside_trigger_rects,
            context.colors.capsule_inside_trigger,
        );
        draw_capsule_rects(
            gizmos,
            camera_rect,
            world_origin,
            &layout.vertical_transit_up_trigger_rects,
            context.colors.capsule_vertical_transit_up_trigger,
        );
        draw_capsule_rects(
            gizmos,
            camera_rect,
            world_origin,
            &layout.vertical_transit_down_trigger_rects,
            context.colors.capsule_vertical_transit_down_trigger,
        );
        draw_capsule_rects(
            gizmos,
            camera_rect,
            world_origin,
            &layout.vertical_transit_up_stop_rects,
            context.colors.capsule_vertical_transit_up_stop,
        );
        draw_capsule_rects(
            gizmos,
            camera_rect,
            world_origin,
            &layout.vertical_transit_down_stop_rects,
            context.colors.capsule_vertical_transit_down_stop,
        );
        draw_capsule_rects(
            gizmos,
            camera_rect,
            world_origin,
            layout.platforms.iter().map(|platform| &platform.rect),
            context.colors.capsule_platform,
        );
    }
}

fn draw_capsule_rects<'a>(
    gizmos: &mut Gizmos<TileCollisionGizmos>,
    camera_rect: Option<Rect>,
    world_origin: Vec2,
    rects: impl IntoIterator<Item = &'a Rect>,
    color: Color,
) {
    for rect in rects {
        let world_rect =
            Rect::from_corners(rect.min + world_origin, rect.max + world_origin);
        if camera_rect.is_some_and(|camera| {
            world_rect.max.x < camera.min.x
                || world_rect.min.x > camera.max.x
                || world_rect.max.y < camera.min.y
                || world_rect.min.y > camera.max.y
        }) {
            continue;
        }
        draw_rect(gizmos, world_rect, color);
    }
}

fn camera_world_rect(
    q_camera: &Query<(&GlobalTransform, &Projection), With<MainCamera>>,
    padding: f32,
) -> Option<Rect> {
    let Ok((transform, projection)) = q_camera.single() else {
        return None;
    };
    let Projection::Orthographic(projection) = projection else {
        return None;
    };
    let padding = padding.max(0.0);
    let offset = transform.translation().truncate();
    Some(Rect::from_corners(
        projection.area.min + offset - Vec2::splat(padding),
        projection.area.max + offset + Vec2::splat(padding),
    ))
}

fn draw_rect(
    gizmos: &mut Gizmos<TileCollisionGizmos>,
    rect: Rect,
    color: Color,
) {
    let center = (rect.min + rect.max) * 0.5;
    let size = rect.max - rect.min;
    gizmos.rect_2d(
        Isometry2d::from_translation(center),
        size,
        color,
    );
}

fn draw_circle(
    gizmos: &mut Gizmos<TileCollisionGizmos>,
    center: Vec2,
    radius: f32,
    color: Color,
) {
    gizmos.circle_2d(
        Isometry2d::from_translation(center),
        radius,
        color,
    );
}

fn draw_collider(
    gizmos: &mut Gizmos<TileCollisionGizmos>,
    offset: Vec2,
    collider: &Collider,
    color: Color,
) {
    match collider {
        Collider::Cuboid { half_extents } => {
            draw_rect(gizmos, Rect::from_center_size(offset, *half_extents * 2.0), color);
        },
        Collider::MultiRect { rects } => {
            for rect in rects {
                draw_rect(gizmos, Rect::from_corners(rect.min + offset, rect.max + offset), color);
            }
        },
        Collider::Circle { radius } => draw_circle(gizmos, offset, *radius, color),
        Collider::Empty | Collider::Capsule { .. } => {},
    }
}

fn hitbox_color(
    groups: Option<&CollisionGroups>,
    colors: &TileCollisionDebugColors,
) -> Option<Color> {
    let membership = groups.map(|g| g.memberships()).unwrap_or(Group::NONE);
    if membership.intersects(PLAYER_ATTACK) {
        Some(colors.player_hitbox)
    } else if membership.intersects(ENEMY_ATTACK) {
        Some(colors.enemy_hitbox)
    } else {
        None
    }
}
