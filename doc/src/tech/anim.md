# Animations How-To

Animation Assets are how we create sprite-based animations.

Each such file is an animation that can be played by the game, and describes how
it should be played.

Animations are actually based on [scripts](./script.md). They support everything
that scripts can do + additional features tailored for animation use cases.

[See here for a reference on the syntax/format.](./anim-ref.md)

## Art Assets

An animation file requires a "texture atlas". That is the file (typically PNG
format) which provides the actual image data / frames to be displayed.

Please don't confuse the two. They are separate things, two separate files:
 - The Texture Atlas, containing the image data.
All the frames that can be displayed by an animation, in a grid layout.
One such file can be used by multiple Animations.
 - The Animation Asset (what this page is about): the script that tells the game how to actually play the animation.

You should create an animation asset for every piece/clip of animation that
we want to be able to play in the game.

The texture atlases can be reused or split as convenient. Usually there will
be one atlas per animation, but sometimes it might make sense to have one atlas
shared between multiple animations:
 - If different animations need to display some of the same frames.
 - If you have multiple variants of an animation, like `Run` and `RunWithDamage` (which adds a blinking effect),
   or left/right flipped variants.

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

## Playing animations from Rust code

// TODO

I still need to figure out exactly how the Rust code for switching between
different animations (say, as the player performs different actions on different
inputs) is going to work. Coming soon.
