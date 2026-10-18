# Contributing to The Seeker

Thank you for helping. Code, art, audio, level plans, documentation, bug reports, and playtesting are all welcome, whether you work by hand or with an AI coding agent.

## Set up

1. Install the latest stable [Rust](https://rustup.rs/) and [Git LFS](https://git-lfs.com/). Images, audio, fonts, and LDtk projects are stored in Git LFS, so run `git lfs install` once before cloning.
2. On Linux, install the audio, input, and windowing libraries:

   ```sh
   sudo apt-get install pkg-config libx11-dev libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev
   ```

3. Clone and run the game from the repository root:

   ```sh
   git clone https://github.com/TheSeekerGame/the-seeker.git
   cd TheSeeker
   cargo run
   ```

The first build compiles Bevy and takes a while; later builds are incremental. The asset and level tools under `tools/` need Python 3.11 or newer. Their setup is described in [Level Generation](docs/architecture/level-generation.md).

## Find your way around

- [README.md](README.md) introduces the game and the repository layout.
- [docs/](docs/README.md) holds the game design vision, the architecture of each major system, and the asset authoring guides.
- [AGENTS.md](AGENTS.md) maps every subsystem to its source files and lists the runtime rules a change must preserve. Read the relevant section before changing a subsystem; it is written for AI agents but is the fastest orientation for people too.

## Make a change

- Keep each pull request focused on one topic. Open an issue or a draft pull request first to discuss larger changes.
- Preserve gameplay tuning, movement units, timing, collision behavior, and asset bindings unless changing them is the point of your change. A refactor must not quietly rebalance the game.
- Keep generic facilities in `engine/` and game rules, enemies, abilities, and combat policy in `game/`.
- Match the style of the surrounding code. Do not run `cargo fmt`, and turn off format-on-save for this repository: the codebase is not formatted with rustfmt, so it would rewrite unrelated code.
- Update the documentation in the same pull request: anything in `README.md`, `CONTRIBUTING.md`, `AGENTS.md`, or `docs/` that your change makes inaccurate or outdated, including the [AGENTS.md](AGENTS.md) source map when you add, move, or remove files under `game/` or `engine/`.
- Follow the [asset guidelines](docs/authoring/assets.md) for new art and audio.

## Verify

Run these from the repository root before opening a pull request:

```sh
cargo check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The build denies warnings, and continuous integration runs the same Clippy and test commands on every pull request. Use development builds (no `--release`) for testing. Tests cover simulation logic, but not visuals or game feel, so play the game for anything you can see or hear, and say in your pull request what you played.

## Submit a pull request

1. Fork the repository and create a branch for your change.
2. Open a pull request against `main`. Draft pull requests are welcome for early feedback.
3. Describe what changed and why, which checks you ran, and whether you playtested.

Maintainers review and merge pull requests. Mark a draft as ready for review when you want it merged.

## Working with AI agents

[AGENTS.md](AGENTS.md) is the instruction file for AI coding agents; most agents read it automatically, and [CLAUDE.md](CLAUDE.md) points Claude Code to it. The same rules apply to generated changes: run the checks above, review the diff yourself, and state in the pull request what was verified and what was not.

## Report bugs and ideas

Open a [GitHub issue](https://github.com/TheSeekerGame/the-seeker/issues). For bugs, include your operating system, the steps that reproduce the problem, and any error output from the terminal.

## Licensing

See [LICENSE.md](LICENSE.md) for the code, asset, and font licenses and the terms that apply to asset contributions.
