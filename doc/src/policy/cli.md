# CLI Commands

These are our policies for CLI Commands (for the [Dev Console](../tech/cli.md), etc.).

## Documentation

When committing new CLI Commands into the GitHub Repo, please make sure to add
an entry for them on [this page](../tech/cli-ref.md), so other people in our
team can know about it.

## Naming

A command's name should be unambiguous/understandable but brief and easy to type. This
is subjective, but try to strike a balance: avoid overly terse abbreviations or overly
long and verbose names.

If you notice that we have existing commands that do something similar to your new
command, try to create/follow a consistent naming pattern to keep things intuitive.

Examples:

Good name: `locale`. Bad name: `chloc`. Bad Name: `change_ui_locale`.

Good name: `level`. Bad name: `chlvl`. Bad name: `switch_to_level`.

Good name: `spawn_anim`. Bad name: `spa`. Bad name: `spawn_animation_test_entity`.

Good name: `spawn_enemy`. Bad name: `place_enemy`. (we have other `spawn_*` commands,
you should follow the naming pattern)

## Arguments

Try to follow a general pattern of least-specific to most-specific where it makes sense.

If there are other commands that do something similar, try to be consistent with them.

Good example:

```
spawn_enemy <kind> <hp> <x> <y>
```

```
spawn_enemy MonsterKind01 50 100 200
```

The kind of enemy is the less-specific piece of info, so it comes first. The X/Y coordinates
where to put it are the more-specific / dynamic piece of info, so they come last.

Bad example:

```
spawn_effect 50 70 SmokeEffect
```

This order of arguments is the opposite of what is recommended. Also, it is inconsistent
with the pattern followed by other similar commands, such as `spawn_anim`.

## Implementation

The Rust function that implements a command, should be named as the command name with a `cli_` prefix.

Good example:

```rust
fn cli_exit(/* ... */) {
    // ...
}

app.register_clicommand_noargs("exit", cli_exit);
```

Bad example:

```rust
fn change_locale(/* ... */) {
    // ...
}

app.register_clicommand_args("locale", change_locale);
```

## Where in the source code to put it?

The general place for miscellaneous CLI commands is `game/src/cli.rs`.
If you don't know where to put something, put it there. When in doubt, put it there.

If a command is specific to some aspect of the game or needs access to private
implementation details, it is acceptable to put it in the file where that
functionality is otherwise implemented. For example, it is okay to have
camera-related CLI commands in `game/src/camera.rs`.

DO NOT put any cli commands in the `engine/` source code tree (the library part of
the repo/project). All cli commands should be in `game/` (the executable/app).

### Dev-only commands

The `game/src/dev.rs` file is only compiled with the `dev` cargo feature. If
you are making a command that should only be available in the dev builds of the
game, put it there.

If you want to make a dev-only command, but have to put it elsewhere, in a file
that is compiled in regular builds of the game, you can add a conditional compilation
attribute:

```rust
#[cfg(feature = "dev")]
fn cli_mydevcommand() {}
```
