# Authoring

This section explains how to add content to the game and documents the asset formats the engine reads.

- [Assets](assets.md): where assets live, how the game finds them, and the file formats and optimizations to use.
- [Scripts](scripts.md): register script assets and control playback from Rust.
    - [Script Format Reference](script-reference.md): triggers, actions, filters, and lifetime rules.
- [Animations](animations.md): register sprite sheets and connect playback to gameplay.
    - [Animation Format Reference](animation-reference.md): frame cadence, clip sections, cues, and sprite actions.
- [Music and Tuning](music.md): the 48 Hz tuning rule, the Seeker tuning file, and composing on the shared beat.

[Timing, Audio, and Animation](../architecture/timing-and-audio.md) describes the shared transport and delayed presentation that scripts and animations run on. [Actors and Actions](../architecture/actors.md) describes execution ownership and game mechanics. Level plans, chunk templates, and generation rules are documented in [Level Generation](../architecture/level-generation.md).

Keep API details alongside the implementation in Rust doc comments. Keep these references consistent with both the asset schema and its runtime behavior; a parsed field is not necessarily an implemented feature.
