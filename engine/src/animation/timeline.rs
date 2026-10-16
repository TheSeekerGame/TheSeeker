//! Playback events originate in the simulation cursor, independently of rendering.

use bevy::prelude::*;
use std::collections::HashMap;

use super::{cue_id, CueId};
use crate::assets::animation::{FrameId, SpriteAnimation};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct PlaybackId(pub u64);

#[derive(Debug, Clone)]
pub struct PlaybackEvent {
    pub playback: PlaybackId,
    pub kind: PlaybackEventKind,
}

#[derive(Debug, Clone)]
pub enum PlaybackEventKind {
    Started { key: Option<String> },
    FrameEntered(FrameId),
    FrameExited(FrameId),
    Cue(CueId),
    Looped,
    Seeked(FrameId),
    Completed,
    Interrupted,
}

#[derive(Resource, Default)]
pub struct PlaybackEvents(pub HashMap<Entity, Vec<PlaybackEvent>>);

#[derive(Default)]
pub(super) struct PlaybackTimeline {
    pub id: PlaybackId,
    pub key: Option<String>,
    pub frame: Option<FrameId>,
    entered: HashMap<FrameId, Vec<PlaybackEventKind>>,
    exited: HashMap<FrameId, Vec<PlaybackEventKind>>,
    completed: Vec<CueId>,
    pending: Vec<PlaybackEvent>,
    ended: bool,
    pub begun: bool,
}

impl PlaybackTimeline {
    pub fn configure(&mut self, animation: &SpriteAnimation) {
        let mut bookmarks: Vec<_> = animation.frame_bookmarks.iter().collect();
        bookmarks.sort_by_key(|(name, _)| *name);
        for (name, frame) in bookmarks {
            self.entered.entry(*frame).or_default().push(
                PlaybackEventKind::Cue(cue_id(name)),
            );
        }
        for event in &animation.events {
            self.configure_cue(event, animation);
        }
    }

    fn configure_cue(
        &mut self,
        event: &crate::assets::animation::SpriteAnimationEvent,
        animation: &SpriteAnimation,
    ) {
        let id = cue_id(&event.emit);
        let bookmark = event.on.bookmark.as_ref()
            .and_then(|name| animation.frame_bookmarks.get(name).copied());
        if let Some(frame) = event.on.frame_enter.or(bookmark) {
            self.entered.entry(frame).or_default().push(PlaybackEventKind::Cue(id));
        }
        if let Some(frame) = event.on.frame_exit {
            self.exited.entry(frame).or_default().push(PlaybackEventKind::Cue(id));
        }
        if event.on.anim_finished {
            self.completed.push(id);
        }
    }

    pub fn begin(&mut self) {
        if self.begun { return; }
        self.begun = true;
        self.emit(PlaybackEventKind::Started {
            key: self.key.clone(),
        });
    }

    #[cfg(test)]
    pub fn start(&mut self, frame: FrameId) {
        self.begin();
        self.enter(frame, false);
    }

    pub fn enter(&mut self, frame: FrameId, looped: bool) {
        if self.ended || (self.frame == Some(frame) && !looped) {
            return;
        }
        self.exit_frame();
        if looped {
            self.emit(PlaybackEventKind::Looped);
        }
        self.frame = Some(frame);
        self.emit(PlaybackEventKind::FrameEntered(frame));
        if let Some(cues) = self.entered.get(&frame) {
            self.pending.extend(cues.iter().cloned().map(|kind| PlaybackEvent {
                playback: self.id,
                kind,
            }));
        }
    }

    pub fn seek(&mut self, frame: FrameId) {
        self.emit(PlaybackEventKind::Seeked(frame));
        self.enter(frame, false);
    }

    pub fn finish(&mut self, interrupted: bool) {
        if self.ended {
            return;
        }
        self.ended = true;
        self.exit_frame();
        if interrupted {
            self.emit(PlaybackEventKind::Interrupted);
        } else {
            self.emit(PlaybackEventKind::Completed);
            self.pending.extend(self.completed.iter().map(|id| PlaybackEvent {
                playback: self.id,
                kind: PlaybackEventKind::Cue(*id),
            }));
        }
    }

    pub fn flush(&mut self, entity: Entity, events: &mut PlaybackEvents) {
        if !self.pending.is_empty() {
            events.0.entry(entity).or_default().append(&mut self.pending);
        }
    }

    fn exit_frame(&mut self) {
        let Some(frame) = self.frame else { return; };
        self.emit(PlaybackEventKind::FrameExited(frame));
        if let Some(cues) = self.exited.get(&frame) {
            self.pending.extend(cues.iter().cloned().map(|kind| PlaybackEvent {
                playback: self.id,
                kind,
            }));
        }
    }

    fn emit(&mut self, kind: PlaybackEventKind) {
        self.pending.push(PlaybackEvent { playback: self.id, kind });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hold_reverse_seek_and_loop_are_distinct() {
        let mut timeline = PlaybackTimeline::default();
        timeline.start(FrameId(3));
        timeline.enter(FrameId(3), false);
        timeline.enter(FrameId(2), false);
        timeline.seek(FrameId(1));
        timeline.enter(FrameId(1), true);
        let entered: Vec<_> = timeline.pending.iter().filter_map(|event| {
            match event.kind {
                PlaybackEventKind::FrameEntered(frame) => Some(frame.0),
                _ => None,
            }
        }).collect();
        assert_eq!(entered, [3, 2, 1, 1]);
        assert_eq!(timeline.pending.iter().filter(|event| {
            matches!(event.kind, PlaybackEventKind::Looped)
        }).count(), 1);
    }

    #[test]
    fn completion_and_interruption_are_emitted_once() {
        for interrupted in [false, true] {
            let mut timeline = PlaybackTimeline::default();
            timeline.start(FrameId(1));
            timeline.finish(interrupted);
            timeline.finish(true);
            timeline.enter(FrameId(2), false);
            let endings: Vec<_> = timeline.pending.iter().filter(|event| matches!(event.kind, PlaybackEventKind::Completed | PlaybackEventKind::Interrupted)).collect();
            assert_eq!(endings.len(), 1);
            assert_eq!(matches!(endings[0].kind, PlaybackEventKind::Interrupted), interrupted);
        }
    }

    #[test]
    fn repeated_clip_activations_have_distinct_identity() {
        let mut events = PlaybackEvents::default();
        let entity = Entity::from_raw_u32(1).unwrap();
        for execution in [1, 2] {
            let mut timeline = PlaybackTimeline {
                id: PlaybackId(execution),
                key: Some("same_clip".to_owned()),
                ..Default::default()
            };
            timeline.start(FrameId(1));
            timeline.flush(entity, &mut events);
        }
        let started: Vec<_> = events.0[&entity].iter().filter_map(|event| {
            matches!(event.kind, PlaybackEventKind::Started { .. }).then_some(event.playback.0)
        }).collect();
        assert_eq!(started, [1, 2]);
    }
}
