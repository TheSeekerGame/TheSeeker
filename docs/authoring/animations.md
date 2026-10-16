# Animations

An `.anim.toml` asset describes sprite playback, frame cues, and optional script actions. It combines an image, atlas metadata, and a playback definition. [The format reference](animation-reference.md) lists the supported fields.

Gameplay uses a 96 Hz simulation clock. An eight-tick frame has a 12 Hz cadence. The game defaults to global cadence: the first frame enters immediately and subsequent frames join the shared grid. These are simulation events; the renderer presents their historical poses with a small delay. See [Timing, Audio, and Animation](../architecture/timing-and-audio.md) for the transport and presentation design.

## Register the image and animation together

Place animations beneath `assets/animations/`. At startup, `game/src/assets.rs` recursively scans `.anim.toml` files and reads their `[asset]` metadata:

```toml
[asset]
key = "anim.example.Pulse"
image = "animations/example/Pulse__r1c4__32x32.png"

[settings]
ticks_per_frame = 8
frame_min = 1
frame_max = 4
frame_start = 1
```

This example assumes a matching image exists. Its path is relative to `assets/`. The filename declares one row, four columns, and 32×32-pixel cells. The scanner registers `anim.example.Pulse`, `anim.example.Pulse.image`, and `anim.example.Pulse.atlas`. Use `keys = ["anim.example.Pulse", "anim.example.OtherPulse"]` instead of `key` for aliases. New files or registration metadata require rerunning the startup scan.

Frame numbers are one-based, traversing the sheet left to right, then top to bottom. Keep them within the atlas capacity. The image dimensions should match the declared rows, columns, and cell size. Shared images are allowed; do not combine sheets solely to satisfy a naming convention.

Opaque magenta pixels (`#FF00FF`) in a frame mark its collider. `engine/src/assets/sprite_colliders.rs` covers the convex hull of each frame's magenta pixels with at most five rectangles and clears those pixels so they never show. Editing a mask or the sheet layout therefore changes combat geometry as well as the artwork.

## Choose playback and cues

The required settings above play through once. Add a bounded clip for an explicit loop:

```toml
[playback]
start = 1
end = 4
ticks_per_frame = 8
mode = "loop"
```

Add named sections when one sheet contains several phases. The existing `assets/animations/player/skills/DownwardDash.anim.toml` shows separate flight and impact sections. Add `[[events]]` for named gameplay cues, and `[[script]]` for supported runtime actions such as sound playback or sprite changes. Put combat decisions and damage rules in the owning game action, as described in [Actors and Actions](../architecture/actors.md).

## Play from Rust

For a standalone preview created by a development system:

```rust
use bevy::prelude::*;
use theseeker_engine::animation::SpriteAnimationBundle;

fn spawn_preview(mut commands: Commands) {
    commands.spawn((
        Sprite { texture_atlas: Some(TextureAtlas::default()), ..default() },
        Transform::from_xyz(100.0, 200.0, 0.0),
        SpriteAnimationBundle::new_play_key("anim.example.Pulse"),
    ));
}
```

Run this system once when the assets and a suitable camera are ready, and give the preview an appropriate cleanup lifetime. For an actor, use its existing presentation owner and action binding. A standalone animation must have a `Sprite` with a texture atlas; the runtime resolves its image and atlas from the registered keys.

Do not write an animation-owned atlas index directly from unrelated Rust systems. Use playback requests, sections, and animation actions so collision poses, frame cues, and audio agree. Completion stops playback but does not automatically despawn the sprite or hide its last frame.

## Verify in context

Test cadence, facing, cues, collision, interruption, and cleanup through the owning gameplay action or a dedicated preview system. F9 opens the timing probe; F10 compares reference and gameplay clicks, and F11/F12 compare player pixel alignment and interpolation. On macOS, function keys may require Fn. Controls and logs are described in [Timing, Audio, and Animation](../architecture/timing-and-audio.md#measure-scheduling-and-display-behavior-separately).
