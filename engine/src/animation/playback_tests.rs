use super::*;
use bevy::ecs::system::SystemState;

struct Fixture {
    world: World,
    entity: Entity,
    tracker: SpriteAnimationTracker,
    settings: SpriteAnimationSettings,
}

impl Fixture {
    fn new(start: u32, end: u32, mode: ClipMode, reversed: bool) -> Self {
        let mut world = World::new();
        world.insert_resource(GameTime::default());
        world.insert_resource(PlaybackEvents::default());
        world.add_schedule(Schedule::new(crate::time::GameTickPre));
        world.add_schedule(Schedule::new(crate::time::GameTickUpdate));
        world.add_schedule(Schedule::new(crate::time::GameTickPost));
        let entity = world.spawn(Sprite { texture_atlas: Some(TextureAtlas { layout: Handle::default(), index: 0 }), ..Sprite::default() }).id();
        let mut tracker = SpriteAnimationTracker {
            frame_min: FrameId(start), frame_max: FrameId(end), ticks_per_frame: 2,
            next_frame: Some(FrameId(if reversed { end } else { start })), reversed,
            clip_mode: Some(mode), ..SpriteAnimationTracker::default()
        };
        tracker.timeline.id = PlaybackId(1);
        tracker.timeline.begin();
        let clip: SpriteAnimation = toml::from_str(include_str!("../../../assets/animations/player/skills/DownwardDash.anim.toml")).unwrap();
        Self { world, entity, tracker, settings: clip.settings.extended }
    }

    fn step(&mut self, frozen: bool) -> Vec<PlaybackEvent> {
        if !frozen { self.update(); }
        let mut events = self.world.resource_mut::<PlaybackEvents>();
        self.tracker.timeline.flush(self.entity, &mut events);
        let events = events.0.remove(&self.entity).unwrap_or_default();
        let clock = crate::time::MasterClock { tick: self.world.resource::<GameTime>().tick() + 1, ..Default::default() };
        self.world.resource_mut::<GameTime>().update_from_master(&clock);
        crate::time::run_gametickupdate_schedule(&mut self.world);
        events
    }

    fn update(&mut self) {
        let mut state = SystemState::<<SpriteAnimationTracker as ScriptTracker>::UpdateParam>::new(&mut self.world);
        self.tracker.update(self.entity, &self.settings, &mut state.get_mut(&mut self.world).expect("fixture system parameters are valid"), &mut Vec::new());
        state.apply(&mut self.world);
    }
}

fn frames(events: &[PlaybackEvent]) -> Vec<u32> {
    events.iter().filter_map(|event| match event.kind { PlaybackEventKind::FrameEntered(frame) => Some(frame.0), _ => None }).collect()
}

#[test]
fn global_cadence_enters_immediately_then_joins_every_authored_period() {
    for period in [2, 4, 6, 8, 16] {
        for phase in 0..period {
            assert_global_phase(period, phase);
        }
    }
}

fn assert_global_phase(period: u32, phase: u32) {
    let mut fixture = Fixture::new(1, 20, ClipMode::Loop, false);
    for _ in 0..phase { fixture.step(true); }
    fixture.tracker.frame_clock = FrameClock::Global;
    fixture.tracker.ticks_per_frame = period;
    assert_eq!(frames(&fixture.step(false)), [1]);
    for tick in phase + 1..period * 3 {
        let entered = frames(&fixture.step(false));
        assert_eq!(!entered.is_empty(), tick.is_multiple_of(period), "period {period}, start {phase}, tick {tick}");
    }
}

#[test]
fn global_freeze_never_catches_up_and_thaw_waits_for_a_later_boundary() {
    let mut fixture = Fixture::new(1, 20, ClipMode::Loop, false);
    fixture.tracker.frame_clock = FrameClock::Global;
    fixture.tracker.ticks_per_frame = 8;
    assert_eq!(frames(&fixture.step(false)), [1]);
    for _ in 1..16 { fixture.step(true); }
    assert!(frames(&fixture.step(false)).is_empty());
    for _ in 17..24 { assert!(frames(&fixture.step(false)).is_empty()); }
    assert_eq!(frames(&fixture.step(false)), [2]);
    fixture.update();
    fixture.update();
    assert_eq!(fixture.tracker.last_tick, Some(25));
}

#[test]
fn real_cursor_single_frame_loop_and_hold_have_distinct_entries() {
    let mut hold = Fixture::new(1, 1, ClipMode::Hold, false);
    let mut looping = Fixture::new(1, 1, ClipMode::Loop, false);
    let held: Vec<_> = (0..7).flat_map(|_| hold.step(false)).collect();
    let looped: Vec<_> = (0..7).flat_map(|_| looping.step(false)).collect();
    assert_eq!(frames(&held), [1]);
    assert_eq!(frames(&looped), [1, 1, 1, 1]);
    assert_eq!(looped.iter().filter(|event| matches!(event.kind, PlaybackEventKind::Looped)).count(), 3);
}

#[test]
fn real_cursor_reverse_completes_once_after_final_frame_duration() {
    let mut fixture = Fixture::new(1, 3, ClipMode::Once, true);
    let events: Vec<_> = (0..9).flat_map(|_| fixture.step(false)).collect();
    assert_eq!(frames(&events), [3, 2, 1]);
    assert_eq!(events.iter().filter(|event| matches!(event.kind, PlaybackEventKind::Completed)).count(), 1);
    assert!(!events.iter().any(|event| matches!(event.kind, PlaybackEventKind::Looped)));
}

#[test]
fn freeze_resume_and_repeated_update_do_not_duplicate_entries() {
    let mut fixture = Fixture::new(1, 3, ClipMode::Once, false);
    fixture.update();
    let remaining = fixture.tracker.ticks_remain;
    fixture.update();
    assert_eq!(fixture.tracker.ticks_remain, remaining);
    assert_eq!(frames(&fixture.step(false)), [1]);
    for _ in 0..12 { assert!(fixture.step(true).is_empty()); }
    assert!(frames(&fixture.step(false)).is_empty());
    assert_eq!(frames(&fixture.step(false)), [2]);
}

#[test]
fn completion_from_previous_execution_does_not_finish_started_clip() {
    let mut state = PlaybackState::default();
    state.apply_events(vec![
        PlaybackEvent { playback: PlaybackId(1), kind: PlaybackEventKind::Completed },
        PlaybackEvent { playback: PlaybackId(2), kind: PlaybackEventKind::Started { key: Some("same".to_owned()) } },
        PlaybackEvent { playback: PlaybackId(2), kind: PlaybackEventKind::FrameEntered(FrameId(1)) },
    ]);
    assert_eq!(state.playback, PlaybackId(2));
    assert!(state.started);
    assert!(!state.finished);
}

#[test]
fn scheduled_seek_is_distinct_from_loop_and_same_frame_request_holds() {
    let mut fixture = Fixture::new(1, 3, ClipMode::Once, false);
    for _ in 0..5 { fixture.step(false); }
    fixture.tracker.set_next_frame(FrameId(1));
    fixture.step(false);
    let sought = fixture.step(false);
    assert_eq!(frames(&sought), [1]);
    assert!(sought.iter().any(|event| matches!(event.kind, PlaybackEventKind::Seeked(FrameId(1)))));
    assert!(!sought.iter().any(|event| matches!(event.kind, PlaybackEventKind::Looped)));
    fixture.tracker.set_next_frame(FrameId(1));
    let held: Vec<_> = (0..10).flat_map(|_| fixture.step(false)).collect();
    assert!(held.is_empty());
}

#[test]
fn commit_enters_the_initial_frame_without_advancing_the_tick_cursor() {
    let mut fixture = Fixture::new(2, 4, ClipMode::Once, false);
    fixture.tracker.initial_frame = FrameId(2);
    let mut state = SystemState::<<SpriteAnimationTracker as ScriptTracker>::UpdateParam>::new(&mut fixture.world);
    fixture.tracker.flush_events(fixture.entity, &mut state.get_mut(&mut fixture.world).expect("fixture system parameters are valid"));
    let events = &fixture.world.resource::<PlaybackEvents>().0[&fixture.entity];
    assert_eq!(frames(events), [2]);
    assert_eq!(fixture.tracker.ticks_remain, 0);
    assert_eq!(fixture.tracker.last_tick, None);
}

#[test]
fn immediate_seek_to_held_frame_does_not_repeat_frame_actions() {
    let mut fixture = Fixture::new(1, 3, ClipMode::Once, false);
    fixture.step(false);
    fixture.tracker.frame_actions.insert(FrameId(1), vec![123]);
    let params: SpriteAnimationScriptParams = toml::from_str("").unwrap();
    let mut atlas = TextureAtlas::default();
    apply_set_frame_now(&mut fixture.tracker, &params, None,
        Some(&OneOrMany::Single(FrameId(1))), &mut atlas, ScriptActionTiming::UnknownTick);
    assert!(fixture.tracker.q_extra.is_empty());
    apply_set_frame_now(&mut fixture.tracker, &params, None,
        Some(&OneOrMany::Single(FrameId(2))), &mut atlas, ScriptActionTiming::UnknownTick);
    apply_set_frame_now(&mut fixture.tracker, &params, None,
        Some(&OneOrMany::Single(FrameId(1))), &mut atlas, ScriptActionTiming::UnknownTick);
    assert_eq!(fixture.tracker.q_extra.len(), 1);
    assert_eq!(fixture.tracker.q_extra[0].action, 123);
}
