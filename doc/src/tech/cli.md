# Dev Console

The Dev Console is a very handy and important thing to use during game
development. It is expected that everyone in the project is familiar with it.
It allows you to mess around with things in-game, which is very important for
testing out different functionality, incl. stuff that is not yet available via
the normal gameplay or UI.

## How to use

The most obvious way to use CLI commands is interactively via the on-screen
in-game console prompt.

To bring up the console, press the `` ` `` or `~` key.

This will bring up an on-screen prompt, where you can type your command.

Command history (using up-arrow to bring up previous commands) is supported.

## Available commands

We document all commands [on this page](./cli-ref.md).

If you want to know what commands are available in your build of the game,
you can use the [`help`](./cli-ref.md#help) command to list them.

[See here for how to create your own commands.](./cli-howto.md)

## From Rust code

CLI commands are useful beyond just the dev console. They are effectively
named Bevy systems that you can run by name.

If you want to run a CLI command from Rust, you can do so.

Using `Commands` (yes, the regular bevy kind), from a Bevy system:

```rust
fn my_system(
    mut commands: Commands,
) {
    // provide exactly the same string as you would in the devconsole prompt
    commands.run_clicommand("dothing arg 1.0 2.0 3.0");
    commands.run_clicommand("hello");
}
```

This will queue up your CLI to run whenever Bevy `Commands` get applied next.

If you have direct `World` access (from a Bevy exclusive system):

```rust
fn my_system(
    world: &mut World,
) {
    world.run_clicommand("dothing blah blah");
}
```

This will run the CLI immediately.

## From Bevy UI

CLI commands can also be hooked up to Bevy UI. For example, our main menu
buttons are implemented to just call the respective CLI commands.

You can do so using the `OnClick` component (from `iyes_ui`):

```rust
// create a button to run the `exit` command, to exit the app
commands.spawn((
    ButtonBundle {
        // ...
        ..default()
    },
    OnClick::new().cli("exit"),
));

// create a button to run multiple CLI commands: enter the game, and print hello to log
commands.spawn((
    ButtonBundle {
        // ...
        ..default()
    },
    OnClick::new().cli("AppState InGame").cli("hello"),
));
```

## From Scripts

Commands can also be called from [script](./script.md) and
[animation](./anim.md) assets.

Yes, you can make an animation run arbitrary cli commands on
specific animation frames or whatever. ;)

This is very powerful and useful. If you need some custom functionality for your
scripts/animations, because you want to trigger it that way (very useful if you
want precise timings), just [create a new cli command](./cli-howto.md)!  Voila,
now you can trigger your custom behavior from scripts/animations!

```toml
# On a specific tick
[[script]]
run_at_tick = 96
action = "RunCli"
cli = [
  "dothing blah blah",
  "domore etc etc",
]

# In animations, when a given frame index is displayed
[[script]]
run_at_frame = 7
action = "RunCli"
cli = [
  "hello",
]
```
