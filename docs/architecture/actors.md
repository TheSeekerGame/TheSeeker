# Actors and actions

This is the design document for enemy and player state machines. The goal is readable, composable behavior with one execution algorithm per mechanic. Small Rust controllers decide what an actor does; reusable actions execute it; typed assets supply the variation. Adding an enemy that combines existing mechanics should require data. A genuinely new mechanic deserves a small, explicit Rust implementation.

## Separate decisions from execution

| Responsibility | Owns |
| --- | --- |
| Perception | Target selection, visibility, reaction history, and physical observations |
| Controller | Decisions and the memory needed to make them |
| Action | Execution phases, commitment, cancellation, and temporary effects |
| Motor | Movement and collision response |
| Presentation | Sprite playback, procedural pose, orientation, and visual effects |

Enemy controllers use Combat, Wander, Hopper, or Ambush strategies. Patrol phases and decision timers belong to the controller. Grounded status and collision contacts are physical facts. Playing an animation does not reset decision timing.

Engine code provides playback, collision, and the 96 Hz simulation clock. Game code owns controllers, action definitions, combat policy, and procedural enemy behavior. Keep transitions explicit and ordered; do not require every possible activation condition to be mutually exclusive.

## Share mechanics across enemies and the player

An action receives its owner, targeting information, damage attribution, motion parameters, and presentation binding. Its execution should not depend on whether input or AI selected it.

Spider shots and player Spit share projectile execution. Bat dives and player downward dash share lunge execution. Their speed, direction, release timing, cancellation, damage, and artwork can differ through typed definitions. Copying an enemy ability means reusing the mechanic while allowing player-specific tuning. Apart from the starting kit and crafted deployable items, every player ability is copied from a creature: design the creature that uses it first, then bind the player's copy to the creature's mechanic and the creature's animation.

Compose repeated behavior, such as Buzzer's projectile burst, through action parameters. Keep distinct algorithms explicit instead of building a general-purpose gameplay scripting language.

## Keep player control responsive

Player locomotion owns idle, running, jumping, falling, and physical contact state. Skill admission owns equipment requirements, input buffering, cooldowns, stock, and sustained-use checks. Toolbar order resolves competing foreground requests; at most one succeeds per tick. Instant uses can succeed alongside it, regardless of their toolbar position.

The foreground slot stores one typed execution. Mechanic-specific ECS tags only index that execution for queries; they do not store another copy of its state. Basic weapon attacks select their weapon and directional variant. Copied abilities bind shared mechanics to their own tuning and presentation. Existing dedicated mechanics use the same foreground lifecycle, so replacing their content does not require changing locomotion.

Locomotion and the foreground skill are separate concerns. Locomotion keeps tracking the appropriate state and background presentation while an action owns selected outputs. Movement curves obey the foreground motion ownership and the player's hit-stop policies. Releasing an action lets presentation select the current locomotion animation without skill-specific restoration code.

The player graphics record a resting anchor independently of an action's temporary offset. Releasing presentation restores that anchor only for the owned execution. Spike recovery suspends motion and action admission under its own lifetime, releases foreground resources, and resumes on a physically supported, hazard-free position after validating the destination again. The recovery anchor and combat's grounded classification have separate meanings.

Passives modify stats and react to combat or skill events through their own systems. Independently living projectiles, deployed objects, and instant skills do not need to occupy the foreground action slot. Shared actions complement the player skill system; they do not require every skill to use the same execution algorithm. Instant placement/payment/effect execution is shared by input admission and Pulse Drive through `skills/instant.rs`, with an explicit activation origin for their different admission and stealth rules. Tent and tether hold/release previews share `skills/placement_preview.rs`; their placement evaluators and spawned-object simulations remain separate.

## Treat items as independently living effects

An item can spawn immediately or use a foreground deployment action. Instant use spends available backpack stock and creates the object without taking movement or presentation ownership. A deployment can hold the player still, play a pinned once clip, and create the same kind of object at a named release cue.

Deployment validates placement and reserves stock together when it starts. Reserved stock is unavailable to other uses or crafting. Placement is checked again at release. Cancellation before release refunds the reservation; release consumes it exactly once. Completing or interrupting the player action leaves an already deployed object alive under its own lifetime rules. Cooldowns and other activation rules remain at the skill layer; object behavior belongs to the spawned item.

## Make ownership and cleanup explicit

Each actor has one foreground action. The action declares which movement axes, facing, and presentation it controls. Resolve death, status effects, knockback, actions, and locomotion through explicit ownership and actor-specific precedence.

Every execution has a unique identity. Completion, cancellation, interruption, and component removal release only that execution's temporary hitboxes and overrides. Cleanup is idempotent. Launched projectiles and other independent effects follow their own lifetimes. Dead or cancelled actions cannot emit stale combat effects.

## Use authoritative timing

Decision clocks, action progress, and playback position remain separate. Sprite actions use authored frames, named cues, and once/loop/hold sections. Procedural actions use fixed-tick phases. Both obey the same action lifecycle and ownership contract.

Playback emits ordered start, frame-entry/exit, cue, loop, seek, completion, and interruption events tagged with its execution ID. Consumers use the execution they own. A same-clip restart creates a new execution; holding a frame does not repeat its entry effects. Playback advances at most once per simulation tick, with requests committed at an explicit schedule boundary. Player admission commits before background locomotion and foreground execution; movement is resolved before collision-dependent combat effects. Deferred completion checks the execution identity at commit, so it cannot terminate a replacement action.

Frame-authored walking is local displacement on actual frame entry. Locomotion explicitly selects continuous movement or frame steps. Each step passes through normal tile collision; blocked displacement is discarded. Actor movement uses pixels per tick, while projectile conventions remain explicit at their own boundary. Gameplay facing is world direction; native sprite orientation belongs to presentation.

Procedural presentation does not need a fictitious sprite timeline. Tentacle geometry follows its authoritative fixed-tick pose, with attack timing and hitbox lifetime owned by the action. Void combines multipart presentation with the shared flying pursuit controller. Pursuit approaches a perceived target continuously and uses contact damage and its own contact cooldown, without entering a timeline attack activity.

## Make authoring predictable

An enemy definition composes body, perception, controller, action bindings, presentation, stats, and lifecycle policy. Resolve references and typed variants into immutable definitions. Validate required assets, cues, sections, numeric values, and body compatibility before publishing them.

An active action retains its resolved definition and dependencies. Valid edits apply on subsequent activation; looping locomotion adopts edits at a loop or phase boundary. Structural body changes apply on respawn. Invalid edits retain the last valid definition and report a precise error.

Clarity and preserved gameplay feel take priority over minimizing line count. Keep focused tests for timing, cancellation, output ownership, collision, cleanup, and controller decisions. Measure performance with clearly stated workloads and scope.
