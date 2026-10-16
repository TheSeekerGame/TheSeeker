# Project guidance for agents

The Seeker is a 2D metroidvania built with Rust and Bevy 0.19.1. The root `Cargo.toml` defines one package with the `theseeker_engine` library at `engine/src/lib.rs` and the `theseeker_game` binary at `game/src/main.rs`. Run Cargo commands from the repository root; `game/` and `engine/` do not have separate manifests.

Use this file for project constraints and navigation. Source code and authored assets establish current behavior; the documents under `docs/` explain intent and architecture. Verify the relevant implementation before changing a subsystem. `CLAUDE.md` imports this file for Claude Code; [CONTRIBUTING.md](CONTRIBUTING.md) is the human-facing summary of the same workflow.

## Working rules

- **Never run `cargo fmt`. Never use `--release` for testing or verification.** The codebase is not formatted with rustfmt, so running it rewrites unrelated code. Format changed code manually to match its surroundings.
- Preserve gameplay tuning, movement units, tick boundaries, collision policy, and asset bindings unless the task authorizes changing them. A refactor must not quietly rebalance the game.
- Keep engine facilities generic; put game rules, enemy definitions, player abilities, and combat policy in the game binary. Reuse existing mechanics and helpers before adding another execution path.
- Favor clear names, small functions, shallow nesting, and explicit ownership. Use comments to explain constraints or intent that the code cannot express clearly.
- Describe current behavior as a timeless fact in code comments and documentation. Do not write migration narratives or references to previous implementations.
- Resolve routine implementation choices from the code and the user's instructions. Ask when a missing design decision materially changes the requested behavior; continue independent work while clarifying.
- Preserve unrelated working files. Investigate both sides of subsystem contracts: producer and consumer, collision memberships and filters, requested and committed playback, reserved and consumed inventory.
- Keep this guide's source map and invariants accurate when adding, removing, moving, or significantly changing files under `game/` or `engine/`. Update the relevant subsystem entry; add a specific entry for a new responsibility or important entry point. Do not grow an exhaustive list of every helper and content module.
- Keep `docs/` accurate when a change alters behavior a page describes. See [Documentation](#documentation) for where each kind of document belongs.

## Build and verification

```sh
cargo run
cargo check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

`cargo run` launches the desktop game with graphics and audio. Headless tests are useful for simulation logic and schedule initialization, but do not establish visual correctness or gameplay feel. Report which checks ran and whether an interactive playtest was performed.

- PNG, FLAC, TTF, LDtk, and KTX2 files are Git LFS objects (`.gitattributes`). Tests read level rules and artwork from `assets/`, so they fail on LFS pointer files; run `git lfs pull` if assets look like small text files.
- `.github/workflows/rust-quality.yml` runs the Clippy gate and `cargo test --workspace` on every pull request. `deploy-mdbook.yml` publishes `docs/` and `deploy-rustdoc.yml` publishes the API documentation on pushes to `main`.
- `.cargo/config.toml` uses `clippy-driver` as the workspace compiler wrapper. Ordinary Cargo builds also enforce lint gates; the explicit all-targets Clippy command remains required for substantive Rust changes.
- Bevy uses an explicit Cargo feature list for the desktop 2D game. Keep scene support, gizmos, text/default font, gamepads, audio assets, and threading; `DefaultPlugins` follows that list. Hanabi enables 2D rendering and serialized effect assets. Check transitive feature activation with `cargo tree -e features` when changing dependency features.
- `Cargo.toml` denies warnings and unsafe code. `clippy.toml` sets nesting to 3 and cognitive complexity to 50. Do not bypass or weaken the gate. A local `#[allow(...)]` is a last resort and requires a concrete `reason`.
- Run focused tests for changed behavior, `cargo check`, and the required Clippy gate. Run the workspace suite for changes spanning systems, scheduling, assets, or shared interfaces.
- Documentation-only edits and isolated, low-risk tuning or presentation edits do not require Rust tests or compilation. Validate links, paths, and facts for documentation; use normal gates whenever control flow, types, interfaces, or subsystem interactions change.
- Do not add tests that merely repeat the implementation. Test observable timing, cancellation, ownership, collision, inventory accounting, or another meaningful contract.
- A clean build does not prove a module is used. The game binary permits dead code for data-driven components, prototypes, and optional plugins. Inspect plugin wiring, queries, asset references, and call sites before declaring code stale.

## Explore without exhausting context

Start with the relevant source-map entry, its plugin registration, and the specific types or functions involved. Use bounded searches and reads:

```sh
rg --files game/src/game/player
rg -n 'ForegroundAction|transition_action|finish_action' game/src/game/player
rg -n 'register_animation_asset|GENERATED_GAMEPLAY_ASSET_DIRS' game/src/assets.rs
```

Avoid dumping large modules, asset registries, or generated data. In particular, `game/src/levelgen/mod.rs`, several UI modules, animation assets, and the shipped background banks under `assets/levels/background_bank/` deserve targeted reads. Scope searches to `game/src`, `engine/src`, `docs`, or the relevant asset directory rather than recursively searching build output (`target/`) or Python environments (`tools/backgrounds/.venv/`).

## Source map

Paths in this guide, including Markdown links, are relative to the repository root. This map lists responsibilities and useful entry points; use `rg --files` for a complete inventory.

```text
Cargo.toml                  Package targets, dependencies, profiles, and lint policy
Cargo.lock                  Locked dependency versions (committed; the package is an application)
.cargo/config.toml          Cargo compiler wrapper
clippy.toml                 Complexity and nesting thresholds
build.rs, game/macos/       macOS Info.plist embedding for the game binary
.github/workflows/          Continuous integration and documentation publishing
engine/src/                 Generic playback, timing, assets, audio, collision, and effects
game/src/                   Game rules, player/enemy integration, world generation, UI, graphics
assets/                     Authored animations, images, audio, levels, and gameplay definitions
tools/                      Python asset and level-generation tools and their source art
docs/                       Design vision, architecture, and authoring guides (GitHub Markdown and mdBook)
README.md, CONTRIBUTING.md  Project introduction and contributor workflow
LICENSE.md, LICENSE-*       Code, asset, and font licensing
```

### App, engine, and shared gameplay

| Responsibility | Start here |
| --- | --- |
| App/plugin wiring and preludes | `game/src/main.rs`, `game/src/game.rs`, `engine/src/lib.rs` |
| App screens and pause state | `game/src/appstate.rs`, `game/src/gamestate.rs`, `game/src/screens/loading.rs`, `game/src/ui/mainmenu.rs`, `game/src/game/game_over.rs` |
| Asset registration and preloading | `game/src/assets.rs`, `engine/src/assets.rs` |
| Musical transport and fixed-tick input | `engine/src/time.rs`, `engine/src/input.rs`, `engine/src/audio/transport.rs`, [timing-and-audio.md](docs/architecture/timing-and-audio.md) |
| Prepared audio, sample scheduling, and CPAL device clock | `engine/src/audio.rs`, `engine/src/audio/custom.rs`, `engine/src/audio/mixer.rs`, `engine/src/audio/device.rs` |
| Soundtrack commands, transitions, and loop metadata | `game/src/music.rs`, `assets/audio/music/tracks.music.toml` |
| Authoritative poses and timestamped render proxies | `engine/src/gent.rs`, `engine/src/presentation.rs`, `game/src/graphics/particle_presentation.rs` |
| Generic afterimages and sprite stretching | `engine/src/effects/ghosting.rs`, `engine/src/effects/sprite_stretch.rs`, `engine/src/effects/scale_recovery.rs` |
| Playback, script slots, and presentation ownership | `engine/src/script.rs`, `engine/src/script/common.rs`, `engine/src/animation.rs`, `engine/src/animation/timeline.rs` |
| Script, animation, attack, projectile, and config formats | `engine/src/assets/script.rs`, `engine/src/assets/animation.rs`, `engine/src/assets/attack.rs`, `engine/src/assets/projectile.rs`, `engine/src/assets/config.rs` |
| Collider shapes, spatial queries, sprite colliders, and groups | `engine/src/physics.rs`, `engine/src/physics/geometry.rs`, `engine/src/assets/sprite_colliders.rs` (colliders from animation masks) |
| Tile collision, movement resolution, probes, and ray casts | `engine/src/physics/tiles.rs`, `game/src/game/physics/tile_collision_debug.rs` (debug overlay) |
| Game movement curves, knockback, and projectile integration | `game/src/game/physics/mod.rs`, `game/src/game/physics/projectile.rs` |
| Damage targeting, resolution, hit limits, and cleanup | `game/src/game/combat/mod.rs`, `game/src/game/combat/damage_pipeline.rs`, `game/src/game/combat/damage_source.rs`, `game/src/game/combat/effects.rs` |
| Attack and projectile descriptor use | `game/src/game/enemy/actor_descriptors.rs` (enemies), `WeaponAttack` in `game/src/game/player/weapon.rs` (player melee), `game/src/game/actors/projectile.rs` (shared projectile spawning) |
| Blood, sparks, and actor status effects | `game/src/game/combat/blood.rs`, `game/src/game/combat/sparks.rs`, `game/src/game/effects/` |
| Effect quad geometry and empty-mesh asset publication | `game/src/graphics/quad_mesh.rs` |
| Timing probe, presentation comparison, and main/render frame measurements | `game/src/timing_probe.rs`, `game/src/frame_metrics.rs` |
| Profiling scopes (`TickProfiler::scope`) and schedule validation | `engine/src/profile.rs`, `game/src/game/tick_profile.rs`, `game/src/game/schedule_tests.rs` |

### Actors and the player

| Responsibility | Start here |
| --- | --- |
| Enemy definitions, resolution, and shared mechanics | `game/src/game/actors/definition.rs`, `game/src/game/actors/catalog.rs`, `game/src/game/actors/action.rs` |
| Enemy decisions and perception | `game/src/game/actors/controller.rs`, `game/src/game/actors/hopper.rs`, `game/src/game/actors/observations.rs`, `game/src/game/actors/sensors.rs` |
| Shared movement, projectiles, dives, and procedural strikes | `game/src/game/actors/motion.rs`, `game/src/game/actors/projectile.rs`, `game/src/game/actors/lunge.rs`, `game/src/game/actors/tick_strike.rs` |
| Enemy assembly, distribution, and system ordering | `game/src/game/enemy/mod.rs`, `game/src/game/enemy/actor_setup.rs`, `game/src/game/enemy/spawn.rs` |
| Enemy action/body integration | `game/src/game/enemy/actuation.rs`, `game/src/game/enemy/actor_descriptors.rs`, `game/src/game/enemy/ground_motor.rs`, `game/src/game/enemy/flying_motor.rs`, `game/src/game/enemy/jump_motor.rs` |
| Tentacle pose and trap behavior | `game/src/game/enemy/tentacle.rs`, `game/src/game/enemy/trap_controller.rs` |
| Enemy collision, contact damage, death, corpse loot, and projectile lifetime | `game/src/game/enemy/collision.rs`, `game/src/game/enemy/contact_damage.rs`, `game/src/game/enemy/death.rs`, `game/src/game/enemy/corpse_loot.rs`, `game/src/game/enemy/projectiles.rs` |
| Player assembly and tick stages | `game/src/game/player/spawn.rs`, `game/src/game/player/orchestration.rs` |
| Player input, buffering, facing, and sensors | `game/src/game/player/player_action.rs`, `game/src/game/player/input_buffer.rs`, `game/src/game/player/autoaim.rs`, `game/src/game/player/sensors.rs` |
| Physically supported recovery anchors and spike recovery lifetime | `game/src/game/player/sensors.rs` (`SafeGroundedPosition`, `RecoveryCollisionWorld`), `game/src/game/spikes.rs`, `game/src/game/spikes/tests.rs` |
| Player foreground lifetime and typed queries | `game/src/game/player/foreground.rs` |
| Skill admission, metadata, cooldowns, and channels | `game/src/game/player/skills/activation.rs`, `game/src/game/player/skills/registry.rs`, `game/src/game/player/skills/types.rs`, `game/src/game/player/skills/cooldowns.rs`, `game/src/game/player/skills/channeled.rs` |
| Instant items and animated deployment | `game/src/game/player/skills/instant.rs`, `game/src/game/player/skills/item_use.rs`, individual modules under `game/src/game/player/skills/`, object systems under `game/src/game/player/spawns/` |
| Locomotion and foreground mechanic execution | `game/src/game/player/states/mod.rs`, `game/src/game/player/states/locomotion/`, `game/src/game/player/states/skill/`, `game/src/game/player/states/elevator.rs` |
| Copied spider shot and bat dive bindings | `game/src/game/player/spawns/spit.rs`, `game/src/game/player/states/skill/dash_striking.rs`, `assets/actions/player_spit.action.toml`, `assets/actions/player_dive.action.toml` |
| Player collision, presentation, weapons, and equipment | `game/src/game/player/collision.rs`, `game/src/game/player/collision_view.rs`, `game/src/game/player/player_anim.rs`, `game/src/game/player/dust.rs`, `game/src/game/player/weapon.rs`, `game/src/game/player/equipment.rs` |
| Player stats, passives, and critical hits | `game/src/game/player/stats.rs`, `game/src/game/player/passives/mod.rs` (`PassiveOwner`, what passives read about their owner), `game/src/game/player/passives/definition.rs`, `game/src/game/player/passives/evaluation.rs`, `game/src/game/player/passives/runtime.rs`, individual passive modules beside them, `game/src/game/player/crits.rs` |
| Inventory and progression | `game/src/game/player/mod.rs`, `game/src/game/player/material_inventory.rs`, `game/src/game/player/progression.rs` |
| Placement previews, tent interiors, and tether network | `game/src/game/player/skills/placement_preview.rs`, `game/src/game/player/skills/portable_tent.rs`, `game/src/game/player/skills/tether_node.rs`, `game/src/game/player/spawns/tether_node.rs` |

### World, graphics, and UI

| Responsibility | Start here |
| --- | --- |
| Level selection, explicit build lifetime, and plugin wiring | `game/src/levelgen/mod.rs`, `game/src/levelgen/pipeline.rs`, `game/src/levelgen/publication.rs` |
| Shared palette compilation, depth biomes, and footprint-aware template assignment | `game/src/levelgen/compiler.rs`, `game/src/levelgen/mapgen.rs`, `tools/generate_demo_map.py` |
| Chunk-plan generation, connected cave geometry, and traversal validation | `tools/generate_level_chunks.py`, `tools/large_level_chunks.py` (broad encounter floors, wall-supported climbs, fragment and sealed-pocket cleanup) |
| Generation assets, stable mosaics, and spatial placement | `game/src/levelgen/catalog.rs`, `game/src/levelgen/mosaic.rs`, `game/src/levelgen/placement.rs` (`place_level`), `game/src/levelgen/spatial.rs` |
| Deterministic generation rolls (a seed reproduces its level only while these stay unchanged) | `game/src/levelgen/hash.rs` |
| Regional terrain and shading | `game/src/levelgen/terrain.rs`, `game/src/levelgen/surface.rs`, `game/src/levelgen/ground_shading.rs`, `game/src/levelgen/distance.rs` |
| Near background and shared pixel operations | `game/src/levelgen/background.rs`, `game/src/levelgen/background_openings.rs` (terrain-driven openings), `game/src/levelgen/background_synthesis.rs` (detailing copied from the artist's exemplar), `game/src/levelgen/background_surface.rs`, `game/src/levelgen/image_ops.rs` |
| Shipped middle/far background generation and loading | `tools/backgrounds/generate.py`, `tools/backgrounds/openings.py`, `tools/backgrounds/synthesis.py`, `tools/backgrounds/exemplars.py`, `game/src/levelgen/background_bank.rs`, [level-generation.md](docs/architecture/level-generation.md) |
| Authored world spawns | `game/src/levelgen/world_spawn.rs` |
| LDtk auto-tile rules | `game/src/levelgen/ldtk_rules.rs` |
| Harvesting, breakable walls, secret rooms and their covers, and optional spikes | `game/src/game/spikes.rs` (spike hits and respawn), `game/src/levelgen/harvestable_materials.rs`, `game/src/levelgen/breakable_wall.rs`, `game/src/levelgen/secret_rooms.rs`, `game/src/levelgen/secret_covers.rs`, `game/src/levelgen/optional_spikes.rs`, `game/src/levelgen/adaptation.rs` |
| Foliage rendering and interaction | `game/src/levelgen/foliage.rs` |
| Capsules, shelter entry, and capsule crafting and research | `game/src/game/world_objects.rs`, `game/src/game/interaction.rs`, `game/src/game/capsule_commerce.rs` |
| Capsule collision, trigger, and light geometry | `game/src/game/world_objects/capsule_shape.rs` |
| Damage-spark, foreground-fog, and moss-bat artwork generators | `tools/generate_damage_sparks.py` (from `tools/source_art/damage_sparks/`), `tools/generate_foreground_fog.py`, `tools/generate_moss_bat.py` (with `tools/source_art/moss_bat/`) |
| Landing cinematic, surface backdrop, and pre-impact crater | `game/src/game/intro.rs`, `game/src/levelgen/surface_backdrop.rs`, `game/src/levelgen/impact_crater.rs`, `tools/surface_art/generate.py` |
| NPC behavior and proximity | `game/src/game/npc.rs`, `game/src/game/interaction.rs`, `game/src/game/yak.rs` |
| Camera, pixel scale, interpolation, and parallax | `game/src/camera.rs` (cameras, placement, shake), `game/src/camera/scale.rs` (pixel scale and letterboxed viewport), `game/src/camera/framing.rs` (what the camera frames, the follow rig, and the open-space search), `game/src/graphics/pixel_view.rs`, `game/src/graphics/pixel_perfect.rs`, `game/src/parallax.rs` |
| Render pass scheduling, fullscreen setup, and quad meshes | `game/src/graphics/mod.rs`, `game/src/graphics/fullscreen.rs`, `game/src/graphics/quad_mesh.rs` |
| Particle presentation, circular presets, projectile trails, and scatter/homing motion | `game/src/graphics/particle_presentation.rs`, `game/src/graphics/particles_util.rs`, `game/src/graphics/projectile_particles.rs`, `game/src/game/player/scatter_homing.rs` |
| Falling snow, parallax, and background blur | `game/src/graphics/snow.rs`, `game/src/parallax.rs`, `game/src/graphics/background_blur.rs` |
| Enemy health bars and damage numbers (off unless `DAMAGE_NUMBERS_ENABLED`) | `game/src/graphics/enemy_hp.rs`, `game/src/graphics/dmg_numbers.rs` |
| Darkness, capsule dimming, tether-responsive fog, ripple, and vignette | `game/src/graphics/post_processing/`, `game/src/graphics/ripple.rs` |
| UI wiring, UI scale, and gameplay input suppression under open panels | `game/src/ui.rs` |
| Controls reference | `game/src/ui/controls_overlay.rs` |
| Capsule terminal, storage, and hotspot prompt | `game/src/ui/capsule_ui.rs`, `game/src/ui/capsule_ui/panels.rs` |
| Player inventory panel | `game/src/ui/inventory.rs` |
| Skill toolbar, health and Alignment bars, interact prompt, and level-up captions | `game/src/ui/skill_toolbar.rs` |
| CPU-drawn pixel panels, shared panel look and keys, and bitmap font metrics | `game/src/ui/pixel_canvas.rs`, `game/src/ui/pixel_panel.rs`, `game/src/ui/pixel_font.rs` |
| Alignment screen frame and pulse, Essence HUD, and the low-health vignette | `game/src/ui/screen_frame.rs`, `game/src/ui/planet_status.rs`, `game/src/graphics/player_hp.rs` |

## Runtime contracts

### Scheduling and entity lifetime

- Gameplay runs on the custom 96 Hz `GameTickPre` → `GameTickUpdate` → `GameTickPost` schedules. Use `GameTime` and fixed-tick input for simulation; do not substitute render-frame delta time or Bevy's default fixed schedule.
- `AppState` is `AssetsLoading`, `MainMenu`, `InGame`, or `Restart`; `GameState` separately controls `Playing`/`Paused`. Match existing state gates when registering systems. `StateDespawnMarker` scopes entities to app-state lifetime.
- Read the relevant plugin's ordering before adding a system. Player stages are sensors → input → behavior → collisions → animation. Within behavior, admission precedes locomotion and foreground execution. Enemy stages are declared in `game/src/game/enemy/mod.rs`; damage ordering is declared in `damage_pipeline.rs`.
- `ScriptSet::Init` commits playback requests; `ScriptSet::Run` advances playback; `AnimationSet::Signals` publishes events. Preserve the producer/consumer boundary and deferred-command visibility. A playback must advance at most once per simulation tick.
- `Gent` points to authoritative graphics/effects entities. Animation and `AnimationCollider` read their simulation `Sprite`; `PresentationProxy` entities display bounded tick histories and never own gameplay. `TransformGfxFromGent` copies authoritative transforms in `GameTickPost`; rendering interpolates historical motion and applies pixel alignment independently. Mark discrete motion with `PresentationJump(tick)` and unparented equipment with `PresentationAnchor`.
- Account for despawn/replacement between queuing and applying commands. Clean up owned entities without destroying independently living effects. When parenting visible children, ensure the parent hierarchy has the necessary transform and visibility components.

### Actions and animation ownership

The single high-level design document is [actors.md](docs/architecture/actors.md). Keep implementation guidance here brief and update that document when the architectural intent changes; do not create overlapping state-machine specs or progress documents.

- Controllers choose actions; shared mechanics execute them. Sprite and procedural actors share lifecycle rules without requiring every body to have a sprite timeline.
- Player mechanic values live in `ForegroundAction`. Use `transition_action`/`start_action`, execution-scoped `finish_action`, and `ActionRef<T>`/`ActionMut<T>` queries. `ActionTag<T>` is an index, not independent state storage. Do not insert skill runtime values as standalone components or mutate query tags directly.
- Use `playback_id()` for the foreground's effective playback, including shared-action phase changes. Consume only events for the owned execution; do not infer completion or release by comparing animation keys or displayed frames.
- Locomotion requests background presentation while foreground actions own their outputs. Use scoped presentation and slot APIs; do not add skill-specific Idle/Run restoration or expanding exclusion lists.
- `PlayerGfx::base_offset` owns the resting sprite anchor. Releasing an execution restores that anchor only when the execution still owns presentation; temporary action offsets do not become the resting offset.
- Temporary melee hitboxes and overrides belong to their execution. Launched projectiles and deployed objects have independent lifetimes. Preserve each actor's hit-stop, freeze, knockback, and facing rules.
- Instant uses do not occupy the foreground slot. Animated deployment reserves backpack stock at activation, validates placement again at the release cue, consumes once on release, and refunds a pending reservation on interruption. The deployment runtime is available for content bindings; current instant items do not automatically gain deployment animations.
- Copied actions may have different motion, timing, targeting, damage, and presentation. Player down-dash uses the shared bat lunge with its own parameters and the `anim.player.DownwardDash` binding. Inspect the bound image when identifying artwork; filenames and weapon-related names alone do not establish its contents.
- Resolved actor/action definitions pin their dependencies for an execution. Catalog publication validates edits and retains the last valid catalog on failure. Runtime adoption boundaries and on-disk file watching are separate concerns; verify asset delivery when diagnosing reload behavior.

### Asset registration and animation authoring

Authoring guides and format references live in `docs/authoring/`: `assets.md`, `scripts.md`, `script-reference.md`, `animations.md`, and `animation-reference.md`. Keep examples consistent with both asset parsing and runtime behavior.

- `game/src/assets.rs` recursively scans `assets/animations/` for `.anim.toml` files. An `[asset]` block declares `key` or `keys` and an `image` path relative to `assets/`. Registration creates the animation key plus `.image` and `.atlas` keys.
- Sprite-sheet PNG filenames encode atlas layout: `Name__r1c15__90x90.png` means 1 row, 15 columns, and 90×90-pixel cells. This defines atlas capacity, not the played frame range. `engine/src/assets/animation.rs` implements the parser.
- `[settings]` defines frame bounds, start, ticks per frame, and playback direction. Optional `[playback]` and named `[sections.*]` define bounded once/loop/hold playback. `FrameId` is one-based; sprite atlas indices are zero-based.
- The game selects global animation cadence: the first frame enters immediately and the next frame waits for the next strictly later boundary of its authored period. `[settings] frame_clock = "Local"` selects local countdowns; `"Global"` selects global cadence explicitly. Common-script `tick_quant` only quantizes its script origin.
- `[[events]]` declares named timeline cues. Keep sprite action timing and frame-entry displacement close to the authored clip; procedural phases use ticks. Holding a frame does not repeat entry effects.
- Archetypes, actions, and projectiles are registered from `assets/enemies/archetypes/`, `assets/actions/`, and `assets/projectiles/` with `arch.`, `action.`, and `projectile.` prefixes. Those scans inspect files directly in each directory, unlike the recursive animation scan.
- Other dynamic keys are loaded from `assets/audio.assets.ron`, `assets/gameplay.assets.ron`, and `assets/ui.assets.ron`. Level content under `assets/levels/` is read by level generation, and icons, fonts, effect textures, and shaders are loaded by path from Rust. Check registration and references together; do not assume every asset key belongs in a RON registry. Before removing an asset, search `game/src`, `engine/src`, and `assets/` for both its key and its path.
- Magenta masks in animation images feed `SpriteShapeMap` collision geometry: `engine/src/assets/sprite_colliders.rs` covers each frame's mask's convex hull with at most five rectangles and clears the mask pixels. Changing image layout or masks can change combat geometry as well as visuals; validate both.

### Movement and collision

- Physics is custom. There are no Rapier/Avian rigid bodies to configure. `TileCollisionWorld` handles tiles; `PhysicsWorld` queries the `EntityCollisionWorld` spatial index. Prefer the canonical `groups` helpers for collision masks and verify the two-way membership/filter contract.
- Actor `LinearVelocity` values are **pixels of displacement per simulation tick**. Player and enemy motors resolve that displacement against tiles. Gravity, jump, steering, and knockback algorithms can change it each tick; do not assume all motion is constant velocity.
- The spawn director selects ordinary enemy families and spider tiers from depth and biome, independently of immunity and habitat counts. Valid sites use resolved body clearance and grounded support, with setup offsets cancelled at the blueprint boundary. Immunity and depth determine population, capped at eight per active region; maximum immunity reserves a Void slot at every depth and biome. Buzzer and Opi require tundra at depth six or deeper; Squid requires depth six. Tentacles have no director entry.
- **Projectile conventions differ.** For example, `game/src/game/physics/projectile.rs` integrates projectile velocity with `1.0 / time.hz`. Inspect the integrator before converting quantities; do not apply an actor-wide unit conversion to projectiles.
- Frame-step locomotion applies local displacement once per actual frame entry, including loops, through collision resolution. It must not become persistent velocity or accumulate blocked movement.
- Preserve sensor thresholds, collision ordering, damage attribution, hit limits, and owner validity when changing movement or combat. Do not collapse physical grounding and action-related airborne combat classification into a single inferred boolean.
- Spike recovery records `SafeGroundedPosition` after movement only when the player has physical support, no solid overlap, and no active spike contact. Recovery holds motion and admission, releases foreground ownership, and revalidates the anchor in its collision space before teleporting. An invalid anchor or initial spawn resolves to nearby safe support. Successful recovery reconciles capsule occupancy from the authored cabin envelope, including upper-deck anchors outside the entry trigger. `LastGroundedPosition` retains its separate passive/combat meaning. PNGs do not define recovery points.

### Inventory, crafting, and passives

- `MaterialInventory` separates field-accessible backpack stock from protected capsule storage. Field item use pays from the backpack. Capsule crafting may pay from both locations, taking backpack stock first.
- Player death persists storage rather than backpack stock. `PersistedPlayerProgression` in `game/src/game/player/spawn.rs` also carries skills, weapons, passives, crafting knowledge, and progression across player respawn; do not confuse it with an on-disk save system.
- `PlayerItemGrant` in `game/src/game/capsule_commerce.rs` gives crafting consistent inventory, skill, weapon, ammunition, and passive initialization. Reuse it rather than updating only a UI count. `CraftingKnowledge` there holds discovered recipes.
- `Passives::default()` starts with no equipped or unlocked passives. Passive ownership, rolls, and upgrades live in `owned`; availability and equipment are distinct. Check crafting/unlock flows rather than assuming debug unlocks describe starting inventory.
- Passive definitions keep names, descriptions, icons, roll tuning, and evaluation priority together. A rolled passive's description shows its current value in the roll's `StatUnit` in place of `{}`, so upgrades read directly. Evaluation resolves the equipped enum values into a stable, stack-sized order; actions are collected before health/cooldown mutation.
- Field payment uses `MaterialInventory::remove_from_backpack`. Capsule crafting uses `MaterialInventory::try_pay`, which pays a whole recipe or nothing, taking backpack stock before storage (`remove_combined`).
- Passive stat calculation and event processing are separate. Preserve pre-movement stat refresh and post-damage event ordering. `HitCountAdvanced { owner, hit_count }` reports advancement of the owner's hit counter; `ScheduleNextCrit` sets the owner's next-hit flag through the passive runtime. Follow the actual producer/consumer ordering when changing critical-hit timing.

### World generation and presentation

The single design and authoring document is [level-generation.md](docs/architecture/level-generation.md). Keep generation formats, architecture, tool instructions, and design goals there rather than creating overlapping guides or hand-off documents.

- `PngLevelPlugin` owns the explicit request/build/publication lifetime. Carve startup rooms before generating mosaics so inner walls receive their edge-dependent artwork. Covers and adaptations reuse those categories. Required collision, enemy, darkness, and render products must be ready before generation completes.
- Secret-room entrances must use original cave air. Covers must never show: rooms stay two cover borders from each other, optional spikes, and the intro crater, and what grows inside a room stays hidden until it opens. `secret_cover_tests.rs` checks that, whichever rooms are open, the visible art equals the level baked with only those rooms carved.
- Shipped middle/far background manifests record a fingerprint of the exemplars, `tools/backgrounds/source/`, and every non-test `tools/backgrounds/*.py` file, comments included. Editing any of them requires regenerating the banks, or `tools/backgrounds/test_generate.py` reports them stale.
- Regional output uses 96×96 tiles with operation-specific margins and global-coordinate dithering. Middle/far backgrounds are shipped pairs from the Python tool; near backgrounds are terrain-dependent. Darkness consumes CPU coverage and publishes R8 atlas pages. None of these imply bounded total world residency.
- Prepared adaptation state is independent of rendering assets. Activation uses prepared handle switches, bounded collision changes, and entity activation before simulation. Preserve spike immunity, evaluation timing, player distance, and one-way behavior.
- `PixelView` derives one camera/grid mapping after camera presentation. `RenderBand` names world (0/1), pixelated world (3), and actor overlay (2). Rasterization and position alignment remain separate: `PixelAlignedWorldMesh` compensates native pixel meshes; historical `PresentationAlignment` aligns displayed sprite proxies after interpolation and attachment correction. The composite preserves two-stage rounding and centered viewport cropping.
- Pixel-perfect rendering spans `GameplayPixelScale`, the camera, the composite pass, render layers, and UI scale. Preserve nearest sampling and integer physical-pixel alignment; do not fix one overlay by independently rounding its world anchor. The gameplay viewport holds an even count of whole game pixels on each axis, centered in the window, so the camera's center falls between pixels on the screen and on the pixel layer; the main camera clears the window outside it to black (`camera::LETTERBOX`). The UI camera shares that viewport, so pixel panels anchor within it (`UiPixelGrid::area`), not the window.
- The capsule's walls, platforms, triggers, and light silhouette are code-defined in pod-sprite pixels (`capsule_shape.rs`) and share the sprite's pixel grid, so capsule floors lie on whole pixels. A newly spawned player overlapping an inside trigger starts as the occupant; `HoldSpawnOccupancy` defers that while the landing cinematic runs. `SnapCameraNext` holds the camera on the player until they are grounded; `ScriptedCameraFocus` replaces player framing while present.
- Startup enters the generated world's landing cinematic directly after asset loading. New Game starts a fresh world and progression; custom levels bypass the cinematic. Diagnostics mode and `SEEKER_SKIP_INTRO=1` skip it. It runs on game ticks, hides the HUD (`GameplayHud` roots), player, capsule, and tether ring, and hands the camera back at the resting capsule view.
- Capsule descent requires a fresh grounded jump inside that capsule followed by takeoff and a downward crossing of the lift entrance. Intent survives the jump curve's stationary startup ticks and controls both physical hatch support and lift admission. Walking across the hatch and finishing an upward transit do not arm descent. Cancelling before takeoff, landing, entering transit, leaving the capsule, and recovery clear that intent.
- Switching between the capsule's outside and inside artwork crossfades (`CAPSULE_ART_FADE_TICKS`). `CapsuleInfluenceGrowth` spreads a capsule's visual tether ring and fog protection outward from zero; gameplay influence is unaffected.
- UI art is pixel perfect: item and skill icons are 12×12 and, like bitmap glyphs, are never rescaled. One art pixel spans a whole number of physical pixels at the gameplay scale.
- The pause menu, capsule terminal and storage, player inventory, controls reference, HUD, and interact prompt are `pixel_canvas` rasters redrawn from state each frame; each keeps one image node whose pixels are replaced only when the drawing changes. Capsule panels anchor to their hotspot's composited screen position and reserve toolbar space; inventory and controls are centered. `pixel_panel.rs` holds their shared palette, frames, wells, keycaps, prompt plate, navigation keys, and native-pixel scrolling. Compact inventory layouts and scrolling keep every selection reachable in small viewports. Empty item wells remain selectable.
- Esc opens the pixel-font menu with Resume, New Game, Load Custom Level, and Quit. Menu, inventory, and capsule panel input runs each frame in `PreUpdate`, before state transitions and simulation. `PauseRequests` combines independent manual and modal owners and pauses the transport. `ui.rs` flushes both gameplay action banks and buffered edges while a modal owns input; held actions require release before gameplay accepts them again. Test focus/navigation and input suppression together; `SEEKER_UI_SNAPSHOTS=<dir> cargo test --bin theseeker_game snapshots` writes panel PNGs.
- `pixel_font.rs` describes the variable-width `PixelFont2.png` atlas (95 printable ASCII glyphs, each including its shadow column) and wraps text. Bitmap text and Bevy `Text` coexist; use the representation required by the specific UI.
- `graphics/mod.rs` schedules the `Core2d` rendering systems: pixel composite → darkness → capsule dim → ripple → vignette. Each effect owns its uniforms, target semantics, and shader, and draws through a `fullscreen::FullscreenPass` from the view's current image into its next, within the camera's viewport. The image spans the whole window, so shaders read it at `in.position`; `in.uv` spans the viewport. Do every fallible lookup before `FullscreenPass::draw`: it swaps the view's images, so nothing may skip the draw after it. The foreground fog describes its rear and detail layers as `FogLayer` values drawn by one shader function; the tether protection mask reads the fog's protection band from its material. A uniform and its WGSL struct must match field for field; `graphics/tests.rs` composes each fullscreen shader and checks every uniform in its table (names, order, offsets, size), so add a new effect's uniform there and derive `Reflect` on it.
- Keep `bevy_post_process` enabled: its MSAA writeback preserves the composited world when the player overlay camera loads the shared target. `graphics/tests.rs::pixel_layer_reaches_the_composited_output` checks this with GPU readback through `graphics::tests::Headless`, which fails a test on any GPU validation error. In the same file, `letterboxed_view_shows_whole_layer_texels_between_black_bars` pins the composite in a letterboxed view pixel for pixel, and `every_fullscreen_pass_compiles_and_draws_in_a_letterboxed_view` builds every pass against its bind group layout. `darkness/tests.rs::darkness_eases_in_below_the_surface_then_deepens_steadily` renders the darkness curve. Run them explicitly on a host with a GPU adapter (no window or audio) with `cargo test --bin theseeker_game -- --ignored pixel_layer_reaches_the_composited_output letterboxed darkness_eases_in`.
- Dynamic effect mesh owners release their handles while geometry is empty. Publish through `quad_mesh::publish_mesh` so GPU assets retire when geometry disappears and are allocated again when it returns; do not upload zero-vertex meshes or substitute placeholder geometry.
- The skill toolbar is a `pixel_canvas` raster: its bars fill the frame art's transparent windows, a pressed icon drops its outer ring instead of scaling, and cooldown shading moves in whole rows. Stock counts are `Text` nodes over the slots, because pixel digits would cover a 12×12 icon. The toolbar is spawned on entering the game so `GameplayHud` hiding covers its first frame.
- Ghost velocity-following uses live source velocity with a spawn-velocity fallback; displacement-following uses parenting. Hierarchy despawn removes child emitter descriptors. Their render proxies survive only until the recorded death tick, without an additional particle tail.

## Focused verification and diagnostics

| Change | Useful coverage or entry point |
| --- | --- |
| Playback timing, seeks, loops, holds, interruption | `engine/src/animation/playback_tests.rs`, `engine/src/animation/timeline.rs` |
| Fixed-tick input edge retention and catch-up delivery | `engine/src/input/tests.rs` |
| Render schedule parameters, fullscreen shader composition, and uniform layouts (headless); the composited letterboxed view and every pass pipeline on the GPU | `game/src/graphics/tests.rs` |
| Darkness bake, lightmap atlas pages, and depth darkness on the GPU | `game/src/graphics/post_processing/darkness/bake_tests.rs`, `game/src/graphics/post_processing/darkness/tests.rs` |
| Sample placement, clock recovery, and delayed rendering | `engine/src/audio/mixer/tests.rs`, `engine/src/audio/transport.rs`, `engine/src/time/tests.rs`, `engine/src/presentation/tests.rs` |
| Presentation ownership and pending slots | `engine/src/script/ownership_tests.rs` |
| Actor definitions, shared action timing, variants, movement | `game/src/game/actors/fixtures.rs` |
| Enemy assembly and collision integration; the squid's hop, pinned tick for tick | `game/src/game/enemy/integration_tests.rs`, `game/src/game/enemy/collision_tests.rs` |
| Player lifetime, replacement, and cleanup | `game/src/game/player/foreground/tests.rs` |
| Repeat spike contacts, safe recovery, cancellation, and lift descent intent | `game/src/game/spikes/tests.rs`, tests in `game/src/game/player/sensors.rs`, `game/src/game/world_objects.rs` |
| Skill admission and item accounting | `game/src/game/player/skills/activation/tests.rs`, `game/src/game/player/skills/item_use/tests.rs` |
| Down-dash activation, sweep, and flight/impact playback | Tests in `game/src/game/player/states/skill/dash_striking.rs` |
| Cross-system scheduling and query initialization | `game/src/game/schedule_tests.rs` |
| Inventory, crafting, camera, UI, and level rules | Local `#[cfg(test)]` modules beside the implementation |
| Regional surface seams, adaptation collision, restart, and rule semantics | `game/src/levelgen/surface_tests.rs`, `game/src/levelgen/secret_cover_tests.rs`, `game/src/levelgen/secret_room_tests.rs`, `game/src/levelgen/biome_rule_tests.rs`, `game/src/levelgen/ldtk_rule_tests.rs` |
| Background asset freshness, blur gutters, opening spacing, and detailing reach | `tools/backgrounds/test_generate.py` |
| Near openings and detailing; `SEEKER_NEAR_FIXTURE_OUTPUT` renders them beside the artist's | `game/src/levelgen/background_openings.rs`, `game/src/levelgen/background_synthesis.rs` |
| Fresh startup timing and bounded compiler scaling | `game/src/levelgen/diagnostics.rs`, `game/src/levelgen/throughput_tests.rs`, [level-generation.md](docs/architecture/level-generation.md) |

Use `cargo test <test-name-fragment>` for a focused run. For simulation performance, use comparable development builds and representative workloads; record measured scope rather than asserting a frame rate from an isolated subsystem benchmark.

```sh
cargo test controller_action_population_cost -- --ignored --nocapture
```

This measures the controller/action stage at 500 and 1,000 actors, excluding rendering and complete game simulation. `game/src/game/actors/population_probe.rs` provides a loaded-game probe via `cargo run -- --actor-benchmark=500` or `cargo run -- --actor-benchmark=1000`; enter gameplay to seed its controlled population. It modifies the benchmark world's population/protection, measures simulation ticks, and exits automatically.

`SEEKER_TIMING_PROBE=1` shows the timing panel (F9 toggles it; F10 cycles click modes; F11/F12 compare pixel alignment/interpolation; P pauses). [timing-and-audio.md](docs/architecture/timing-and-audio.md) describes calibration and guarantees. `SEEKER_TICK_PROFILE=1` enables stage/system profiling; `SEEKER_TIME_DIAGNOSTICS=1` enables clock diagnostics. These are diagnostic workloads, not normal gameplay defaults. State explicitly when visuals, audio, interaction, or total frame cost have not been verified.

## Documentation

All project documentation is GitHub-flavored Markdown under `docs/`. It renders on GitHub and builds with mdBook (`mdbook serve docs`; `docs/SUMMARY.md` is the table of contents) into the published site. [docs/README.md](docs/README.md) is the index and states the writing conventions.

| Location | Use it for |
| --- | --- |
| `README.md` | Project introduction, build instructions, controls, and repository layout |
| `CONTRIBUTING.md` | Contributor setup, verification, and pull-request workflow |
| `AGENTS.md` (this file) | Source map, working rules, and runtime contracts; `CLAUDE.md` imports it |
| `docs/design/` | Game design vision: audience, art direction, lore, monetization, intended mechanics. Intent, not current behavior |
| [docs/architecture/actors.md](docs/architecture/actors.md) | The single enemy/player behavior design document |
| [docs/architecture/timing-and-audio.md](docs/architecture/timing-and-audio.md) | The single timing, audio, animation-cadence, and presentation design document |
| [docs/architecture/level-generation.md](docs/architecture/level-generation.md) | The single level-generation design, authoring, and tool document |
| `docs/authoring/` | Asset guidelines and the script and animation format references |

- Extend the existing document for a subsystem; do not add overlapping specs, progress notes, or hand-off documents.
- Add each new page to `docs/SUMMARY.md`. Link between pages with relative `.md` links and refer to source files by repository path in backticks. Relative links that leave `docs/` break in the built site, so link root files through their GitHub URL.
- Check formats against their Rust types before treating a documented example as a runtime contract, and update the page when they differ.
