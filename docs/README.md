# The Seeker documentation

These pages describe the design and the technical architecture of The Seeker. The same Markdown files render on GitHub and build into the [documentation site](https://theseekergame.github.io/doc/). API documentation for the Rust crates is published at [theseekergame.github.io/api](https://theseekergame.github.io/api/theseeker_game/).

| Section | Read it for |
| --- | --- |
| [Game Design](design/README.md) | The vision: audience, art direction, lore, monetization, and intended mechanics |
| [Architecture](architecture/README.md) | How the game works: actor behavior, the timing and audio model, and level generation |
| [Authoring](authoring/README.md) | How to add content: asset guidelines and the script and animation formats |

The source code and authored assets define current behavior. Design pages record intent, and architecture pages explain the contracts the implementation preserves. When a page and the code disagree, trust the code and fix the page.

Project-wide guides live in the repository root:

- [README.md](https://github.com/TheSeekerGame/TheSeeker/blob/main/README.md): what the game is and how to build and run it.
- [CONTRIBUTING.md](https://github.com/TheSeekerGame/TheSeeker/blob/main/CONTRIBUTING.md): how to set up, verify, and submit changes.
- [AGENTS.md](https://github.com/TheSeekerGame/TheSeeker/blob/main/AGENTS.md): the source map and the runtime rules every change must preserve. It is written for AI coding agents and is equally useful to people.

## Build the site locally

Install [mdBook](https://rust-lang.github.io/mdBook/) 0.4 (`cargo install mdbook --version 0.4.37`) and run from the repository root:

```sh
mdbook serve docs --open
```

`mdbook build docs` writes the static site to `docs/book/`, which is not committed. Pushes to `main` that change `docs/` publish the site automatically.

## Writing documentation

- Write plain GitHub-flavored Markdown that reads well both on GitHub and in the book.
- Link between pages with relative `.md` links. Refer to source files by their repository path in backticks, such as `game/src/levelgen/mod.rs`. Relative links that leave `docs/` break in the built site; link root files through their GitHub URL.
- Add every new page to [SUMMARY.md](SUMMARY.md), which is the book's table of contents.
- Describe current behavior as fact. Leave history to Git rather than writing migration notes.
