# Script Format Reference

This page describes the syntax / file format for [Script Assets](./script.md).

## Settings

A script file can have an optional `[settings]` section, where you can put
parameters that govern how the script is to be run by the game.

Everything here is optional.

Example:

```toml
[settings]
time_base = "Relative"
tick_quant = "8+4"
```

The following properties are available:

<details>
  <summary>
  <code>time_base</code>
  </summary>

Configures the "time zero" point. From when does the script begin counting time?

 - `Relative`: from the moment it is spawned (default)
 - `Level`: from the moment the current level was loaded
 - `Startup`: from app startup

This allows you to use scripts for "global" behavior, which should be tied to
the level or the entire app runtime. Could be useful for background events.

The script will "catch up" on any missed time after it is spawned. For example,
if you spawn a script with `time_base = "Level"` long after the level has been
loaded, any non-periodic actions in the script that should have happened in the
time before the script was spawned will be performed at the first tick when the
script runs.

</details>

<details>
  <summary>
  <code>tick_quant</code>
  </summary>

Ensure the script time is quantized to a specific number of ticks. This is
useful when specific scripts must run aligned to specific time intervals.

For example, `"8+4"` means that the script is only allowed to start on a tick
number that is a multiple of 8, offset/delayed by 4 ticks.

</details>

## Actions

A script file can contain any number of *actions* to be performed. This is the
meat of the script.

Every action is declared using a `[[script]]` section. Each such section
specifies one "action" with its associated parameters. You can have any number
of them in a script file.

Example:

```toml
[[script]]
run_at_tick = 0
action = "RunCli"
cli = [ "dothing blah blah" ]
```

Every `[[script]]` section *must* contain:

 - a *run condition* to determine *when* to perform the action
 - an `action` field to specify the kind of action to perform
 - any additional parameters as required by the action, depending on the action kind

## Available Run Conditions

The run condition is a mandatory part of every `[[script]]` section. It
determines when to run the action.

Multiple run conditions are currently not supported/allowed. There must be
exactly one.

The available run conditions are:

<details>
  <summary>
  <code>run_at_tick</code>
  </summary>

Example:

```toml
[[script]]
run_at_tick = 96
action = "..."
```

Run the action once, on the given tick number.

Tick numbers count from the start of the script. May be affected by script settings.

</details>

<details>
  <summary>
  <code>run_every_n_ticks</code>
  </summary>

Example:

```toml
[[script]]
run_every_n_ticks = "16"
action = "..."

[[script]]
run_every_n_ticks = "12+8"
action = "..."
```

Run the action periodically, quantized to the given TickQuant value.

In the example above:
 - the first action will run on ticks 0, 16, 32, 48, 64, 80, ...
 - the second action will run on ticks 8, 20, 32, 44, 56, 68 ...

Does not catch up on missed time (if the `time_base` setting is not `Relative`).

</details>

<details>
  <summary>
  <code>run_at_millis</code>
  </summary>

Example:

```toml
[[script]]
run_at_millis = 250
action = "..."
```

Run the action once, after the given number of milliseconds have elapsed.

Time is counted from the start of the script. Affected by the `time_base` setting.

</details>

<details>
  <summary>
  <code>run_at_time</code>
  </summary>

Example:

```toml
[[script]]
run_at_time = "1:13:17.315"
action = "..."
```

Run the action once, after the given amount of time has elapsed.

Time is counted from the start of the script. Affected by the `time_base` setting.

The syntax for the time allows you to specify: hours, minutes, seconds, fraction
of the second. Everything except for the seconds is optional. Minutes/seconds
over 60 are accepted. Leading zeros are optional.

Examples:
 - `"10"`: 10 seconds
 - `"5:00"`: 5 minutes
 - `1:00:05`: 1 hour, 5 seconds
 - `1:7:5.25`: 1 hour, 7 minutes, 5.25 seconds
 - `5:80`: 5 minutes + 80 seconds (effectively the same as `"6:20"`)

</details>

## Available Actions

The action kind is a mandatory part of every `[[script]]` section. There must be
exactly one `action` field per section. If you want to perform more than one
action, create multiple sections (possibly with the same run condition).

Most actions accept/require additional parameters. Simply add those to the
`[[script]]` section, depending on the kind of action.

The available actions are:

<details>
  <summary>
  <code>DespawnEntity</code>
  </summary>

Example:

```toml
[[script]]
action = "DespawnEntity"

[[script]]
action = "DespawnEntity"
label = "mythingies"
```

If the `label` field is not present, despawns the current entity (the one
hosting the script). This will cause the script to be terminated.

If the `label` field is present, will use the [`EntityLabels`] resource to find
all entities with the given label string and despawn them.

</details>

<details>
  <summary>
  <code>RunCli</code>
  </summary>

Example:

```toml
[[script]]
action = "RunCli"
cli = [
  "hello",
]
```

Runs one or more [CLI commands](./cli-ref.md), just like you can do manually
from the [dev console](./cli.md).

Requires the `cli` field, which is a list of CLI strings to evaluate.

All of them will run on the same tick / script update, in order.

</details>

<details>
  <summary>
  <code>SpawnScript</code>
  </summary>

Example:

```toml
[[script]]
action = "SpawnScript"
asset_key = "script.asset.key"
```

Runs the given script asset. Will create a new Bevy entity for it.

</details>

[`EntityLabels`]: https://theseekergame.github.io/api/theseeker_engine/script/label/struct.EntityLabels.html
