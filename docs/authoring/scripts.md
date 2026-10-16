# Scripts

Script assets sequence a fixed set of engine actions: changing slots, spawning scripts, despawning entities, and playing or stopping audio. Animations extend the same format with frame triggers and sprite actions. Use Rust game systems for new mechanics rather than treating asset scripts as a general-purpose programming language.

The game currently ships no standalone `.script.toml` assets; every `.anim.toml` animation uses the same triggers and actions. The engine still loads and runs common scripts, as described below.

## Create and load a script

Place `.script.toml` files under `assets/`. For playback by a dynamic asset key, add a `File` entry to `assets/gameplay.assets.ron`:

```ron
"script.example.Ambience": File (
    path: "scripts/example/Ambience.script.toml",
),
```

This is an entry inside the existing RON map, not a complete asset collection. The script file could contain:

```toml
[settings]
time_base = "Relative"

[[script]]
run_on_playback_control = "Start"
action = "SlotEnable"
slot = "Ready"
```

See [the format reference](script-reference.md) for triggers, lifetime behavior, parameters, and actions. Key registration is needed for `play_key`; direct handles can also be loaded with Bevy's `AssetServer`.

## Start playback from Rust or another script

```rust
use bevy::prelude::*;
use theseeker_engine::script::common::ScriptBundle;

fn start_sequence(mut commands: Commands) {
    commands.spawn(ScriptBundle::new_play_key("script.example.Ambience"));
}
```

Register the system at the intended gameplay boundary and provide entity cleanup when needed. `ScriptBundle` creates a player, not a rendered sprite. A `SpawnScript` action creates a separate entity whose lifetime is independent of its caller:

```toml
[[script]]
run_on_playback_control = "Start"
action = "SpawnScript"
asset_key = "script.example.Ambience"
```

## Control active playback

`ScriptPlayer<T>` owns a playback request and its runtime. `T` is `Script` for common scripts or `SpriteAnimation` for animations. Systems can set slots, pause, stop, or request another asset.

```rust
use bevy::prelude::*;
use theseeker_engine::assets::script::Script;
use theseeker_engine::script::ScriptPlayer;

fn mark_ready(mut players: Query<&mut ScriptPlayer<Script>>) {
    for mut player in &mut players {
        if player.current_key() == Some("script.example.Ambience") {
            player.set_slot("Ready", true);
        }
    }
}
```

Slots belong to the particular player/runtime; they are not global flags. A `[config]` table is parsed into the asset's `config` field, a map of `DynamicConfigValue` values. Rust code that needs those values reads them from the loaded asset in `Assets<Script>` and matches the `Int`, `Float`, or `String` variant; `7` and `7.0` are different variants.

`play_key` queues a request. `ScriptSet::Init` commits requests before `ScriptSet::Run` advances playback in `GameTickUpdate`. Whether a request starts in the current or a later tick depends on when it was submitted and whether its asset is ready. Until a replacement commits, ordinary slot/config access still addresses the old runtime. Slots set before the first runtime exists are retained for initialization. Actor presentation also has execution-scoped overrides; use that ownership API for actor behavior.

A completed common script stops running, but its entity remains until explicitly despawned. A slot-only script does not remain active indefinitely just because it declares slot triggers. See the reference's [lifetime rules](script-reference.md#lifetime-and-ordering) before authoring persistent listeners.

## Keep timing authoritative

Scripts execute on simulation ticks, not display refreshes. Millisecond and time triggers use simulation `ClockTime`; scheduled audio preserves their timestamp even though Rust dispatch happens during a tick. Use `run_on_playback_control = "Start"` for immediate initialization. The current nonperiodic tick/time trigger checks are strictly past the requested boundary; `run_at_tick = 0` runs on the following tick.

Use [animation frame cues](animation-reference.md#named-cues) for frame-owned effects and the game music controller for soundtrack transitions. [Timing, Audio, and Animation](../architecture/timing-and-audio.md) explains the shared transport, pauses, presentation delay, and audio buses.
