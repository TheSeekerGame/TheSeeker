# Script Format Reference

Common scripts are `.script.toml` assets. Their schema is defined in `engine/src/assets/script.rs`; execution lives in `engine/src/script.rs` and `engine/src/script/common.rs`. [Scripts](scripts.md) explains registration and playback. [Animations](animation-reference.md) inherit these settings, triggers, and actions.

## File structure

`[config]`, `[settings]`, and the list of `[[script]]` entries are optional for a common script. Every action entry needs exactly one trigger, an `action` name, and that action's required fields. Names and enum values are case-sensitive.

```toml
[config]
threshold = 7.0
count = 3
variant = "quiet"

[settings]
time_base = "Relative"

[[script]]
run_on_playback_control = "Start"
action = "SlotEnable"
slot = "Ready"

[[script]]
run_every_n_ticks = "64"
require_slots_all = ["Ready"]
action = "PlayAudio"
asset_key = "audio.game.PlayerGetsDamaged"
volume = 0.25
label = "BeatTest"

[[script]]
run_on_playback_control = "Stop"
action = "StopAudio"
label = "BeatTest"
```

Config values may be integers, floats, or strings. Booleans, arrays, and nested config tables are not supported by `DynamicConfigValue`. Rust code must match the corresponding `DynamicConfigValue` variant; `7` and `7.0` are different types.

## Settings

| Field | Default | Meaning |
| --- | --- | --- |
| `time_base` | `"Relative"` | Origin for nonperiodic tick/time actions: playback initialization |
| `time_base = "Level"` | — | Origin from `LevelLoadTime`, when supplied by the game |
| `time_base = "Startup"` | — | Origin at simulation time/tick zero |
| `tick_quant` | Unset | Quantizes the selected start **tick**, independently of the time origin and animation cadence |

Entity `TimeBase` and `ScriptTickQuant` components override the corresponding asset settings. All time origins use game simulation time, which pauses with the transport, rather than wall time.

Quant strings have the form `"N"`, `"N+offset"`, or `"N-offset"`, with positive `N`. A pattern matches when `(value - offset) % N == 0`. For start-tick quantization the implementation uses `value - (value - offset) % N`; for example, start tick 13 with `tick_quant = "8+4"` selects origin 12. It does not defer the entire script until a future tick: start actions and animation initialization still run when playback starts. Keep offsets within the intended origin range; the parser does not comprehensively validate unsafe quant values such as zero periods.

`tick_quant` does not adjust the seconds origin and does not control `run_every_n_ticks`. Leave it unset unless that distinction is intentional. Use animation `frame_clock` for global/local frame cadence.

## Available trigger conditions

| Field | Example value | Runtime behavior |
| --- | --- | --- |
| `run_on_playback_control` | `"Start"` or `"Stop"` | Initialization or normal playback shutdown/changeover |
| `run_on_slot_enable` | `"Attack"` | A slot changes from false to true |
| `run_on_slot_disable` | `"Attack"` | A slot changes from true to false |
| `run_at_tick` | `96` or `[12, 16, 18]` | Nonperiodic offsets from the selected start tick |
| `run_every_n_ticks` | `"64"` or `"12+8"` | Periodic matches against the **absolute game tick** |
| `run_at_millis` | `250` or `[100, 200, 300]` | Nonperiodic offsets in milliseconds from the selected time origin |
| `run_at_time` | `"1:13:17.315"` or `["1:00", "2:00"]` | Nonperiodic duration offsets from the selected time origin |

Nonperiodic tick/time actions become eligible when elapsed tick/time is **strictly greater** than the requested value. Consequently `run_at_tick = 0` executes on the following tick; use `"Start"` for initialization. Catch-up queues all overdue nonperiodic entries, retaining their original timestamps for audio scheduling. A periodic trigger checks only the current absolute tick and does not replay missed matches.

For example, `run_every_n_ticks = "16"` matches global ticks 0, 16, 32, …; `"12+8"` matches 8, 20, 32, …. These phases do not restart when the script starts. `run_at_time` accepts quoted seconds, `"minutes:seconds"`, or `"hours:minutes:seconds"`, with optional fractional seconds; `"5:80"` is equivalent to `"6:20"`.

Setting an already-enabled slot to true does not trigger it again. Slots are local to the player/runtime. They can be changed by Rust or by the slot actions below.

## Common parameters

These fields filter or delay an action after its trigger becomes eligible. Empty slot lists impose no restriction.

| Field | Meaning |
| --- | --- |
| `rng_pct` | Probability from 0 to 100; runtime clamps the probability to this range |
| `delay_ticks` | Defer execution by this many game ticks; other common filters are evaluated when it runs |
| `if_previous_script_key` | Require the asset key of the preceding runtime |
| `require_slots_all` | Every named slot must be enabled |
| `require_slots_any` | At least one named slot must be enabled |
| `forbid_slots_all` | Reject when **all** named slots are enabled together |
| `forbid_slots_any` | Reject when **any** named slot is enabled |
| `if_runcount_lt`, `if_runcount_le` | Require a run count below/below-or-equal to the value |
| `if_runcount_gt`, `if_runcount_ge` | Require a run count above/above-or-equal to the value |
| `if_runcount_is`, `if_runcount_is_not` | Match/exclude an integer or list of integers |
| `if_runcount_quant` | Match the run count against a quant string |

Run counts start at **1** and are shared by asset handle within that script asset type's runtime resource, across entities. They are not an individual actor's local execution counter. `if_runcount_le = 3` selects the first three initializations; `if_runcount_is_not = [3, 7, 9]` excludes those initializations.

```toml
[[script]]
run_on_slot_enable = "Attack"
delay_ticks = 2
forbid_slots_any = ["Frozen", "Dead"]
action = "PlayAudio"
asset_key = "audio.game.PlayerGetsDamaged"
```

Use positive delays. A delayed action requires its runtime to remain active until delivery; delay does not extend the lifetime of a common script by itself.

## Available actions

### SlotEnable, SlotDisable, SlotToggle

Require `slot = "Name"`. Enable, disable, or invert that runtime's boolean slot. Slot transitions can queue further actions during the same script update.

### DespawnEntity

Despawns the script's own entity. It takes no parameters.

### SpawnScript

Requires `asset_key`. Creates an independent entity with a common-script player. The caller's despawn does not automatically stop the spawned script.

### PlayAudio

Plays a one-shot on the SFX bus. Required: `asset_key`. Optional: `volume` (default 1.0), `pan` (default 0.0; -1 left to +1 right), and `label`. The runtime chooses from the audio asset binding and tracks the instance under the current script execution.

Audio is prepared outside the callback and scheduled at the action's absolute musical timestamp. A late sound starts inside its remaining tail or is discarded if finished; it is not moved to a buffer boundary. Stop these instances with `StopAudio`.

### PlayBackgroundAudio

Plays on the **Ambience** bus through the same precisely scheduled custom mixer. Required: `asset_key`. Optional: `volume` (default 1.0), `loop` (default false), and `label`. This action has no pan parameter. Looping spans the prepared sample; authored music loop/downbeat metadata belongs to the music controller.

```toml
[[script]]
run_on_playback_control = "Start"
action = "PlayBackgroundAudio"
asset_key = "audio.example.Ambience"
volume = 0.04
loop = true
label = "RoomAmbience"

[[script]]
run_on_playback_control = "Stop"
action = "StopBackgroundAudio"
label = "RoomAmbience"
```

The example requires the ambience key to be registered. For the soundtrack, use `MusicCommand` and `tracks.music.toml`, described in [Timing, Audio, and Animation](../architecture/timing-and-audio.md#give-music-independent-ownership). Actor ambience and music have separate ownership.

### StopAudio, StopBackgroundAudio

Both accept `current_script_only` (default true) and optional `label`. `StopAudio` selects SFX; `StopBackgroundAudio` selects ambience. Neither stops the music bus.

When `current_script_only` is true, only instances started by this runtime are candidates. False searches that bus across owners. A label narrows the selection in either case. The stop is timestamped by the executing action. These commands also cancel selected pending starts; ordinary script completion alone does not imply that all its sounds stop.

## Lifetime and ordering

Script requests commit in `ScriptSet::Init` and execute in `ScriptSet::Run` during `GameTickUpdate`. Action filters and slots can affect later actions in that update. Entries sharing a nonperiodic timestamp retain their authored order within that trigger list. Do not assume different trigger families form one globally timestamp-sorted script list; the mixer independently orders submitted audio commands by sample position and submission order.

A common script finishes once its nonperiodic tick/time lists are exhausted and it has no periodic tick actions. Slot listeners and queued delays do not keep it alive. For a persistent listener, author an intentional periodic action or use a game-owned lifetime; merely declaring `run_on_slot_enable` is insufficient. An animation's extended tracker also contributes its playback lifetime.

Normal stop and replacement run `"Stop"` actions. Despawning the entity directly removes the runtime without promising a final script update, so do not rely on a stop action as the sole cleanup mechanism for resources that must be released on arbitrary despawn. Playback completion itself does not despawn the entity.

Global pause freezes simulation and scheduled gameplay audio together. Individually pausing a script stops its updates but does not freeze every voice it previously started. See the [timing architecture](../architecture/timing-and-audio.md) for recovery and presentation behavior.
