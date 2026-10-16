# Animation Format Reference

An `.anim.toml` asset extends the [common script format](script-reference.md). Its schema is in `engine/src/assets/animation.rs`; playback is in `engine/src/animation.rs` and `engine/src/animation/timeline.rs`. [Animations](animations.md) explains registration and Rust playback.

## File structure

```toml
[asset]
key = "anim.example.Strike"
image = "animations/example/Strike__r1c8__32x32.png"

[settings]
time_base = "Relative"
frame_clock = "Global"
ticks_per_frame = 8
frame_min = 1
frame_max = 8
frame_start = 1

[playback]
start = 1
end = 8
ticks_per_frame = 8
mode = "once"

[frame_bookmarks]
release = 4

[[events]]
on = { bookmark = "release" }
emit = "Attack"

[[script]]
run_at_frame = "release"
action = "PlayAudio"
asset_key = "audio.game.PlayerGetsDamaged"
volume = 0.25
```

This example requires the declared image to exist. `[asset]` is read by the game's startup registration scanner; the runtime reads the animation definition. `[settings]` is required. `[config]`, `[playback]`, `[sections.*]`, `[frame_bookmarks]`, `[[events]]`, `[[frame_impulse]]`, and `[[script]]` are optional.

## Sprite sheet and indexing

The game recursively scans `assets/animations/` for `.anim.toml` files. `[asset]` supplies one `key` or a list of `keys`, plus an `image` path relative to `assets/`. The image name must encode atlas dimensions, such as `Strike__r1c8__32x32.png` for one row, eight columns, and 32×32-pixel cells. Registration creates the animation key and its `.image` and `.atlas` keys.

All authored frame indices are **one-based**; Bevy's atlas index is zero-based. Frames traverse rows left to right, then top to bottom. Frame 1 is the first cell, and zero is invalid. The same atlas can serve several clips. Explicit `image_asset_key` and `atlas_asset_key` settings can select other registered assets.

Keep frame ranges, bookmarks, cues, and seeks within the sheet's capacity. Do not depend on malformed input being rejected uniformly: some checks occur during game definition resolution, while generic playback assumes valid data.

## Settings

| Field | Required/default | Meaning |
| --- | --- | --- |
| `ticks_per_frame` | Required, author a positive integer | Frame period in 96 Hz simulation ticks |
| `frame_min` | Required | Lowest frame in the default playback range |
| `frame_max` | Required | Highest frame in the default playback range |
| `frame_start` | Required | Initial frame when no clip section overrides it |
| `frame_clock` | Inherit application policy | `"Global"` or `"Local"` |
| `play_reversed` | false | Initially decrement rather than increment frame indices |
| `image_asset_key` | `<animation-key>.image` | Registered image key |
| `atlas_asset_key` | `<animation-key>.atlas` | Registered layout key |
| `time_base`, `tick_quant` | Common defaults | [Common-script origin settings](script-reference.md#settings), independent of frame cadence |

Without a clip override or script seek/loop, playback stops after the terminal frame's duration. It leaves the sprite at that frame; lifetime and visibility belong to the caller.

The game's application policy is global; the generic engine default is local. Global playback enters the first frame immediately, then advances on strictly later global boundaries of the current period. With period 8 and start tick 5, frame entries occur at ticks 5, 8, 16, 24, …. The first frame is shortened. Periods 2, 4, 6, 8, and 16 all remain valid; a six-tick cadence is not forced into a power of two.

Local playback counts each frame's duration from its own entry. An individually frozen global animation emits no catch-up entries and waits for the next strictly later boundary after thawing. Holding a frame does not repeat its entry actions. Display refresh and pixel rounding do not alter this simulation cadence.

## Bounded clips and sections

`[playback]` selects a default bounded clip. `[sections.name]` defines an explicitly selected phase; the remaining settings are still required.

```toml
[sections.flight]
start = 13
end = 15
ticks_per_frame = 8
mode = "loop"

[sections.impact]
start = 16
end = 17
ticks_per_frame = 8
mode = "hold"
```

These sections come from the existing `anim.player.DownwardDash` asset. Game code selects a named section through the owning action's presentation binding, which passes it to `ScriptPlayer<SpriteAnimation>::play_snapshot(key, clip, Some(section))`.

| Clip field | Meaning |
| --- | --- |
| `start`, `end` | Inclusive bounds; require positive `start <= end` |
| `ticks_per_frame` | Positive period overriding the base settings for this clip |
| `mode = "once"` | Default: complete after the last frame's duration |
| `mode = "loop"` | Return to `loop_start`, or the first playback frame when omitted |
| `mode = "hold"` | Hold the terminal frame until replaced/stopped; do not complete automatically |
| `loop_start` | Optional return frame within the clip bounds |

A selected clip starts at `start` in forward playback and `end` in reverse playback. An explicit loop start is used in either direction. Base `frame_min`, `frame_max`, `frame_start`, and `ticks_per_frame` are overridden by the selected section. Frame-clock policy is inherited from settings.

## Frame bookmarks

`[frame_bookmarks]` maps names to absolute one-based frames. Each bookmark also emits a cue of the same name when its frame is entered.

A numeric frame with `frame_bookmark = "name"` is relative to that bookmark:

```text
resolved frame = bookmark frame + numeric frame - 1
```

```toml
[frame_bookmarks]
loop_start = 10
loop_end = 19

[[script]]
run_at_frame = "loop_end"
action = "SetFrameNext"
to_frame_bookmark = "loop_start"

[[script]]
run_at_frame = 4
frame_bookmark = "loop_start"
action = "SetSpriteFlip"
flip_x = true
```

The second action runs on absolute frame 13. Numeric target 2 relative to that bookmark would select frame 11. `to_frame_bookmark` overrides the entry's bookmark for `SetFrameNow`/`SetFrameNext`; an omitted target index defaults to 1, meaning the bookmark itself.

A string-valued `run_at_frame` or current-frame condition resolves its named bookmark directly. Bookmark offsets never change `run_at_tick` or time triggers. Missing bookmarks fall back to frame 1 in generic script frame resolution; author valid names and use game definition validation rather than relying on that fallback.

## Named cues

Cues describe frame-owned events for Rust consumers; their `emit` strings are not commands executed by the animation engine.

```toml
[[events]]
on = { frame_enter = 4 }
emit = "Attack"

[[events]]
on = { frame_exit = 6 }
emit = "Recover"

[[events]]
on = { anim_finished = true }
emit = "Finished"
```

`on.bookmark` selects frame entry at a named bookmark. If both `frame_enter` and `bookmark` are supplied, the explicit frame takes precedence. Entry, exit, and completion selectors can coexist in one event definition; prefer one trigger per entry for clarity. Completion cues run on completion, not interruption or an indefinitely held clip.

Playback publishes ordered start, frame-entry, frame-exit, cue, loop, seek, completion, and interruption events tagged with `PlaybackId`. `AnimationSet::Signals` publishes the current tick's events into each animation entity's `PlaybackState` component after script execution. Consumers must match the playback execution they own. A held frame emits no repeated entry cue, while a real loop or seek to another frame creates new entries. Action cancellation and cleanup remain in game code.

## Frame-authored movement

```toml
[[frame_impulse]]
frame = 4
x = 2.0
y = 0.0
mode = "once_on_enter"
```

`frame`, `x`, and `y` are required; `mode` defaults to `"once_on_enter"`, the only supported mode. These are local displacement descriptors for the game's frame-step walking path. They do not move every standalone animation automatically. An actor must select frame-step walking; its resolver can use these entries when explicit walking steps are absent. The owning movement system applies facing, tuning scale, and normal collision policy on actual frame entry.

## Available trigger conditions

Animations support all [common triggers](script-reference.md#available-trigger-conditions), plus:

| Field | Value | Behavior |
| --- | --- | --- |
| `run_at_frame` | Frame, bookmark, or array of either | Run on entry into any listed frame |
| `run_every_n_frames` | Quant string | Match the zero-based frame offset, adjusted by `frame_bookmark` if present |

Use the singular field name `run_at_frame` even for an array:

```toml
[[script]]
run_at_frame = [1, 3, 5]
action = "SetSpriteFlip"
flip_y = false
```

`run_every_n_frames = "8"` matches frames 1, 9, 17, …; `"8+3"` matches frames 4, 12, 20, …. These patterns describe sheet indices, not the number of frames elapsed since playback began. Frame entry occurs in simulation, not every display refresh. Seeks to a different frame trigger its entry actions; repeatedly selecting the current frame does not retrigger them.

## Common parameters

All [common-script parameters](script-reference.md#common-parameters) apply. Animation entries also accept:

| Field | Meaning |
| --- | --- |
| `frame_bookmark` | Base for numeric current-frame conditions, frame triggers, and seek destinations |
| `if_frame_lt`, `if_frame_le`, `if_frame_gt`, `if_frame_ge` | Compare the authoritative current frame to a number or bookmark |
| `if_frame_is` | Equality condition; prefer a single number or bookmark (see limitation below) |
| `if_frame_is_not` | Exclude a number, bookmark, or any entry in a list |
| `if_oldanim_frame_lt`, `if_oldanim_frame_le`, `if_oldanim_frame_gt`, `if_oldanim_frame_ge` | Compare the preceding animation's frame at changeover |
| `if_oldanim_frame_was`, `if_oldanim_frame_was_not` | Match/exclude a literal frame or any literal frame in a list from that preceding animation |
| `if_playing_reversed` | true for reverse-only, false for forward-only; omission accepts either |

The current `if_frame_is` array implementation requires equality to **every** listed value. A list of distinct frames therefore never passes. Use a single value, bounds, or separate entries; do not treat it as an any-of test. `run_at_frame` arrays and `if_frame_is_not` arrays do use any-of matching.

Old-animation frame conditions are absolute numeric indices from another asset; current bookmarks do not offset them. Combine them with `if_previous_script_key` when a particular predecessor is required.

## Available actions

All [common actions](script-reference.md#available-actions) are accepted, plus the following animation actions. The examples below are complete action entries to append to an existing animation definition.

### SetFrameNext

Schedules the next frame at the next cadence advance. Optional `frame_index` defaults to 1; optional `to_frame_bookmark` selects a destination base. Selecting the current frame creates a hold without repeating frame-entry effects.

```toml
[[script]]
run_at_frame = 8
action = "SetFrameNext"
frame_index = 1
```

### SetFrameNow

Seeks during the current simulation update without waiting for the next cadence boundary. Destination fields match `SetFrameNext`. It does not rewind time or replay skipped frames.

```toml
[[script]]
run_on_slot_enable = "Impact"
action = "SetFrameNow"
frame_index = 4
```

Both seek actions accept an array for `frame_index` to map corresponding positions in `if_oldanim_frame_was`. Use equally sized source/destination lists when mapping; a list without a matching old-frame mapping uses its first value. Do not use empty lists or out-of-bounds targets.

### SetTicksPerFrame

Requires `ticks_per_frame`; optional `reset_progress` defaults to false. The runtime clamps the new period to at least 1. False limits remaining progress to the new period; true resets the local remaining count. Global cadence still aligns subsequent advancement to the new period's global boundaries.

```toml
[[script]]
run_at_frame = 4
action = "SetTicksPerFrame"
ticks_per_frame = 4
reset_progress = true
```

### ReversePlayback

Optional `reversed = true` selects backward playback; false selects forward. Omission toggles direction. It recomputes the next frame from the current frame and obeys the clip's end/loop/hold policy.

```toml
[[script]]
run_on_slot_enable = "Reverse"
action = "ReversePlayback"
reversed = true
```

### SetSpriteColor

Requires `color`. Hex strings use sRGB (`"#RRGGBB"` or `"#RRGGBBAA"`); three/four-element numeric arrays are LCH/LCHA, **not RGB/RGBA**. Arrays are passed to Bevy's `Color::lch`/`Color::lcha`; hue is in degrees. Prefer hex for ordinary tint authoring. Invalid hex logs an error and falls back to white.

```toml
[[script]]
run_on_slot_enable = "Flash"
action = "SetSpriteColor"
color = "#FF8080FF"
```

### SetSpriteFlip

Optional `flip_x` and `flip_y` booleans change the corresponding sprite flags; omitted axes keep their current values. This changes artwork orientation, not gameplay facing.

### Transform actions

Transform numeric parameters use **quoted** decimal or fractional strings such as `"1.25"` or `"5/4"`. Use finite values and nonzero denominators.

| Action | Parameters | Effect |
| --- | --- | --- |
| `TransformMove` | Optional `x`, `y`, `z` | Add local translation |
| `TransformTeleport` | Required `x`, `y`; optional `z` | Set local translation |
| `TransformRotateTurns` | Required `turns` | Add rotation; 1 turn = 360 degrees |
| `TransformRotateDegrees` | Required `degrees` | Add rotation in degrees |
| `TransformSetRotationTurns` | Required `turns` | Set rotation |
| `TransformSetRotationDegrees` | Required `degrees` | Set rotation in degrees |
| `TransformSetScale` | Required `x`, `y` | Set horizontal and vertical scale |

```toml
[[script]]
run_at_frame = 3
action = "TransformMove"
x = "5/4"
```

These actions modify the animation entity's transform. They are not an actor movement/collision API; an authoritative `TransformGfxFromGent` binding can overwrite that transform during tick preparation. Use the game motor/frame-step path for collision-resolved movement. Translation actions mark presentation discontinuities so sprite interpolation does not smear them across the previous tick.

## Authoritative playback and delayed display

Only the animation runtime should modify its authoritative atlas index. Colliders and gameplay cues read that simulation pose. Render-only proxies display tick histories, interpolate continuous movement, and independently round pixel-aligned positions. They do not own script execution or gameplay lifetime. [Timing, Audio, and Animation](../architecture/timing-and-audio.md) explains the shared audio/presentation cursor and the limits imposed by physical displays.
