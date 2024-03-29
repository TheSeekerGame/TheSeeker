# Animations How-To

Animation Assets are how we create sprite-based animations.

Each such file is an animation that can be played by the game, and describes how
it should be played.

Animations are actually based on [scripts](./script.md). They support everything
that scripts can do + additional features tailored for animation use cases.

[See here for a reference on the syntax/format.](./anim-ref.md)

## Art Assets

An animation consists of 3 parts:
 - The "spritesheet image" (typically PNG format), which provides the actual
   image data / frames to be displayed.
 - The "texture atlas layout": the metadata describing the dimensions of each
   frame and the number of rows and columns in the spritesheet image.
 - The "animation script", which allows our animation system to play the animation
   and allows extra features and Rust integrations to be hooked up to it.

Normally, when you want to add a new animation to the game, you should declare
the above three things in `animations.assets.ron`. Example:

```ron
    "anim.player.FallForward": File (
        path: "animations/player/movement/FallForward.anim.toml",
    ),
    "anim.player.FallForward.image": File (
        path: "animations/player/movement/FallForward.png",
    ),
    "anim.player.FallForward.atlas": TextureAtlasLayout (
        tile_size_x: 96.,
        tile_size_y: 96.,
        rows: 1,
        columns: 4,
    ),
```

The convention is to append `.image` and `.atlas` for the asset keys of
the spritesheet image and texture atlas layout, respectively.

## Testing

To see how an animation looks in-game, you can test it using the [dev
console](./cli.md). Use the [`spawn_anim`](./cli-ref.md#spawn_anim) command:

```
spawn_anim anim.asset.key
```

Or optionally with X/Y coordinates where to display it:

```
spawn_anim anim.asset.key 100 200
```
