use bevy::camera::ClearColorConfig;
use bevy::render::render_resource::TextureFormat;
use bevy::camera::visibility::RenderLayers;
use theseeker_engine::animation::SpriteAnimationBundle;
use theseeker_engine::physics::{
    geometry::ProbeDirection, Collider, CollisionGroups, LinearVelocity, TileCollisionWorld, PLAYER,
    SENSOR,
};

use crate::camera::{MainCamera, SnapCameraNext};
use crate::game::gentstate::Facing;
use crate::game::player::states::{
    transition_action, transition_locomotion, Idle, Ready,
};
use crate::game::player::Player;
use crate::game::world_objects::WorldObjectKind;
use crate::prelude::*;

const TENT_HALF_EXTENTS: Vec2 = Vec2::new(14.0, 12.0);
const TENT_INTERACTION_HALF_EXTENTS: Vec2 = Vec2::new(12.0, 10.0);
const TENT_FORWARD_OFFSET: f32 = 18.0;
const TENT_SEARCH_START_OFFSET_Y: f32 = 16.0;
const TENT_GROUND_SEARCH_DISTANCE: f32 = 80.0;
const TENT_SUPPORT_SAMPLE_OFFSET: f32 = 0.5;
pub(super) const TENT_Z: f32 = 13.0 * 0.000_001;

pub const TENT_INTERIOR_RENDER_LAYER: usize = 3;
const TENT_INTERIOR_WORLD_ORIGIN: Vec2 = Vec2::new(20_000.0, 20_000.0);
const TENT_INTERIOR_BACKDROP_Z: f32 = 1.0 * 0.000_001;
const TENT_INTERIOR_OUTSIDE_CAMERA_LAYERS: [usize; 2] = [0, 1];
const TENT_INTERIOR_IMAGE_PATH: &str = "animations/objects/TentInside.png";
const TENT_INTERIOR_GROUND_CONTACT_EPS: f32 = 0.01;
const TENT_INTERIOR_SPAWN_SEARCH_DISTANCE: f32 = 4.0;

#[derive(Component)]
pub struct PortableTent;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShelterInteriorKind {
    Tent,
}

impl ShelterInteriorKind {
    const ALL: [Self; 1] = [Self::Tent];

    fn image_path(self) -> &'static str {
        match self {
            Self::Tent => TENT_INTERIOR_IMAGE_PATH,
        }
    }
}

#[derive(Component, Debug, Clone, Copy)]
#[component(storage = "SparseSet")]
pub struct PendingTentEntry {
    pub shelter: Entity,
    pub kind: ShelterInteriorKind,
    pub return_position: Vec2,
}

#[derive(Component, Debug, Clone, Copy)]
#[component(storage = "SparseSet")]
pub struct TentInteriorOccupant {
    pub kind: ShelterInteriorKind,
    pub return_position: Vec2,
}

#[derive(Component)]
pub(crate) struct TentInteriorBackdrop;


#[derive(Resource, Default)]
pub struct TentInteriorAssets {
    tent: ShelterInteriorAssetState,
}

impl TentInteriorAssets {
    fn state(&self, kind: ShelterInteriorKind) -> &ShelterInteriorAssetState {
        match kind {
            ShelterInteriorKind::Tent => &self.tent,
        }
    }

    fn state_mut(
        &mut self,
        kind: ShelterInteriorKind,
    ) -> &mut ShelterInteriorAssetState {
        match kind {
            ShelterInteriorKind::Tent => &mut self.tent,
        }
    }

    pub fn map(&self, kind: ShelterInteriorKind) -> Option<&TentInteriorMap> {
        self.state(kind).map.as_ref()
    }

    fn display_image(
        &self,
        kind: ShelterInteriorKind,
    ) -> Option<&Handle<Image>> {
        let state = self.state(kind);
        (state.display_image != Handle::<Image>::default())
            .then_some(&state.display_image)
    }
}

#[derive(Default)]
struct ShelterInteriorAssetState {
    image: Handle<Image>,
    display_image: Handle<Image>,
    map: Option<TentInteriorMap>,
}

#[derive(Clone)]
pub struct TentInteriorMap {
    size: UVec2,
    solids: TileCollisionWorld,
    exits: TileCollisionWorld,
}

impl TentInteriorMap {
    pub fn center_world(&self) -> Vec2 {
        TENT_INTERIOR_WORLD_ORIGIN
            + Vec2::new(self.size.x as f32, self.size.y as f32) * 0.5
    }

    pub fn to_local(&self, world: Vec2) -> Vec2 {
        world - TENT_INTERIOR_WORLD_ORIGIN
    }

    pub fn to_world(&self, local: Vec2) -> Vec2 {
        TENT_INTERIOR_WORLD_ORIGIN + local
    }

    pub fn solids(&self) -> &TileCollisionWorld {
        &self.solids
    }

    pub fn exit_overlaps_world(
        &self,
        world_position: Vec2,
        half_extents: Vec2,
    ) -> bool {
        self.exits
            .overlaps_solid(self.to_local(world_position), half_extents)
    }

    pub fn find_spawn_local(&self, half_extents: Vec2) -> Option<Vec2> {
        let width = self.size.x as i32;
        let height = self.size.y as i32;
        let center_x = (self.size.x / 2) as i32;

        self.find_grounded_spawn(height, width, center_x, half_extents)
            .or_else(|| self.find_open_spawn(height, width, half_extents))
    }

    fn find_grounded_spawn(
        &self,
        height: i32,
        width: i32,
        center_x: i32,
        half_extents: Vec2,
    ) -> Option<Vec2> {
        (0..height).find_map(|y| {
            self.find_grounded_spawn_in_row(y, width, center_x, half_extents)
        })
    }

    fn find_open_spawn(
        &self,
        height: i32,
        width: i32,
        half_extents: Vec2,
    ) -> Option<Vec2> {
        (0..height)
            .find_map(|y| self.find_open_spawn_in_row(y, width, half_extents))
    }

    fn find_grounded_spawn_in_row(
        &self,
        y: i32,
        width: i32,
        center_x: i32,
        half_extents: Vec2,
    ) -> Option<Vec2> {
        (0..width)
            .flat_map(|dx| centered_x_candidates(center_x, dx))
            .find_map(|candidate_x| {
                self.grounded_spawn_candidate(
                    candidate_x,
                    y,
                    width,
                    half_extents,
                )
            })
    }

    fn find_open_spawn_in_row(
        &self,
        y: i32,
        width: i32,
        half_extents: Vec2,
    ) -> Option<Vec2> {
        (0..width)
            .map(|x| Vec2::new(x as f32, y as f32))
            .find(|candidate| self.open_spawn_candidate(*candidate, half_extents))
    }

    fn grounded_spawn_candidate(
        &self,
        candidate_x: i32,
        y: i32,
        width: i32,
        half_extents: Vec2,
    ) -> Option<Vec2> {
        if candidate_x < 0 || candidate_x >= width {
            return None;
        }

        let candidate = Vec2::new(candidate_x as f32, y as f32);
        self.open_spawn_candidate(candidate, half_extents)
            .then_some(candidate)
            .filter(|candidate| {
                let ground_distance = self.solids.distance(
                    ProbeDirection::Down,
                    *candidate,
                    half_extents,
                    TENT_INTERIOR_SPAWN_SEARCH_DISTANCE,
                );
                ground_distance <= TENT_INTERIOR_GROUND_CONTACT_EPS
            })
    }

    fn open_spawn_candidate(&self, candidate: Vec2, half_extents: Vec2) -> bool {
        !self.solids.overlaps_solid(candidate, half_extents)
            && !self.exits.overlaps_solid(candidate, half_extents)
    }
}

fn centered_x_candidates(center_x: i32, dx: i32) -> [i32; 2] {
    if dx == 0 {
        return [center_x, center_x];
    }

    [center_x - dx, center_x + dx]
}

pub fn load_tent_interior_image(
    asset_server: Res<AssetServer>,
    mut tent_interior: ResMut<TentInteriorAssets>,
) {
    for kind in ShelterInteriorKind::ALL {
        let state = tent_interior.state_mut(kind);
        if state.image == Handle::<Image>::default() {
            state.image = asset_server.load(kind.image_path());
        }
    }
}

pub fn populate_tent_interior_map(
    mut images: ResMut<Assets<Image>>,
    mut tent_interior: ResMut<TentInteriorAssets>,
) {
    for kind in ShelterInteriorKind::ALL {
        let image_handle = tent_interior.state(kind).image.clone();
        let needs_map = tent_interior.state(kind).map.is_none();
        let needs_display =
            tent_interior.state(kind).display_image == Handle::<Image>::default();

        if image_handle == Handle::<Image>::default()
            || (!needs_map && !needs_display)
        {
            continue;
        }

        let Some((map, display_image)) =
            pending_tent_interior_assets(&images, &image_handle, needs_map, needs_display)
        else {
            continue;
        };

        let state = tent_interior.state_mut(kind);
        if state.map.is_none() {
            state.map = map;
        }
        if state.display_image == Handle::<Image>::default() {
            assign_tent_display_image(state, display_image, &mut images);
        }
    }
}

fn pending_tent_interior_assets(
    images: &Assets<Image>,
    image_handle: &Handle<Image>,
    needs_map: bool,
    needs_display: bool,
) -> Option<(Option<TentInteriorMap>, Option<Image>)> {
    let image = images.get(image_handle)?;
    let map = if needs_map {
        Some(build_tent_interior_map(image).ok()?)
    } else {
        None
    };
    let display_image = if needs_display {
        clone_tent_interior_display_image(image)
    } else {
        None
    };

    Some((map, display_image))
}

fn assign_tent_display_image(
    state: &mut ShelterInteriorAssetState,
    display_image: Option<Image>,
    images: &mut Assets<Image>,
) {
    let Some(display_image) = display_image else {
        return;
    };

    state.display_image = images.add(display_image);
}

pub fn collision_space<'a>(
    interior_kind: Option<ShelterInteriorKind>,
    tent_interior: Option<&'a TentInteriorAssets>,
    world_collision: Option<&'a TileCollisionWorld>,
) -> Option<(&'a TileCollisionWorld, Vec2)> {
    if let Some(kind) = interior_kind {
        tent_interior
            .and_then(|assets| assets.map(kind))
            .map(|map| (map.solids(), TENT_INTERIOR_WORLD_ORIGIN))
    } else {
        world_collision.map(|world| (world, Vec2::ZERO))
    }
}

pub fn interior_center_world(
    tent_interior: Option<&TentInteriorAssets>,
    kind: ShelterInteriorKind,
) -> Option<Vec2> {
    tent_interior
        .and_then(|assets| assets.map(kind))
        .map(TentInteriorMap::center_world)
}


pub fn resolve_pending_tent_entry(
    mut commands: Commands,
    mut player_q: Query<
        (
            Entity,
            &mut Transform,
            &mut LinearVelocity,
            &Collider,
            &PendingTentEntry,
            Option<&TentInteriorOccupant>,
        ),
        (With<Player>, Without<crate::game::spikes::SpikeRespawnPending>),
    >,
    shelter_q: Query<(), With<WorldObjectKind>>,
    tent_interior: Option<Res<TentInteriorAssets>>,
) {
    let Some(tent_interior) = tent_interior.as_deref() else {
        return;
    };

    for (
        entity,
        mut transform,
        mut velocity,
        collider,
        pending_entry,
        interior_occupant,
    ) in player_q.iter_mut()
    {
        if interior_occupant.is_some() {
            commands.entity(entity).remove::<PendingTentEntry>();
            continue;
        }
        if shelter_q.get(pending_entry.shelter).is_err() {
            commands.entity(entity).remove::<PendingTentEntry>();
            continue;
        }
        let Some(map) = tent_interior.map(pending_entry.kind) else {
            continue;
        };

        let Some(cuboid_half) = collider.cuboid_half_extents() else {
            commands.entity(entity).remove::<PendingTentEntry>();
            continue;
        };
        let Some(spawn_local) = map.find_spawn_local(cuboid_half)
        else {
            commands.entity(entity).remove::<PendingTentEntry>();
            continue;
        };

        transform.translation =
            map.to_world(spawn_local).extend(transform.translation.z);
        velocity.0 = Vec2::ZERO;

        commands
            .entity(entity)
            .insert(TentInteriorOccupant {
                kind: pending_entry.kind,
                return_position: pending_entry.return_position,
            })
            .remove::<PendingTentEntry>();
        commands.insert_resource(SnapCameraNext);

        transition_action(&mut commands, entity, Ready);
        transition_locomotion(&mut commands, entity, Idle);
    }
}

pub fn exit_tent_on_exit_overlap(
    mut commands: Commands,
    mut player_q: Query<
        (
            Entity,
            &TentInteriorOccupant,
            &mut Transform,
            &mut LinearVelocity,
            &Collider,
        ),
        (With<Player>, Without<crate::game::spikes::SpikeRespawnPending>),
    >,
    tent_interior: Option<Res<TentInteriorAssets>>,
) {
    let Some(tent_interior) = tent_interior.as_deref() else {
        return;
    };

    for (entity, occupant, mut transform, mut velocity, collider) in
        player_q.iter_mut()
    {
        let Some(map) = tent_interior.map(occupant.kind) else {
            continue;
        };
        let Some(cuboid_half) = collider.cuboid_half_extents() else {
            continue;
        };
        let player_pos = transform.translation.truncate();
        if !map.exit_overlaps_world(player_pos, cuboid_half) {
            continue;
        }

        transform.translation =
            occupant.return_position.extend(transform.translation.z);
        velocity.0 = Vec2::ZERO;

        commands.entity(entity).remove::<TentInteriorOccupant>();
        commands.insert_resource(SnapCameraNext);

        transition_action(&mut commands, entity, Ready);
        transition_locomotion(&mut commands, entity, Idle);
    }
}

pub fn sync_tent_interior_backdrop(
    mut commands: Commands,
    player_inside_q: Query<&TentInteriorOccupant, With<Player>>,
    backdrop_q: Query<(Entity, &Sprite), With<TentInteriorBackdrop>>,
    tent_interior: Option<Res<TentInteriorAssets>>,
) {
    let Some(tent_interior) = tent_interior.as_deref() else {
        for (entity, _) in backdrop_q.iter() {
            commands.entity(entity).despawn();
        }
        return;
    };

    let Some(occupant) = player_inside_q.iter().next() else {
        for (entity, _) in backdrop_q.iter() {
            commands.entity(entity).despawn();
        }
        return;
    };

    let Some(map) = tent_interior.map(occupant.kind) else {
        return;
    };
    let Some(display_image) = tent_interior.display_image(occupant.kind) else {
        for (entity, _) in backdrop_q.iter() {
            commands.entity(entity).despawn();
        }
        return;
    };

    let display_image = display_image.clone();
    let mut matching_backdrop = None;
    for (entity, sprite) in backdrop_q.iter() {
        if matching_backdrop.is_none() && sprite.image == display_image {
            matching_backdrop = Some(entity);
            continue;
        }
        commands.entity(entity).despawn();
    }

    if matching_backdrop.is_some() {
        return;
    }

    commands.spawn((
        TentInteriorBackdrop,
        Sprite {
            image: display_image,
            ..default()
        },
        Transform::from_translation(
            map.center_world().extend(TENT_INTERIOR_BACKDROP_Z),
        ),
        GlobalTransform::default(),
        Visibility::Visible,
        InheritedVisibility::VISIBLE,
        ViewVisibility::default(),
        RenderLayers::layer(TENT_INTERIOR_RENDER_LAYER),
        StateDespawnMarker,
        Name::new("TentInteriorBackdrop"),
    ));
}

pub fn sync_tent_interior_view(
    player_inside_q: Query<(), (With<Player>, With<TentInteriorOccupant>)>,
    mut main_camera_q: Query<(&mut Camera, &mut RenderLayers), With<MainCamera>>,
) {
    let Ok((mut camera, mut layers)) = main_camera_q.single_mut() else {
        return;
    };

    if player_inside_q.is_empty() {
        camera.clear_color = ClearColorConfig::Default;
        *layers =
            RenderLayers::from_layers(&TENT_INTERIOR_OUTSIDE_CAMERA_LAYERS);
        return;
    }

    camera.clear_color = ClearColorConfig::Custom(Color::BLACK);
    *layers = RenderLayers::layer(TENT_INTERIOR_RENDER_LAYER);
}



fn compute_tent_placement(
    player_transform: &Transform,
    facing: &Facing,
    collision_world: Option<&TileCollisionWorld>,
    collision_origin: Vec2,
    allow_placement: bool,
) -> super::placement_preview::Placement {
    let player_pos = player_transform.translation.truncate() - collision_origin;
    let center_x =
        player_pos.x + facing.direction() * TENT_FORWARD_OFFSET;
    let center_y =
        tent_grounded_center_y(player_pos, center_x, collision_world)
            .unwrap_or(player_pos.y);

    let center_local = Vec2::new(center_x.round(), center_y.round());
    let valid = allow_placement
        && collision_world
            .is_some_and(|world| is_valid_tent_placement(world, center_local));
    let center = center_local + collision_origin;

    super::placement_preview::Placement { center, valid }
}

fn tent_grounded_center_y(
    player_pos: Vec2,
    center_x: f32,
    collision_world: Option<&TileCollisionWorld>,
) -> Option<f32> {
    let collision_world = collision_world.filter(|world| world.is_ready())?;
    let start_center = Vec2::new(
        center_x,
        player_pos.y + TENT_HALF_EXTENTS.y + TENT_SEARCH_START_OFFSET_Y,
    );
    let distance = collision_world.distance(
        ProbeDirection::Down,
        start_center,
        TENT_HALF_EXTENTS,
        TENT_GROUND_SEARCH_DISTANCE,
    );

    (distance < TENT_GROUND_SEARCH_DISTANCE).then_some(start_center.y - distance)
}

fn is_valid_tent_placement(
    collision_world: &TileCollisionWorld,
    center: Vec2,
) -> bool {
    if !collision_world.is_ready()
        || collision_world.overlaps_solid(center, TENT_HALF_EXTENTS)
    {
        return false;
    }

    let sample_y =
        center.y - TENT_HALF_EXTENTS.y - TENT_SUPPORT_SAMPLE_OFFSET;
    let left =
        center.x - TENT_HALF_EXTENTS.x + TENT_SUPPORT_SAMPLE_OFFSET;
    let right =
        center.x + TENT_HALF_EXTENTS.x - TENT_SUPPORT_SAMPLE_OFFSET;
    let step =
        (collision_world.tile_size * 0.5).max(TENT_SUPPORT_SAMPLE_OFFSET);

    let mut sample_x = left;
    loop {
        if !collision_world
            .is_solid_at_world_point(Vec2::new(sample_x, sample_y))
        {
            return false;
        }

        if sample_x >= right {
            break;
        }
        sample_x = (sample_x + step).min(right);
    }

    true
}


pub(super) fn spawn_portable_tent(commands: &mut Commands, center: Vec2) {
    commands.spawn((
        PortableTent,
        WorldObjectKind::PortableTent,
        Collider::cuboid(
            TENT_INTERACTION_HALF_EXTENTS.x,
            TENT_INTERACTION_HALF_EXTENTS.y,
        ),
        CollisionGroups::new(SENSOR, PLAYER),
        SpriteAnimationBundle::new_play_key("anim.objects.Tent"),
        Sprite {
            texture_atlas: Some(TextureAtlas::default()),
            ..default()
        },
        Transform::from_translation(center.extend(TENT_Z)),
        GlobalTransform::default(),
        Visibility::Visible,
        InheritedVisibility::VISIBLE,
        ViewVisibility::default(),
        RenderLayers::layer(0),
        StateDespawnMarker,
        Name::new("PortableTent"),
    ));
}



fn clone_tent_interior_display_image(image: &Image) -> Option<Image> {
    let mut cleaned = image.clone().convert(TextureFormat::Rgba8UnormSrgb)?;
    let data = cleaned.data.as_mut()?;
    for rgba in data.as_chunks_mut::<4>().0.iter_mut() {
        if rgba[0] == 0xF5
            && rgba[1] == 0xD9
            && rgba[2] == 0x3F
            && rgba[3] != 0
        {
            rgba.copy_from_slice(&[0, 0, 0, 0]);
        }
    }
    Some(cleaned)
}

fn build_tent_interior_map(image: &Image) -> Result<TentInteriorMap, ()> {
    let Some(converted) = image.clone().convert(TextureFormat::Rgba8UnormSrgb)
    else {
        return Err(());
    };
    let Some(bytes) = converted.data.as_ref() else {
        return Err(());
    };

    let width = converted.width() as usize;
    let height = converted.height() as usize;
    let mut solid_tiles = vec![false; width * height];
    let mut exit_tiles = vec![false; width * height];

    for y in 0..height {
        for x in 0..width {
            let pixel_start = (y * width + x) * 4;
            let rgba = &bytes[pixel_start..pixel_start + 4];
            let idx = y * width + x;
            record_tent_interior_pixel(
                rgba,
                idx,
                &mut solid_tiles,
                &mut exit_tiles,
            );
        }
    }

    let mut solids = TileCollisionWorld::default();
    solids.rebuild(width as u32, height as u32, 1.0, &solid_tiles);

    let mut exits = TileCollisionWorld::default();
    exits.rebuild(width as u32, height as u32, 1.0, &exit_tiles);

    Ok(TentInteriorMap {
        size: UVec2::new(width as u32, height as u32),
        solids,
        exits,
    })
}

fn record_tent_interior_pixel(
    rgba: &[u8],
    idx: usize,
    solid_tiles: &mut [bool],
    exit_tiles: &mut [bool],
) {
    if rgba[3] == 0 {
        return;
    }

    if rgba[0] == 0 && rgba[1] == 0 && rgba[2] == 0 {
        solid_tiles[idx] = true;
        return;
    }

    if rgba[0] == 0xF5 && rgba[1] == 0xD9 && rgba[2] == 0x3F {
        exit_tiles[idx] = true;
    }
}

pub(super) fn preview_placement(
    transform: &Transform,
    facing: &Facing,
    occupant: Option<&TentInteriorOccupant>,
    tiles: Option<&TileCollisionWorld>,
    interiors: Option<&TentInteriorAssets>,
) -> super::placement_preview::Placement {
    let Some(occupant) = occupant else {
        return compute_tent_placement(transform, facing, tiles, Vec2::ZERO, true);
    };
    let tiles = interiors.and_then(|assets| assets.map(occupant.kind)).map(TentInteriorMap::solids);
    compute_tent_placement(transform, facing, tiles, TENT_INTERIOR_WORLD_ORIGIN, false)
}
