# Cli Commands Reference

This page lists the available [dev console](./cli.md) commands.

All of these commands can also be called from [scripts](./script.md)
using the [`RunCli`](./script-ref.md#RunCli) script action.

## General Commands

These commands are available in all builds of the game.

<details>
  <summary>
  <code>AppState</code>
  </summary>

Args:

```
AppState <State>
```

Example:

```
AppState InGame
AppState MainMenu
```

Immediately triggers a global AppState transition.

</details>

<details>
  <summary>
  <code>camera_at</code>
  </summary>

Args:

```
camera_at <x> <y>
```

Example:

```
camera_at 200 300
```

Makes the camera jump to the given coordinates. Useful if you want to look at
a specific location on the map.

</details>

<details>
  <summary>
  <code>camera_limits</code>
  </summary>

Used to manage camera limits (the viewable area). During normal gameplay, the
camera control algorithm should make sure nothing outside of these coordinates
is displayed on the screen.

Noargs:

```
camera_limits
```

Prints the current camera limits to log.

Args:

```
camera_limits <xmin> <ymin> <xmax> <ymax>
```

Example:

```
camera_limits 100 200 300 400
```

Sets the camera limits.

</details>

<details>
  <summary>
  <code>exit</code>
  </summary>

Noargs:

```
exit
```

Quits the app.

</details>

<details>
  <summary>
  <code>hello</code>
  </summary>

Silly trivial command for testing and example purposes. May be useful as
a placeholder during development.

Noargs:

```
hello
```

Prints "Hello!" to log.

Args:

```
hello <arg>
```

Example:

```
hello world
```

Prints "Hello, {arg}!" to log. For example: "Hello, world!"

</details>

<details>
  <summary>
  <code>help</code>
  </summary>

Noargs:

```
help
```

Prints a list/summary of all available CliCommands to log.

</details>

<details>
  <summary>
  <code>locale</code>
  </summary>

Args:

```
locale <locale>
```

Example:

```
locale bg-BG
locale en-US
```

Instantly changes the game's locale (UI/text language) at runtime.

Useful for testing how things look in different locales.

</details>

## Dev-only Commands

These commands are only available if the game was compiled with the `dev`
feature.

For example, you can run the game as:

```sh
cargo run --features dev
```

<details>
  <summary>
  <code>spawn_anim</code>
  </summary>

Args:

```
spawn_anim <asset_key> [<X> <Y>]
```

Example:

```
spawn_anim anim.player.Run
spawn_anim anim.player.RunWithDamage 50 60
```

Spawns an animation test entity to play/preview the given animation asset.

Useful when developing new animations, to test them, before they are integrated
into actual gameplay.

The X/Y coordinates are optional and default to 0,0.

</details>

<details>
  <summary>
  <code>spawn_phystester</code>
  </summary>

Args:

```
spawn_phystester <X> <Y>
```

Example:

```
spawn_phystester 200 300
```

Spawns a physics test entity. The entity is displayed as a pink square, but with
a circular collider. It has a full Dynamic rigid body, so it will fall with
gravity and bounce off walls and other colliders.

</details>

<details>
  <summary>
  <code>spawn_script</code>
  </summary>

Args:

```
spawn_script <asset_key>
```

Example:

```
spawn_script script.cutscene.intro
```

Spawns an entity to run the given script.

This can be useful for testing scripts.

</details>
