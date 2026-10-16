# Timing, audio, and animation

The goal is one musical timeline shared by sound, gameplay, and presentation. Simulation produces timestamped events ahead of playback; the audio mixer places sounds at those sample positions, and rendering presents the corresponding historical poses. Audio buffers and display refreshes consume this timeline at their own rates.

This document describes the current implementation and the contracts to preserve when extending it. [Actors and actions](actors.md) explains action decisions and execution ownership; [the animation format reference](../authoring/animation-reference.md) covers asset authoring.

## Separate the responsibilities

| Responsibility | Owns |
| --- | --- |
| Musical grid | Tick frequency, beat divisions, and absolute tick-to-sample conversion |
| Device backend | Output negotiation, continuously advancing sample cursor, and callback timestamps |
| Game transport | Pausable musical position, scheduling lead, and recovery epochs |
| Simulation | Input, movement, collisions, action execution, and authoritative animation events |
| Audio mixer | Timestamped voices, loops, gain envelopes, and bus mixing |
| Music controller | Track selection, preparation, beat/bar transitions, and soundtrack ownership |
| Presentation | Historical sprite/camera poses, display-time selection, and pixel alignment |

The game uses Bevy 0.19.1 with a custom CPAL output mixer. Bevy registers and loads `AudioSource` assets, but its output `AudioPlugin` is disabled. Symphonium decodes and resamples audio for the custom mixer. Rodio remains a transitive asset-support dependency; it does not play the game's sounds. Engine code owns generic timing and playback facilities. Game code owns music selection, actor behavior, presentation policy, and controls.

## Define musical relationships once

The musical grid (`engine/src/audio/transport.rs`) defines these relationships:

| Quantity | Frequency | Ticks | Sample frames at 48 kHz |
| --- | ---: | ---: | ---: |
| Fundamental | 48 Hz | 2 | 1,000 |
| Simulation | 96 Hz | 1 | 500 |
| Standard animation | 12 Hz | 8 | 4,000 |
| Beat | 90 BPM / 1.5 Hz | 64 | 32,000 |
| Four-beat bar | — | 256 | 128,000 |

A sample frame includes all channels. Scheduling uses `round(tick × sample_rate / tick_rate)` with wide integer arithmetic and a transport epoch. It never accumulates a rounded tick length. At 44.1 kHz, tick intervals alternate between 459 and 460 frames; absolute boundaries stay within half a sample frame indefinitely within the supported integer range.

## Keep device, simulation, and presentation time distinct

| Position | Meaning | Main types |
| --- | --- | --- |
| Device cursor | Sample frames supplied continuously, including silence during pause | `AudioClock`, callback snapshot |
| Game transport | Musical position mapped onto the device timeline; can pause or begin a new epoch | `GameTransport`, `MasterClock` |
| Simulation cursor | Next tick to execute and the target admitted by the master clock | `GameTime`, `ClockTime` |
| Presentation cursor | Musical moment selected for the expected display time | `RenderInterpolation`, `PresentationTime` |

A transport epoch identifies one mapping between device frames and musical time. Resetting that mapping invalidates commands from the previous epoch. Ordinary tick advancement and lead adjustment keep the same epoch.

`GameTickPre → GameTickUpdate → GameTickPost` executes every admitted simulation tick. Simulation runs ahead of playback so its commands can reach the mixer before their deadlines. The initial lead is four ticks (41.67 ms). Lead grows to cover sustained observed frame intervals, output latency, buffer duration, and a safety tick. It is rounded up to whole ticks and does not shrink during playback. Increasing lead only admits more future simulation ticks: it preserves the audio epoch, active voices, and every scheduled sample timestamp. A late-play diagnostic alone never resets the transport.

The callback publishes coherent bounded snapshots: device position, host playback estimate, mapping, epoch, callback size, latency, and diagnostics. A full snapshot channel evicts an obsolete snapshot. Timestamp regression, implausible latency, and discontinuities fall back to callback arrival plus an estimated buffer of latency. Host-time estimation affects presentation and simulation admission, never scheduled sample positions.

Frame-pacing calibration uses the 90th percentile of a complete 32-sample window. Intervals of 100 ms or more are loading/stall samples and do not enter this estimate. A few irregular startup frames cannot set the persistent lead before the window fills; longer sustained slow rendering can increase it without restarting playback.

## Pause and recover explicitly

Short rendering stalls execute the missing simulation ticks. A severe backlog starts an explicit epoch at the next unexecuted tick, silences stale voices, and restores soundtrack phase. It does not increment the gameplay tick past unexecuted work. The host estimate holds while awaiting the callback's epoch acknowledgment. Missing devices use monotonic silent time; device retries create a fresh epoch. `SEEKER_SILENT_AUDIO=1` intentionally disables device access.

`PauseRequests` combines manual P and modal menu, inventory, and capsule-panel ownership before frame clock admission, and derives `GameState` and `GameTransport::paused` together. Closing a modal preserves an independent manual pause; the menu's Resume action clears it. Simulation stops admitting ticks, and the mixer freezes music and SFX playheads at the scheduled boundary while continuing to supply silent device buffers. Resume preserves those playheads and replenishes scheduling lead. Menu/loading UI uses real time; gameplay presentation uses `PresentationTime`. Modal input flushes both gameplay action banks and buffered edges, so resume does not replay menu presses.

## Follow one event from gameplay to output

For a globally clocked eight-tick animation starting at tick 5, the first frame enters at tick 5 and the second at tick 8. If the second frame triggers an SFX, its onset is transport sample frame 4,000 at 48 kHz.

1. `GameTickPre → GameTickUpdate → GameTickPost` executes tick 8. Script requests commit in `ScriptSet::Init`; playback advances in `ScriptSet::Run` and publishes ordered events. Cues, collision poses, and frame-driven movement use this authoritative entry.
2. The script schedules the sound at tick 8. The engine converts that absolute timestamp into a sample position and flushes commands after each simulation tick.
3. `GameTickPost` prepares actor and attachment transforms, then captures the tick-stamped sprite and camera poses.
4. The audio callback applies the sound at sample frame 4,000, even if that position lies in the middle of its buffer.
5. Rendering selects the historical pose for its estimated display time. The second sprite frame becomes eligible when that cursor reaches tick 8.

The device epoch determines when sample frame 4,000 is heard in wall time. Scheduling lead gives simulation time to produce the event before then. It is not an extra delay added independently to each sound. When rendering catches up through several ticks, every simulation entry still executes even if the display cannot show every intermediate pose.

## Prepare assets outside the audio callback

- The output preference is 48 kHz stereo and 256 frames. Device negotiation can select another supported rate, channel count, sample format, or buffer size.
- Bevy `AudioSource` bytes are decoded and resampled on the async compute pool. SFX are prepared before the loading screen completes. Music is prepared on demand; only current, pending, and fading tracks remain decoded.
- Commands carry an absolute transport frame, epoch, and stable ordering number. Plays, stops, loops, voice gain ramps, and bus gains are applied inside callbacks at their offsets.
- Late one-shots advance into their remaining tails, or finish silently if their whole duration has passed. Their original onset is never shifted to a buffer boundary.
- SFX and background voices have separate fixed pools. SFX pressure steals the oldest SFX; it cannot steal music. Background exhaustion is reported, and the music controller can retry.
- Callback queues, scheduled storage, mixing scratch space, and voice slots are bounded. Decode, file access, logging, and heap allocation remain on other threads. Sample and completion pins outlive callback ownership, including stream shutdown, so the callback does not destroy their final heap allocations.
- Critical controls have a separate queue. Main-thread pressure or exhaustion of callback control storage requests explicit resynchronization instead of silently abandoning a stop or pause. Completion markers do not depend on an overflow-prone completion queue.

`AudioBus::Music`, `Ambience`, and `Sfx` are independent. Actor scripts retain label/instance ownership. The yak's loop uses the `Ambience` label and cannot stop the soundtrack.

## Give animation an explicit cadence

The engine default frame clock is local; the game installs `AnimationClock(FrameClock::Global)`. Each asset can override it with `[settings] frame_clock = "Local"` or `"Global"`.

A global animation enters its first frame immediately. Later entries occur on strictly later multiples of its current frame period. An eight-tick animation starting at tick 5 enters its second frame at tick 8, then at 16, 24, and so on. Periods such as 2, 4, 6, and 16 remain valid. An individually frozen animation emits no catch-up entries on thaw; global playback waits for the next strictly later boundary. Holds, seeks, reversal, action ownership, and once-per-tick advancement remain authoritative simulation operations. Common-script `tick_quant` retains its script-origin meaning.

Local cadence counts ticks from that playback's own frame entries. Global cadence aligns subsequent entries to the shared grid and can shorten the initial frame. Selecting a cadence does not change execution ownership. Common-script origin quantization and animation-frame cadence remain separate settings.

Simulation sprites are the collision contract. `AnimationCollider` continues to reference the authoritative entity. Frame cues, displacement, collider selection, and audio commands use that tick's frame entry. Render-only proxies never carry actor ownership or colliders.

## Present historical poses without extending gameplay ownership

Sprite presentation (`engine/src/presentation.rs`) keeps up to 64 samples per tracked sprite. Animation/Gent sprites and their sprite children are tracked automatically; other tick-driven sprites use `PresentationSource`. Each history stores the full sprite pose, visibility, layers, transform, and camera-grid alignment policy. A proxy appears at its birth tick and survives logical despawn until its death tick is presented. App-state exit clears the proxies explicitly.

## Keep interpolation and pixel alignment independent

Continuous transforms interpolate between bracketing samples. Sprite atlas frames and visibility step at their recorded boundary. `PresentationJump(tick)` suppresses interpolation across teleports and frame-step displacement. `PresentationPolicy` exposes independent `pixel_aligned` and `stepped` state overrides. Player graphics remain pixel aligned by default in all existing states. `PresentationAnchor` gives equipment the same fractional-to-integer anchor correction as its player without changing gameplay parenting.

Interpolation is useful even for a pixel-aligned player: selecting the position between two simulation samples before rounding gives the display the nearest pixel at the requested time. Rounding still produces deliberate pixel steps. Floating positions can be selected by a state policy without changing simulation or audio timing. Disabling interpolation holds the earlier sample until the next boundary; it does not change the animation cadence.

Camera position and shake use the same presentation cursor. Particle emitter descriptors remain game owned while delayed Hanabi instances consume `PresentationTime`; equipment and attached sprites use historical poses. `PresentationAlignment` is captured with each sprite pose and copied onto its proxy before game-owned pixel-view alignment. Damage sparks use `UprightCameraPixels`, retaining camera alignment through logical despawn without recovering markers from a dead source. Foliage shader animation and gameplay visual timers consume presentation time. Display refresh is independent of the musical grid: `AutoVsync`, pipelined rendering, and a requested maximum frame latency of two remain enabled. Two permits more CPU/GPU overlap; one remains a comparison option for lower display latency. The backend treats this setting as a hint, not a measured display delay.

## Give music independent ownership

The music controller (`game/src/music.rs`) owns the soundtrack independently of animation scripts. Entering gameplay requests track `main` on the next beat. Track metadata (`assets/audio/music/tracks.music.toml`) maps this to the existing `audio.game.Music` / Music3 binding.

The editable metadata includes source sample rate, first downbeat, loop start/end, gain, and beats per bar. The default starts at source frame zero, loops `[0, 15_360_000)` at 48 kHz (480 beats), and uses gain 0.04. The additional 1,920 silent frames in the source are outside the loop; the file is unchanged. A nonzero first downbeat starts playback at that source offset, skipping an intro.

Game systems use `MessageWriter<MusicCommand>` to submit these commands (`commands` below is that message writer):

```rust
commands.write(MusicCommand::Play {
    track: "main".into(), boundary: MusicBoundary::NextBeat,
});
commands.write(MusicCommand::Transition {
    track: "another_track".into(), boundary: MusicBoundary::NextBar,
    fade_bars: 1,
});
commands.write(MusicCommand::SetVolume(0.8));
commands.write(MusicCommand::Stop { boundary: MusicBoundary::NextBeat });
```

`NextBeat` and `NextBar` select strictly later boundaries. `Immediate` selects the current admitted simulation target, which can still be ahead of audible playback. Beat/bar boundaries belong to the global grid. Transitions crossfade for one bar by default; `fade_bars` is configurable. A track that is not prepared leaves the current track playing and chooses the next eligible boundary once ready. Music volume changes the music bus, including both sides of a crossfade, without disturbing voice envelopes. The default `Play` also fades in over one bar. Stop uses a short 10 ms fade at its selected boundary. Reconnection schedules a restart on the next beat and seeks to the track's elapsed transport phase; recovery need not be acoustically gapless.

## Measure scheduling and display behavior separately

```sh
SEEKER_TIMING_PROBE=1 SEEKER_TIME_DIAGNOSTICS=1 cargo run
```

- F8: compare requested maximum queued frames of one and two, including while the panel is hidden. Two is the default. Use the recorded `frame_latency` and FPS to compare before recalibrating AV offset. macOS media-key mode requires Fn+F8.
- F9: show/hide the panel.
- F10: cycle test clicks off, callback-generated reference impulses, and impulses submitted by normal game ticks. Music and ordinary SFX keep playing.
- F11: compare pixel-aligned and floating player positions.
- F12: compare interpolated and held movement.
- `[` / `]`: change AV offset by 1 ms.
- P: pause/resume the transport.

F10–F12 and AV-offset controls require the panel to be visible. F8 works with it hidden; P follows the game pause controls. macOS media-key mode may require Fn with any function key.

`target/timing-diagnostics.log` is recreated at startup and records one row per second, plus pause/resynchronization changes. It includes device health, late plays/controls, callback gaps/cost, history usage, music selection/volume, and presentation settings. `SEEKER_TIMING_LOG` overrides its path. Logging stops after 86,400 rows to bound disk usage. File writes run on the main thread, never in the audio callback. Rows also aggregate FPS, mean/worst frame intervals, main-world work, and simulation work across the entire interval; `frame_ms` alone is only the latest frame. Main-world measurements run from `First` to `Last` and exclude render-subapp work and subsequent frame-pacing waits. The probe text refreshes on 12 Hz frame changes rather than every display update.

Render-thread aggregates separate preparation (including acquiring the next surface texture) from submission/presentation/cleanup. These wall times include driver waits; they do not measure GPU execution time. Render and main-world work overlap and must not be summed as CPU utilization. Rows include window physical resolution and the number of focused frames; Bevy throttles unfocused windows, so compare focused intervals in the same scene and exclude loading hitches.

The panel alternates at 12 Hz and shows tick/frame/beat/bar numbers. It reports output settings, scheduling lead, estimated output latency, late plays versus controls, maximum lateness, queue pressure, stolen voices, callback cost, callback gaps, timestamp fallbacks, history use, and resynchronizations. Callback gaps are a portable underrun symptom, not proof that a particular backend dropped samples.

`SEEKER_DISPLAY_LATENCY_MS` defaults to 16.667 and estimates extraction-to-display latency. `SEEKER_AV_OFFSET_MS` defaults to zero; a positive offset selects a later visual moment to compensate for a slower display. Neither setting changes audio sample timestamps. Calibration cannot identify speaker, Bluetooth, display processing, or scanout delay from software timestamps alone.

Offline tests establish sample placement, rational conversion, pause phase, ordering, cancellation, crossfades, exhaustion, recovery, animation cadence, history lifetime, and presentation sampling at 59.94/60/75/120/144 Hz and variable intervals. Hardware observation is still needed for physical AV alignment. A 12 Hz frame can span different numbers of refreshes, especially at 59.94 Hz: eight clicks per logical animation frame does not imply eight clicks in every physical screen hold.

## Extend the system at its owning layer

Use `GameTime` and fixed-tick schedules for gameplay. Use `PresentationTime` for gameplay visual time, and `Time<Real>` for menu/loading UI. Rendering may read history but must not emit gameplay cues or decide damage from the displayed sprite.

Add rhythm constants and conversions to the shared grid. Schedule sounds through `AudioEngine` or the existing script actions; do not add another output stream or a render-frame timer for rhythmic events. Keep stop labels scoped to their bus and execution. Add soundtrack choices to the music library and controller rather than attaching them to an actor animation.

Capture newly introduced tick-driven visuals through `PresentationSource` or the appropriate particle presentation path. Preserve birth, death, visibility, and discontinuity boundaries. Mark sprite teleports and frame-step movement with `PresentationJump`; do not assume a component used by one presentation path automatically applies to every other path. A displayed effect may outlive its logical source, but cannot keep that source's gameplay ownership alive.

Keep callback work bounded and retain shared buffers until their last callback reference has been released. Prefer removing measured redundant work over changing cadence, reducing simulation frequency, or sacrificing event ordering. Sprite proxies update render layers directly and avoid unchanged transform/visibility writes.

The shared `PixelView` derives the presented camera position, physical viewport, integer gameplay scale, raster center, and rounded residual once per render frame. Native pixel meshes use its camera compensation; sprite overlays use camera-relative alignment. The low-resolution pass still rasterizes complete shapes. Named render bands retain their composition order; different bands cannot arbitrarily interleave by object Z after compositing.

## Find the implementation

| Concern | Entry points |
| --- | --- |
| Grid and sample conversion | `engine/src/audio/transport.rs` |
| Master clock, admission, pause, and recovery | `engine/src/time.rs` |
| Assets, voice ownership, preparation, and command submission | `engine/src/audio/custom.rs` |
| Callback negotiation and device timing | `engine/src/audio/device.rs` |
| Sample mixing, queues, and voice envelopes | `engine/src/audio/mixer.rs` |
| Script execution and audio actions | `engine/src/script.rs`, `engine/src/script/common.rs` |
| Animation cadence and ordered playback events | `engine/src/animation.rs`, `engine/src/animation/timeline.rs` |
| Sprite histories and render proxies | `engine/src/presentation.rs` |
| Camera and particle presentation | `game/src/camera.rs`, `game/src/camera/framing.rs`, `game/src/graphics/pixel_view.rs`, `game/src/graphics/particle_presentation.rs` |
| Music commands and track metadata | `game/src/music.rs`, `assets/audio/music/tracks.music.toml` |
| Probe, logs, and frame measurements | `game/src/timing_probe.rs`, `game/src/frame_metrics.rs` |

Focused tests cover rational conversion, mixer partitioning, pause phase, ordering, cancellation, recovery, animation cadence, and presentation lifetime. Verify changes with the checks in [AGENTS.md](https://github.com/TheSeekerGame/TheSeeker/blob/main/AGENTS.md). Software tests establish logical rhythm and sample placement within sample-rate resolution. User-controlled playtests establish responsiveness and visual behavior on the tested hardware; physical speaker/display alignment requires observation or recording.
