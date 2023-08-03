# Assets Organization

This page describes our practices for the game's asset files.

## Source Files

If there are any "source" files (project files from the artist's workflow, when
the artist works in formats other than what the game ingests), they can be kept
in the `assets-src` folder, and must be added via [Git
LFS](https://docs.github.com/en/repositories/working-with-files/managing-large-files/configuring-git-large-file-storage).

This is optional and up to `c12` and artists. They should decide what files we
want to officially store with the GH repo.

## General Organization

The `assets` folder contains the files as they should be ingested by Bevy. This
is what the game loads and uses.

All the assets to be loaded by the game must be listed/declared in the `*.assets.ron`
[dynamic asset](https://github.com/NiklasEi/bevy_asset_loader#dynamic-assets) files.

Please look at those files. Each one has comments describing how it is supposed to be
used.

Currently, the game pre-loads all assets during the startup loading screen. In
the future, assets specific to a given level/area will be loaded dynamically
during gameplay.

## Audio

All audio assets must be in FLAC format. Should be encoded for maximum compression, like:

```sh
flac --best <file.wav>
```

## Images

### (most) Art Assets

Images should be stored as PNG and aggressively optimized for size. If you are going to
add new files to the repo, please install `optipng` and `zopflipng` on your system,
and run the following commands to process the files:

```sh
optipng -i 0 -strip all -zc1-9 -zm1-9 -zs0-3 -f0-5 <file.png>
zopflipng -y -m --lossy_transparent --filters=01234mep <file.png> <file.png>
```

If you have `fd`, you can easily run these commands on all PNG files, using all your
CPU cores for processing:

```sh
cd folder;
fd -e png -x optipng -i 0 -strip all -zc1-9 -zm1-9 -zs0-3 -f0-5 {} \; -x zopflipng -y -m --lossy_transparent {} {}
```

### Special GPU textures

If we need specific GPU Texture features that cannot be used via PNG, then KTX2+zstd
is the format that should be used. Compress at level 19:

```sh
toktx <other_options> --zcmp 19 <file.ktx2>
```
