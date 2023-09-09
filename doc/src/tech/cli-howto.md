# How to create new CLI Commands

([see here](../policy/cli.md) for our policies/conventions on how to do things)

---

If you want to add/implement a new command into the game, here is how.

Every command is implemented as a Bevy system, in the Rust code of the game.
Both regular and exclusive systems are supported. You can use any standard Bevy
system params, to access whatever data you want.

You **must** register your cli commands using the Bevy app builder.

There are two kinds of commands: *noargs* and *args*.

## Noargs commands

*Noargs* commands are bare Bevy systems:

```rust
fn cli_mything(
    mut commands: Commands,
    my_query: Query<&Thing>,
    my_res: Res<&MyThing>,
    // ...
) {
    // ... do something ...
}
```

We can register it as follows:

```rust
app.register_clicommand_noargs("mything", cli_mything);
```

You provide the name string that users will have to type in the console to
invoke the command, and the function (bevy system) that implements the command.
They don't have to match.

You can then call it from the Dev Console as such:

```
mything
```

## Args commands

*Args* commands must start with a special `In` parameter, which is how they
get the values of the arguments provided via the dev console / script:

```rust
fn cli_mything(
    In(args): In<Vec<String>>,
    // bevy system params follow:
    my_query: Query<&Thing>,
    // ...
) {
    // we can now process/parse the args:
    if args.len() != 1 {
        error!("Expected exactly 1 argument!");
        return;
    }
    let Ok(value) = args[0].parse::<f32>() {
        error!("Expected a numeric value as an argument!");
        return;
    }

    // ... do something with `value` and `my_query` or whatever
}
```

(note: the engine only splits the CLI string on spaces and does not do any extra
processing for you. Your function must take `In<Vec<String>>`.)

(also note: the name of the command itself is not included in that `Vec`.
`args[0]` contains the first CLI argument, after the space separator)

We can register it as follows:

```rust
app.register_clicommand_args("mything", cli_mything);
```

You provide the name string that users will have to type in the console to
invoke the command, and the function (bevy system) that implements the command.
They don't have to match.

You can then call it from the Dev Console as such:

```
mything 1.0
```

## How the CLI Runtime works

Depending on what you typed in the dev console / script (the CLI string), the
engine will look for the respective kind of command by name.

When you try to run a command by name only (there are no spaces in the CLI
string), the engine will look for a *noargs* command by that name.

If the CLI string contains spaces, it will be split on those spaces, and
the engine will look for an *args* command whose name matches the first
part of the CLI string. The remaining substrings will be passed as arguments.

It is possible to register both a **noargs** and and **args** command with
the same name. The correct one will be selected, depending on the presence
of spaces in the CLI string.

### Examples

Say we register the following commands:

```rust
app.register_clicommand_args("dothing", cli_dothing);
app.register_clicommand_args("hello", cli_hello_args);
app.register_clicommand_noargs("hello", cli_hello_noargs);
app.register_clicommand_noargs("exit", cli_exit);
```

Here is what will happen if you type different things in the dev console:

---

```
exit
```

Will run the `cli_exit` Bevy system.

---

```
exit blah
```

Error: command not found.

(because no *args* command with the name `exit` exists)

---

```
hello
```

Will run the `cli_hello_noargs` Bevy system.

---

```
hello world
```

Will run the `cli_hello_args` Bevy system and pass in `vec!["world"]` as
the `In` parameter.

---

```
dothing
```

Error: command not found.

(because no *noargs* command with the name `dothing` exists,
it doesn't matter that an *args* command with that name exists)

---

```
dothing 10.5
```

Will run the `cli_dothing` Bevy system and pass in `vec!["10.5"]` as
the `In` parameter. Note that the argument is always passed in as
a string. The Rust function must do any processing it wants, such
as parsing it into a `f32` number, if that is what it expects.
