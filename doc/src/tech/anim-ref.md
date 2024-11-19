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

When using the Scriptable Animation System, *please do not* modify the Bevy
Texture Atlas Index value directly from Rust, outside of it!

## Frame Indexing

All frame index values in animation asset files are 1-indexed!

The first frame of an animation is 1.

When using [frame bookmarks](#frame-bookmarks), this is automatically
compensated for. Frame index 1 means the frame of the bookmark.

## Sprite Sheet

Every animation (obviously) requires a sprite sheet (image and layout).

Those can be specified via the `image_asset_key` and `atlas_asset_key`
settings (see below). If omitted, they will be derived from the asset key
of the animation file, by appending `.image` and `.atlas`.

For example, if you have registered your animation asset with the key
`"anim.Player.Idle"`, the default is to look for `"anim.Player.Idle.image"`
and `"anim.Player.Idle.atlas"`.

However, it is generally good practice to consolidate things into large
sprite sheets, for example having one sprite sheet for all player animations.
In that case, you can still have many animation assets that all share/use
the same sprite sheet, by specifying it explicitly:

```toml
[settings]
image_asset_key = "anim.Player.image"
atlas_asset_key = "anim.Player.atlas"
```

## Frame Bookmarks

To help keep things manageable with complex animations, especially when
you have large consolidated sprite sheets, you can create "frame bookmarks".

Frame bookmarks are names/labels for specific frame index numbers in your
sprite sheet. You can then refer to them in your script actions, to make
any frame numbers mentioned in that `[[script]]` section be relative to
the bookmark.

<details>
  <summary>
  Example:
  </summary>

```toml
[frame_bookmarks]
my_intro = 4
my_idle_loop = 10

[[script]]
frame_bookmark = "my_idle_loop"
run_at_frame = 10 # relative to `frame_bookmark`
action = "SetFrameNext"
frame_index = 1  # relative to `frame_bookmark`

[[script]]
frame_bookmark = "my_intro"
run_on_playback_control = "Start"
action = "SetFrameNow"
frame_index = 1 # This will be frame 4, as per "my_intro"

[[script]]
frame_bookmark = "my_intro"
run_at_frame = 3 # our intro is 3 frames long
action = "SetFrameNext"
# you can use a different bookmark for the destination of
# `SetFrameNext`/`SetFrameNow`, using `to_frame_bookmark`
to_frame_bookmark = "my_idle_loop"
frame_index = 1
```

</details>

Using bookmarks can really help maintainabitily. In the future, if
you want to reorganize your sprite sheets, you can just change
the numbers in the bookmarks and your scripts will just work.

In fact, to fully accomplish this, we can do better than the above
example. We can define everything in bookmarks and not use any
fixed numbers in our script sections.

<details>
  <summary>
  Example:
  </summary>

```toml
[frame_bookmarks]
intro_start = 4
intro_end = 7
idle_loop_start = 10
idle_loop_end = 19

[[script]]
# `run_at_frame` directly accepts bookmarks!
run_at_frame = "idle_loop_end"
action = "SetFrameNext"
to_frame_bookmark = "idle_loop_start"
# if omitted, `frame_index` defaults to 1

[[script]]
run_on_playback_control = "Start"
action = "SetFrameNow"
to_frame_bookmark = "intro_start"

[[script]]
run_at_frame = "intro_end"
action = "SetFrameNext"
to_frame_bookmark = "idle_loop"
```

</details>

## Settings

The `[settings]` section is *required* in animation files.

The common parameters that are optional in [scripts](./script-ref.md#settings)
are also optional here, but there are additional animation-specific settings
that are required in every animation file.

Example:

```toml
[settings]
ticks_per_frame = 8
frame_min = 1
frame_max = 8
frame_start = 1
```

The above example will create an animation that starts from frame index 1,
advances to the next frame automatically every 8 ticks, and ends after frame 8
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

Example:

```toml
[settings]
ticks_per_frame = 8
# ...
```

Configures the rate/speed of animation playback. This is how many ticks each frame
will be displayed for, before automatically transitioning to the next frame.

</details>

<details>
  <summary>
  <code>frame_min</code>
  </summary>

Example:

```toml
[settings]
frame_min = 1
# ...
```

The lowest permitted frame index. Frames below this should never be displayed.
Should the frame index ever be a value below this, the animation will stop
automatically.

</details>

<details>
  <summary>
  <code>frame_max</code>
  </summary>

Example:

```toml
[settings]
frame_max = 20
# ...
```

The highest permitted frame index. Frames above this should never be displayed.
Should the frame index ever be a value above this, the animation will stop
automatically.

</details>

<details>
  <summary>
  <code>frame_start</code>
  </summary>

Example:

```toml
[settings]
frame_start = 2
# ...
```

The initial frame that will be displayed at tick 0 when the animation starts
playing.

</details>

### Optional Settings

The following additional properties may optionally be specified:

<details>
  <summary>
  <code>play_reversed</code>
  </summary>

Example:

```toml
[settings]
play_reversed = true
# ...
```

If set to `true` the animation playback will start reversed. That is,
every `ticks_per_frame`, the frame index will be decreased by one
instead of being increased by one.

If you want to change this dynamically during playback, you can use
the `ReversePlayback` script action.

</details>

<details>
  <summary>
  <code>atlas_asset_key</code>
  </summary>

Example:

```toml
[settings]
atlas_asset_key = "anim.Player.atlas"
# ...
```

Use a specific texture atlas layout, instead of the default. Provide the asset key string.

The default is derived by appending `.atlas` to the asset key of this animation script
asset file.

If you specify this, be sure to also specify `image_asset_key`.

</details>

<details>
  <summary>
  <code>image_asset_key</code>
  </summary>

Example:

```toml
[settings]
image_asset_key = "anim.Player.image"
# ...
```

Use a specific spritesheet image, instead of the default. Provide the asset key string.

The default is derived by appending `.image` to the asset key of this animation script
asset file.

If you specify this, be sure to also specify `atlas_asset_key`.

</details>

## Actions

Just like a [script file](./script-ref.md#actions), animations can contain any
number of *actions* to be performed. The syntax is the same.

Everything available in scripts is also available in animations, but animations
also offer extra animation-specific actions, trigger conditions, parameters, etc.

Example:

```toml
[[script]]
run_at_tick = 24
action = "SetFrameNext"
frame_index = 20

[[script]]
run_at_frame = 8
action = "SetSpriteFlip"
flip_x = true
```

## Available Trigger Conditions

The following animation-specific trigger conditions are available, in addition
to everything supported by [scripts](./script-ref.md#available-trigger-conditions):

<details>
  <summary>
  <code>run_at_frame</code>
  </summary>

Example:

```toml
# using a literal frame number
[[script]]
run_at_frame = 8
action = "..."

# using a bookmark
[[script]]
run_at_frame = "my_bookmark"
action = "..."

# equivalent, but the bookmark
# also applies to any other frame numbers
# in this `[[script]]` section
[[script]]
run_at_frame = 1
frame_bookmark = "my_bookmark"
action = "..."

# you can specify multiple frames
[[script]]
run_at_frames = [ 1, 2, 3, 5, 7, 11 ]
frame_bookmark = "my_bookmark"
action = "..."

# or bookmarks
[[script]]
run_at_frames = [ "bookmark1", "bookmark2", "bookmark3" ]
action = "..."
```

Run the action whenever the given frame is displayed.

Any time the animation switches to that frame (regardless of whether it was
done automatically as part of normal playback, or jumped to using a script
action), the action will be performed.

The frame can be specified using a literal number or a bookmark.
If a number is used and a bookmark is specified using the `frame_bookmark`
[common parameter](#common-parameters), the number will be relative to that.

You can specify multiple frames as an array. In that case, the action will
trigger on any of them.

</details>

<details>
  <summary>
  <code>run_every_n_frames</code>
  </summary>

Example:

```toml
# Every 8 frames, starting from the first
[[script]]
run_every_n_frames = "8"
action = "..."

# Every 8 frames, starting from the fourth
[[script]]
run_every_n_frames = "8+3"
action = "..."
```

Run the action if the frame number matches the pattern specified.

Any time the animation switches to any of those frames (regardless of whether
it was done automatically as part of normal playback, or jumped to using a
script action), the action will be performed.

If a bookmark is specified using the `frame_bookmark` [common
parameter](#common-parameters), the numbers will be relative to that.

</details>

## Common Parameters

These are additional parameters that can be specified regardless of the action.

Whenever the trigger condition is met, these parameters will be evaluated. They
can be used to modify how the action should run or if it should run at all.

All the parameters from [scripts](./script-ref.md#common-parameters) are
available for animations too, plus some additional ones:

<details>
  <summary>
  <code>frame_bookmark</code>
  </summary>

Example:

```toml
[frame_bookmarks]
my_bookmark = 10

# Will actually run on frame 14 (affects `run_at_frame`)
[[script]]
run_at_frame = 4 # Relative to "my_bookmark"
frame_bookmark = "my_bookmark"
action = "..."

# Will actually jump to frame 12 (affects `frame_index`)
[[script]]
run_at_tick = 16
frame_bookmark = "my_bookmark"
action "SetFrameNow"
frame_index = 2
```

Makes all frame numbers mentioned in this `[[script]]` section be
relative to the value of a [frame bookmark](#frame-bookmarks).

This applies to Trigger Conditions like `run_at_tick` as well as
to the parameters of Script Actions like `SetFrameNow`/`SetFrameNext`.

Note: Script Actions like `SetFrameNow`/`SetFrameNext` also allow you
to specify a bookmark for them to use, which can be different from
the one set using this parameter, if any.

If you specify a bookmark that does not exist, this parameter will
have no effect, and the values will be treated as global/absolute.
In dev builds, the game might print warnings to the log/console.

</details>

<details>
  <summary>
  <code>if_frame_lt</code>/<code>if_frame_le</code>/<code>if_frame_gt</code>/<code>if_frame_ge</code>/<code>if_frame_is</code>/<code>if_frame_is_not</code>
  </summary>

Example:

```toml
# Do something every 3 ticks, but only if we are currently
# displaying a frame between 10 and 19
[[script]]
run_every_n_ticks = "3"
if_frame_ge = 10
if_frame_le = 19
action = "..."

# Do something every 5+1 ticks, but only if we haven't reached
# the frame represented by "my_bookmark" yet
[[script]]
run_every_n_ticks = "5+1"
if_frame_lt = "my_bookmark"
action = "..."

# Do something as soon as the "attack" slot is enabled,
# but only if we are currently displaying frame 6
[[script]]
run_on_slot_enable = "attack"
if_frame_is = 6
action = "..."

# Do something every 3 ticks, but only if we are not currently
# displaying one of the special magic frames
[[script]]
run_every_n_ticks = "3"
if_frame_is_not = [ 3, 7, 9 ]
action = "..."
```

Only run the action if the current frame index is:

 - less than a given value (`lt`)
 - less than or equal to a given value (`le`)
 - greater than a given value (`gt`)
 - greater than or equal to a given value (`ge`)
 - equal to any of the specified values (`is`)
 - not equal to any of the specified values (`is_not`)

It can be specified as either a number or a frame bookmark. If specified
as a number, it will be relative to `frame_bookmark`, if set.

</details>

<details>
  <summary>
  <code>if_oldanim_frame_lt</code>/<code>if_oldanim_frame_le</code>/<code>if_oldanim_frame_gt</code>/<code>if_oldanim_frame_ge</code>/<code>if_oldanim_frame_was</code>/<code>if_oldanim_frame_was_not</code>
  </summary>

Example:

```toml
# Do something when the animation starts,
# but only if the previouly-playing animation
# was "anim.Player.Attack"
# and it had not yet reached frame 7
[[script]]
run_on_playback_control = "Start"
if_previous_script_key = "anim.Player.Attack"
if_oldanim_frame_lt = 7
action = "..."

# Do something on tick 3,
# but only if the previously-playing animation
# was "anim.Player.Run"
# and it was on frame 15
# (when the current animation was started)
[[script]]
run_at_tick = 3
if_previous_script_key = "anim.Player.Run"
if_oldanim_frame_was = 15
action = "..."
```

These parameters let you perform actions depending on where the
previously-playing animation (if any) left off. This allows you to implement
special handling for specific transitions between animations.

These parameters should be used together with `if_previous_script_key`.

Only run the action if the frame index of the previously-playing animation
(at the time of switching to the current animation) was:

 - less than a given value (`lt`)
 - less than or equal to a given value (`le`)
 - greater than a given value (`gt`)
 - greater than or equal to a given value (`ge`)
 - equal to any of the specified values (`was`)
 - not equal to any of the specified values (`was_not`)

The provided values must be literal numbers. Since they refer to a different
animation asset, bookmarks are not taken into account.

</details>

<details>
  <summary>
  <code>if_playing_reversed</code>
  </summary>

Example:

```toml
# This action will only run if the animation is playing backwards (reversed)
[[script]]
run_at_frame = 4
if_playing_reversed = true
action = "..."

# This action will only run if the animation is playing forwards (normal)
[[script]]
run_at_frame = 4
if_playing_reversed = false
action = "..."
```

Makes it so that the action only runs if the animation is playing in
the specified direction.
 - `true`: only run when playing backwards / reversed
 - `false`: only run when playing forwards / normally

Neither value is the "default". If this parameter is unset, then the action
runs regardless of the playback direction.

</details>

## Available Actions

The following animation-specific actions are available, in addition to
everything supported by [scripts](./script-ref.md#available-actions):

<details>
  <summary>
  <code>SetFrameNext</code>
  </summary>

Examples:

```toml
# After the current frame,
# Jump to frame 100
[[script]]
action = "SetFrameNext"
frame_index = 100

# After the current frame,
# Jump to the frame indicated by "my_bookmark"
[[script]]
action = "SetFrameNext"
to_frame_bookmark = "my_bookmark"

# After the current frame,
# Jump to 2 frames after the frame indicated by "my_bookmark"
[[script]]
action = "SetFrameNext"
to_frame_bookmark = "my_bookmark"
frame_index = 2

# After frame 10 relative to "my_bookmark",
# continue to frame 5 relative to "my_bookmark"
[[script]]
frame_bookmark = "my_bookmark"
run_at_frame = 10
action = "SetFrameNext"
frame_index = 5

# After frame 8 relative to "my_bookmark",
# continue to frame 4 relative to "my_other_bookmark"
[[script]]
frame_bookmark = "my_bookmark"
run_at_frame = 8
action = "SetFrameNext"
to_frame_bookmark = "my_other_bookmark"
frame_index = 4
```

Change the next automatic frame. Whatever frame is currently displayed
will complete its `ticks_per_frame` duration, and then the animation will
continue to the specified frame, instead of advancing by one. Subsequent
playback will continue as normal from this new location.

This is useful to skip around the sprite sheet, and to implement loops
(by going back to a lower frame index).

---

`frame_index` can be used to specify a literal frame number. It defaults to
`1` if unspecified.

If `to_frame_bookmark` is specified, the `frame_index` will be interpreted
relative to that.

Otherwise, if `frame_bookmark` (the action-agnostic
[parameter](#common-parameters)) is specified, the `frame_index` will be
interpreted relative to that.

Otherwise, the `frame_index` will be global/absolute.

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

Immediately go to the specified frame.

This is useful if you do not want to wait until the next `ticks_per_frame`
interval, such as in response to player inputs, etc.

Whatever frame is currently displayed will be replaced by the specified
frame immediately, and playback will continue as normal from there. The
frame will wait out any remaining `ticks_per_frame` duration (as if this
action wasn't run), before transitioning to the next frame.

Any actions that are configured to trigger on the specified frame will run.

---

`frame_index` can be used to specify a literal frame number. It defaults to
`1` if unspecified.

If `to_frame_bookmark` is specified, the `frame_index` will be interpreted
relative to that.

Otherwise, if `frame_bookmark` (the action-agnostic
[parameter](#common-parameters)) is specified, the `frame_index` will be
interpreted relative to that.

Otherwise, the `frame_index` will be global/absolute.

---

Note: `SetFrameNow` is should normally be used with non-frame-based trigger
conditions. It is recommended that you avoid using `SetFrameNow` in combination
with a `run_at_frame` (or similar) trigger condition. If you do that, the
animation will technically first go through its original frame (and run
any other actions for it) and then immediately replace it with the new one
(and run its actions), which is often not what you want. Use `SetFrameNext`
instead, to avoid processing a frame you do not intend to display.

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
