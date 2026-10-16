use theseeker_engine::physics::geometry::{self, Axis, ProbeDirection};
use crate::game::player::foreground::ActionRef;
use bevy::ecs::hierarchy::ChildOf;
use bevy::camera::visibility::RenderLayers;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::{Gent, TransformGfxFromGent};
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::{animation::SpriteAnimationBundle, physics::Collider};

use crate::graphics::pixel_perfect::pixel_perfect_layers;
use crate::prelude::*;

use super::combat::Health;
use super::gentstate::Dead;
use super::player::skills::portable_tent::{
    ShelterInteriorKind,
};
use super::player::states::{
    transition_action, Elevator, ElevatorDirection,
};
use super::player::{Player, PlayerStateSet};

mod capsule_shape;

const CAPSULE_INSIDE_ANIMATION_KEY: &str = "anim.objects.CapsuleInside";
/// Offset from the capsule anchor to the center of the pod sprite.
const CAPSULE_GFX_OFFSET: Vec2 = Vec2::new(0.0, 21.0);
/// Ticks over which the previous capsule artwork fades after an outside/inside switch.
pub(crate) const CAPSULE_ART_FADE_TICKS: u32 = 36;
/// Just in front of the capsule artwork (z -2e-6).
const CAPSULE_ART_FADE_Z: f32 = -1.95e-6;
const CAPSULE_COLLISION_EPS: f32 = 0.0001;
const CAPSULE_HEALTH_REGEN_PER_TICK: u32 = 1;

pub struct WorldObjectsPlugin;

impl Plugin for WorldObjectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                setup_capsule.run_if(any_matching::<Added<CapsuleBlueprint>>()),
                (
                    mark_spawned_player_occupancy,
                    resolve_spawned_player_occupancy
                        .after(mark_spawned_player_occupancy)
                        .before(update_capsule_occupancy)
                        .run_if(not(resource_exists::<HoldSpawnOccupancy>)),
                    cleanup_capsule_occupant
                        .after(PlayerStateSet::Collisions),
                    update_capsule_occupancy
                        .after(cleanup_capsule_occupant),
                    regenerate_capsule_occupant_health
                        .after(update_capsule_occupancy),
                    sync_world_object_animation
                        .after(update_capsule_occupancy),
                    fade_previous_capsule_art,
                ),
            )
                .run_if(
                    in_state(GameState::Playing)
                        .and_then(in_state(AppState::InGame)),
                )
                .after(theseeker_engine::physics::PhysicsSet),
        );
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldObjectKind {
    PortableTent,
    Capsule,
}

impl WorldObjectKind {
    fn animation_key(self) -> &'static str {
        match self {
            Self::PortableTent => "anim.objects.Tent",
            Self::Capsule => "anim.objects.Capsule",
        }
    }

    fn display_name(self) -> &'static str {
        match self {
            Self::PortableTent => "Tent",
            Self::Capsule => "Capsule",
        }
    }

    pub(crate) fn interior_kind(self) -> Option<ShelterInteriorKind> {
        match self {
            Self::PortableTent => Some(ShelterInteriorKind::Tent),
            Self::Capsule => None,
        }
    }
}

#[derive(Component, Default)]
pub struct CapsuleBlueprint;

#[derive(Bundle, Default)]
pub struct CapsuleBlueprintBundle {
    marker: CapsuleBlueprint,
}

#[derive(Component)]
pub struct Capsule;

#[derive(Component, Debug, Clone, Copy)]
#[component(storage = "SparseSet")]
pub struct CapsuleOccupant {
    pub shelter: Entity,
}

/// Capsule collision areas relative to [`CapsuleCollisionMask::world_origin`].
#[derive(Component, Clone)]
pub(crate) struct CapsuleCollisionMask {
    pub(crate) wall_rects: Vec<Rect>,
    cabin_rects: Vec<Rect>,
    pub(crate) outside_trigger_rects: Vec<Rect>,
    pub(crate) inside_trigger_rects: Vec<Rect>,
    pub(crate) vertical_transit_up_trigger_rects: Vec<Rect>,
    pub(crate) vertical_transit_down_trigger_rects: Vec<Rect>,
    pub(crate) vertical_transit_up_stop_rects: Vec<Rect>,
    pub(crate) vertical_transit_down_stop_rects: Vec<Rect>,
    pub(crate) platforms: Vec<CapsulePlatform>,
}

#[derive(Clone, Copy)]
pub(crate) struct CapsulePlatform {
    pub(crate) rect: Rect,
    pub(crate) requires_lift_jump: bool,
}

impl Default for CapsuleCollisionMask {
    fn default() -> Self { capsule_shape::collision_mask() }
}

/// A freshly spawned player whose starting capsule occupancy is not yet decided.
#[derive(Component)]
pub(crate) struct PendingSpawnOccupancy;

/// Postpones resolving [`PendingSpawnOccupancy`] while present.
#[derive(Resource)]
pub(crate) struct HoldSpawnOccupancy;

#[derive(Component, Clone)]
pub(crate) struct CapsuleLightMask {
    carve_rects: Vec<Rect>,
}

#[derive(Component, Default, Debug, Clone)]
pub(crate) struct CapsuleDropPlatformState {
    contacts: Vec<CapsulePlatformContact>,
    lift_previous_position: Option<Vec2>,
    lift_jump: Option<CapsuleLiftJump>,
}

#[derive(Clone, Copy, Debug)]
struct CapsuleLiftJump {
    shelter: Entity,
    airborne: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CapsulePlatformKey {
    shelter: Entity,
    rect_index: usize,
}

#[derive(Clone, Debug)]
struct CapsulePlatformContact {
    key: CapsulePlatformKey,
    block_on_next_contact: bool,
    support_active: bool,
    pass_through_active: bool,
}

#[derive(Component)]
pub struct WorldObjectGfx {
    pub e_gent: Entity,
}

#[derive(Bundle)]
pub struct WorldObjectGfxBundle {
    marker: WorldObjectGfx,
    gent2gfx: TransformGfxFromGent,
    sprite: Sprite,
    render_layers: RenderLayers,
    transform: Transform,
    global_transform: GlobalTransform,
    visibility: Visibility,
    inherited_visibility: InheritedVisibility,
    view_visibility: ViewVisibility,
    animation: SpriteAnimationBundle,
}

fn setup_capsule(
    mut q: Query<(&mut Transform, Entity), Added<CapsuleBlueprint>>,
    mut commands: Commands,
) {
    for (mut xf_gent, e_gent) in q.iter_mut() {
        xf_gent.translation.z = -2.0 * 0.000_001;
        let e_gfx = commands.spawn(()).id();
        let e_effects_gfx = commands.spawn_empty().id();
        let mut player = ScriptPlayer::<SpriteAnimation>::default();
        player.play_key(WorldObjectKind::Capsule.animation_key());

        commands
            .entity(e_gent)
            .insert((
                Name::new(WorldObjectKind::Capsule.display_name()),
                Capsule,
                WorldObjectKind::Capsule,
                Gent {
                    e_gfx,
                    e_effects_gfx,
                },
                capsule_shape::collision_mask(),
                capsule_shape::light_mask(),
                StateDespawnMarker,
            ))
            .remove::<ChildOf>();

        commands.entity(e_gfx).insert((
            WorldObjectGfxBundle {
                marker: WorldObjectGfx { e_gent },
                gent2gfx: TransformGfxFromGent {
                    pixel_aligned: false,
                    gent: e_gent,
                    offset: Some(CAPSULE_GFX_OFFSET.extend(0.0)),
                },
                sprite: Sprite {
                    texture_atlas: Some(TextureAtlas::default()),
                    ..default()
                },
                render_layers: pixel_perfect_layers(),
                transform: *xf_gent,
                global_transform: GlobalTransform::default(),
                visibility: Visibility::Visible,
                inherited_visibility: InheritedVisibility::VISIBLE,
                view_visibility: ViewVisibility::default(),
                animation: SpriteAnimationBundle { player },
            },
            StateDespawnMarker,
        ));
    }
}

fn cleanup_capsule_occupant(
    mut commands: Commands,
    player_q: Query<(Entity, &CapsuleOccupant), With<Player>>,
    capsule_q: Query<(), With<Capsule>>,
) {
    let Ok((player_entity, occupant)) = player_q.single() else {
        return;
    };
    if capsule_q.get(occupant.shelter).is_err() {
        commands.entity(player_entity).remove::<CapsuleOccupant>();
    }
}

fn mark_spawned_player_occupancy(
    mut commands: Commands,
    player_q: Query<Entity, Added<Player>>,
) {
    for player in player_q.iter() {
        commands.entity(player).insert(PendingSpawnOccupancy);
    }
}

/// A player spawned overlapping a capsule's inside trigger starts as its occupant,
/// exactly as if they had climbed in.
fn resolve_spawned_player_occupancy(
    mut commands: Commands,
    player_q: Query<
        (Entity, &Transform, &Collider),
        (With<Player>, With<PendingSpawnOccupancy>),
    >,
    capsule_q: Query<
        (Entity, &Transform, &CapsuleCollisionMask),
        With<Capsule>,
    >,
    assembling_q: Query<(), (With<CapsuleBlueprint>, Without<Capsule>)>,
) {
    if !assembling_q.is_empty() {
        return;
    }
    for (player, transform, collider) in player_q.iter() {
        commands.entity(player).remove::<PendingSpawnOccupancy>();
        let Some(half_extents) = collider.cuboid_half_extents() else {
            continue;
        };
        let player_pos = transform.translation.truncate();
        let shelter = capsule_q.iter().find(|(_, capsule, mask)| {
            mask.inside_trigger_overlaps(
                capsule.translation.truncate(),
                player_pos,
                half_extents,
            )
        });
        if let Some((shelter, _, _)) = shelter {
            commands.entity(player).insert(CapsuleOccupant { shelter });
        }
    }
}

fn update_capsule_occupancy(
    mut commands: Commands,
    mut player_q: Query<
        (
            Entity,
            &Transform,
            &Collider,
            &theseeker_engine::physics::LinearVelocity,
            Option<&CapsuleOccupant>,
            Option<ActionRef<Elevator>>,
            &crate::game::player::GroundSensor,
            Option<Ref<super::player::states::Jumping>>,
            &mut CapsuleDropPlatformState,
        ),
        (With<Player>, Without<Dead>, Without<super::spikes::SpikeRespawnPending>),
    >,
    capsule_q: Query<
        (Entity, &Transform, &CapsuleCollisionMask),
        With<Capsule>,
    >,
) {
    let Ok((
        player_entity,
        player_transform,
        collider,
        velocity,
        occupant,
        elevator,
        ground,
        jumping,
        mut platform_state,
    )) = player_q.single_mut()
    else {
        return;
    };
    let Some(half_extents) = collider.cuboid_half_extents() else {
        return;
    };

    let player_pos = player_transform.translation.truncate();
    let inside_trigger_capsule = nearest_capsule_trigger(
        player_pos, half_extents, &capsule_q, CapsuleCollisionMask::inside_trigger_overlaps,
    );
    let outside_trigger_capsule = nearest_capsule_trigger(
        player_pos, half_extents, &capsule_q, CapsuleCollisionMask::outside_trigger_overlaps,
    );
    let vertical_velocity = velocity.0.y;
    // Admission can commit before or after the first locomotion step in this tick.
    let fresh_jump = ground.is_grounded
        && jumping.as_ref().is_some_and(|jump| jump.is_added() && jump.tick <= 1);
    platform_state.observe_lift_jump(
        occupant.map(|occupant| occupant.shelter),
        fresh_jump,
        ground.is_grounded,
        elevator.is_some(),
        jumping.is_some(),
    );
    if elevator.is_none() {
        start_vertical_transit_elevator(
            &mut commands,
            player_entity,
            &mut platform_state,
            vertical_velocity,
            player_pos,
            half_extents,
            &capsule_q,
        );
    }
    platform_state.lift_previous_position = Some(player_pos);

    if vertical_velocity > 0.0 {
        let Some(target) = inside_trigger_capsule else {
            return;
        };
        if occupant.map(|occupant| occupant.shelter) != Some(target) {
            commands
                .entity(player_entity)
                .insert(CapsuleOccupant { shelter: target });
        }
        return;
    }

    if vertical_velocity < 0.0
        && occupant
            .is_some_and(|occupant| {
                outside_trigger_capsule == Some(occupant.shelter)
            })
    {
        commands.entity(player_entity).remove::<CapsuleOccupant>();
    }
}

fn nearest_capsule_trigger(
    player_pos: Vec2,
    half_extents: Vec2,
    capsules: &Query<(Entity, &Transform, &CapsuleCollisionMask), With<Capsule>>,
    overlaps: fn(&CapsuleCollisionMask, Vec2, Vec2, Vec2) -> bool,
) -> Option<Entity> {
    capsules.iter().filter_map(|(entity, transform, mask)| {
        let position = transform.translation.truncate();
        overlaps(mask, position, player_pos, half_extents)
            .then_some((entity, position.distance_squared(player_pos)))
    }).min_by(|(_, a), (_, b)| a.total_cmp(b)).map(|(entity, _)| entity)
}

fn regenerate_capsule_occupant_health(
    mut player_q: Query<
        &mut Health,
        (With<Player>, With<CapsuleOccupant>, Without<Dead>),
    >,
) {
    for mut health in player_q.iter_mut() {
        health.current = health
            .current
            .saturating_add(CAPSULE_HEALTH_REGEN_PER_TICK)
            .min(health.max);
    }
}

fn start_vertical_transit_elevator(
    commands: &mut Commands,
    player_entity: Entity,
    platform_state: &mut CapsuleDropPlatformState,
    vertical_velocity: f32,
    player_pos: Vec2,
    half_extents: Vec2,
    capsule_q: &Query<
        (Entity, &Transform, &CapsuleCollisionMask),
        With<Capsule>,
    >,
) {
    let Some(direction) =
        ElevatorDirection::from_vertical_velocity(vertical_velocity)
    else {
        return;
    };
    let Some(target) = nearest_vertical_transit_trigger_capsule(
        direction,
        player_pos,
        half_extents,
        platform_state,
        capsule_q,
    ) else {
        return;
    };

    transition_action(
        commands,
        player_entity,
        Elevator::new(target, direction),
    );
    platform_state.lift_jump = None;
}

fn nearest_vertical_transit_trigger_capsule(
    direction: ElevatorDirection,
    player_pos: Vec2,
    half_extents: Vec2,
    platform_state: &CapsuleDropPlatformState,
    capsule_q: &Query<(Entity, &Transform, &CapsuleCollisionMask), With<Capsule>>,
) -> Option<Entity> {
    capsule_q
        .iter()
        .filter_map(|(entity, transform, mask)| {
            let capsule_pos = transform.translation.truncate();
            let entered = match direction {
                ElevatorDirection::Up => mask.vertical_transit_trigger_overlaps(
                    direction, capsule_pos, player_pos, half_extents,
                ),
                ElevatorDirection::Down => platform_state.descent_crossed(
                    entity, mask, capsule_pos, player_pos, half_extents,
                ),
            };
            entered
            .then_some((entity, capsule_pos.distance_squared(player_pos)))
        })
        .min_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(entity, _)| entity)
}

fn sync_world_object_animation(
    mut commands: Commands,
    player_q: Query<Option<&CapsuleOccupant>, With<Player>>,
    kind_q: Query<(&WorldObjectKind, &Transform)>,
    mut gfx_q: Query<(
        &WorldObjectGfx,
        &mut ScriptPlayer<SpriteAnimation>,
        &Sprite,
    )>,
) {
    let occupied_capsule = player_q
        .single()
        .ok()
        .flatten()
        .map(|occupant| occupant.shelter);

    for (gfx, mut animation, sprite) in gfx_q.iter_mut() {
        let Ok((kind, transform)) = kind_q.get(gfx.e_gent) else {
            continue;
        };
        if *kind != WorldObjectKind::Capsule {
            continue;
        }
        let switched = sync_capsule_animation(&mut animation, *kind, occupied_capsule, gfx.e_gent);
        let position = transform.translation.truncate() + CAPSULE_GFX_OFFSET;
        switched.then(|| spawn_capsule_art_fade(&mut commands, sprite, position));
    }
}

/// Returns whether the capsule switched from one displayed artwork to another.
fn sync_capsule_animation(
    animation: &mut ScriptPlayer<SpriteAnimation>,
    kind: WorldObjectKind,
    occupied_capsule: Option<Entity>,
    capsule_entity: Entity,
) -> bool {
    let desired_key =
        capsule_animation_key(kind, occupied_capsule, capsule_entity);
    let had_key = match animation.current_key() {
        Some(current) if current == desired_key => return false,
        current => current.is_some(),
    };
    animation.play_key(desired_key);
    had_key
}

/// The capsule's previous artwork, fading out over its new one.
#[derive(Component)]
struct CapsuleArtFade {
    remaining_ticks: u32,
}

fn spawn_capsule_art_fade(commands: &mut Commands, previous: &Sprite, position: Vec2) {
    if previous.image == Handle::default() {
        return;
    }
    commands.spawn((
        Name::new("CapsuleArtFade"),
        CapsuleArtFade { remaining_ticks: CAPSULE_ART_FADE_TICKS },
        previous.clone(),
        Transform::from_translation(position.extend(CAPSULE_ART_FADE_Z)),
        pixel_perfect_layers(),
        StateDespawnMarker,
    ));
}

fn fade_previous_capsule_art(
    mut commands: Commands,
    mut fades: Query<(Entity, &mut CapsuleArtFade, &mut Sprite)>,
) {
    for (entity, mut fade, mut sprite) in &mut fades {
        fade.remaining_ticks = fade.remaining_ticks.saturating_sub(1);
        if fade.remaining_ticks == 0 {
            commands.entity(entity).despawn();
            continue;
        }
        let remaining = fade.remaining_ticks as f32 / CAPSULE_ART_FADE_TICKS as f32;
        let opacity = remaining * remaining * (3.0 - 2.0 * remaining);
        sprite.color = sprite.color.with_alpha(opacity);
    }
}

fn capsule_animation_key(
    kind: WorldObjectKind,
    occupied_capsule: Option<Entity>,
    capsule_entity: Entity,
) -> &'static str {
    if occupied_capsule == Some(capsule_entity) {
        return CAPSULE_INSIDE_ANIMATION_KEY;
    }

    kind.animation_key()
}

type CapsuleCollisionQuery<'w, 's, F> = Query<'w, 's, (Entity, &'static Transform, &'static CapsuleCollisionMask), F>;

/// Each capsule's solids in world space, with the player's drop-through platform state.
fn capsule_solids<'a, F: bevy::ecs::query::QueryFilter>(
    capsule_q: &'a CapsuleCollisionQuery<'_, '_, F>,
    platforms: Option<&'a CapsuleDropPlatformState>,
) -> impl Iterator<Item = CapsuleSolids<'a>> {
    capsule_q.iter().map(move |(shelter, transform, mask)| CapsuleSolids {
        shelter,
        origin: mask.world_origin(transform.translation.truncate()),
        mask,
        platforms,
    })
}

/// The nearest capsule stop along `axis`; platforms only stop downward motion.
pub(crate) fn resolve_capsule_axis(
    axis: Axis,
    position: Vec2,
    delta: f32,
    half_extents: Vec2,
    platforms: Option<&CapsuleDropPlatformState>,
    capsule_q: &CapsuleCollisionQuery<'_, '_, impl bevy::ecs::query::QueryFilter>,
) -> (f32, bool) {
    let i = axis.index();
    if delta == 0.0 {
        return (position[i], false);
    }
    capsule_solids(capsule_q, platforms)
        .map(|solids| solids.resolve_axis(axis, position, delta, half_extents))
        .filter(|(_, blocked)| *blocked)
        .fold((position[i] + delta, false), |best, (candidate, _)| {
            let nearer = if delta < 0.0 { candidate > best.0 } else { candidate < best.0 };
            if nearer { (candidate, true) } else { best }
        })
}

pub(crate) fn capsule_distance(
    direction: ProbeDirection,
    position: Vec2,
    half_extents: Vec2,
    max_distance: f32,
    platforms: Option<&CapsuleDropPlatformState>,
    capsule_q: &CapsuleCollisionQuery<'_, '_, impl bevy::ecs::query::QueryFilter>,
) -> f32 {
    capsule_solids(capsule_q, platforms)
        .map(|solids| solids.distance(direction, position, half_extents, max_distance))
        .fold(max_distance, f32::min)
}

pub(crate) fn capsule_overlaps_solid(
    position: Vec2,
    half_extents: Vec2,
    capsule_q: &CapsuleCollisionQuery<'_, '_, impl bevy::ecs::query::QueryFilter>,
) -> bool {
    let body = aabb_from_center_half_extents(position, half_extents);
    capsule_solids(capsule_q, None)
        .any(|solids| solids.walls().any(|wall| rects_overlap(body, wall)))
}

/// Resolves capsule ownership from a destination, including cabin decks above its entry trigger.
/// Teleport callers validate body clearance and support before adopting this occupancy.
pub(crate) fn capsule_occupant_at_world_position(
    position: Vec2,
    capsule_q: &CapsuleCollisionQuery<'_, '_, impl bevy::ecs::query::QueryFilter>,
) -> Option<CapsuleOccupant> {
    capsule_q.iter().filter_map(|(shelter, transform, mask)| {
        let capsule_position = transform.translation.truncate();
        mask.contains_cabin_point(capsule_position, position)
            .then_some((shelter, capsule_position.distance_squared(position)))
    }).min_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(shelter, _)| CapsuleOccupant { shelter })
}

pub(crate) fn capsule_is_solid_at_world_point(
    point: Vec2,
    capsule_q: &Query<
        (Entity, &Transform, &CapsuleCollisionMask),
        impl bevy::ecs::query::QueryFilter,
    >,
) -> bool {
    capsule_q.iter().any(|(_, transform, mask)| {
        mask.is_solid_at_world_point(transform.translation.truncate(), point)
    })
}

pub(crate) fn capsule_elevator_stop_y(
    position: Vec2,
    delta_y: f32,
    half_extents: Vec2,
    elevator: Elevator,
    capsule_q: &Query<
        (Entity, &Transform, &CapsuleCollisionMask),
        impl bevy::ecs::query::QueryFilter,
    >,
) -> Option<f32> {
    if delta_y == 0.0 {
        return None;
    }

    let Ok((_, transform, mask)) = capsule_q.get(elevator.shelter()) else {
        return Some(position.y);
    };

    mask.vertical_transit_stop_y(
        elevator.direction(),
        transform.translation.truncate(),
        position,
        delta_y,
        half_extents,
    )
}

pub(crate) fn sync_capsule_drop_platform_state(
    previous_player_pos: Vec2,
    player_pos: Vec2,
    half_extents: Vec2,
    state: &mut CapsuleDropPlatformState,
    capsule_q: &Query<
        (Entity, &Transform, &CapsuleCollisionMask),
        impl bevy::ecs::query::QueryFilter,
    >,
) {
    for (shelter, transform, mask) in capsule_q.iter() {
        let origin = mask.world_origin(transform.translation.truncate());
        for (rect_index, platform) in mask.platforms.iter().enumerate() {
            let world_rect = translate_rect(platform.rect, origin);
            let contact = state.contact_mut(CapsulePlatformKey {
                shelter,
                rect_index,
            });
            sync_capsule_platform_contact(
                contact,
                world_rect,
                previous_player_pos,
                player_pos,
                half_extents,
            );
        }
    }
}

fn sync_capsule_platform_contact(
    contact: &mut CapsulePlatformContact,
    world_rect: Rect,
    previous_player_pos: Vec2,
    player_pos: Vec2,
    half_extents: Vec2,
) {
    if platform_support_contact(world_rect, player_pos, half_extents) {
        contact.support_active = true;
        contact.pass_through_active = false;
        return;
    }

    if contact.support_active {
        contact.support_active = false;
        contact.block_on_next_contact = false;
        return;
    }

    if should_start_platform_pass_through(
        contact,
        world_rect,
        previous_player_pos,
        player_pos,
        half_extents,
    ) {
        contact.pass_through_active = true;
    }

    if contact.pass_through_active
        && platform_cleared_below(world_rect, player_pos, half_extents)
    {
        contact.pass_through_active = false;
        contact.block_on_next_contact = true;
    }
}

fn should_start_platform_pass_through(
    contact: &CapsulePlatformContact,
    world_rect: Rect,
    previous_player_pos: Vec2,
    player_pos: Vec2,
    half_extents: Vec2,
) -> bool {
    !contact.block_on_next_contact
        && !contact.pass_through_active
        && platform_started_down_pass_through(
            world_rect,
            previous_player_pos,
            player_pos,
            half_extents,
        )
}

impl CapsuleDropPlatformState {
    fn observe_lift_jump(
        &mut self,
        occupant: Option<Entity>,
        fresh_jump: bool,
        grounded: bool,
        in_elevator: bool,
        jumping: bool,
    ) {
        if in_elevator {
            self.lift_jump = None;
            return;
        }
        if fresh_jump {
            self.lift_jump = occupant.map(|shelter| CapsuleLiftJump { shelter, airborne: false });
            return;
        }
        let Some(intent) = self.lift_jump.as_mut() else { return; };
        if occupant != Some(intent.shelter) || (grounded && (intent.airborne || !jumping)) {
            self.lift_jump = None;
            return;
        }
        // The authored jump begins with stationary ticks before leaving its support.
        intent.airborne |= !grounded;
    }

    fn descent_crossed(
        &self,
        shelter: Entity,
        mask: &CapsuleCollisionMask,
        capsule_pos: Vec2,
        player_pos: Vec2,
        half_extents: Vec2,
    ) -> bool {
        if !self.lift_jump.is_some_and(|jump| jump.shelter == shelter && jump.airborne) {
            return false;
        }
        let Some(previous) = self.lift_previous_position else { return false; };
        let origin = mask.world_origin(capsule_pos);
        mask.vertical_transit_down_trigger_rects.iter().any(|rect| {
            platform_started_down_pass_through(
                translate_rect(*rect, origin), previous, player_pos, half_extents,
            )
        })
    }

    fn platform_blocks(&self, shelter: Entity, rect_index: usize, requires_lift_jump: bool) -> bool {
        if requires_lift_jump {
            return !self.lift_jump.is_some_and(|jump| jump.shelter == shelter && jump.airborne);
        }
        self.contacts
            .iter()
            .find(|contact| {
                contact.key.shelter == shelter
                    && contact.key.rect_index == rect_index
            })
            .map(|contact| contact.block_on_next_contact)
            .unwrap_or(true)
    }

    fn contact_mut(
        &mut self,
        key: CapsulePlatformKey,
    ) -> &mut CapsulePlatformContact {
        if let Some(index) = self.contacts.iter().position(|contact| contact.key == key) {
            return &mut self.contacts[index];
        }
        self.contacts.push(CapsulePlatformContact {
            key,
            block_on_next_contact: true,
            support_active: false,
            pass_through_active: false,
        });
        self.contacts
            .last_mut()
            .expect("just pushed a capsule platform contact")
    }
}

impl CapsuleCollisionMask {
    fn contains_cabin_point(&self, capsule_pos: Vec2, point: Vec2) -> bool {
        let local = point - self.world_origin(capsule_pos);
        self.cabin_rects.iter().any(|rect| rect_contains_point(*rect, local))
            && !self.wall_rects.iter().any(|rect| rect_contains_point(*rect, local))
    }

    /// Collision areas share the pod artwork's pixel grid, so capsule floors lie on
    /// whole pixels like terrain and resting sprites round stably.
    pub(crate) fn world_origin(&self, capsule_pos: Vec2) -> Vec2 {
        pod_sprite_origin(capsule_pos)
    }

    fn outside_trigger_overlaps(
        &self,
        capsule_pos: Vec2,
        player_pos: Vec2,
        half_extents: Vec2,
    ) -> bool {
        overlaps_any_rect(
            &self.outside_trigger_rects,
            self.world_origin(capsule_pos),
            player_pos,
            half_extents,
        )
    }

    fn inside_trigger_overlaps(
        &self,
        capsule_pos: Vec2,
        player_pos: Vec2,
        half_extents: Vec2,
    ) -> bool {
        overlaps_any_rect(
            &self.inside_trigger_rects,
            self.world_origin(capsule_pos),
            player_pos,
            half_extents,
        )
    }

    fn vertical_transit_trigger_overlaps(
        &self,
        direction: ElevatorDirection,
        capsule_pos: Vec2,
        player_pos: Vec2,
        half_extents: Vec2,
    ) -> bool {
        let trigger_rects = match direction {
            ElevatorDirection::Up => {
                &self.vertical_transit_up_trigger_rects
            },
            ElevatorDirection::Down => {
                &self.vertical_transit_down_trigger_rects
            },
        };
        overlaps_any_rect(
            trigger_rects,
            self.world_origin(capsule_pos),
            player_pos,
            half_extents,
        )
    }

    fn vertical_transit_stop_y(
        &self,
        direction: ElevatorDirection,
        capsule_pos: Vec2,
        player_pos: Vec2,
        delta_y: f32,
        half_extents: Vec2,
    ) -> Option<f32> {
        let stop_rects = match direction {
            ElevatorDirection::Up => {
                &self.vertical_transit_up_stop_rects
            },
            ElevatorDirection::Down => {
                &self.vertical_transit_down_stop_rects
            },
        };
        let origin = self.world_origin(capsule_pos);
        swept_vertical_stop_y(
            stop_rects,
            origin,
            player_pos,
            delta_y,
            half_extents,
        )
    }

    fn is_solid_at_world_point(&self, capsule_pos: Vec2, point: Vec2) -> bool {
        let origin = self.world_origin(capsule_pos);
        world_rects(&self.wall_rects, origin)
            .into_iter()
            .any(|rect| rect_contains_point(rect, point))
    }
}

struct CapsuleSolids<'a> {
    shelter: Entity,
    origin: Vec2,
    mask: &'a CapsuleCollisionMask,
    platforms: Option<&'a CapsuleDropPlatformState>,
}

impl CapsuleSolids<'_> {
    fn walls(&self) -> impl Iterator<Item = Rect> + '_ {
        world_rects(&self.mask.wall_rects, self.origin)
    }

    fn resolve_axis(&self, axis: Axis, position: Vec2, delta: f32, half_extents: Vec2) -> (f32, bool) {
        let walls = geometry::resolve_rects_axis(self.walls(), position, delta, half_extents, axis, CAPSULE_COLLISION_EPS);
        if !matches!(axis, Axis::Y) || delta >= 0.0 {
            return walls;
        }
        let (platform_y, platform_blocked) = resolve_platform_axis_y(
            self.shelter,
            &self.mask.platforms,
            self.origin,
            position,
            delta,
            half_extents,
            self.platforms,
        );
        if platform_blocked && platform_y > walls.0 { (platform_y, true) } else { walls }
    }

    fn distance(&self, direction: ProbeDirection, position: Vec2, half_extents: Vec2, max_distance: f32) -> f32 {
        let walls = geometry::distance_to_rects(
            self.walls(),
            position,
            half_extents,
            max_distance,
            direction,
            CAPSULE_COLLISION_EPS,
        );
        if !matches!(direction, ProbeDirection::Down) {
            return walls;
        }
        walls.min(platform_ground_distance(
            self.shelter,
            &self.mask.platforms,
            self.origin,
            position,
            half_extents,
            max_distance,
            self.platforms,
        ))
    }
}

impl CapsuleLightMask {
    pub(crate) fn world_rects(&self, capsule_pos: Vec2) -> Vec<Rect> {
        world_rects(&self.carve_rects, pod_sprite_origin(capsule_pos)).collect()
    }
}

/// Bottom-left corner of the pod sprite in world space.
fn pod_sprite_origin(capsule_pos: Vec2) -> Vec2 {
    capsule_pos + CAPSULE_GFX_OFFSET - capsule_shape::POD_SPRITE_SIZE.as_vec2() * 0.5
}

fn world_rects(rects: &[Rect], origin: Vec2) -> impl Iterator<Item = Rect> + '_ {
    rects
        .iter()
        .copied()
        .map(move |rect| translate_rect(rect, origin))
}

fn translate_rect(rect: Rect, offset: Vec2) -> Rect {
    Rect::from_corners(rect.min + offset, rect.max + offset)
}

fn overlaps_any_rect(
    rects: &[Rect],
    origin: Vec2,
    position: Vec2,
    half_extents: Vec2,
) -> bool {
    let player_aabb = aabb_from_center_half_extents(position, half_extents);
    rects
        .iter()
        .copied()
        .map(|rect| translate_rect(rect, origin))
        .any(|rect| rects_overlap(player_aabb, rect))
}

fn swept_vertical_stop_y(
    rects: &[Rect],
    origin: Vec2,
    position: Vec2,
    delta_y: f32,
    half_extents: Vec2,
) -> Option<f32> {
    let player_aabb = aabb_from_center_half_extents(position, half_extents);
    let target_y = position.y + delta_y;

    rects
        .iter()
        .copied()
        .map(|rect| translate_rect(rect, origin))
        .filter(|rect| {
            ranges_overlap(
                player_aabb.min.x,
                player_aabb.max.x,
                rect.min.x,
                rect.max.x,
            )
        })
        .filter_map(|rect| {
            vertical_stop_y_for_rect(rect, position, target_y, half_extents)
        })
        .reduce(|best, candidate| {
            if delta_y > 0.0 {
                best.min(candidate)
            } else {
                best.max(candidate)
            }
        })
}

fn vertical_stop_y_for_rect(
    rect: Rect,
    position: Vec2,
    target_y: f32,
    half_extents: Vec2,
) -> Option<f32> {
    let player_aabb = aabb_from_center_half_extents(position, half_extents);
    if rects_overlap(player_aabb, rect) {
        return Some(position.y);
    }

    if target_y > position.y {
        let start_top = position.y + half_extents.y;
        let target_top = target_y + half_extents.y;
        if start_top <= rect.min.y + CAPSULE_COLLISION_EPS
            && target_top >= rect.min.y - CAPSULE_COLLISION_EPS
        {
            return Some(rect.min.y - half_extents.y);
        }
        return None;
    }

    let start_bottom = position.y - half_extents.y;
    let target_bottom = target_y - half_extents.y;
    if start_bottom >= rect.max.y - CAPSULE_COLLISION_EPS
        && target_bottom <= rect.max.y + CAPSULE_COLLISION_EPS
    {
        return Some(rect.max.y + half_extents.y);
    }

    None
}

fn aabb_from_center_half_extents(center: Vec2, half_extents: Vec2) -> Rect {
    Rect::from_corners(center - half_extents, center + half_extents)
}

fn rects_overlap(a: Rect, b: Rect) -> bool {
    ranges_overlap(a.min.x, a.max.x, b.min.x, b.max.x)
        && ranges_overlap(a.min.y, a.max.y, b.min.y, b.max.y)
}

fn ranges_overlap(min_a: f32, max_a: f32, min_b: f32, max_b: f32) -> bool {
    geometry::intervals_overlap(min_a, max_a, min_b, max_b, CAPSULE_COLLISION_EPS)
}

fn rect_contains_point(rect: Rect, point: Vec2) -> bool {
    point.x >= rect.min.x
        && point.x < rect.max.x
        && point.y >= rect.min.y
        && point.y < rect.max.y
}









fn resolve_platform_axis_y(
    shelter: Entity,
    platforms: &[CapsulePlatform],
    origin: Vec2,
    position: Vec2,
    delta_y: f32,
    half_extents: Vec2,
    platform_state: Option<&CapsuleDropPlatformState>,
) -> (f32, bool) {
    if delta_y >= 0.0 {
        return (position.y + delta_y, false);
    }

    let target_y = position.y + delta_y;
    let start_bottom = position.y - half_extents.y;
    let target_bottom = target_y - half_extents.y;
    let target_aabb =
        aabb_from_center_half_extents(Vec2::new(position.x, target_y), half_extents);
    let mut resolved_y = target_y;
    let mut blocked = false;

    for (rect_index, platform) in platforms.iter().enumerate() {
        if !platform_state
            .map(|state| state.platform_blocks(shelter, rect_index, platform.requires_lift_jump))
            .unwrap_or(true)
        {
            continue;
        }
        let world_rect = translate_rect(platform.rect, origin);
        if !ranges_overlap(
            target_aabb.min.x,
            target_aabb.max.x,
            world_rect.min.x,
            world_rect.max.x,
        ) {
            continue;
        }
        if start_bottom < world_rect.max.y - CAPSULE_COLLISION_EPS {
            continue;
        }
        if target_bottom > world_rect.max.y + CAPSULE_COLLISION_EPS {
            continue;
        }

        let stop_y = world_rect.max.y + half_extents.y;
        if stop_y > resolved_y {
            resolved_y = stop_y;
            blocked = true;
        }
    }

    (resolved_y, blocked)
}

fn platform_ground_distance(
    shelter: Entity,
    platforms: &[CapsulePlatform],
    origin: Vec2,
    position: Vec2,
    half_extents: Vec2,
    max_distance: f32,
    platform_state: Option<&CapsuleDropPlatformState>,
) -> f32 {
    let bottom = position.y - half_extents.y;
    let player_aabb = aabb_from_center_half_extents(position, half_extents);
    platforms
        .iter()
        .enumerate()
        .fold(max_distance, |best, (rect_index, platform)| {
            if !platform_state
                .map(|state| state.platform_blocks(shelter, rect_index, platform.requires_lift_jump))
                .unwrap_or(true)
            {
                return best;
            }
            let world_rect = translate_rect(platform.rect, origin);
            if !ranges_overlap(
                player_aabb.min.x,
                player_aabb.max.x,
                world_rect.min.x,
                world_rect.max.x,
            ) {
                return best;
            }
            if bottom < world_rect.max.y - CAPSULE_COLLISION_EPS {
                return best;
            }
            best.min((bottom - world_rect.max.y).max(0.0))
        })
}

fn platform_support_contact(
    rect: Rect,
    player_pos: Vec2,
    half_extents: Vec2,
) -> bool {
    let player_aabb = aabb_from_center_half_extents(player_pos, half_extents);
    ranges_overlap(player_aabb.min.x, player_aabb.max.x, rect.min.x, rect.max.x)
        && (player_aabb.min.y - rect.max.y).abs() <= CAPSULE_COLLISION_EPS
}

fn platform_started_down_pass_through(
    rect: Rect,
    previous_player_pos: Vec2,
    player_pos: Vec2,
    half_extents: Vec2,
) -> bool {
    let previous_aabb =
        aabb_from_center_half_extents(previous_player_pos, half_extents);
    let player_aabb = aabb_from_center_half_extents(player_pos, half_extents);
    let swept_aabb = Rect::from_corners(
        Vec2::new(
            previous_aabb.min.x.min(player_aabb.min.x),
            previous_aabb.min.y.min(player_aabb.min.y),
        ),
        Vec2::new(
            previous_aabb.max.x.max(player_aabb.max.x),
            previous_aabb.max.y.max(player_aabb.max.y),
        ),
    );
    if !rects_overlap(swept_aabb, rect) {
        return false;
    }

    

    previous_aabb.min.y >= rect.max.y - CAPSULE_COLLISION_EPS
            && player_aabb.min.y < rect.max.y - CAPSULE_COLLISION_EPS
}

fn platform_cleared_below(
    rect: Rect,
    player_pos: Vec2,
    half_extents: Vec2,
) -> bool {
    let player_aabb = aabb_from_center_half_extents(player_pos, half_extents);
    player_aabb.max.y <= rect.min.y + CAPSULE_COLLISION_EPS
}

#[cfg(test)]
mod hatch_support_tests {
    use super::*;
    use crate::game::player::{GroundSensor, Ready, foreground::{ForegroundAction, start_action}};
    use crate::game::player::states::{Falling, Idle, Jumping, transition_locomotion};
    use theseeker_engine::physics::LinearVelocity;

    #[test]
    fn walking_off_and_back_onto_the_hatch_keeps_it_solid() {
        let shelter = Entity::from_raw_u32(1).unwrap();
        let mask = CapsuleCollisionMask::default();
        let (index, hatch) = mask.platforms.iter().enumerate().find(|(_, platform)| platform.requires_lift_jump).unwrap();
        let origin = mask.world_origin(Vec2::ZERO);
        let rect = translate_rect(hatch.rect, origin);
        let half = Vec2::new(2.0, 5.0);
        let standing = Vec2::new(rect.center().x, rect.max.y + half.y);
        let beside = standing + Vec2::X * (rect.width() * 0.5 + half.x + 1.0);
        let mut state = CapsuleDropPlatformState::default();
        let contact = state.contact_mut(CapsulePlatformKey { shelter, rect_index: index });
        sync_capsule_platform_contact(contact, rect, standing, standing, half);
        sync_capsule_platform_contact(contact, rect, standing, beside, half);
        sync_capsule_platform_contact(contact, rect, beside, standing, half);

        let solids = CapsuleSolids { shelter, origin, mask: &mask, platforms: Some(&state) };
        assert_eq!(solids.distance(ProbeDirection::Down, beside, half, 50.0), 0.0,
            "the shell's adjacent floor supports a walk away from the hatch");
        assert_eq!(solids.distance(ProbeDirection::Down, standing, half, 50.0), 0.0);
        assert_eq!(solids.resolve_axis(Axis::Y, standing, -1.0, half), (standing.y, true),
            "returning over the hatch without a jump cannot fall through");

        state.observe_lift_jump(Some(shelter), true, true, false, true);
        state.observe_lift_jump(Some(shelter), false, false, false, true);
        let solids = CapsuleSolids { shelter, origin, mask: &mask, platforms: Some(&state) };
        assert!(solids.distance(ProbeDirection::Down, standing, half, 50.0) > 0.0);
        assert_eq!(solids.resolve_axis(Axis::Y, standing, -1.0, half), (standing.y - 1.0, false),
            "a launched cabin jump permits the downward hatch crossing");
    }

    #[test]
    fn upward_arrival_keeps_hatch_support_after_crossing_its_contact_height() {
        let shelter = Entity::from_raw_u32(1).unwrap();
        let mask = CapsuleCollisionMask::default();
        let (index, hatch) = mask.platforms.iter().enumerate().find(|(_, platform)| platform.requires_lift_jump).unwrap();
        let origin = mask.world_origin(Vec2::ZERO);
        let rect = translate_rect(hatch.rect, origin);
        let half = Vec2::new(2.0, 5.0);
        let standing = Vec2::new(rect.center().x, rect.max.y + half.y);
        let arriving = standing + Vec2::Y * 3.0;
        let mut state = CapsuleDropPlatformState::default();
        let contact = state.contact_mut(CapsulePlatformKey { shelter, rect_index: index });
        sync_capsule_platform_contact(contact, rect, standing - Vec2::Y * 3.0, standing, half);
        sync_capsule_platform_contact(contact, rect, standing, arriving, half);
        let solids = CapsuleSolids { shelter, origin, mask: &mask, platforms: Some(&state) };
        assert_eq!(solids.distance(ProbeDirection::Down, arriving, half, 50.0), 3.0);
        assert_eq!(solids.resolve_axis(Axis::Y, arriving, -4.0, half), (standing.y, true),
            "the lift's three-pixel arrival gap lands on the hatch instead of falling down the rail");
    }

    #[test]
    fn a_tick_zero_ground_jump_crosses_the_hatch_and_hands_off_to_the_down_elevator() {
        let mut world = World::new();
        let mask = CapsuleCollisionMask::default();
        let hatch = mask.platforms.iter().find(|platform| platform.requires_lift_jump).unwrap();
        let rect = translate_rect(hatch.rect, mask.world_origin(Vec2::ZERO));
        let half = Vec2::new(2.0, 5.0);
        let standing = Vec2::new(rect.center().x, rect.max.y + half.y);
        let shelter = world.spawn((Capsule, Transform::default(), mask)).id();
        let player = world.spawn((Player, Idle, Transform::from_translation(standing.extend(0.0)),
            Collider::cuboid(half.x, half.y), LinearVelocity::default(), CapsuleOccupant { shelter },
            GroundSensor::default(), CapsuleDropPlatformState::default())).id();
        start_action(&mut world, player, Ready);
        let mut schedule = Schedule::default();
        // Admission follows the curve motor to exercise the allowed schedule order where
        // the newly added Jumping component reaches occupancy with its clock still at zero.
        schedule.add_systems((sense_capsule_support, advance_authored_curve, admit_ground_jump,
            resolve_capsule_motion, update_capsule_occupancy).chain());
        schedule.run(&mut world);
        assert_eq!(world.get::<Jumping>(player).unwrap().tick, 0);
        assert!(world.get::<GroundSensor>(player).unwrap().is_grounded);
        assert!(world.get::<CapsuleDropPlatformState>(player).unwrap().lift_jump.is_some());
        for _ in 0..2 { schedule.run(&mut world); }
        assert_eq!(world.get::<Transform>(player).unwrap().translation.xy(), standing,
            "the authored launch starts with two stationary ticks");
        assert!(world.get::<CapsuleDropPlatformState>(player).unwrap().lift_jump.is_some());

        let mut rose_above_hatch = false;
        let handed_off = (0..160).any(|_| {
            schedule.run(&mut world);
            rose_above_hatch |= world.get::<Transform>(player).unwrap().translation.y > standing.y;
            world.get::<ForegroundAction>(player).unwrap().state::<Elevator>().is_some()
        });
        assert!(rose_above_hatch);
        assert!(handed_off);
        let elevator = world.get::<ForegroundAction>(player).unwrap().state::<Elevator>().unwrap();
        assert_eq!(elevator.direction(), ElevatorDirection::Down);
        assert_eq!(elevator.shelter(), shelter);
        assert!(world.get::<CapsuleDropPlatformState>(player).unwrap().lift_jump.is_none());
        assert_eq!(world.get::<CapsuleOccupant>(player).unwrap().shelter, shelter,
            "the downward handoff happens before crossing the exterior occupancy trigger");
    }

    fn sense_capsule_support(
        mut players: Query<(&Transform, &Collider, &mut GroundSensor, &CapsuleDropPlatformState), With<Player>>,
        capsules: CapsuleCollisionQuery<(With<Capsule>, Without<Player>)>,
    ) {
        for (transform, collider, mut sensor, state) in &mut players {
            let half = collider.cuboid_half_extents().unwrap();
            sensor.distance = capsule_distance(ProbeDirection::Down, transform.translation.truncate(),
                half, 50.0, Some(state), &capsules);
            sensor.is_grounded = sensor.distance <= 0.01;
        }
    }

    fn admit_ground_jump(
        players: Query<(Entity, &GroundSensor), (With<Player>, Added<Player>)>,
        mut commands: Commands,
    ) {
        for (entity, sensor) in &players {
            assert!(sensor.is_grounded);
            transition_locomotion(&mut commands, entity, Jumping::new());
        }
    }

    fn advance_authored_curve(
        mut players: Query<(Entity, &mut LinearVelocity, Option<&mut Jumping>, Option<&mut Falling>), With<Player>>,
        mut commands: Commands,
    ) {
        for (entity, mut velocity, jumping, falling) in &mut players {
            advance_actor_curve(entity, &mut velocity, jumping, falling, &mut commands);
        }
    }

    fn advance_actor_curve(
        entity: Entity,
        velocity: &mut LinearVelocity,
        jumping: Option<Mut<Jumping>>,
        falling: Option<Mut<Falling>>,
        commands: &mut Commands,
    ) {
        match (jumping, falling) {
            // Release the short jump below the upper deck, as the locomotion motor does.
            (Some(jump), _) if jump.tick >= 8 => {
                transition_locomotion(commands, entity, Falling::from_jump(jump.jump_count));
            },
            (Some(mut jump), _) => {
                velocity.0.y = crate::game::physics::jump_velocity(jump.tick);
                jump.tick += 1;
            },
            (None, Some(mut fall)) => {
                velocity.0.y = crate::game::physics::gravity_velocity(fall.fall_ticks);
                fall.fall_ticks += 1;
            },
            (None, None) => {},
        }
    }

    fn resolve_capsule_motion(
        mut players: Query<(&mut Transform, &mut LinearVelocity, &Collider, &mut CapsuleDropPlatformState), With<Player>>,
        capsules: CapsuleCollisionQuery<(With<Capsule>, Without<Player>)>,
    ) {
        for (mut transform, mut velocity, collider, mut state) in &mut players {
            let half = collider.cuboid_half_extents().unwrap();
            let previous = transform.translation.truncate();
            let (y, blocked) = resolve_capsule_axis(Axis::Y, previous, velocity.0.y, half, Some(&state), &capsules);
            transform.translation.y = y;
            clear_blocked_velocity(&mut velocity, blocked);
            sync_capsule_drop_platform_state(previous, transform.translation.truncate(), half, &mut state, &capsules);
        }
    }

    fn clear_blocked_velocity(velocity: &mut LinearVelocity, blocked: bool) {
        if blocked { velocity.0.y = 0.0; }
    }
}

#[cfg(test)]
mod lift_entry_tests {
    use super::*;

    #[test]
    fn a_suspended_jump_clock_cannot_rearm_descent_after_lift_arrival() {
        use crate::game::player::{GroundSensor, states::Jumping, foreground::{ForegroundAction, start_action}};
        use theseeker_engine::physics::LinearVelocity;

        let mut world = World::new();
        let mask = capsule_shape::collision_mask();
        let entrance = translate_rect(mask.vertical_transit_down_trigger_rects[0], mask.world_origin(Vec2::ZERO));
        let half = Vec2::new(2.0, 5.0);
        let above = Vec2::new(entrance.center().x, entrance.max.y + half.y + 2.0);
        let shelter = world.spawn((Capsule, Transform::default(), mask)).id();
        let player = world.spawn((Player, Transform::from_translation(above.extend(0.0)),
            Collider::cuboid(half.x, half.y), LinearVelocity(Vec2::ZERO), CapsuleOccupant { shelter },
            GroundSensor { is_grounded: true, distance: 0.0 }, Jumping::new(),
            CapsuleDropPlatformState::default())).id();
        start_action(&mut world, player, super::super::player::Ready);
        let mut schedule = Schedule::default();
        schedule.add_systems(update_capsule_occupancy);
        schedule.run(&mut world);
        assert_eq!(world.get::<CapsuleDropPlatformState>(player).unwrap().lift_jump.unwrap().shelter, shelter);
        for tick in 0..3 {
            let velocity = crate::game::physics::jump_velocity(tick);
            world.get_mut::<Jumping>(player).unwrap().tick = tick + 1;
            world.get_mut::<LinearVelocity>(player).unwrap().0.y = velocity;
            world.get_mut::<GroundSensor>(player).unwrap().is_grounded = velocity == 0.0;
            schedule.run(&mut world);
            assert_eq!(world.get::<CapsuleDropPlatformState>(player).unwrap().lift_jump.unwrap().airborne, velocity > 0.0);
        }

        // Elevator ownership suspends the jump clock; arrival cannot reuse that component.
        world.get_mut::<CapsuleDropPlatformState>(player).unwrap().lift_jump = None;
        schedule.run(&mut world);
        assert!(world.get::<CapsuleDropPlatformState>(player).unwrap().lift_jump.is_none());
        world.entity_mut(player).remove::<Jumping>();
        world.entity_mut(player).insert(Jumping { tick: 1, jump_count: 1 });
        world.get_mut::<GroundSensor>(player).unwrap().is_grounded = true;
        world.get_mut::<LinearVelocity>(player).unwrap().0.y = crate::game::physics::jump_velocity(0);
        schedule.run(&mut world);
        world.get_mut::<GroundSensor>(player).unwrap().is_grounded = false;
        world.get_mut::<Transform>(player).unwrap().translation.y -= 4.0;
        world.get_mut::<LinearVelocity>(player).unwrap().0.y = -1.0;
        schedule.run(&mut world);
        assert_eq!(world.get::<ForegroundAction>(player).unwrap().state::<Elevator>().unwrap().direction(), ElevatorDirection::Down);
        assert!(world.get::<CapsuleDropPlatformState>(player).unwrap().lift_jump.is_none());
    }

    #[test]
    fn walking_and_upward_arrival_cannot_start_descent() {
        let shelter = Entity::from_raw_u32(1).unwrap();
        let mask = capsule_shape::collision_mask();
        let entrance = translate_rect(mask.vertical_transit_down_trigger_rects[0], mask.world_origin(Vec2::ZERO));
        let half = Vec2::new(2.0, 5.0);
        let above = Vec2::new(entrance.center().x, entrance.max.y + half.y + 2.0);
        let below = above - Vec2::Y * 4.0;
        let mut state = CapsuleDropPlatformState { lift_previous_position: Some(above), ..default() };
        state.observe_lift_jump(Some(shelter), false, true, false, false);
        assert!(!state.descent_crossed(shelter, &mask, Vec2::ZERO, below, half));
        state.observe_lift_jump(Some(shelter), true, true, false, true);
        state.observe_lift_jump(Some(shelter), false, false, true, true);
        assert!(!state.descent_crossed(shelter, &mask, Vec2::ZERO, below, half));
    }

    #[test]
    fn a_fresh_ground_jump_allows_one_downward_crossing_and_landing_disarms_it() {
        let shelter = Entity::from_raw_u32(1).unwrap();
        let mask = capsule_shape::collision_mask();
        let entrance = translate_rect(mask.vertical_transit_down_trigger_rects[0], mask.world_origin(Vec2::ZERO));
        let half = Vec2::new(2.0, 5.0);
        let above = Vec2::new(entrance.center().x, entrance.max.y + half.y + 2.0);
        let below = above - Vec2::Y * 4.0;
        let mut state = CapsuleDropPlatformState { lift_previous_position: Some(above), ..default() };
        state.observe_lift_jump(Some(shelter), true, true, false, true);
        state.observe_lift_jump(Some(shelter), false, true, false, true);
        assert!(!state.descent_crossed(shelter, &mask, Vec2::ZERO, below, half));
        state.observe_lift_jump(Some(shelter), false, false, false, true);
        assert!(state.descent_crossed(shelter, &mask, Vec2::ZERO, below, half));
        assert!(!state.descent_crossed(shelter, &mask, Vec2::ZERO, above, half));
        state.observe_lift_jump(Some(shelter), false, true, false, false);
        assert!(!state.descent_crossed(shelter, &mask, Vec2::ZERO, below, half));
    }

    #[test]
    fn a_jump_cancelled_before_takeoff_cannot_arm_a_later_fall() {
        let shelter = Entity::from_raw_u32(1).unwrap();
        let mut state = CapsuleDropPlatformState::default();
        state.observe_lift_jump(Some(shelter), true, true, false, true);
        state.observe_lift_jump(Some(shelter), false, true, false, false);
        state.observe_lift_jump(Some(shelter), false, false, false, false);
        assert!(state.lift_jump.is_none());
    }
}
