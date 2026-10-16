//! Authored assets resolve transactionally into immutable actor/action snapshots.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use theseeker_engine::assets::{
    animation::SpriteAnimation, attack::AttackDescriptor,
    projectile::ProjectileDescriptor, PreloadedAssets,
};

use super::action::{ActionDefinition, Mechanic, ResolvedAction};
use super::controller::ControllerSpec;
use super::definition::{ArchetypeStats, EnemyArchetype, EnemyFacingSetup, EnemyGfxKind, EnemySetupController};

#[derive(Asset, TypePath, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AuthoredActor(pub EnemyArchetype);

#[derive(Clone)]
pub struct ResolvedActor {
    pub archetype: EnemyArchetype,
    pub body: ActorBody,
    pub behavior: ControllerSpec,
    pub walking: super::motion::ResolvedWalkMotion,
    pub actions: HashMap<String, Arc<ResolvedAction>>,
    pub clips: HashMap<String, Arc<SpriteAnimation>>,
}

impl ResolvedActor {
    /// Publication requires stats, so every resolved actor has them.
    pub fn stats(&self) -> &ArchetypeStats {
        self.archetype.stats.as_ref().expect("published actors have stats")
    }

    /// Structural body and lifecycle settings belong to the spawned actor.
    pub fn adopt_actions(&self, latest: &Self) -> Self {
        if self.body.controller != latest.body.controller { return self.clone(); }
        let mut archetype = self.archetype.clone();
        archetype.anim.clone_from(&latest.archetype.anim);
        archetype.actions.clone_from(&latest.archetype.actions);
        Self {
            archetype, body: self.body.clone(), behavior: latest.behavior.clone(), walking: latest.walking.clone(),
            actions: latest.actions.clone(), clips: latest.clips.clone(),
        }
    }
}

/// The body an actor spawns with: its authored setup with defaults applied.
#[derive(Clone)]
pub struct ActorBody {
    pub name: String,
    pub controller: EnemySetupController,
    pub collider_half_extents: Vec2,
    pub spawn_offset: Vec3,
    pub facing: EnemyFacingSetup,
    /// Playback slots for facing right and left.
    pub facing_slots: Option<(String, String)>,
    pub gfx: EnemyGfxKind,
    pub gfx_offset: Vec3,
    pub pixel_aligned: bool,
    /// Animation bindings tried in order for the first clip.
    pub animation_keys: Vec<String>,
    pub needs_line_of_sight: bool,
    /// Floors for the vision and melee ranges after per-spawn jitter.
    pub min_vision_range: f32,
    pub min_melee_range: f32,
    pub trap_prey: bool,
}

impl ActorBody {
    fn resolve(actor: &EnemyArchetype) -> Result<Self, String> {
        let setup = actor.setup.as_ref().ok_or_else(|| format!("{}: missing body setup", actor.id))?;
        let controller = setup.controller.ok_or_else(|| format!("{}: missing body controller", actor.id))?;
        let collider = setup.collider.ok_or_else(|| format!("{}: missing body collider", actor.id))?;
        if collider.half_extents.iter().any(|value| !value.is_finite() || *value <= 0.0) {
            return Err(format!("{}: invalid body collider", actor.id));
        }
        let gfx = setup.gfx.clone().unwrap_or_default();
        let minimums = setup.stat_minimums;
        Ok(Self {
            name: setup.name.clone().unwrap_or_else(|| actor.id.clone()),
            controller,
            collider_half_extents: Vec2::from_array(collider.half_extents),
            spawn_offset: Vec3::from_array(setup.spawn_offset.unwrap_or_default()),
            facing: setup.facing.unwrap_or(EnemyFacingSetup::Right),
            facing_slots: setup.facing_slots.as_ref().map(|slots| (slots.right.clone(), slots.left.clone())),
            gfx: gfx.kind.unwrap_or(EnemyGfxKind::PixelPerfect),
            gfx_offset: gfx.offset.map_or(Vec3::ZERO, Vec3::from_array),
            pixel_aligned: gfx.pixel_aligned.unwrap_or(true),
            animation_keys: gfx.keys.unwrap_or_default(),
            needs_line_of_sight: setup.needs_line_of_sight.unwrap_or(false),
            min_vision_range: minimums.map_or(0.0, |minimums| minimums.vision_range),
            min_melee_range: minimums.map_or(0.0, |minimums| minimums.melee_range),
            trap_prey: setup.trap_prey.unwrap_or(false),
        })
    }
}

#[derive(Resource, Default)]
pub struct ActorCatalog {
    pub actors: HashMap<String, Arc<ResolvedActor>>,
    pub actions: HashMap<String, Arc<ResolvedAction>>,
    pub generation: u64,
}

#[derive(SystemParam)]
pub struct DefinitionAssets<'w> {
    sources: Res<'w, Assets<AuthoredActor>>,
    actions: Res<'w, Assets<ActionDefinition>>,
    clips: Res<'w, Assets<SpriteAnimation>>,
    projectiles: Res<'w, Assets<ProjectileDescriptor>>,
    attacks: Res<'w, Assets<AttackDescriptor>>,
}

pub fn resolve_catalog(assets: DefinitionAssets, preloaded: Res<PreloadedAssets>, mut catalog: ResMut<ActorCatalog>) {
    if !assets.changed() && catalog.generation != 0 {
        return;
    }
    match assets.resolve(&preloaded) {
        Ok(mut resolved) => {
            resolved.generation = catalog.generation.wrapping_add(1);
            *catalog = resolved;
        },
        Err(error) => error!("Actor definitions rejected: {error}"),
    }
}

impl DefinitionAssets<'_> {
    fn changed(&self) -> bool {
        self.sources.is_changed() || self.actions.is_changed() || self.clips.is_changed()
            || self.projectiles.is_changed() || self.attacks.is_changed()
    }

    pub(super) fn resolve(&self, preloaded: &PreloadedAssets) -> Result<ActorCatalog, String> {
        let sources: HashMap<_, _> = self.sources.iter()
            .map(|(_, source)| (source.0.id.clone(), source.0.clone())).collect();
        let definitions: HashMap<_, _> = self.actions.iter()
            .map(|(_, action)| (action.id.clone(), action.clone())).collect();
        if sources.len() != self.sources.len() { return Err("duplicate actor id".to_owned()); }
        if definitions.len() != self.actions.len() { return Err("duplicate action id".to_owned()); }
        let mut catalog = ActorCatalog::default();
        for (id, source) in &sources {
            let actor = resolve_actor_source(source, &sources)?;
            let resolved = self.resolve_actor(actor, &definitions, preloaded)?;
            catalog.actors.insert(id.clone(), Arc::new(resolved));
        }
        for definition in definitions.values() {
            self.resolve_standalone_action(definition, &definitions, preloaded, &mut catalog)?;
        }
        Ok(catalog)
    }

    fn resolve_standalone_action(
        &self,
        source: &ActionDefinition,
        definitions: &HashMap<String, ActionDefinition>,
        preloaded: &PreloadedAssets,
        catalog: &mut ActorCatalog,
    ) -> Result<(), String> {
        let definition = resolve_action_source(source, definitions)?;
        if definition.presentation.as_ref().is_some_and(|binding| !binding.animation.starts_with("anim.")) {
            return Ok(());
        }
        let resolved = self.resolve_action(definition, None, preloaded)?;
        catalog.actions.insert(source.id.clone(), Arc::new(resolved));
        Ok(())
    }

    fn resolve_actor(
        &self,
        actor: EnemyArchetype,
        definitions: &HashMap<String, ActionDefinition>,
        preloaded: &PreloadedAssets,
    ) -> Result<ResolvedActor, String> {
        actor.validate().map_err(|error| error.to_string())?;
        actor.validate_animations(preloaded, &actor.anim.values().cloned().collect::<Vec<_>>())
            .map_err(|error| error.to_string())?;
        let behavior = actor.behavior.clone().ok_or_else(|| format!("{}: missing behavior", actor.id))?;
        let mut actions = HashMap::new();
        for (slot, id) in &actor.actions {
            let source = definitions.get(id).ok_or_else(|| format!("{}: missing action {id}", actor.id))?;
            let definition = resolve_action_source(source, definitions)?;
            let resolved = self.resolve_action(definition, Some(&actor), preloaded)?;
            actions.insert(slot.clone(), Arc::new(resolved));
        }
        let body = ActorBody::resolve(&actor)?;
        validate_actor_body(&actor, body.controller, &behavior, &actions)?;
        let walking = self.resolve_walking(&actor, preloaded)?;
        let clips = self.resolve_actor_clips(&actor, preloaded)?;
        Ok(ResolvedActor { archetype: actor, body, behavior, walking, actions, clips })
    }

    fn resolve_actor_clips(&self, actor: &EnemyArchetype, preloaded: &PreloadedAssets) -> Result<HashMap<String, Arc<SpriteAnimation>>, String> {
        let mut clips = HashMap::new();
        for (slot, key) in &actor.anim {
            let clip = preloaded.get_single_asset::<SpriteAnimation>(key).and_then(|handle| self.clips.get(&handle))
                .ok_or_else(|| format!("{}: unloaded animation {key}", actor.id))?;
            validate_clip_sections(&actor.id, None, clip)?;
            validate_clip_cues(&actor.id, clip)?;
            clips.insert(slot.clone(), Arc::new(clip.clone()));
        }
        Ok(clips)
    }

    fn resolve_walking(&self, actor: &EnemyArchetype, preloaded: &PreloadedAssets) -> Result<super::motion::ResolvedWalkMotion, String> {
        use super::motion::{WalkMotion, ResolvedWalkMotion, FrameStep};
        let Some(WalkMotion::FrameSteps { track, scale, steps }) = &actor.walking else { return Ok(ResolvedWalkMotion::Continuous); };
        if !scale.is_finite() { return Err(format!("{}: walking scale must be finite", actor.id)); }
        let key = actor.anim.get(track).unwrap_or(track);
        let handle = preloaded.get_single_asset::<SpriteAnimation>(key)
            .ok_or_else(|| format!("{}: missing movement track {key}", actor.id))?;
        let clip = self.clips.get(&handle).ok_or_else(|| format!("{}: unloaded movement track {key}", actor.id))?;
        let mut steps = steps.clone().unwrap_or_else(|| clip.frame_impulse.iter()
            .map(|step| FrameStep { frame: step.frame, delta: [step.x, step.y] }).collect());
        for step in &mut steps {
            validate_movement_step(step, *scale, &actor.id)?;
        }
        Ok(ResolvedWalkMotion::FrameSteps(steps))
    }

    fn resolve_action(
        &self,
        source: ActionDefinition,
        actor: Option<&EnemyArchetype>,
        preloaded: &PreloadedAssets,
    ) -> Result<ResolvedAction, String> {
        let id = source.id;
        let mechanic = source.mechanic.ok_or_else(|| format!("{id}: missing mechanic"))?;
        let mut presentation = source.presentation;
        let clip = self.resolve_clip(&id, &mut presentation, actor, preloaded)?;
        let mut timing = source.timing.unwrap_or_default();
        if let Mechanic::TentacleStrike { phases } = &mechanic {
            phases.validate().map_err(|error| format!("{id}: {error}"))?;
            timing.duration_ticks = Some(phases.duration());
        }
        validate_action_timing(&id, &timing, clip.as_deref())?;
        let projectile = resolve_projectile(&mechanic, preloaded, &self.projectiles)?;
        let projectile_clips = self.resolve_projectile_clips(&id, projectile.as_ref(), preloaded)?;
        let attack = resolve_attack(&mechanic, preloaded, &self.attacks)?;
        if source.damage.is_some_and(|damage| !damage.is_finite() || damage < 0.0) {
            return Err(format!("{id}: damage must be finite and nonnegative"));
        }
        if source.motion.is_some_and(|motion| !motion.speed.is_finite() || motion.speed < 0.0) {
            return Err(format!("{id}: motion speed must be finite and nonnegative"));
        }
        let motion = source.motion.or_else(|| resolve_lunge_motion(&mechanic, actor));
        let damage = source.damage.or_else(|| resolve_action_damage(&mechanic, actor));
        validate_action_motion(&id, &mechanic, motion, source.hits)?;
        Ok(ResolvedAction {
            cues: (&timing).into(),
            id, mechanic, presentation, clip, timing, projectile, projectile_clips, attack,
            damage, motion, controls: source.controls.unwrap_or_default(),
            hits: source.hits,
        })
    }

    fn resolve_projectile_clips(&self, id: &str, projectile: Option<&ProjectileDescriptor>, preloaded: &PreloadedAssets) -> Result<Option<super::action::ProjectileClips>, String> {
        let Some(sprite) = projectile.and_then(|projectile| projectile.sprite.as_ref()) else { return Ok(None); };
        let resolve = |key: &str| -> Result<Arc<SpriteAnimation>, String> {
            let clip = preloaded.get_single_asset::<SpriteAnimation>(key).and_then(|handle| self.clips.get(&handle))
                .ok_or_else(|| format!("{id}: missing projectile animation {key}"))?;
            validate_clip_sections(id, None, clip)?;
            validate_clip_cues(id, clip)?;
            Ok(Arc::new(clip.clone()))
        };
        Ok(Some(super::action::ProjectileClips { idle: resolve(&sprite.idle_anim_key)?, pop: resolve(&sprite.pop_anim_key)? }))
    }

    fn resolve_clip(
        &self,
        id: &str,
        binding: &mut Option<super::action::PresentationBinding>,
        actor: Option<&EnemyArchetype>,
        preloaded: &PreloadedAssets,
    ) -> Result<Option<Arc<SpriteAnimation>>, String> {
        let Some(binding) = binding else { return Ok(None); };
        if !binding.animation.starts_with("anim.") {
            binding.animation = actor.and_then(|actor| actor.anim.get(&binding.animation)).cloned()
                .ok_or_else(|| format!("{id}: missing animation binding {}", binding.animation))?;
        }
        let handle = preloaded.get_single_asset::<SpriteAnimation>(&binding.animation)
            .ok_or_else(|| format!("{id}: missing animation {}", binding.animation))?;
        let clip = self.clips.get(&handle).ok_or_else(|| format!("{id}: animation is not loaded"))?;
        if binding.offset.iter().any(|value| !value.is_finite()) { return Err(format!("{id}: presentation offset must be finite")); }
        validate_clip_cues(id, clip)?;
        validate_clip_sections(id, binding.section.as_deref(), clip)?;
        validate_clip_sections(id, binding.impact_section.as_deref(), clip)?;
        Ok(Some(Arc::new(clip.clone())))
    }
}

fn resolve_actor_source(source: &EnemyArchetype, sources: &HashMap<String, EnemyArchetype>) -> Result<EnemyArchetype, String> {
    let mut actor = source.clone();
    if let Some(base) = &source.base {
        let parent = sources.get(base).ok_or_else(|| format!("{}: missing base {base}", source.id))?;
        if parent.base.is_some() {
            return Err(format!("{}: actor variants require a concrete base", source.id));
        }
        actor.merge_with_base(parent);
    }
    actor.base = None;
    Ok(actor)
}

pub fn resolve_action_source(source: &ActionDefinition, sources: &HashMap<String, ActionDefinition>) -> Result<ActionDefinition, String> {
    let Some(base) = &source.base else { return Ok(source.clone()); };
    let parent = sources.get(base).ok_or_else(|| format!("{}: missing base action {base}", source.id))?;
    if parent.base.is_some() {
        return Err(format!("{}: action variants require a concrete base", source.id));
    }
    Ok(ActionDefinition {
        id: source.id.clone(), base: None,
        mechanic: source.mechanic.clone().or_else(|| parent.mechanic.clone()),
        presentation: source.presentation.clone().or_else(|| parent.presentation.clone()),
        timing: source.timing.clone().or_else(|| parent.timing.clone()),
        damage: source.damage.or(parent.damage), motion: source.motion.or(parent.motion),
        controls: source.controls.or(parent.controls), hits: source.hits.or(parent.hits),
    })
}

fn validate_clip_sections(id: &str, section: Option<&str>, clip: &SpriteAnimation) -> Result<(), String> {
    let settings = &clip.settings.extended;
    if settings.ticks_per_frame == 0 || settings.frame_min.0 == 0 || settings.frame_min > settings.frame_max || settings.frame_start < settings.frame_min || settings.frame_start > settings.frame_max { return Err(format!("{id}: invalid clip frame range or tick duration")); }
    for section in clip.playback.iter().chain(clip.sections.values()) {
        section.validate().map_err(|error| format!("{id}: {error}"))?;
        if section.end > settings.frame_max || section.start < settings.frame_min {
            return Err(format!("{id}: clip section lies outside the animation frame range"));
        }
    }
    if section.is_some_and(|name| !clip.sections.contains_key(name)) {
        return Err(format!("{id}: unknown clip section {section:?}"));
    }
    Ok(())
}

fn validate_action_timing(id: &str, timing: &super::action::ActionTiming, clip: Option<&SpriteAnimation>) -> Result<(), String> {
    for cue in [&timing.release, &timing.cancel, &timing.complete].into_iter().flatten() {
        let Some(clip) = clip else { return Err(format!("{id}: cue {cue} requires a clip")); };
        if !clip.events.iter().any(|event| &event.emit == cue) && !clip.frame_bookmarks.contains_key(cue) {
            return Err(format!("{id}: missing cue {cue}"));
        }
    }
    if timing.duration_ticks == Some(0) {
        return Err(format!("{id}: action duration must be positive"));
    }
    Ok(())
}

fn resolve_projectile(mechanic: &Mechanic, preloaded: &PreloadedAssets, assets: &Assets<ProjectileDescriptor>) -> Result<Option<ProjectileDescriptor>, String> {
    let Mechanic::Projectile { descriptor } = mechanic else { return Ok(None); };
    let value = preloaded.get_single_asset::<ProjectileDescriptor>(descriptor)
        .and_then(|handle| assets.get(&handle)).cloned()
        .ok_or_else(|| format!("missing projectile {descriptor}"))?;
    value.validate()?;
    Ok(Some(value))
}

fn resolve_attack(mechanic: &Mechanic, preloaded: &PreloadedAssets, assets: &Assets<AttackDescriptor>) -> Result<Option<AttackDescriptor>, String> {
    let Mechanic::Contact { descriptor } = mechanic else { return Ok(None); };
    let value = preloaded.get_single_asset::<AttackDescriptor>(descriptor)
        .and_then(|handle| assets.get(&handle)).cloned()
        .ok_or_else(|| format!("missing attack {descriptor}"))?;
    value.validate()?;
    Ok(Some(value))
}

fn validate_movement_step(step: &mut super::motion::FrameStep, scale: f32, actor: &str) -> Result<(), String> {
    if step.frame.0 == 0 || step.delta.iter().any(|value| !value.is_finite()) {
        return Err(format!("{actor}: invalid frame step at {}", step.frame.0));
    }
    step.delta = [step.delta[0] * scale, step.delta[1] * scale];
    Ok(())
}

fn resolve_lunge_motion(mechanic: &Mechanic, actor: Option<&EnemyArchetype>) -> Option<super::action::LungeMotion> {
    if !matches!(mechanic, Mechanic::Lunge) { return None; }
    let actor = actor?;
    let speed = actor.stats.as_ref()?.chase_speed;
    let multiplier = actor.flying.and_then(|flying| flying.attack).and_then(|attack| attack.speed_multiplier).unwrap_or(2.0);
    Some(super::action::LungeMotion { speed: speed * multiplier, direction: None, impact_ticks: None })
}

fn resolve_action_damage(mechanic: &Mechanic, actor: Option<&EnemyArchetype>) -> Option<f32> {
    let stats = actor?.stats.as_ref()?;
    Some(match mechanic { Mechanic::Projectile { .. } => stats.dmg_ranged as f32, _ => stats.dmg_melee as f32 })
}

fn validate_action_motion(id: &str, mechanic: &Mechanic, motion: Option<super::action::LungeMotion>, hits: Option<super::lunge::LungeHits>) -> Result<(), String> {
    if matches!(mechanic, Mechanic::Lunge) && motion.is_none() { return Err(format!("{id}: lunge requires motion")); }
    if let Some(motion) = motion {
        if !motion.speed.is_finite() || motion.speed < 0.0 || motion.impact_ticks == Some(0)
            || motion.direction.is_some_and(|direction| direction.iter().any(|value| !value.is_finite()) || direction == [0.0, 0.0])
        { return Err(format!("{id}: invalid lunge motion")); }
    }
    if let Some(super::lunge::LungeHits::Sweep { radius, impact_radius, impact_max_targets }) = hits {
        if !radius.is_finite() || radius <= 0.0 || !impact_radius.is_finite() || impact_radius < 0.0 || impact_max_targets == 0 {
            return Err(format!("{id}: invalid sweep hit policy"));
        }
    }
    Ok(())
}

fn validate_clip_cues(id: &str, clip: &SpriteAnimation) -> Result<(), String> {
    let max = clip.settings.extended.frame_max;
    for event in &clip.events {
        let on = &event.on;
        let count = usize::from(on.frame_enter.is_some()) + usize::from(on.frame_exit.is_some())
            + usize::from(on.bookmark.is_some()) + usize::from(on.anim_finished);
        let invalid_frame = on.frame_enter.into_iter().chain(on.frame_exit).any(|frame| frame.0 == 0 || frame > max);
        if event.emit.is_empty() || count != 1 || invalid_frame || on.bookmark.as_ref().is_some_and(|name| !clip.frame_bookmarks.contains_key(name)) {
            return Err(format!("{id}: invalid cue {}", event.emit));
        }
    }
    Ok(())
}

fn validate_actor_body(
    actor: &EnemyArchetype,
    body: EnemySetupController,
    behavior: &ControllerSpec,
    actions: &HashMap<String, Arc<ResolvedAction>>,
) -> Result<(), String> {
    let compatible = match behavior {
        ControllerSpec::Combat { approach: super::controller::Approach::Flying, .. } | ControllerSpec::Pursuit => body == EnemySetupController::Flying,
        ControllerSpec::Combat { .. } => body == EnemySetupController::Ground,
        ControllerSpec::Hopper => body == EnemySetupController::Jump,
        ControllerSpec::Ambush => body == EnemySetupController::StationaryTrap,
        ControllerSpec::Wander => matches!(body, EnemySetupController::Ground | EnemySetupController::Flying),
    };
    if !compatible { return Err(format!("{}: controller is incompatible with body {body:?}", actor.id)); }
    validate_controller_bindings(actor, behavior, body)?;
    for (slot, action) in actions {
        validate_action_body(&actor.id, slot, &action.mechanic, body)?;
    }
    Ok(())
}

fn validate_controller_bindings(actor: &EnemyArchetype, behavior: &ControllerSpec, body: EnemySetupController) -> Result<(), String> {
    use super::controller::Approach;
    let mut clips = vec!["Idle"];
    let mut actions = Vec::new();
    match behavior {
        ControllerSpec::Combat { approach, defensive, .. } => {
            actions.push("Attack");
            if *defensive { actions.push("Defense"); }
            clips.extend(match approach { Approach::Ground => Some("Chase"), Approach::Roll => Some("Roll"), Approach::Flying => Some("Fly"), Approach::Stationary => None });
        },
        ControllerSpec::Hopper => { actions.extend(["Jump", "AirJump"]); clips.push("Fall"); },
        ControllerSpec::Ambush => { actions.extend(["AttackLeft", "AttackRight"]); clips.extend(["Prepare", "EatLeft", "EatRight"]); },
        ControllerSpec::Wander => {},
        ControllerSpec::Pursuit => { clips.push("Fly"); },
    }
    if body == EnemySetupController::Ground { clips.push("Walk"); }
    for slot in clips {
        if !actor.anim.contains_key(slot) { return Err(format!("{}: controller requires animation binding {slot}", actor.id)); }
    }
    for slot in actions {
        if !actor.actions.contains_key(slot) { return Err(format!("{}: controller requires action binding {slot}", actor.id)); }
    }
    Ok(())
}

fn validate_action_body(id: &str, slot: &str, mechanic: &Mechanic, body: EnemySetupController) -> Result<(), String> {
    let compatible = match mechanic {
        Mechanic::Lunge => body == EnemySetupController::Flying,
        Mechanic::Jump => body == EnemySetupController::Jump,
        Mechanic::TrapStrike => body == EnemySetupController::StationaryTrap,
        Mechanic::TentacleStrike { .. } => false,
        Mechanic::Projectile { .. } | Mechanic::Contact { .. } | Mechanic::Guard => true,
    };
    if compatible { Ok(()) } else { Err(format!("{id}: action {slot} is incompatible with body {body:?}")) }
}
