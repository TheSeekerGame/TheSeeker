# Architecture

These documents explain how the major systems are organized and which contracts their implementation preserves. Each is the single design document for its subsystem: extend the relevant page instead of adding an overlapping one.

| Document | Covers |
| --- | --- |
| [Actors and Actions](actors.md) | Enemy and player state machines: perception, controllers, shared actions, motors, presentation ownership, items, and cleanup |
| [Timing, Audio, and Animation](timing-and-audio.md) | The shared musical timeline, the 96 Hz simulation, sample-accurate audio scheduling, animation cadence, delayed presentation, and the timing probe |
| [Level Generation](level-generation.md) | PNG level plans, chunk assembly, mosaics, secret rooms, adaptations, baked surfaces and lighting, backgrounds, and the offline generation tools |

For a file-by-file map of the source and the runtime rules that every change must keep, see [AGENTS.md](https://github.com/TheSeekerGame/TheSeeker/blob/main/AGENTS.md).
