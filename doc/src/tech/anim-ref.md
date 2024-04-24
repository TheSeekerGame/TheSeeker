# Animation Format Reference

This page describes the syntax / file format for [Animation Assets](./anim.md).

The animation format is based on the [script format](./script-ref.md), with some
differences and extensions (additional functionality). Everything supported in
scripts is also supported in animations.

## How Animation Playback Works

Animations will automatically advance their frames at a fixed rate. All you have
to do is configure the start frame index, the max frame index, and the speed.

For animations that just play through, once, from start to finish, this is enough.

If you need any more advanced control (such as to make animations that loop or
skip frames), you can add `[[script]]` sections, using animation-specific
[actions](#available-actions).

## Texture Atlas

At the top of the file, before any other sections, you must specify the asset key
of the texture atlas to be used for the animation:

```toml
atlas_asset_key = "anim.myanimation.sprite"
```

## Settings

The `[settings]` section is *required* in animation files.

The common parameters that are optional in [scripts](./script-ref.md#settings)
are also optional here, but there are additional animation-specific settings
that are required in every animation file.

Example:

```toml
[settings]
ticks_per_frame = 8
frame_min = 0
frame_max = 7
frame_start = 0
```

The above example will create an animation that starts from frame index 0,
advances to the next frame automatically every 8 ticks, and ends after frame 7
is displayed.

This is perfectly sufficient for a basic animation that does not require any
additional behaviors (such as looping or scripts).

You can have an animation without any `[[script]]` sections.

### Required Settings

The following properties must be specified:

<details>
  <summary>
  <code>ticks_per_frame</code>
  </summary>

Configures the rate/speed of animation playback. This is how many ticks each frame
will be displayed for, before automatically transitioning to the next frame.

</details>

<details>
  <summary>
  <code>frame_min</code>
  </summary>

The lowest permitted frame index. Frames below this should never be displayed.
Should the frame index ever be a value below this, the animation will stop
automatically.

</details>

<details>
  <summary>
  <code>frame_max</code>
  </summary>

The highest permitted frame index. Frames above this should never be displayed.
Should the frame index ever be a value above this, the animation will stop
automatically.

</details>

<details>
  <summary>
  <code>frame_start</code>
  </summary>

The initial frame that will be displayed at tick 0 when the animation starts
playing.

</details>

### Optional Settings

The following additional properties may optionally be specified:

<details>
  <summary>
  <code>atlas_asset_key</code>
  </summary>

Use a specific texture atlas layout, instead of the default. Provide the asset key string.

The default is derived by appending `.atlas` to the asset key of this animation script
asset file.

</details>

<details>
  <summary>
  <code>image_asset_key</code>
  </summary>

Use a specific spritesheet image, instead of the default. Provide the asset key string.

The default is derived by appending `.image` to the asset key of this animation script
asset file.

</details>

## Actions

Just like a [script file](./script-ref.md#actions), animations can contain any
number of *actions* to be performed. The syntax is the same.

All run conditions and actions available in scripts are also available in
animations, but animations also offer extra animation-specific ones.

Example:

```toml
[[script]]
run_at_tick = 24
action = "SetFrameNext"
frame_index = 20

[[script]]
run_at_frame = 7
action = "SetSpriteFlip"
flip_x = true
```

## Available Run Conditions

The following animation-specific run conditions are available, in addition to
everything supported by [scripts](./script-ref.md#available-run-conditions):

<details>
  <summary>
  <code>run_at_frame</code>
  </summary>

Example:

```toml
[[script]]
run_at_frame = 8
action = "..."
```

Run the action whenever the given frame index is displayed.

Any time the animation switches to that frame (regardless of whether it was done
automatically, using a script, or from Rust code), the action will be performed.

</details>

## Available Actions

The following animation-specific actions are available, in addition to
everything supported by [scripts](./script-ref.md#available-actions):

<details>
  <summary>
  <code>SetFrameNext</code>
  </summary>

Example:

```toml
[[script]]
action = "SetFrameNext"
frame_index = 100
```

Change the next automatic frame. Whatever frame is currently displayed will
complete its `ticks_per_frame` duration, and then the animation will jump to the
provided `frame_index`, instead of advancing by one.

Subsequent playback will continue from this new index (will not jump back).

This is useful to implement loops (by going back to a lower frame index) and
skips (if you need to jump over a range of frames).

</details>

<details>
  <summary>
  <code>SetFrameNow</code>
  </summary>

Example:

```toml
[[script]]
action = "SetFrameNow"
frame_index = 100
```

Immediately display the provided `frame_index`.

Does not affect the next automatic frame. The animation playback is otherwise unaffected.

Useful for special effects where you have a special frame you want to "flash"
in-between regular animation frames.

</details>

<details>
  <summary>
  <code>SetSpriteColor</code>
  </summary>

Example:

```toml
[[script]]
action = "SetSpriteColor"
color = "#ff00ff"

[[script]]
action = "SetSpriteColor"
color = [0.75, 0.5, 120.0]
```

Changes the colorization of the sprite. The RGBA values of the pixels will be
multiplied by the provided value.

The `color` field can be specified as either:
 - `[L, C, H]` for LCH color
 - `[L, C, H, A]` for LCH color + Alpha
 - `#RRGGBB` for RGB color
 - `#RRGGBBAA` for RGB + Alpha

RGB color is specified in hexadecimal notation, like for Web/CSS.

LCH color is specified as:
 - Lightness has range 0.0 to 1.5
 - Chroma has range 0.0 to 1.5
 - Hue has range 0.0 to 360.0 (degrees)

</details>

<details>
  <summary>
  <code>SetSpriteFlip</code>
  </summary>

Example:

```toml
[[script]]
action = "SetSpriteFlip"
flip_x = true
flip_y = true
```

Changes whether the sprite image should be displayed flipped/mirrored, along
either axis, or both axes.

Each of the `flip_x` and `flip_y` fields are optional. If omitted, the old
value will be kept.

Useful for making left/right facing animations from the same texture atlas.

</details>

<details>
  <summary>
  <code>SetTicksPerFrame</code>
  </summary>

Example:

```toml
[[script]]
action = "SetTicksPerFrame"
ticks_per_frame = 4
```

Changes the rate of animation playback.

Useful if you want to use a different rate (from what you specified globally in
the [settings](#settings)) for some portion of the animation.

</details>

<details>
  <summary>
  <code>ReversePlayback</code>
  </summary>

Example:

```toml
[[script]]
action = "ReversePlayback"
reversed = true # Play backwards

[[script]]
action = "ReversePlayback"
reversed = false # Play normally

[[script]]
action = "ReversePlayback"
# if `reversed` is omitted,
# toggles the current direction of playback
```

Reverses the playback direction.

If the animation is reversed, the frame index will be decremented, instead of
incremented, as the animation plays. The animation will end when it reaches
the `frame_min` setting instead of the usual `frame_max`.

</details>

<details>
  <summary>
  <code>TransformMove</code>
  </summary>

Example:

```toml
[[script]]
action = "TransformMove"
x = "2.0"
y = "1.0"
z = "0.0"
```

Relative move. Cause the sprite entity's Transform to be translated by the given values.

Each of the `x`, `y`, `z` fields are optional. Omit those you want to leave untouched.

The values must be in quotes and can be specified as either:
 - decimal syntax, like: `"1.25"`
 - fraction syntax, like: `"5/4"`

</details>

<details>
  <summary>
  <code>TransformTeleport</code>
  </summary>

Example:

```toml
[[script]]
action = "TransformTeleport"
x = "2.0"
y = "1.0"
z = "5.0"
```

Teleport the entity to the given position. Set the sprite entity's Transform's translation to the given values.

The `z` field is optional. `x` and `y` are required.

The values must be in quotes and can be specified as either:
 - decimal syntax, like: `"1.25"`
 - fraction syntax, like: `"5/4"`

</details>

<details>
  <summary>
  <code>TransformSetScale</code>
  </summary>

Example:

```toml
[[script]]
action = "TransformSetScale"
x = "2.0"
y = "1.0"
```

Set the scale that the sprite should be displayed as.

Both `x` and `y` are required.

The values must be in quotes and can be specified as either:
 - decimal syntax, like: `"1.25"`
 - fraction syntax, like: `"5/4"`

</details>

<details>
  <summary>
  <code>TransformRotateTurns</code>
  </summary>

Example:

```toml
[[script]]
action = "TransformRotateTurns"
turns = "-1/4"
```

Rotate the sprite by N turns. 1 turn = 360 degrees.

The values must be in quotes and can be specified as either:
 - decimal syntax, like: `"1.25"`
 - fraction syntax, like: `"5/4"`

</details>

<details>
  <summary>
  <code>TransformRotateDegrees</code>
  </summary>

Example:

```toml
[[script]]
action = "TransformRotateDegrees"
degrees = "-15.0"
```

Rotate the sprite by N degrees.

The values must be in quotes and can be specified as either:
 - decimal syntax, like: `"1.25"`
 - fraction syntax, like: `"5/4"`

</details>

<details>
  <summary>
  <code>TransformSetRotationTurns</code>
  </summary>

Example:

```toml
[[script]]
action = "TransformSetRotationTurns"
turns = "-1/4"
```

Set the sprite's rotation to a specific value (in turns). 1 turn = 360 degrees.

The values must be in quotes and can be specified as either:
 - decimal syntax, like: `"1.25"`
 - fraction syntax, like: `"5/4"`

</details>

<details>
  <summary>
  <code>TransformSetRotationDegrees</code>
  </summary>

Example:

```toml
[[script]]
action = "TransformSetRotationDegrees"
degrees = "-15.0"
```

Set the sprite's rotation to a specific value (in degrees).

The values must be in quotes and can be specified as either:
 - decimal syntax, like: `"1.25"`
 - fraction syntax, like: `"5/4"`

</details>
