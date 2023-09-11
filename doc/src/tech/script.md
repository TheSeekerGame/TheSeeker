# Scripts How-To

Script Assets are a way to drive/trigger things to happen in the game, without
using Rust code, at precise times.

Think of it like a sequencer. Useful if you want specific things to happen at
specific points in time, or repeat in specific intervals.

This should make it easy for us to author interesting in-game scenarios, such as
cutscenes, or general things to happen in the background in specific
rooms/areas/levels, etc.

## Creating new script files

Script files end in `.script.toml` and are loaded as Bevy assets, so they need
to be part of the `assets` folder. Make sure to create an entry in
`gameplay.assets.ron`, to ensure your scripts get loaded.

[See here for the syntax / what you can put in the file.](./script-ref.md)

## How to run your scripts

### CLI

You can use the [`spawn_script`](./cli-ref.md#spawn_script) CLI command (for
example, from the [dev console](./cli.md), for testing), to spawn a script into
the game.

```
spawn_script script_asset_key
```

### Rust

From Rust code, you can run a script by spawning a special entity, as follows:

```rust
commands.spawn(ScriptBundle {
    key: "script.asset.key",
});
```

### Scripts

From another script:

```toml
[[script]]
run_at_tick = 100 # or whatever
action = "SpawnScript"
asset_key = "script.asset.key"
```

## Extended Scripts

We have other asset types (such as [animations](./anim.md)) that are based on
Scripts, but extend them with additional features to tailor to specific use
cases.

These other asset types support everything that regular scripts can do, and
more.
