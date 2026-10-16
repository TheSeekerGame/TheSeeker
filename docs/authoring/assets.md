# Assets

The `assets/` folder holds files exactly as the game loads them. The game preloads every asset during the startup loading screen.

## Git LFS

PNG, FLAC, TTF, LDtk, and KTX2 files are stored with [Git LFS](https://docs.github.com/en/repositories/working-with-files/managing-large-files/configuring-git-large-file-storage), as configured in `.gitattributes`. Install Git LFS (`git lfs install`) before cloning. Without it, these files are small text pointers, and the game and its tests cannot read them.

## How the game finds assets

| Assets | Location | How they are found |
| --- | --- | --- |
| Animations | `assets/animations/**/*.anim.toml` | Recursive scan; each file's `[asset]` block declares its key and sprite sheet. See [Animations](animations.md). |
| Enemy archetypes, actions, projectiles | `assets/enemies/archetypes/*.arch.toml`, `assets/actions/*.action.toml`, `assets/projectiles/*.projectile.toml` | Scan of each folder (not recursive); the key is the prefix plus the file stem, such as `arch.bat_big` |
| Audio, shared attack descriptors, HUD artwork | `assets/audio.assets.ron`, `assets/gameplay.assets.ron`, `assets/ui.assets.ron` | [Dynamic asset](https://github.com/NiklasEi/bevy_asset_loader#dynamic-assets) registries mapping keys to files |
| Level plans, chunks, secret rooms, tiles, surface art, and shipped backgrounds | `assets/levels/` | Read by level generation; see [Level Generation](../architecture/level-generation.md) |
| Item and skill icons, fonts, effect textures, shaders | `assets/items/`, `assets/ui/`, `assets/font/`, `assets/fx/`, `assets/shaders/` | Loaded by path from Rust code |

Each RON registry starts with a comment describing what belongs in it. When you add an asset, follow the convention of its folder rather than adding a registry entry everywhere. When you remove one, search `game/src`, `engine/src`, and `assets/` for its key and path.

## Source files

Source artwork that the game does not load lives under `tools/`, next to the scripts that turn it into game assets: `tools/source_art/` for sprites and effects and `tools/backgrounds/source/` for background exemplars and LDtk definitions.

## Audio

All audio assets must be in FLAC format. Encode for maximum compression:

```sh
flac --best <file.wav>
```

## Images

### Art assets

Store images as PNG and optimize them for size. Before adding new files, install `optipng` and `zopflipng` and process each file:

```sh
optipng -i 0 -strip all -zc1-9 -zm1-9 -zs0-3 -f0-5 <file.png>
zopflipng -y -m --lossy_transparent --filters=01234mep <file.png> <file.png>
```

With `fd`, you can process every PNG in a folder using all CPU cores:

```sh
cd folder;
fd -e png -x optipng -i 0 -strip all -zc1-9 -zm1-9 -zs0-3 -f0-5 {} \; -x zopflipng -y -m --lossy_transparent {} {}
```

Animation sheets can carry magenta collider masks that define combat geometry, and their filenames encode the atlas layout; see [Animations](animations.md). Check changed sheets in game.

### Special GPU textures

If a texture needs GPU features that PNG cannot provide, use KTX2 with zstd at level 19:

```sh
toktx <other_options> --zcmp 19 <file.ktx2>
```
