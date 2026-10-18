# Level generation

The goal is a varied, readable cave world with rich pixel artwork and inexpensive gameplay rendering. PNG plans define traversable geometry and authored markers. Shared generation rules supply rock mosaics, vegetation, objects, and biome detail. Prepared surfaces and local state changes let the planet become harsher as the player explores without generating textures during play.

Keep logical world data separate from its presentation. Reuse a small number of algorithms, preserve authored constraints, and measure complete workloads. Favor removing repeated work over adding caches, scheduling frameworks, or special cases. This is the single design and authoring document for the level-generation system; source code and assets establish exact tuning.

## Separate geometry, appearance, and publication

| Responsibility | Owns |
| --- | --- |
| Request | Seed, selected map, generation configuration, and build identity |
| Asset catalog | Loaded artwork, decoded masks, parsed rules, and filename metadata |
| Assembly | Compatible chunk selection, flips, palette compilation, and authored markers |
| Geometry | Secret-room carving, terrain/hazard state, and collision inputs |
| Cosmetic generation | Mosaic categories, depth biomes with geyser patches, and decoration/object placement |
| Surface baking | Native artwork, edge darkening, backgrounds, concealment covers, and prepared alternatives |
| Darkness | CPU coverage, sky visibility through every parallax layer, and a distance-field bake into R8 light |
| Publication | Collision, enemy placement catalog, render assets, and readiness |
| Adaptation | Persistent world-change state and activation of prepared products |

The dependency order is **load assets → assemble map → carve rooms → generate terrain mosaics → place geysers and determine biomes → place decorations and objects → prepare surfaces and adaptations → publish collision, enemy catalog, and darkness → ready**. Near-background generation consumes the carved terrain and biome data. Middle/far backgrounds are built offline and loaded as a selected pair.

`GenerationRequest` chooses a random seed for normal games and accepts a fixed seed for diagnostics. A seed reproduces a request with the same assets and algorithms; it is not a promise of identical output across code or content changes. `GenerationBuild` owns an identifier and explicit Assets, Ground, Background, Surfaces, Publishing, Ready, or Failed phase. No timeout skips missing darkness dependencies. Readiness requires collision and enemy data, completed darkness, required image uploads, and a render pass with no waiting pipelines. Restart discards world products and rejects publication from an old generation.

## Preserve the authoring formats

Map PNGs live under `assets/levels/maps`. A map pixel represents one 96×96-tile chunk: black is an ordinary occupied cell, white selects the unique player-spawn template, and `#45ff00` requests geyser content. Transparent cells are empty. Arbitrary rectangular images are cropped to occupied bounds; holes remain holes.

Chunk PNGs live under `assets/levels/chunks`. Their final underscore suffix declares connectors in `U`, `D`, `L`, and `R` directions, such as `generated_chunk_0042_UDR.png`. The runtime indexes connector-compatible templates and their horizontal flips. It exhausts compatible unused templates before deterministic reuse. The spawn template remains unique. Selected pixels compile directly into world arrays without a composite-PNG round trip.

`large_chunk_3x3_<number>_<connectors>.png` plans are 288×288 tiles. Assembly reserves a complete occupied 3×3 block before assigning ordinary chunks, provided every external neighbor meets the middle cell of an edge and the block contains no spawn cell. A large plan exposes centered twelve-tile ports on those middle edges and seals its corners. Each matching large plan is consumed once per world; exhausted pools fall back to ordinary chunks. Logical slices preserve the plan's continuity, while collision, decoration quotas, adaptations, and rendering keep their existing 96×96 regions.

`tools/generate_demo_map.py` authors the demo map with 64 connected cells, sixteen depth rows, loops, and three eligible large-room blocks. Generated terrain starts in ice, blends in coarse patches from four to six chunks below the spawn row, and uses tundra below that. Geysers add local tundra patches. The baseline biome also controls decoration and object eligibility; it is not limited to terrain colors.

A custom level PNG uses the same tile palette and bypasses chunk selection. PNG rows point down; world Y points up. Tiles are six world pixels by default and terrain artwork is six pixels per tile. Palette classification ignores pixels with alpha below 128. Paint exact colors without anti-aliasing.

| Tile-plan color | Meaning |
| --- | --- |
| `#000000` | Solid terrain |
| `#FFFFFF` | Active spikes |
| `#808080` | Optional spikes, initially solid terrain |
| `#FFFF00` | Player spawn |
| `#BB5E02` | Vagrant/yak pair |
| `#456368` | Collector |
| `#FFEEC3` | Monk |
| `#654170` | Exiled |
| `#928C8C` | Capsule marker |
| `#4DFBFF` | Ore marker |
| `#5B666F` | Manual near-background coverage |

The compiler resolves capsule placement in the player-spawn chunk. Enemy populations come from collision-driven habitat analysis; they are not inferred from arbitrary blue pixels.

Spike recovery points are runtime physical anchors rather than authored PNG markers. Content placement excludes empty map cells and the exterior air above the first rock in an occupied chunk with no chunk above it, except for the playable spawn crater. This filter runs before rule quotas and spacing, so inaccessible outer tops do not consume content budgets. Enemy habitat analysis uses the same filter.

Each kind's spacing rules apply only to its own kind, so different entities can overlap, such as a rock at the foot of a totem. Every placed entity therefore gets its own depth just behind the terrain: taller sprites stand behind shorter ones, and placement order breaks ties. Sprites sharing a depth have no stable draw order and would flicker.

Generation PNG filenames carry numeric rule ordering, probability, permitted pattern flips, random output flips, sprite weights, offsets, spacing, and placement limits. `catalog.rs` is the parser for those conventions. Preserve rule order and distinguish a pattern flip used for matching from an artwork flip used for variety. Rule-mask images are authoring data, not terrain texture layers.

## Generate mosaics from the exposed rock

Carve all startup rooms before running terrain mosaic rules. These rules depend on terrain edges: a room's inner wall and an isolated platform inside it need the same category generation as an outside platform. Clipping categories generated for an unbroken wall cannot create the missing transitions around a new cavity.

Generate the three mosaic masks once for the carved terrain. Each layer consumes the preceding layer's output, and higher layers own overlapping cells. Secret-room covers and optional-spike alternatives reuse this cosmetic structure, clipped to their terrain state. Runtime activation does not rerun mosaic rules.

LDtk selects the actual tiles from a complete category grid. Terrain values are Wall0/base `18`, mosaic1 `17`, mosaic2 `19`, and mosaic3 `20`; spikes have their own category. An empty mosaic result uses distance-based categories as a fallback. The category grid and the pixel-distance shading field have different purposes and must not be substituted for one another.

Terrain rule priority is spikes → wall0–wall3 → fog1–fog3 → fog_add. The first priority stage producing a tile wins. Within a stage, authored ordering, probability, allowed flips, and `break_on_match` control the result. Unresolved cells use catchall tiles discovered from the LDtk data. Tundra grass is evaluated only where the tundra terrain program leaves a cell unresolved. Biome selection happens before per-cell rule evaluation, while matching can still read neighboring values from the full grid.

LDtk pattern `0` ignores a neighbor, positive `N` requires that value, negative `-N` forbids it, `1000001` requires any occupied value, and `-1000001` requires empty. Out-of-world LDtk samples are empty. Near-background rules evaluate fog transitions before wall catchalls and see all category boundaries in their plane. Background values `1`, `2`, and `3` describe main/middle/far wall categories; tundra also uses near-wall `4`. These categories are not additional parallax planes.

## Keep secret rooms independently discoverable

Secret-room PNG rules live under `assets/levels/secret_rooms`. Offsets are relative to the image center; a `_ruleflip-x` filename allows horizontal matching. Each rule is placed at most once, with deterministic scoring among valid anchors.

| Room-rule color | Meaning |
| --- | --- |
| Black | Carve room air |
| White | Carve the breakable entrance and create its cover |
| `#5B666F` | Carve and add manual near background |
| `#4DFBFF` | Carve and add an ore marker |
| `#6ABE30` | Require rock without carving it |
| `#AC3232` | Require existing cave air without spikes |

Entrances are checked against the original assembled cave, so the air created by one room cannot serve as another room's entrance. Carved cells and breakable entrances must be terrain. Placement-only requirements do not become cover pixels or collision tiles. A rule with no valid placement is skipped.

The playable geometry, mosaic detail, and exposed-wall shading are prepared at startup. A room's cover is the level as it was before carving, over the room and the border its carving reshades (`cover_border_tiles`: the reach of edge shading plus a tile). Covers never show: every room stays two borders away from other rooms, optional spikes, and the tiles the intro impact changes. So no carving reaches another room's cover, no cover hides art that changes during play, and each cover matches the ground wherever it ends, whichever rooms are open. Covers are baked with the level's decorations; decorations anchored in a room are drawn on a hidden contents sprite instead of the ground, since they could reach past the thin wall into the entrance. For the same reason, entity placement skips room tiles; rooms hold only their authored ore.

Temporary collision contributions keep a room closed until its entrance breaks. The breakable wall takes five hits; opening removes its wall entity, room cover, and dynamic collision contribution, and reveals the room's contents. Room opening performs bounded entity/collision changes, with no image baking.

## Prepare the planet's changes before play

Optional spikes express the planet's increasing hostility and make backtracking surprising. `PreparedAdaptation` owns stable identity, available states, affected chunks, prepared visual handles, terrain/hazard deltas, and entity activation data. Logical adaptation state is separate from disposable rendering assets.

Startup prepares the initial terrain once and alternative images only for affected regions and their required margins. Nearby spike markers that influence the same surfaces share an activation group. Activation remains one-way, starts at immunity 2, evaluates every 16 game ticks, and waits until the group is outside the player's two-chunk active radius. Disabled hazard entities are prepared with their images.

Activation switches prepared material handles, updates tile state, patches base collision while retaining secret-cover contributions, and enables the hazards before simulation observes the new state. Repeated activation is harmless. No normal activation performs rule matching, texture baking, or a whole-world collision rebuild.

Use this interface for additional adaptations when their behavior fits. An adaptation changing background openings or light coverage needs explicit prepared background/darkness outputs and a publication contract; a generic darkness-patch consumer is not implemented. Prepare only interacting local states, rather than a Cartesian product of every change on the planet.

## Bake bounded surfaces and cheap lighting

Render output uses 96×96-tile regions at native artwork resolution. Each region reads enough neighboring input for tile edges, shading, decorations, and blur. Global coordinates control dithering and sampling; internal output borders are not world edges. Up to four Bevy task-pool workers bake independent regions with reusable scratch storage. Ordered logical decisions remain deterministic.

Ground darkening uses Euclidean distance from exposed terrain edges, the established falloff, linear-color attenuation tables, and global-coordinate dithering. Transparent pixels are skipped. The distance implementation is shared with darkness. A ground bake reads an 11-tile input margin for the current shading support. Decorations are indexed to contributing output regions and composited with their existing alpha and placement rules.

The near background, described below, is warped for its parallax depth. Depth biomes receive bounded geyser-radius stamping with the existing inner/outer rings and authored chunk restrictions. Spacing indexes and sparse decoration ownership keep placement checks local.

Gaussian background blur uses linear premultiplied color, horizontal/vertical passes, and one conversion in each direction. Texture gutters and kernel margins preserve continuity. Terrain uses nearest sampling; blurred background textures use their existing filtering. All regional pieces of a parallax plane share a common world center and transform.

The farthest cave plane represents distant light. Openings in nearer cave walls allow that light through; depth and the player's backlight supply the rest of the feel. A baked foreground-darkness layer and inexpensive fullscreen composition express this without dynamic lights or shadow maps.

Darkness consumes compact CPU coverage from unblurred terrain/background artwork, including relevant decoration alpha. The lightmap has two pixels per tile. A pixel is a light source where a camera centred on it would see through the ground and every background layer to the sky; parallax shows a layer at depth *d* with its content *d* times nearer the layer's centre. One distance field over the lightmap gives every pixel the light of its nearest source: full at the source, fading with the square of the distance to nothing at 24 world units. Chunks outside the map hold no sources. The bake depends on neither the camera nor the window, and the loading screen waits for it. The result is stored in bounded 2,048-pixel texture-array pages with gutters and an R32Uint page table. No redundant RGBA mask textures or source cameras are needed.

The fullscreen pass darkens by depth below the surface, then baked light and the player's backlight each lift a share of that darkness, and dithering hides 8-bit banding. `DarknessSettings` in `darkness.rs` documents the uniform the pass reads.

## Follow the artist's cave backgrounds

The artist's hand-made LDtk level defines the look of every background plane. `tools/backgrounds/exemplars.py` extracts its middle, far, and near IntGrid layers for both biomes into exemplar PNGs: `tools/backgrounds/source/exemplars/` for the offline planes and `assets/levels/tiles/<biome>/near_bg/exemplar.png` for the near plane. Each color is one IntGrid value, black marks the artist's terrain, and transparent cells lie outside the painted area. The artist's project is not part of this repository, but the extracted exemplars are; rerun the extraction only when that reference changes. The same LDtk rules turn these values into tiles, so reproducing the artist's values reproduces the artist's tiles.

Generation separates *where a plane opens* from *how its wall is detailed*:

- **Openings.** The near plane follows the carved terrain (`background_openings.rs`): it hugs most terrain surfaces with a skin up to five tiles thick and leaves the rest bare, swells into occasional masses up to about eleven tiles beside the terrain, bridges gaps up to about six tiles wide, and leaves the cave farther from terrain open. Middle and far planes cannot see the terrain: `openings.py` places the artist's own openings, flipped for variety, in wavy horizontal strata as the artist does. Far openings gather behind middle openings so light reaches through. Strata are laid out in view space and sampled through each plane's parallax depth, so the planes line up wherever the camera stands. Each plane covers the map the same way: a camera at view position *v* shows the plane's cell *c* + (*v* − *c*)/*d* at the screen's centre, so a plane holds the map's footprint drawn *d* times nearer the centre *c*, widened by the part of half a view (40 tiles) that its slower motion uncovers, (1 − 1/*d*) of it. Wherever the camera stands in the map, every plane has art behind it.
- **Spacing.** Walls between neighbouring middle/far openings are at least three tiles thick, openings keep three tiles from the covered area's edge, and every opening is one whole artist opening. Near openings are never one tile wide, near wall never floats or thins to a strand in the air, and near openings smaller than 40 tiles close.
- **Detailing.** Image analogies (`synthesis.py` offline and `background_synthesis.rs` at runtime) copy each wall cell's value from the exemplar cell whose surrounding walls, openings, and terrain match best. Copies continue from already-written neighbours, so the artist's strokes, clusters, and small openings carry over whole. The near exemplar's context is the artist's near wall tidied by the same rules as generated openings, so synthesis also restores the artist's ragged edges. Detailing reaches up to twelve tiles into middle and far walls, eight into near walls, and two into an opening.

Manual near-background markers add coverage to the generated near wall before detailing; use them for fixups rather than to shape the plane. Ice and tundra share the near openings and differ in their exemplars. Middle/far pairs blend both biomes' openings and artwork by depth in camera space, accounting for each plane's parallax so the background follows the world transition.

## Build the middle and far backgrounds offline

`tools/backgrounds/generate.py` builds three paired variants per map from its footprint, independent of the randomly selected terrain templates. Exemplars and middle/far LDtk definitions live under `tools/backgrounds/source`. Every cave layer leaves the player-spawn chunk empty for the surface backdrop. Shared tilesheets stay under `assets/levels/tiles/ldtk_rules`; the runtime LDtk project owns terrain, near background, and object definitions. Depth-mode pairs combine both authored tilesheets in one native-size atlas before baking, so runtime loading keeps the ordinary bank path.

Use Python 3.11+ and the pinned requirements:

```sh
python3 -m venv tools/backgrounds/.venv
tools/backgrounds/.venv/bin/pip install -r tools/backgrounds/requirements.txt
tools/backgrounds/.venv/bin/python tools/backgrounds/generate.py
tools/backgrounds/.venv/bin/python tools/backgrounds/generate.py --level assets/levels/level_test.png
tools/backgrounds/.venv/bin/python tools/backgrounds/generate.py --biome tundra --preview /absolute/dir --preview-only
tools/backgrounds/.venv/bin/python tools/backgrounds/exemplars.py path/to/level.01.ldtk
```

The default command processes all map PNGs with `--biome depth`; custom levels use ice by default. `--preview DIR` also writes each pair's unblurred far and middle planes composited over the sky; `--preview-only` skips the banks. `--biome ice` or `--biome tundra` generates a single-biome inspection set; tundra banks go under `background_bank/tundra/`, which the runtime does not load. `--map assets/levels/maps/map1.png` restricts the input and `--variant 2` rebuilds one pair. Explicit input paths are relative to the working directory; other paths resolve from the script. Custom PNGs need a bank under `custom/<filename stem>`; custom stems must be unique within a shipped asset set.

Output lives under `assets/levels/background_bank/<map>/<variant>-<layer>/`. Variants are 0–2; layer 1 is middle and layer 2 is far. Each directory contains a manifest, numbered blurred RGBA chunks, and corresponding `-coverage.png` masks. The manifest records format version, dimensions, map/source hashes, and chunk bounds. Two tiles of outer padding, blur radii of three/five pixels, and one-pixel filtering gutters preserve the layer appearance. Three pairs mean six logical layers per map, split into bounded files.

Regenerate after changing a map footprint, exemplar, shared tilesheet, or generator. Density and spacing constants sit at the top of `openings.py`; raising `FILL` opens more of each plane and lets more light through. Runtime loading checks map hashes and geometry and reports invalid or missing banks explicitly; there is no procedural fallback. The Python validation suite also checks source freshness and shared gutters. The request seed selects a pair; `SEEKER_BACKGROUND_VARIANT=0`, `1`, or `2` overrides selection for inspection.

## Show the open surface around the spawn

The player-spawn template is an open crater under the sky. `surface_backdrop.rs` draws the sky, high clouds, distant range, nearer hills, and the crater's far wall as one-chunk-wide images from `assets/levels/surface`, placed by spawn-chunk rows. Their parallax is measured from the player spawn, so the resting capsule view shows the artwork as authored; below that view the layers stay fixed to the world so they never sink into the caves. The opaque sky sits in front of the middle and far cave planes, and the far wall in front of the near plane, because those planes' parallax shifts other cave content under the spawn chunk. The same module publishes an `OpenSkyRegion` in which foreground fog clears above the crater rim. Fullscreen depth darkness is measured from the surface above the capsule: it stays at zero through the first chunk below and eases in over the next.

A spawn template may have an intact-ground sibling with the same file name under `assets/levels/chunks/pre_impact/`; only its `#000000` terrain pixels are read. `impact_crater.rs` bakes that terrain, plus the cave directly below any opening it closes (bounded, deeper than any pre-impact view), as hidden `ImpactIntact` surface patches beside the `ImpactFormed` crater patches. Collision always uses the formed crater. The landing cinematic in `game/src/game/intro.rs` shows the intact ground until the impact.

`tools/surface_art/generate.py` generates the backdrop layers and the cinematic's effect sheets under `assets/animations/intro` (Pillow and NumPy; the background requirements suffice). Its colors come from the ice-biome tilesets so the surface continues into the caves. Keep its layer rows in sync with `surface_backdrop.rs`; `--preview /absolute/dir` writes composited views at several camera heights and effect strips.

## Generate and validate chunk plans

`tools/generate_level_chunks.py` creates deterministic chunks using Pillow. It carves a room/corridor graph on a 48×48 construction grid and expands each cell into a 2×2 tile block. Declared connectors use centered 12-tile openings; undeclared edges stay sealed. Broad ledges support the normal route, while optional movement-gated branches remain off that route. The tool also places and validates authored spike groups.

```sh
python3 tools/generate_level_chunks.py generate \
  --output assets/levels/chunks --prefix generated_chunk --seed 20260825 \
  --count 30 --connectors U D L R UD LR UL UR DL DR UDL UDR ULR DLR UDLR
python3 tools/generate_level_chunks.py validate assets/levels/chunks/generated_chunk_*.png
python3 tools/generate_level_chunks.py self-test
```

Use `--start-index` for another numbered batch, `--preview /absolute/path.png` for a contact sheet, and `--special-branch-rate` to change the default 0.75 branch probability. Generation refuses replacement without `--overwrite`. `regenerate <paths...>` rebuilds existing 96×96 generated chunks from their stored seeds.

Validation checks palette/dimensions, declared openings, sealed edges, the 2×2 geometry invariant, clearances, connected air, substantial wall widths, spike constraints, and mutually reachable core platforms. Its conservative movement model uses a two-tile-tall player, ten-tile upward reach, fourteen-tile horizontal reach, and twenty-four-tile controlled drop. Optional branches must remain excluded from the base traversal graph. These are tool validation bounds, not replacement player-physics tuning.

`--footprint 3` uses `tools/large_level_chunks.py` to grow 288×288 cave plans from big chambers: rooms with flat floors, steep walls, and domed, uneven ceilings, planned on a two-tile grid. Every chamber joins the cave in a way the player can climb. Side by side, two chambers share a few end columns that open from the higher roof to the lower floor; the lower chamber stops there, so the higher floor ends in a cliff that rises straight from the floor below. A narrow shaft joins a lower chamber's end to the middle of a higher floor; the lower chamber again stops at the shaft, so the shaft's wall on that side climbs unbroken, and the shaft bores through at most 48 tiles of rock. Chambers that are not joined stay more than a shelf, 16 tiles, apart, and the rock each climb relies on stays whole.

Port chambers come first. Side ports open onto chambers at the port's floor, the upward port rises from a chimney whose outer wall lines up with the port's edge and climbs from the chamber floor, and the downward port drops through a chamber's floor. The planner then grows chambers toward separate parts of the cave, or joins existing chambers directly, until the cave is one. It keeps growing until the room is as open as its family asks and has at least three broad floors, discarding layouts that stay too closed. The `network`, `grand`, and `winding` families differ in chamber sizes and openness; `grand` adds one hall up to 224 tiles wide. A batch of 45 supplies three plans for every nonempty connector mask:

```sh
python3 tools/generate_level_chunks.py generate --footprint 3 \
  --output assets/levels/chunks --seed 20261001 --count 45 \
  --connectors U D L R UD UL UR DL DR LR UDL UDR ULR DLR UDLR
python3 tools/generate_level_chunks.py validate assets/levels/chunks/large_chunk_3x3_*.png
python3 tools/generate_demo_map.py
```

Repeat the large-plan generation command with `--overwrite` to rebuild the batch. Each PNG records its seed, family, and footprint.

Floors stay flat for walking and ground encounters; some broad floors carry a raised plateau four to eight tiles high across part of their middle. A sideways warp makes every wall waver while floors stay level, and it drifts at most one grid cell per row so walls stay climbable. A few floating rocks with deep, uneven, bowl-shaped undersides hang high in open space, a hop from a climbable wall, with a ground encounter's headroom beneath them. Thin rock joining two solid masses is cut away, so rock either floats or sits firmly. Convex floor edges lose their corner tile, and concave floor-wall corners ease into a small slope.

Cleanup removes corner-only rock contacts and small detached fragments, then fills sealed air pockets. Large-plan validation checks these topology rules, every boundary pixel, the two-tile geometry grid apart from softened corners, connected air, and movement both to and from every connector and platform. Its movement model includes wall climbing: holding toward a wall and jumping repeatedly climbs any wall face that drifts at most one grid cell per row, and a climber can jump to or drop onto platforms within reach of any point along a face. Standing spots narrower than three tiles are wall texture and need not be reachable. Validation also requires at least three encounter floors with 48 uninterrupted tiles of ground and 12 tiles of headroom. Wide-floor traversal samples nearby takeoff and landing positions as well as platform endpoints and midpoints. Room dimensions grow; movement limits and connector openings do not.

## Verify behavior and report measured scope

```sh
cargo check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
tools/backgrounds/.venv/bin/python -m unittest discover -s tools/backgrounds -v
```

Focused tests cover region seams and transparency, Euclidean shading, LDtk priorities/probability/flips, selective biome evaluation, authored room mosaics, independent room entrances, room spacing, covers matching the level for every set of open rooms, adaptation collision/idempotence, and restart cleanup. Darkness tests pin the light's falloff, the parallax sky test, sky hidden by ground and by chunks outside the map, and atlas gutters; a graphics test checks every fullscreen uniform against the shader Bevy composes. Headless fixtures can render authored room walls with `SEEKER_ROOM_FIXTURE_OUTPUT=/absolute/directory cargo test authored_room_walls_receive_mosaics_after_carving`.

For each authored room, the fixture writes `room-<number>.png` with its interior walls and `room-<number>-before.png` with the categories as they were before carving. It exercises real mosaic rules, LDtk artwork, and regional shading without driving the game window.

For interactive checks, run `cargo run` and inspect room interiors and covers, both biomes, chunk boundaries, moving parallax, background variants, and spike activation during backtracking. Passing tests does not establish gameplay feel or visual correctness everywhere.

```sh
cargo build --bin theseeker_game
SEEKER_LEVELGEN_BENCHMARK=1 SEEKER_LEVEL_SEED=20260908 target/debug/theseeker_game
cargo test selective_biome_rule_cost -- --ignored --nocapture
cargo test occupied_chunk_compiler_throughput -- --ignored --nocapture
```

The startup benchmark auto-starts the generated map and exits three seconds after readiness. `SEEKER_LEVELGEN_SCREENSHOT=/absolute/path.png` saves a representative frame; `SEEKER_LEVELGEN_KEEP_OPEN=1` keeps it open. Record engine initialization separately from request-to-CPU-ready and request-to-published-frame time. Include adaptation preparation, retained output, and temporary-memory scope. Kernel benchmarks and disk-cached launches do not establish a whole-generation speedup.

The selective-biome probe compares full terrain-rule passes with evaluating only the chosen biome and the necessary grass fallbacks, after checking that both choose identical tiles. It is a synthetic mixed-biome rule-selection workload, not a full startup measurement.

The bounded compiler probe exercises 8/80/800/8,000 output chunks and discards completed images. It excludes mosaics, placement, backgrounds, darkness, GPU publication, and resident worlds. It prints its timings and output byte counts. A 1,000× compiler-throughput test does not prove that a rendered 1,000× world fits memory.

## Keep future changes proportionate

World arrays and the CPU R8 light buffer remain dense. Regional images remove oversized intermediate RGBA surfaces and single-texture dimension limits, but retained assets still grow with the world. Texture residency, eviction, and gameplay streaming are separate work needed for much larger worlds on old laptops.

Useful further improvements should remove concrete repeated work: evaluate alternate terrain rules only near changed cells, reuse precomputed candidate offsets where profiling justifies them, and avoid producing empty region buffers. Secret-cover and spike alternatives still build whole-grid logical tile programs even though their raster output is regional. This is a remaining optimization opportunity, not a reason to introduce a general scheduling or dependency framework now. Preserve numerical/visual comparisons and measure the gain before retaining added complexity.

## Source navigation

Paths below are relative to `game/src/levelgen` unless stated otherwise.

| Change | Start here |
| --- | --- |
| Pipeline and readiness | `mod.rs`, `pipeline.rs`, `publication.rs`, `diagnostics.rs` |
| PNG formats and connector pools | `compiler.rs`, `mapgen.rs`, `catalog.rs` |
| Geometry and mosaics | `terrain.rs`, `secret_rooms.rs`, `mosaic.rs`, `secret_room_tests.rs` |
| Room concealment and opening | `secret_covers.rs`, `breakable_wall.rs`, `secret_cover_tests.rs` |
| Tile selection and region baking | `ldtk_rules.rs`, `surface.rs`, `ground_shading.rs`, `distance.rs` |
| Biomes, decorations, and objects | `placement.rs`, `spatial.rs`, `world_spawn.rs` |
| Near background and shipped pairs | `background.rs`, `background_openings.rs`, `background_synthesis.rs`, `background_surface.rs`, `background_bank.rs` |
| Surface backdrop and pre-impact crater | `surface_backdrop.rs`, `impact_crater.rs`, `tools/surface_art/generate.py` |
| Adaptations and collision patches | `adaptation.rs`, `optional_spikes.rs`, `engine/src/physics.rs` |
| Baked light and composition | `game/src/graphics/post_processing/darkness.rs`, `darkness/bake.rs`, `darkness/atlas.rs`, `assets/shaders/post_processing/darkness.wgsl` |
| Offline authoring | `tools/generate_level_chunks.py`, `tools/large_level_chunks.py`, `tools/generate_demo_map.py`, `tools/backgrounds/generate.py` (with `openings.py`, `synthesis.py`, `exemplars.py`), `tools/surface_art/generate.py` |
