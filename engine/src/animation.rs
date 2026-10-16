mod timeline;
pub use timeline::{PlaybackEvent, PlaybackEventKind, PlaybackId};
use timeline::{PlaybackEvents, PlaybackTimeline};

use bevy::ecs::system::lifetimeless::*;
use bevy::ecs::system::SystemParam;

use crate::assets::animation::*;
use crate::assets::script::*;
use crate::data::OneOrMany;
use crate::prelude::*;
use crate::profile::TickProfiler;
use crate::script::common::ExtendedScriptTracker;
use crate::script::*;

/// Playback event publication follows the fixed-tick script commit and advance.
#[derive(SystemSet, Clone, PartialEq, Eq, Debug, Hash)]
pub enum AnimationSet { Signals }

pub struct SpriteAnimationPlugin;

impl Plugin for SpriteAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlaybackEvents>();
        app.add_script_runtime::<SpriteAnimation>();

        app.configure_sets(crate::time::GameTickUpdate, AnimationSet::Signals.after(ScriptSet::Run));

        app.add_systems(
            crate::time::GameTickUpdate,
            (
                update_animation_signals,
                mirror_animation_signals_to_gent,
            )
                .chain()
                .in_set(AnimationSet::Signals),
        );


    }
}

#[derive(Bundle, Default)]
pub struct SpriteAnimationBundle {
    pub player: ScriptPlayer<SpriteAnimation>,
}

impl SpriteAnimationBundle {
    pub fn new_play_handle(handle: Handle<SpriteAnimation>) -> Self {
        let mut player = ScriptPlayer::default();
        player.play_handle(handle);
        Self { player }
    }
    pub fn new_play_key(key: &str) -> Self {
        let mut player = ScriptPlayer::default();
        player.play_key(key);
        Self { player }
    }
}

#[derive(Default)]
pub struct SpriteAnimationTracker {
    timeline: PlaybackTimeline,
    initial_frame: FrameId,
    next_is_loop: bool,
    next_is_seek: bool,
    frame_actions_started: bool,
    last_tick: Option<u64>,
    section: Option<String>,
    clip_mode: Option<ClipMode>,
    loop_start: Option<FrameId>,
    carryover: SpriteAnimationCarryover,
    frame_actions: HashMap<FrameId, Vec<ActionId>>,
    framequant_actions: Vec<(Quant, ActionId)>,
    reversed: bool,
    next_frame: Option<FrameId>,
    frame_min: FrameId,
    frame_max: FrameId,
    ticks_per_frame: u32,
    ticks_remain: u32,
    frame_clock: FrameClock,
    bookmarks: HashMap<String, FrameId>,
    q_extra: Vec<QueuedAction>,
}

#[derive(Default)]
pub struct SpriteAnimationCarryover {
    frame: Option<FrameId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CueId(pub u64);

pub fn cue_id(name: &str) -> CueId {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0100_0000_01b3;

    let mut hash = FNV_OFFSET;
    for byte in name.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    CueId(hash)
}

#[derive(Component, Debug, Clone, Default)]
pub struct PlaybackState {
    pub playback: PlaybackId,
    pub ordered: Vec<PlaybackEvent>,
    pub current_key: Option<String>,
    pub current_frame: Option<FrameId>,
    pub started: bool,
    pub finished: bool,
    pub interrupted: bool,
    pub looped: bool,
    pub frame_entered: Option<FrameId>,
    pub frame_exited: Option<FrameId>,
}

impl PlaybackState {
    pub fn clear_for_tick(&mut self) {
        self.ordered.clear();
        self.clear_edges();
    }

    fn clear_edges(&mut self) {
        self.interrupted = false;
        self.started = false;
        self.finished = false;
        self.looped = false;
        self.frame_entered = None;
        self.frame_exited = None;
    }

    pub fn has_event(&self, event: &str) -> bool { self.has_cue_for(self.playback, cue_id(event)) }

}

impl SpriteAnimationTracker {
    fn resolve_bookmark(&self, bm: Option<&String>) -> FrameId {
        let default = || FrameId::from_sprite_index(0);
        let Some(bm) = bm else {
            return default();
        };
        if let Some(i) = self.bookmarks.get(bm) {
            *i
        } else {
            default()
        }
    }

    /// Maps a frame index based on the old animation frame and mapping lists
    fn map_frame_index(
        &self,
        old_frame_list: Option<&OneOrMany<FrameId>>,
        new_frame_list: Option<&OneOrMany<FrameId>>,
        old_frame: FrameId,
        default_frame: FrameId,
    ) -> FrameId {
        // If no old frame list specified, just use the default or specified frame
        let Some(old_list) = old_frame_list else {
            return new_frame_list
                .and_then(|list| match list {
                    OneOrMany::Single(f) => Some(*f),
                    OneOrMany::Many(v) => v.first().copied(),
                })
                .unwrap_or(default_frame);
        };

        // Check if old frame is in the list and find its position
        let position = frame_position_in_list(old_list, old_frame);

        // If old frame not in list, return default
        let Some(pos) = position else {
            return default_frame;
        };

        // Map to new frame based on the new frame list
        match new_frame_list {
            None => {
                // No new list: preserve the old frame
                old_frame
            },
            Some(OneOrMany::Single(f)) => {
                // Single value: all old frames map to this one
                *f
            },
            Some(OneOrMany::Many(new_frames)) => {
                mapped_frame_from_list(new_frames, pos, old_frame)
            },
        }
    }

    fn resolve_frame(
        &self,
        bm: Option<&String>,
        frame: &FrameIndexOrBookmark,
    ) -> FrameId {
        let bm_offset = self.resolve_bookmark(bm);
        match frame {
            FrameIndexOrBookmark::Index(i) => *i + bm_offset,
            FrameIndexOrBookmark::Bookmark(bm) => {
                self.resolve_bookmark(Some(bm))
            },
        }
    }

    fn set_next_frame(&mut self, index: FrameId) {
        self.next_is_loop = false;
        self.next_is_seek = self.timeline.frame != Some(index);
        if index < self.frame_min || index > self.frame_max {
            self.next_frame = None;
        } else {
            self.next_frame = Some(index);
        }
    }

    fn set_auto_next_frame(&mut self, current: FrameId) {
        self.next_is_loop = false;
        self.next_is_seek = false;
        self.next_frame = if self.reversed {
            previous_frame(current, self.frame_min)
        } else {
            next_frame(current, self.frame_max)
        };
        if self.next_frame.is_none() {
            self.apply_clip_end(current);
        }
    }
    fn apply_clip_end(&mut self, current: FrameId) {
        let start = self.loop_start.unwrap_or(if self.reversed { self.frame_max } else { self.frame_min });
        match self.clip_mode {
            Some(ClipMode::Loop) => {
                self.next_frame = Some(start);
                self.next_is_loop = true;
            },
            Some(ClipMode::Hold) => self.next_frame = Some(current),
            Some(ClipMode::Once) | None => {},
        }
    }

    fn configure_clip(&mut self, animation: &SpriteAnimation) -> Option<FrameId> {
        let section = match &self.section {
            Some(name) => animation.sections.get(name),
            None => animation.playback.as_ref(),
        }?;
        section.validate().expect("invalid animation clip section");
        self.frame_min = section.start;
        self.frame_max = section.end;
        self.ticks_per_frame = section.ticks_per_frame;
        self.clip_mode = Some(section.mode);
        self.loop_start = section.loop_start;
        let start = if self.reversed { section.end } else { section.start };
        self.initial_frame = start;
        self.next_frame = Some(start);
        Some(start)
    }

}

fn frame_position_in_list(
    old_list: &OneOrMany<FrameId>,
    old_frame: FrameId,
) -> Option<usize> {
    match old_list {
        OneOrMany::Single(frame) => (old_frame == *frame).then_some(0),
        OneOrMany::Many(frames) => frames.iter().position(|&f| f == old_frame),
    }
}

fn mapped_frame_from_list(
    new_frames: &[FrameId],
    position: usize,
    old_frame: FrameId,
) -> FrameId {
    if new_frames.is_empty() {
        return old_frame;
    }

    new_frames.get(position).copied().unwrap_or(old_frame)
}

fn previous_frame(current: FrameId, frame_min: FrameId) -> Option<FrameId> {
    (current > frame_min).then_some(current - 1)
}

fn next_frame(current: FrameId, frame_max: FrameId) -> Option<FrameId> {
    (current < frame_max).then_some(current + 1)
}

impl ScriptRunIf for SpriteAnimationScriptRunIf {
    type Tracker = SpriteAnimationTracker;
}

impl SpriteAnimationScriptParams {
    fn old_animation_frame_passes(
        &self,
        oldanim_index: Option<FrameId>,
    ) -> bool {
        let Some(oldanim_index) = oldanim_index else {
            return true;
        };

        if self
            .if_oldanim_frame_lt
            .is_some_and(|lt| oldanim_index >= lt)
            || self
                .if_oldanim_frame_le
                .is_some_and(|le| oldanim_index > le)
            || self
                .if_oldanim_frame_gt
                .is_some_and(|gt| oldanim_index <= gt)
            || self
                .if_oldanim_frame_ge
                .is_some_and(|ge| oldanim_index < ge)
        {
            return false;
        }

        if self
            .if_oldanim_frame_was
            .as_ref()
            .is_some_and(|frames| !frame_id_matches(frames, oldanim_index))
        {
            return false;
        }

        !self
            .if_oldanim_frame_was_not
            .as_ref()
            .is_some_and(|frames| frame_id_matches(frames, oldanim_index))
    }

    fn current_frame_passes(
        &self,
        tracker: &SpriteAnimationTracker,
        current_index: FrameId,
    ) -> bool {
        if !self.current_frame_bounds_pass(tracker, current_index) {
            return false;
        }

        if self
            .if_frame_is
            .as_ref()
            .is_some_and(|frames| {
                !resolved_frame_match(
                    frames,
                    tracker,
                    self.frame_bookmark.as_ref(),
                    current_index,
                    ResolvedFrameMatch::All,
                )
            })
        {
            return false;
        }

        !self
            .if_frame_is_not
            .as_ref()
            .is_some_and(|frames| {
                resolved_frame_match(
                    frames,
                    tracker,
                    self.frame_bookmark.as_ref(),
                    current_index,
                    ResolvedFrameMatch::Any,
                )
            })
    }

    fn current_frame_bounds_pass(
        &self,
        tracker: &SpriteAnimationTracker,
        current_index: FrameId,
    ) -> bool {
        let bookmark = self.frame_bookmark.as_ref();
        if self
            .if_frame_lt
            .as_ref()
            .is_some_and(|lt| current_index >= tracker.resolve_frame(bookmark, lt))
            || self
                .if_frame_le
                .as_ref()
                .is_some_and(|le| current_index > tracker.resolve_frame(bookmark, le))
            || self
                .if_frame_gt
                .as_ref()
                .is_some_and(|gt| current_index <= tracker.resolve_frame(bookmark, gt))
            || self
                .if_frame_ge
                .as_ref()
                .is_some_and(|ge| current_index < tracker.resolve_frame(bookmark, ge))
        {
            return false;
        }

        self.if_playing_reversed
            .is_none_or(|reversed| reversed == tracker.reversed)
    }
}

fn frame_id_matches(frames: &OneOrMany<FrameId>, current: FrameId) -> bool {
    match frames {
        OneOrMany::Single(frame) => current == *frame,
        OneOrMany::Many(frames) => frames.contains(&current),
    }
}

fn resolved_frame_match(
    frames: &OneOrMany<FrameIndexOrBookmark>,
    tracker: &SpriteAnimationTracker,
    bookmark: Option<&String>,
    current: FrameId,
    mode: ResolvedFrameMatch,
) -> bool {
    match frames {
        OneOrMany::Single(frame) => {
            current == tracker.resolve_frame(bookmark, frame)
        },
        OneOrMany::Many(frames) => match mode {
            ResolvedFrameMatch::All => frames
                .iter()
                .all(|frame| current == tracker.resolve_frame(bookmark, frame)),
            ResolvedFrameMatch::Any => frames
                .iter()
                .any(|frame| current == tracker.resolve_frame(bookmark, frame)),
        },
    }
}

enum ResolvedFrameMatch {
    All,
    Any,
}

impl ScriptActionParams for SpriteAnimationScriptParams {
    type ShouldRunParam = (SQuery<(&'static Sprite,)>,);
    type Tracker = SpriteAnimationTracker;

    fn should_run(
        &self,
        entity: Entity,
        tracker: &mut Self::Tracker,
        _action_id: ActionId,
        (q_self,): &mut <Self::ShouldRunParam as SystemParam>::Item<'_, '_>,
    ) -> Result<(), ScriptUpdateResult> {
        if !self.old_animation_frame_passes(tracker.carryover.frame) {
            return Err(ScriptUpdateResult::NormalRun);
        }

        let current_index = FrameId::from_sprite_index(
            q_self
                .get(entity)
                .unwrap()
                .0
                .texture_atlas
                .as_ref()
                .unwrap()
                .index,
        );

        if !self.current_frame_passes(tracker, current_index) {
            return Err(ScriptUpdateResult::NormalRun);
        }

        Ok(())
    }
}

impl ScriptAction for SpriteAnimationScriptAction {
    type ActionParams = SpriteAnimationScriptParams;
    type Param = (
        SQuery<(
            &'static mut Sprite,
            &'static mut Transform,
        )>,
        Commands<'static, 'static>,
        SRes<GameTime>,
    );
    type Tracker = SpriteAnimationTracker;

    fn run(
        &self,
        entity: Entity,
        timing: ScriptActionTiming,
        actionparams: &Self::ActionParams,
        tracker: &mut Self::Tracker,
        (q, commands, time): &mut <Self::Param as SystemParam>::Item<'_, '_>,
    ) -> ScriptUpdateResult {
        if matches!(self, Self::TransformTeleport { .. } | Self::TransformMove { .. }) {
            commands.entity(entity).insert(crate::presentation::PresentationJump(time.tick()));
        }
        let (mut sprite, mut xf) = q
            .get_mut(entity)
            .expect("Entity is missing sprite animation components!");
        let atlas = sprite
            .texture_atlas
            .as_mut()
            .expect("Animation entity must have Texture Atlas");

        match self {
            SpriteAnimationScriptAction::SetFrameNext {
                to_frame_bookmark,
                frame_index,
            } => {
                apply_set_frame_next(
                    tracker,
                    actionparams,
                    to_frame_bookmark.as_ref(),
                    frame_index.as_ref(),
                );
                ScriptUpdateResult::NormalRun
            },
            SpriteAnimationScriptAction::SetFrameNow {
                to_frame_bookmark,
                frame_index,
            } => {
                apply_set_frame_now(
                    tracker,
                    actionparams,
                    to_frame_bookmark.as_ref(),
                    frame_index.as_ref(),
                    &mut *atlas,
                    timing,
                );
                ScriptUpdateResult::NormalRun
            },
            SpriteAnimationScriptAction::SetTicksPerFrame {
                ticks_per_frame,
                reset_progress,
            } => {
                apply_ticks_per_frame(
                    tracker,
                    *ticks_per_frame,
                    *reset_progress,
                );
                ScriptUpdateResult::NormalRun
            },
            SpriteAnimationScriptAction::SetSpriteColor { color } => {
                sprite.color = (*color).into();
                ScriptUpdateResult::NormalRun
            },
            SpriteAnimationScriptAction::SetSpriteFlip { flip_x, flip_y } => {
                apply_sprite_flip(&mut sprite, *flip_x, *flip_y);
                ScriptUpdateResult::NormalRun
            },
            SpriteAnimationScriptAction::ReversePlayback { reversed } => {
                apply_reverse_playback(tracker, &*atlas, *reversed);
                ScriptUpdateResult::NormalRun
            },
            transform_action => {
                apply_transform_action(transform_action, &mut xf);
                ScriptUpdateResult::NormalRun
            },
        }
    }
}

fn apply_ticks_per_frame(
    tracker: &mut SpriteAnimationTracker,
    ticks_per_frame: u32,
    reset_progress: Option<bool>,
) {
    let ticks_per_frame = ticks_per_frame.max(1);
    tracker.ticks_per_frame = ticks_per_frame;
    if matches!(reset_progress, Some(true)) {
        tracker.ticks_remain = ticks_per_frame;
        return;
    }

    tracker.ticks_remain = tracker.ticks_remain.min(ticks_per_frame);
}

fn apply_sprite_flip(sprite: &mut Sprite, flip_x: Option<bool>, flip_y: Option<bool>) {
    if let Some(flip_x) = flip_x {
        sprite.flip_x = flip_x;
    }
    if let Some(flip_y) = flip_y {
        sprite.flip_y = flip_y;
    }
}

fn apply_set_frame_next(
    tracker: &mut SpriteAnimationTracker,
    actionparams: &SpriteAnimationScriptParams,
    to_frame_bookmark: Option<&String>,
    frame_index: Option<&OneOrMany<FrameId>>,
) {
    let bm_offset =
        frame_action_bookmark_offset(tracker, actionparams, to_frame_bookmark);
    let target_frame = mapped_target_frame(tracker, actionparams, frame_index);
    tracker.set_next_frame(target_frame + bm_offset);
}

fn apply_set_frame_now(
    tracker: &mut SpriteAnimationTracker,
    actionparams: &SpriteAnimationScriptParams,
    to_frame_bookmark: Option<&String>,
    frame_index: Option<&OneOrMany<FrameId>>,
    atlas: &mut TextureAtlas,
    timing: ScriptActionTiming,
) {
    let bm_offset =
        frame_action_bookmark_offset(tracker, actionparams, to_frame_bookmark);
    let index = mapped_target_frame(tracker, actionparams, frame_index)
        + bm_offset;

    let entered = tracker.timeline.frame != Some(index);
    atlas.index = index.as_sprite_index();
    tracker.timeline.seek(index);
    tracker.set_auto_next_frame(index);
    if tracker.ticks_remain == 0 {
        tracker.ticks_remain = tracker.ticks_per_frame;
    }
    if entered || !tracker.frame_actions_started {
        tracker.frame_actions_started = true;
        queue_frame_action_events(tracker, index, timing);
    }
}

fn frame_action_bookmark_offset(
    tracker: &SpriteAnimationTracker,
    actionparams: &SpriteAnimationScriptParams,
    to_frame_bookmark: Option<&String>,
) -> FrameId {
    tracker.resolve_bookmark(
        to_frame_bookmark.or(actionparams.frame_bookmark.as_ref()),
    )
}

fn mapped_target_frame(
    tracker: &SpriteAnimationTracker,
    actionparams: &SpriteAnimationScriptParams,
    frame_index: Option<&OneOrMany<FrameId>>,
) -> FrameId {
    let default_frame = first_frame_index(frame_index);
    let Some(old_frame) = tracker.carryover.frame else {
        return default_frame;
    };

    tracker.map_frame_index(
        actionparams.if_oldanim_frame_was.as_ref(),
        frame_index,
        old_frame,
        default_frame,
    )
}

fn first_frame_index(frame_index: Option<&OneOrMany<FrameId>>) -> FrameId {
    match frame_index {
        Some(OneOrMany::Single(frame)) => *frame,
        Some(OneOrMany::Many(frames)) if !frames.is_empty() => frames[0],
        _ => FrameId::default(),
    }
}

fn queue_frame_action_events(
    tracker: &mut SpriteAnimationTracker,
    index: FrameId,
    timing: ScriptActionTiming,
) {
    if let Some(actions) = tracker.frame_actions.get(&index) {
        tracker.q_extra.extend(
            actions
                .iter()
                .map(|&action| QueuedAction { timing, action }),
        );
    }
    for (quant, action_id) in &tracker.framequant_actions {
        if quant.check(index.as_sprite_index() as i64) {
            tracker.q_extra.push(QueuedAction {
                timing,
                action: *action_id,
            });
        }
    }
}

fn apply_reverse_playback(
    tracker: &mut SpriteAnimationTracker,
    atlas: &TextureAtlas,
    reversed: Option<bool>,
) {
    let reversed = reversed.unwrap_or(!tracker.reversed);
    match (tracker.reversed, reversed) {
        (false, true) => {
            tracker.reversed = true;
            tracker.set_auto_next_frame(FrameId::from_sprite_index(atlas.index));
        },
        (true, false) => {
            tracker.reversed = false;
            tracker.set_auto_next_frame(FrameId::from_sprite_index(atlas.index));
        },
        _ => {},
    }
}

fn apply_transform_action(
    action: &SpriteAnimationScriptAction,
    xf: &mut Transform,
) {
    match action {
        SpriteAnimationScriptAction::TransformMove { x, y, z } => {
            if let Some(x) = x {
                xf.translation.x += f32::from(*x);
            }
            if let Some(y) = y {
                xf.translation.y += f32::from(*y);
            }
            if let Some(z) = z {
                xf.translation.z += f32::from(*z);
            }
        },
        SpriteAnimationScriptAction::TransformTeleport { x, y, z } => {
            xf.translation.x = f32::from(*x);
            xf.translation.y = f32::from(*y);
            if let Some(z) = z {
                xf.translation.z = f32::from(*z);
            }
        },
        SpriteAnimationScriptAction::TransformSetScale { x, y } => {
            xf.scale.x = f32::from(*x);
            xf.scale.y = f32::from(*y);
        },
        SpriteAnimationScriptAction::TransformRotateDegrees { degrees } => {
            xf.rotate_z(f32::from(*degrees).to_radians());
        },
        SpriteAnimationScriptAction::TransformRotateTurns { turns } => {
            xf.rotate_z(f32::from(*turns) * 2.0 * std::f32::consts::PI);
        },
        SpriteAnimationScriptAction::TransformSetRotationDegrees {
            degrees,
        } => {
            xf.rotation =
                Quat::from_rotation_z(f32::from(*degrees).to_radians());
        },
        SpriteAnimationScriptAction::TransformSetRotationTurns { turns } => {
            xf.rotation = Quat::from_rotation_z(
                f32::from(*turns) * 2.0 * std::f32::consts::PI,
            );
        },
        _ => {},
    }
}

impl ScriptTracker for SpriteAnimationTracker {
    type ActionParams = SpriteAnimationScriptParams;
    type Carryover = SpriteAnimationCarryover;
    type CarryoverParam = (SQuery<&'static Sprite>,);
    type InitParam = (SQuery<&'static mut Sprite>, Option<SRes<AnimationClock>>);
    type RunIf = SpriteAnimationScriptRunIf;
    type Settings = SpriteAnimationSettings;
    type UpdateParam = (
        SRes<GameTime>,
        SQuery<&'static mut Sprite>,
        SResMut<PlaybackEvents>,
    );

    fn init(
        &mut self,
        entity: Entity,
        settings: &Self::Settings,
        metadata: &ScriptMetadata,
        carryover: Self::Carryover,
        (q, clock): &mut <Self::InitParam as SystemParam>::Item<'_, '_>,
    ) {
        self.section.clone_from(&metadata.section);
        self.timeline.id = PlaybackId(metadata.execution);
        self.timeline.key.clone_from(&metadata.key);
        self.carryover = carryover;
        self.ticks_per_frame = settings.ticks_per_frame.max(1);
        self.frame_clock = settings.frame_clock.unwrap_or_else(|| clock.as_ref().map_or(FrameClock::Local, |clock| clock.0));
        self.ticks_remain = 0;
        self.initial_frame = settings.frame_start;
        self.next_frame = Some(settings.frame_start);
        self.frame_min = settings.frame_min;
        self.frame_max = settings.frame_max;
        self.reversed = settings.play_reversed;

        let mut sprite = q
            .get_mut(entity)
            .expect("Animation entity must have Sprite component");
        let atlas = sprite
            .texture_atlas
            .as_mut()
            .expect("Animation entity must have Texture Atlas");
        atlas.index = settings.frame_start.as_sprite_index();
    }

    fn produce_carryover(
        &self,
        entity: Entity,
        (q,): &mut <Self::CarryoverParam as SystemParam>::Item<'_, '_>,
    ) -> Self::Carryover {
        SpriteAnimationCarryover {
            frame: q.get(entity).ok().map(|s| {
                FrameId::from_sprite_index(
                    s.texture_atlas.as_ref().unwrap().index,
                )
            }),
        }
    }

    fn transfer_progress(&mut self, other: &Self) {
        self.ticks_remain = other.ticks_remain.min(self.ticks_per_frame);
    }

    fn finalize(&mut self) {
        // nothing really needs to be done here
    }

    fn track_action(
        &mut self,
        run_if: &Self::RunIf,
        params: &Self::ActionParams,
        action_id: ActionId,
    ) {
        let bm = params.frame_bookmark.as_ref();
        match run_if {
            SpriteAnimationScriptRunIf::Frame(frame) => {
                track_frame_actions(self, bm, frame, action_id);
            },
            SpriteAnimationScriptRunIf::FrameQuant(quant) => {
                // adjust based on bookmark
                let bm_offset = self.resolve_bookmark(bm);
                let mut quant = *quant;
                quant.offset += bm_offset.as_sprite_index() as i64;
                self.framequant_actions.push((quant, action_id));
            },
        }
    }

    fn update(
        &mut self,
        entity: Entity,
        _settings: &Self::Settings,
        (gt, q, _events): &mut <Self::UpdateParam as SystemParam>::Item<'_, '_>,
        queue: &mut Vec<QueuedAction>,
    ) -> ScriptUpdateResult {
        if self.last_tick == Some(gt.tick()) {
            return ScriptUpdateResult::NormalRun;
        }
        align_frame_clock(self, gt.tick());
        self.last_tick = Some(gt.tick());
        let mut sprite = q
            .get_mut(entity)
            .expect("Animation entity must have Sprite component");
        let atlas = sprite.texture_atlas.as_mut().expect("Animation entity must have Texture Atlas");

        let result = if self.ticks_remain == 0 {
            advance_frame(self, gt.tick(), atlas, queue)
        } else {
            ScriptUpdateResult::NormalRun
        };
        if matches!(result, ScriptUpdateResult::Finished) {
            return result;
        }

        self.ticks_remain -= 1;

        ScriptUpdateResult::NormalRun
    }

    fn do_start(
        &mut self,
        _entity: Entity,
        _settings: &Self::Settings,
        _param: &mut <Self::UpdateParam as SystemParam>::Item<'_, '_>,
        _queue: &mut Vec<QueuedAction>,
    ) {
        self.timeline.begin();
    }

    fn do_stop(
        &mut self,
        _entity: Entity,
        _settings: &Self::Settings,
        _param: &mut <Self::UpdateParam as SystemParam>::Item<'_, '_>,
        _queue: &mut Vec<QueuedAction>,
    ) {
        self.timeline.finish(true);
    }

    fn flush_events(
        &mut self,
        entity: Entity,
        param: &mut <Self::UpdateParam as SystemParam>::Item<'_, '_>,
    ) {
        if self.timeline.begun && self.timeline.frame.is_none() {
            self.timeline.enter(self.initial_frame, false);
        }
        self.timeline.flush(entity, &mut param.2);
    }

    fn queue_extra_actions(
        &mut self,
        _settings: &Self::Settings,
        queue: &mut Vec<QueuedAction>,
    ) {
        queue.append(&mut self.q_extra);
    }
}

fn align_frame_clock(tracker: &mut SpriteAnimationTracker, tick: u64) {
    if tracker.frame_clock != FrameClock::Global || !tracker.frame_actions_started { return; }
    let continuous = tracker.last_tick.is_some_and(|previous| previous.saturating_add(1) == tick);
    let boundary = tick.is_multiple_of(u64::from(tracker.ticks_per_frame.max(1)));
    tracker.ticks_remain = u32::from(!(continuous && boundary));
}

fn track_frame_actions(
    tracker: &mut SpriteAnimationTracker,
    bm: Option<&String>,
    frame: &OneOrMany<FrameIndexOrBookmark>,
    action_id: ActionId,
) {
    match frame {
        OneOrMany::Single(frame) => {
            let index = tracker.resolve_frame(bm, frame);
            add_frame_action(&mut tracker.frame_actions, index, action_id);
        },
        OneOrMany::Many(frames) => {
            track_many_frame_actions(tracker, bm, frames, action_id);
        },
    }
}

fn track_many_frame_actions(
    tracker: &mut SpriteAnimationTracker,
    bm: Option<&String>,
    frames: &[FrameIndexOrBookmark],
    action_id: ActionId,
) {
    for frame in frames {
        let index = tracker.resolve_frame(bm, frame);
        add_frame_action(&mut tracker.frame_actions, index, action_id);
    }
}

fn advance_frame(
    tracker: &mut SpriteAnimationTracker,
    tick: u64,
    atlas: &mut TextureAtlas,
    queue: &mut Vec<QueuedAction>,
) -> ScriptUpdateResult {
    let Some(next_frame) = tracker.next_frame else {
        tracker.timeline.finish(false);
        return ScriptUpdateResult::Finished;
    };

    let held = tracker.frame_actions_started && tracker.timeline.frame == Some(next_frame) && !tracker.next_is_loop;
    if tracker.next_is_seek { tracker.timeline.seek(next_frame); }
    else { tracker.timeline.enter(next_frame, tracker.next_is_loop); }
    tracker.ticks_remain = tracker.ticks_per_frame;
    if held { return ScriptUpdateResult::NormalRun; }
    tracker.frame_actions_started = true;
    queue_frame_actions(tracker, next_frame, tick, queue);
    queue_framequant_actions(tracker, next_frame, tick, queue);
    atlas.index = next_frame.as_sprite_index();
    tracker.ticks_remain = tracker.ticks_per_frame;
    tracker.set_auto_next_frame(next_frame);

    ScriptUpdateResult::NormalRun
}

fn queue_frame_actions(
    tracker: &SpriteAnimationTracker,
    next_frame: FrameId,
    tick: u64,
    queue: &mut Vec<QueuedAction>,
) {
    let Some(actions) = tracker.frame_actions.get(&next_frame) else {
        return;
    };

    queue.extend(actions.iter().map(|&action| QueuedAction {
        timing: ScriptActionTiming::Tick(tick),
        action,
    }));
}

fn queue_framequant_actions(
    tracker: &SpriteAnimationTracker,
    next_frame: FrameId,
    tick: u64,
    queue: &mut Vec<QueuedAction>,
) {
    for (quant, action_id) in &tracker.framequant_actions {
        if !quant.check(next_frame.as_sprite_index() as i64) {
            continue;
        }

        queue.push(QueuedAction {
            timing: ScriptActionTiming::Tick(tick),
            action: *action_id,
        });
    }
}

fn add_frame_action(
    actions: &mut HashMap<FrameId, Vec<ActionId>>,
    index: FrameId,
    action_id: ActionId,
) {
    actions.entry(index).or_default().push(action_id);
}

impl ScriptAsset for SpriteAnimation {
    type Action = ExtendedScriptAction<SpriteAnimationScriptAction>;
    type ActionParams = ExtendedScriptParams<SpriteAnimationScriptParams>;
    type BuildParam = (
        SQuery<(&'static mut Sprite,)>,
        SRes<PreloadedAssets>,
    );
    type RunIf = ExtendedScriptRunIf<SpriteAnimationScriptRunIf>;
    type Settings = ExtendedScriptSettings<SpriteAnimationSettings>;
    type Tracker = ExtendedScriptTracker<SpriteAnimationTracker>;

    fn settings(&self) -> Self::Settings {
        self.settings.clone()
    }

    fn build(
        &self,
        mut builder: ScriptRuntimeBuilder<Self>,
        entity: Entity,
        (q_atlas, preloaded): &mut <Self::BuildParam as SystemParam>::Item<
            '_,
            '_,
        >,
    ) -> ScriptRuntimeBuilder<Self> {
        let (mut sprite,) = q_atlas
            .get_mut(entity)
            .expect("Animation entity must have Sprite components");

        let (h_image, h_layout) = self
            .resolve_image_atlas(preloaded, builder.asset_key())
            .expect(
                "Cannot resolve Animation asset's Image and Layout assets.",
            );

        sprite.image = h_image;
        sprite.texture_atlas = Some(TextureAtlas {
            layout: h_layout,
            index: self
                .settings
                .extended
                .frame_start
                .min(self.settings.extended.frame_max)
                .max(self.settings.extended.frame_min)
                .as_sprite_index(),
        });

        builder.replace_config(&self.config);
        builder.tracker_mut().extended.timeline.configure(self);
        if let Some(start) = builder.tracker_mut().extended.configure_clip(self) {
            sprite.texture_atlas.as_mut().expect("sprite atlas").index = start.as_sprite_index();
        }
        builder
            .tracker_mut()
            .extended
            .bookmarks
            .clone_from(&self.frame_bookmarks);
        for action in self.script.iter() {
            builder = builder.add_action(
                &action.run_if,
                &action.action,
                &action.params,
            );
        }

        builder
    }
}

fn update_animation_signals(
    mut commands: Commands,
    mut query: Query<(Entity, Option<&mut PlaybackState>), With<ScriptPlayer<SpriteAnimation>>>,
    mut events: ResMut<PlaybackEvents>,
) {
    for (entity, signals) in &mut query {
        let pending = events.0.remove(&entity).unwrap_or_default();
        if let Some(mut signals) = signals {
            signals.clear_for_tick();
            signals.apply_events(pending);
        } else {
            let mut signals = PlaybackState::default();
            signals.apply_events(pending);
            commands.entity(entity).try_insert(signals);
        }
    }
    events.0.clear();
}

impl PlaybackState {
    pub fn has_cue_for(&self, playback: PlaybackId, cue: CueId) -> bool {
        self.ordered.iter().any(|event| event.playback == playback && matches!(
            event.kind, PlaybackEventKind::Cue(id) if id == cue
        ))
    }

    fn apply_events(&mut self, events: Vec<PlaybackEvent>) {
        for event in &events {
            self.apply_event(event);
        }
        self.ordered = events;
    }

    fn apply_event(&mut self, event: &PlaybackEvent) {
        match &event.kind {
                PlaybackEventKind::Started { key } => {
                    self.playback = event.playback;
                    self.current_key.clone_from(key);
                    self.clear_edges();
                    self.current_frame = None;
                    self.started = true;
                },
                PlaybackEventKind::FrameEntered(frame) => {
                    self.current_frame = Some(*frame);
                    self.frame_entered = Some(*frame);
                },
                PlaybackEventKind::FrameExited(frame) => self.frame_exited = Some(*frame),
                PlaybackEventKind::Cue(_) | PlaybackEventKind::Seeked(_) => {},
                PlaybackEventKind::Looped => self.looped = true,
                PlaybackEventKind::Completed => {
                    self.finished = true;
                    self.current_key = None;
                },
                PlaybackEventKind::Interrupted => {
                    self.interrupted = true;
                    self.current_key = None;
                },
        }
    }
}

fn mirror_animation_signals_to_gent(
    mut commands: Commands,
    owner_query: Query<(Entity, &crate::gent::Gent)>,
    gfx_query: Query<&PlaybackState>,
    mut profiler: TickProfiler,
) {
    let _profile = profiler.scope("animation_signal_mirror");
    for (entity, gent) in owner_query.iter() {
        if let Ok(signals) = gfx_query.get(gent.e_gfx) {
            commands.entity(entity).try_insert(signals.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn despawn_animation_owners(
        mut commands: Commands,
        query: Query<Entity, With<crate::gent::Gent>>,
    ) {
        for entity in &query {
            commands.entity(entity).despawn();
        }
    }

    #[test]
    fn signal_mirroring_ignores_owner_despawned_in_the_same_flush() {
        let mut world = World::new();
        world.insert_resource(GameTime::default());
        let gfx = world.spawn(PlaybackState::default()).id();
        let effects_gfx = world.spawn_empty().id();
        let owner = world
            .spawn(crate::gent::Gent {
                e_gfx: gfx,
                e_effects_gfx: effects_gfx,
            })
            .id();

        let mut schedule = Schedule::default();
        schedule.add_systems(
            (despawn_animation_owners, mirror_animation_signals_to_gent)
                .chain_ignore_deferred(),
        );
        schedule.run(&mut world);

        assert!(world.get_entity(owner).is_err());
    }
}

#[cfg(test)]
mod playback_tests;
