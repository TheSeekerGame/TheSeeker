//! Render-only proxies preserve historical poses without exposing them to simulation.
use std::collections::VecDeque;
use bevy::ecs::system::SystemParam;
use bevy::render::{Extract, ExtractSchedule, RenderApp};
use bevy::camera::visibility::RenderLayers;
use bevy::sprite::Anchor;
use bevy::sprite_render::{ExtractedSprites, SpriteSystems};
use bevy::transform::TransformSystems;
use crate::assets::animation::SpriteAnimation;
use crate::gent::TransformGfxFromGent;
use crate::script::ScriptPlayer;
use crate::prelude::*;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PresentationSet { Prepare, Capture, Present }

/// Add to tick-driven sprites that do not have an animation player or Gent binding.
#[derive(Component, Default)]
pub struct PresentationSource;
#[derive(Component)]
#[require(RenderLayers, PresentationAlignment)]
pub struct PresentationProxy;
/// Authoritative owner; render proxies never acquire gameplay ownership.
#[derive(Component)]
pub struct PresentationProxySource(pub Entity);
/// Share a pixel-aligned attachment point without parenting gameplay ownership.
#[derive(Component)]
pub struct PresentationAnchor(pub Entity);

/// State-level override. Spatial quantization and temporal interpolation are independent.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct PresentationPolicy {
    pub pixel_aligned: Option<bool>,
    pub stepped: bool,
}

/// Camera-grid alignment is sampled with the pose and survives source destruction.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PresentationAlignment {
    #[default]
    None,
    CameraPixels,
    UprightCameraPixels,
}

/// Marks a discontinuity in authoritative motion at a particular tick.
#[derive(Component, Debug, Clone, Copy)]
pub struct PresentationJump(pub u64);

#[derive(Resource, Debug, Default)]
pub struct PresentationDebug {
    pub player_pixels: Option<bool>,
    pub interpolation_disabled: bool,
    pub retained_samples: usize,
    pub proxies: usize,
}

#[derive(Debug, Clone)]
pub struct TickHistory<T> { samples: VecDeque<(u64, T)>, capacity: usize }
impl<T> Default for TickHistory<T> {
    fn default() -> Self { Self { samples: VecDeque::with_capacity(16), capacity: 64 } }
}
impl<T> TickHistory<T> {
    pub fn push(&mut self, tick: u64, value: T) {
        if self.samples.back().is_some_and(|sample| sample.0 >= tick) { self.samples.pop_back(); }
        if self.samples.len() == self.capacity { self.samples.pop_front(); }
        self.samples.push_back((tick, value));
    }
    pub fn pair(&self, position: f64) -> Option<(&T, &T, f32)> {
        let index = self.samples.partition_point(|sample| sample.0 as f64 <= position);
        let (tick, previous) = self.samples.get(index.checked_sub(1)?)?;
        let Some((next_tick, next)) = self.samples.get(index) else { return Some((previous, previous, 0.0)); };
        let alpha = ((position - *tick as f64) / (*next_tick - *tick) as f64).clamp(0.0, 1.0) as f32;
        Some((previous, next, alpha))
    }
    pub fn discard_before(&mut self, position: f64) {
        let keep = self.samples.partition_point(|sample| sample.0 as f64 <= position).saturating_sub(1);
        self.samples.drain(..keep);
    }
    pub fn clear(&mut self) { self.samples.clear(); }
    pub fn len(&self) -> usize { self.samples.len() }
    pub fn is_empty(&self) -> bool { self.samples.is_empty() }
}

#[derive(Clone)]
struct SpritePose {
    sprite: Sprite,
    sprite_anchor: Anchor,
    transform: Transform,
    visible: bool,
    layers: RenderLayers,
    pixel_aligned: bool,
    alignment: PresentationAlignment,
    anchor: Option<Vec3>,
    stepped: bool,
    jump: bool,
}
struct SpriteTrack {
    proxy: Entity,
    history: TickHistory<SpritePose>,
    dead_at: Option<u64>,
}
#[derive(Resource, Default)]
pub struct SpriteTracks { tracks: HashMap<Entity, SpriteTrack> }

type SourceFilter = Or<(With<PresentationSource>, With<TransformGfxFromGent>, With<ScriptPlayer<SpriteAnimation>>)>;
type ProxySprites<'w, 's> = Query<'w, 's,
    (&'static mut Sprite, &'static mut Anchor, &'static mut Transform, &'static mut Visibility, &'static mut RenderLayers, &'static mut PresentationAlignment),
    With<PresentationProxy>>;

pub struct PresentationPlugin;
impl Plugin for PresentationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpriteTracks>().init_resource::<PresentationDebug>();
        app.configure_sets(GameTickPost, (PresentationSet::Prepare, PresentationSet::Capture).chain());
        app.add_systems(GameTickPost, mark_attached_sprites.in_set(PresentationSet::Prepare));
        app.add_systems(GameTickPost, capture_sprites.in_set(PresentationSet::Capture));
        app.add_systems(PostUpdate, present_sprites.in_set(PresentationSet::Present).before(TransformSystems::Propagate));
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render.add_systems(ExtractSchedule, suppress_simulation_sprites.after(SpriteSystems::ExtractSprites));
        }
    }
}

fn mark_attached_sprites(
    mut commands: Commands,
    sprites: Query<Entity, (With<Sprite>, Without<PresentationSource>, Or<(Added<Sprite>, Changed<ChildOf>)>)>,
    parents: Query<&ChildOf>, sources: Query<(), Or<(With<crate::gent::Gent>, SourceFilter)>>,
) {
    for entity in &sprites {
        if attached_to_source(entity, &parents, &sources) { commands.entity(entity).insert(PresentationSource); }
    }
}

fn attached_to_source(mut entity: Entity, parents: &Query<&ChildOf>, sources: &Query<(), Or<(With<crate::gent::Gent>, SourceFilter)>>) -> bool {
    while let Ok(parent) = parents.get(entity) {
        entity = parent.parent();
        if sources.contains(entity) { return true; }
    }
    false
}

#[derive(SystemParam)]
struct CaptureQueries<'w, 's> {
    sprites: Query<'w, 's, (Entity, &'static Sprite, &'static Anchor, Option<&'static RenderLayers>, Option<&'static TransformGfxFromGent>), SourceFilter>,
    transforms: TransformHelper<'w, 's>,
    visibility: Query<'w, 's, (Option<&'static Visibility>, Option<&'static ChildOf>)>,
    policies: Query<'w, 's, &'static PresentationPolicy>,
    alignments: Query<'w, 's, &'static PresentationAlignment>,
    jumps: Query<'w, 's, &'static PresentationJump>,
    anchors: Query<'w, 's, &'static PresentationAnchor>,
    bindings: Query<'w, 's, &'static TransformGfxFromGent>,
    stretches: Query<'w, 's, (&'static crate::effects::SpriteScale, Option<&'static crate::effects::sprite_stretch::SpriteStretch>)>,
}

fn capture_sprites(mut commands: Commands, time: Res<GameTime>, queries: CaptureQueries, mut tracks: ResMut<SpriteTracks>) {
    for track in tracks.tracks.values_mut() {
        if track.dead_at.is_none() { track.dead_at = Some(time.tick()); }
    }
    for (entity, sprite, sprite_anchor, layers, gent) in &queries.sprites {
        let Ok(mut transform) = queries.transforms.compute_global_transform(entity) else { continue; };
        if let Ok((scale, stretch)) = queries.stretches.get(entity) {
            crate::effects::sprite_stretch::apply_render_stretch(scale, stretch, &mut transform);
        }
        let source = gent.map_or(entity, |binding| binding.gent);
        let policy = queries.policies.get(entity).or_else(|_| queries.policies.get(source)).copied().unwrap_or_default();
        let pixel_aligned = policy.pixel_aligned.unwrap_or_else(|| gent.is_some_and(|binding| binding.pixel_aligned));
        let pose = SpritePose {
            sprite: sprite.clone(), sprite_anchor: *sprite_anchor, transform: transform.compute_transform(),
            visible: source_visible(entity, &queries.visibility), layers: layers.cloned().unwrap_or_default(),
            alignment: queries.alignments.get(entity).or_else(|_| queries.alignments.get(source)).copied().unwrap_or_default(),
            pixel_aligned, anchor: attachment_anchor(entity, &queries), stepped: policy.stepped,
            jump: attachment_jumped(entity, time.tick(), &queries),
        };
        capture_sprite(&mut commands, &mut tracks, entity, time.tick(), pose);
    }
}

fn attachment_parent(entity: Entity, queries: &CaptureQueries) -> Option<Entity> {
    queries.anchors.get(entity).ok().map(|anchor| anchor.0)
        .or_else(|| queries.visibility.get(entity).ok().and_then(|(_, parent)| parent.map(ChildOf::parent)))
}

fn attachment_anchor(mut entity: Entity, queries: &CaptureQueries) -> Option<Vec3> {
    while let Some(parent) = attachment_parent(entity, queries) {
        entity = parent;
        if anchor_is_aligned(entity, queries) {
            return queries.transforms.compute_global_transform(entity).ok().map(|transform| transform.translation());
        }
    }
    None
}

fn anchor_is_aligned(entity: Entity, queries: &CaptureQueries) -> bool {
    let Ok(binding) = queries.bindings.get(entity) else { return false; };
    queries.policies.get(entity).or_else(|_| queries.policies.get(binding.gent)).ok()
        .and_then(|policy| policy.pixel_aligned).unwrap_or(binding.pixel_aligned)
}

fn attachment_jumped(mut entity: Entity, tick: u64, queries: &CaptureQueries) -> bool {
    loop {
        let source = queries.bindings.get(entity).map_or(entity, |binding| binding.gent);
        if queries.jumps.get(entity).or_else(|_| queries.jumps.get(source)).is_ok_and(|jump| jump.0 == tick) { return true; }
        let Some(parent) = attachment_parent(entity, queries) else { return false; };
        entity = parent;
    }
}

fn align_pose_pixels(transform: &mut Transform, previous: &SpritePose, next: &SpritePose, alpha: f32) {
    let anchor = if previous.pixel_aligned { Some(transform.translation) }
        else { previous.anchor.map(|anchor| anchor.lerp(next.anchor.unwrap_or(anchor), alpha)) };
    if let Some(anchor) = anchor {
        transform.translation.x += anchor.x.round() - anchor.x;
        transform.translation.y += anchor.y.round() - anchor.y;
    }
}

fn capture_sprite(commands: &mut Commands, tracks: &mut SpriteTracks, entity: Entity, tick: u64, pose: SpritePose) {
    let track = tracks.tracks.entry(entity).or_insert_with(|| {
        commands.entity(entity).insert(PresentationSource);
        SpriteTrack {
        proxy: commands.spawn((PresentationProxy, PresentationProxySource(entity), Sprite::default(), Visibility::Hidden, Transform::default())).id(),
        history: TickHistory::default(), dead_at: None,
        }
    });
    track.dead_at = None;
    track.history.push(tick, pose);
}

fn source_visible(mut entity: Entity, query: &Query<(Option<&Visibility>, Option<&ChildOf>)>) -> bool {
    // Bevy rejects hierarchy cycles; a finite walk also tolerates a just-despawned parent.
    loop {
        let Ok((visibility, parent)) = query.get(entity) else { return false; };
        if visibility == Some(&Visibility::Hidden) { return false; }
        if visibility == Some(&Visibility::Visible) { return true; }
        let Some(parent) = parent else { return true; };
        entity = parent.parent();
    }
}

fn present_sprites(
    mut commands: Commands, mut tracks: ResMut<SpriteTracks>, cursor: Res<RenderInterpolation>,
    mut debug: ResMut<PresentationDebug>,
    mut proxies: ProxySprites,
) {
    let position = cursor.position();
    tracks.tracks.retain(|_, track| {
        if track.dead_at.is_some_and(|tick| position >= tick as f64) {
            commands.entity(track.proxy).try_despawn();
            false
        } else { true }
    });
    debug.retained_samples = tracks.tracks.values().map(|track| track.history.len()).sum();
    debug.proxies = tracks.tracks.len();
    for track in tracks.tracks.values_mut() {
        present_sprite(&mut proxies, track, position, &debug);
        track.history.discard_before(position);
    }
}

fn present_sprite(
    proxies: &mut ProxySprites,
    track: &SpriteTrack, position: f64, debug: &PresentationDebug,
) {
    let Ok((mut sprite, mut sprite_anchor, mut transform, mut visibility, mut layers, mut alignment)) = proxies.get_mut(track.proxy) else { return; };
    let Some((previous, next, alpha)) = track.history.pair(position) else { visibility.set_if_neq(Visibility::Hidden); return; };
    let alpha = if previous.stepped || next.jump || debug.interpolation_disabled { 0.0 } else { alpha };
    let mut pose = interpolate_transform(&previous.transform, &next.transform, alpha);
    align_pose_pixels(&mut pose, previous, next, alpha);
    transform.set_if_neq(pose);
    *sprite = previous.sprite.clone();
    sprite_anchor.set_if_neq(previous.sprite_anchor);
    visibility.set_if_neq(if previous.visible { Visibility::Visible } else { Visibility::Hidden });
    layers.set_if_neq(previous.layers.clone());
    alignment.set_if_neq(previous.alignment);
}

pub fn interpolate_transform(previous: &Transform, next: &Transform, alpha: f32) -> Transform {
    Transform {
        translation: previous.translation.lerp(next.translation, alpha),
        rotation: previous.rotation.slerp(next.rotation, alpha),
        scale: previous.scale.lerp(next.scale, alpha),
    }
}

fn suppress_simulation_sprites(mut sprites: ResMut<ExtractedSprites>, sources: Extract<Query<Entity, (With<Sprite>, SourceFilter)>>) {
    sprites.sprites.retain(|sprite| sources.get(sprite.main_entity).is_err());
}

pub fn clear_presentation(mut commands: Commands, mut tracks: ResMut<SpriteTracks>) {
    for (_, track) in tracks.tracks.drain() { commands.entity(track.proxy).try_despawn(); }
}

#[cfg(test)]
mod tests;
