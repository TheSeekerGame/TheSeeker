# The Seeker

The Seeker is an open-source 2D metroidvania with action-RPG builds, written in Rust with [Bevy](https://bevyengine.org/) 0.19. A settler lands on a frozen planet whose life survives in deep, geothermally heated caves. The game is about exploring those caves out of curiosity, crafting skills and passives at capsules from materials gathered there, and combining them into builds. A procedurally generated world and a soundtrack, animation, and gameplay clock that share one musical timeline round it out.

The game is in active development. Read the [design vision](docs/design/README.md) for where it is heading.

## Run the game

You need the latest stable [Rust](https://rustup.rs/) and [Git LFS](https://git-lfs.com/), because images, audio, fonts, and level projects are stored in LFS:

```sh
git lfs install
git clone https://github.com/TheSeekerGame/TheSeeker.git
cd TheSeeker
cargo run
```

Linux also needs `pkg-config libx11-dev libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev`. The first build compiles Bevy and takes a while.

| Action | Keyboard | Gamepad |
| --- | --- | --- |
| Move | A / D or arrow keys | Left stick |
| Jump | W, Up, or Space | L2 |
| Drop down | S or Down | D-pad down |
| Skills | J K L ; ' or 1–5 | X, R2, A, B |
| Interact | F | Y |
| Inventory | M | |
| Controls reference | C | Start |
| Menu / pause | Esc / P | |

## Repository layout

| Path | Contents |
| --- | --- |
| `game/` | The game binary (`theseeker_game`): game rules, player, enemies, world generation, UI, and rendering |
| `engine/` | The `theseeker_engine` library: generic timing, audio, playback, assets, collision, and effects |
| `assets/` | Everything the game loads: animations, audio, levels, actors, items, shaders, and UI art |
| `tools/` | Python tools that generate levels, backgrounds, and effect artwork, with their source art |
| `docs/` | Design, architecture, and authoring documentation |

The root `Cargo.toml` defines a single package containing both the library and the binary, so run Cargo commands from the repository root.

## Documentation

- [Documentation index](docs/README.md), also published at [theseekergame.github.io/doc](https://theseekergame.github.io/doc/)
- [Rust API documentation](https://theseekergame.github.io/api/theseeker_game/)
- [AGENTS.md](AGENTS.md): the source map and runtime rules for contributors and AI coding agents

## Contributing

Contributions of every kind are welcome. Start with [CONTRIBUTING.md](CONTRIBUTING.md), which covers setup, verification, and pull requests.

## License

Code is licensed under MIT OR Apache-2.0. Project assets are licensed under Creative Commons Attribution 4.0 (CC BY 4.0) unless noted otherwise, and fonts under the SIL Open Font License 1.1. See [LICENSE.md](LICENSE.md) for details and attribution guidance.
