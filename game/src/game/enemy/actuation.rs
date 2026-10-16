use super::*;
use bevy::ecs::system::SystemParam;
use crate::game::actors::{ActorAction, catalog::ActorCatalog};
use crate::game::actors::action::{ActiveAction, ActionRequest, ActionPresentation, ResolvedAction};
use crate::game::actors::controller::{Activity, Approach, ControllerSpec, PatrolPhase};
use theseeker_engine::animation::{PlaybackState, PlaybackId};

type ActorMotorQuery<'w, 's> = Query<'w, 's, (
    Entity, &'static mut ActorController, &'static mut ActorAction,
    &'static mut LinearVelocity, &'static mut Facing, &'static TargetSensor,
    &'static Gent, &'static Transform, &'static mut TurnCooldown,
    Option<&'static mut ProjectileCache>, Option<&'static mut MovementState>,
    Option<&'static mut FlyingMotor>, Has<Knockback>,
), (Without<Dead>, Without<crate::game::effects::frozen::Frozen>)>;

#[derive(SystemParam)]
pub(super) struct ActuationWorld<'w, 's> {
    commands: Commands<'w, 's>,
    gfx: Query<'w, 's, &'static mut ScriptPlayer<SpriteAnimation>, With<EnemyGfx>>,
    targets: Query<'w, 's, &'static Transform, With<Player>>,
    signals: Query<'w, 's, &'static PlaybackState>,
    particles: Res<'w, ArcParticleEffectHandle>,
    catalog: Res<'w, ActorCatalog>,
    time: Res<'w, GameTime>,
}

struct ActorTick<'a> {
    entity: Entity,
    actor: &'a mut ActorController,
    action: &'a mut ActorAction,
    velocity: &'a mut LinearVelocity,
    facing: &'a mut Facing,
    target: &'a TargetSensor,
    gent: &'a Gent,
    transform: &'a Transform,
    turn: &'a mut TurnCooldown,
    cache: Option<&'a mut ProjectileCache>,
    ground: Option<&'a mut MovementState>,
    flying: Option<&'a mut FlyingMotor>,
    knocked: bool,
}

pub(super) fn enemy_actor_actuator(mut query: ActorMotorQuery, mut world: ActuationWorld) {
    for (entity, mut actor, mut action, mut velocity, mut facing, target, gent,
        transform, mut turn, mut cache, mut ground, mut flying, knocked) in &mut query
    {
        tick_actor(&mut ActorTick {
            entity, actor: &mut actor, action: &mut action, velocity: &mut velocity,
            facing: &mut facing, target, gent, transform, turn: &mut turn,
            cache: cache.as_deref_mut(), ground: ground.as_deref_mut(),
            flying: flying.as_deref_mut(), knocked,
        }, &mut world);
    }
}

fn tick_actor(tick: &mut ActorTick, world: &mut ActuationWorld) {
    tick.turn.timer = tick.turn.timer.saturating_sub(1);
    if tick.actor.entered {
        enter_activity(tick, world);
    }
    if tick.actor.activity == Activity::Patrol && tick.actor.patrol_changed && tick.ground.is_some() {
        apply_ground_patrol(tick, world);
    }
    if tick.actor.activity == Activity::Approach {
        approach_target(tick, world);
    }
    adopt_looping_presentation(tick, world);
    advance_action(tick, world);

}

fn adopt_looping_presentation(tick: &mut ActorTick, world: &mut ActuationWorld) {
    if tick.actor.entered || tick.action.0.is_some() { return; }
    let looped = world.signals.get(tick.gent.e_gfx).is_ok_and(|signals| {
        signals.playback == tick.actor.playback && signals.looped
    });
    if !looped { return; }
    let Some(latest) = world.catalog.actors.get(&tick.actor.definition.archetype.id) else { return; };
    let slot = locomotion_animation(tick.actor, tick.ground.is_some());
    if same_clip(tick.actor.definition.clips.get(slot), latest.clips.get(slot)) { return; }
    tick.actor.definition = Arc::new(tick.actor.definition.adopt_actions(latest));
    play_actor_animation(tick, world, slot);
    if slot == "Walk" { bind_walk_playback(tick, world); }
}

fn same_clip(current: Option<&Arc<SpriteAnimation>>, latest: Option<&Arc<SpriteAnimation>>) -> bool {
    match (current, latest) {
        (Some(current), Some(latest)) => Arc::ptr_eq(current, latest),
        (None, None) => true,
        _ => false,
    }
}

fn locomotion_animation(actor: &ActorController, ground: bool) -> &'static str {
    if actor.activity == Activity::Patrol && actor.patrol_phase == PatrolPhase::Walk
        && ground {
        return "Walk";
    }
    activity_animation(actor)
}

fn enter_activity(tick: &mut ActorTick, world: &mut ActuationWorld) {
    if let Some(mut active) = tick.action.0.take() {
        active.interrupt(&mut world.commands);
        if let Ok(mut player) = world.gfx.get_mut(tick.gent.e_gfx) { player.release_presentation(active.id); }
    }
    let id = &tick.actor.definition.archetype.id;
    if let Some(definition) = world.catalog.actors.get(id) {
        tick.actor.definition = Arc::new(tick.actor.definition.adopt_actions(definition));
    }
    apply_motor_intent(tick, world);
    let animation = activity_animation(tick.actor);
    let definition = tick.actor.definition.actions.get(animation).cloned();
    if let Some(definition) = definition {
        start_action(tick, world, definition);
    } else {
        play_actor_animation(tick, world, animation);
    }
}

fn activity_animation(actor: &ActorController) -> &'static str {
    match actor.activity {
        Activity::Patrol | Activity::Idle => "Idle",
        Activity::Approach => match actor.definition.behavior {
            ControllerSpec::Combat { approach: Approach::Roll, .. } => "Roll",
            ControllerSpec::Combat { approach: Approach::Flying, .. } | ControllerSpec::Pursuit => "Fly",
            _ => "Chase",
        },
        Activity::Attack => "Attack",
        Activity::Defense => "Defense",
        Activity::Recover => "Fly",
        Activity::Jump => "Jump",
        Activity::AirJump => "AirJump",
        Activity::Fall => "Fall",
        Activity::Prepare => "Prepare",
        Activity::AttackLeft => "AttackLeft",
        Activity::AttackRight => "AttackRight",
        Activity::EatLeft => "EatLeft",
        Activity::EatRight => "EatRight",
        Activity::Death => "Die",
    }
}

fn start_action(tick: &mut ActorTick, world: &mut ActuationWorld, definition: Arc<ResolvedAction>) {
    face_target(tick, world);
    let mut action = ActionRequest { forward: tick.facing.direction(), actor: tick.entity, target: tick.target.entity,
        damage: definition.damage.unwrap_or(0.0), definition }.activate(PlaybackId::default());
    if let Ok(mut player) = world.gfx.get_mut(tick.gent.e_gfx) {
        action.present(&mut player, ActionPresentation::Execute);
    }
    tick.actor.playback = action.playback;
    action.claim_facing(&mut world.commands);
    tick.action.0 = Some(action);
}

fn play_actor_animation(tick: &mut ActorTick, world: &mut ActuationWorld, slot: &str) {
    let animations = &tick.actor.definition.archetype.anim;
    let Some(key) = animations.get(slot) else { return; };
    if let Ok(mut player) = world.gfx.get_mut(tick.gent.e_gfx) {
        let clip = tick.actor.definition.clips.get(slot).expect("resolved locomotion clip").clone();
        player.request_background_snapshot(key, clip);
        tick.actor.playback = PlaybackId(player.execution_id());
    }
}

fn apply_ground_patrol(tick: &mut ActorTick, world: &mut ActuationWorld) {
    if !tick.actor.entered {
        if let Some(latest) = world.catalog.actors.get(&tick.actor.definition.archetype.id) {
            tick.actor.definition = Arc::new(tick.actor.definition.adopt_actions(latest));
        }
    }
    match tick.actor.patrol_phase {
        PatrolPhase::Idle => {
            stop_ground(tick);
            play_actor_animation(tick, world, "Idle");
        },
        PatrolPhase::Walk => {
            set_ground_motion(tick, MovementType::Walking, MotionBlockedBehavior::TurnAround);
            play_actor_animation(tick, world, "Walk");
            bind_walk_playback(tick, world);
        },
    }
}

fn apply_motor_intent(tick: &mut ActorTick, world: &mut ActuationWorld) {
    let activity = tick.actor.activity;
    if let Some(motor) = tick.flying.as_deref_mut() {
        let intent = flying_intent(tick.actor, activity);
        if intent != flying_intent(tick.actor, tick.actor.previous_activity) {
            set_flying_mode(intent, motor, tick.actor, tick.velocity, tick.gent, &mut world.gfx, &mut world.commands);
        }
    }
    if activity != Activity::Approach {
        stop_ground(tick);
    }
}

pub(super) fn flying_intent(actor: &ActorController, activity: Activity) -> flying_motor::FlyingMode {
    use flying_motor::FlyingMode;
    match activity {
        Activity::Approach => FlyingMode::Stalk,
        Activity::Attack => FlyingMode::Attack,
        Activity::Recover if actor.aligns_during_recovery() => FlyingMode::Stalk,
        Activity::Recover => FlyingMode::Recover,
        _ => FlyingMode::Patrol,
    }
}

pub(super) fn jump_intent(activity: Activity) -> jump_motor::JumpMotorMode {
    use jump_motor::JumpMotorMode;
    match activity {
        Activity::Jump => JumpMotorMode::Jump,
        Activity::AirJump => JumpMotorMode::AirJump,
        Activity::Fall => JumpMotorMode::Fall,
        Activity::Death => JumpMotorMode::Death,
        _ => JumpMotorMode::Idle,
    }
}

pub(super) fn trap_intent(activity: Activity) -> trap_controller::StationaryTrapMode {
    use trap_controller::StationaryTrapMode;
    match activity {
        Activity::Prepare => StationaryTrapMode::Prepare,
        Activity::AttackLeft => StationaryTrapMode::AttackLeft,
        Activity::AttackRight => StationaryTrapMode::AttackRight,
        Activity::EatLeft => StationaryTrapMode::EatLeft,
        Activity::EatRight => StationaryTrapMode::EatRight,
        _ => StationaryTrapMode::Idle,
    }
}

fn approach_target(tick: &mut ActorTick, world: &ActuationWorld) {
    face_target(tick, world);
    let ControllerSpec::Combat { approach, .. } = tick.actor.definition.behavior else { return; };
    match approach {
        Approach::Ground => {
            if !tick.knocked {
                tick.velocity.0.x = target_position(tick, world)
                    .map(|target| (target.x - tick.transform.translation.x).signum()).unwrap_or(0.0);
            }
            set_ground_motion(tick, MovementType::Chasing, MotionBlockedBehavior::Stop);
        },
        Approach::Roll => set_ground_motion(tick, MovementType::Rolling, MotionBlockedBehavior::Stop),
        Approach::Flying | Approach::Stationary => {},
    }
}

fn set_ground_motion(tick: &mut ActorTick, motion: MovementType, blocked: MotionBlockedBehavior) {
    if tick.knocked { return; }
    if let Some(ground) = tick.ground.as_deref_mut() {
        ground.set_blocked_behavior(blocked);
        ground.set_motion_type(motion);
    }
}

fn stop_ground(tick: &mut ActorTick) {
    if tick.knocked { return; }
    if let Some(ground) = tick.ground.as_deref_mut() {
        ground.stop(tick.velocity);
    }
}

fn face_target(tick: &mut ActorTick, world: &ActuationWorld) {
    if tick.turn.timer > 0 { return; }
    let Some(target) = target_position(tick, world) else { return; };
    let facing = if target.x >= tick.transform.translation.x { Facing::Right } else { Facing::Left };
    if *tick.facing != facing {
        *tick.facing = facing;
        tick.turn.timer = 48;
    }
}

fn target_position(tick: &ActorTick, world: &ActuationWorld) -> Option<Vec2> {
    tick.target.entity.and_then(|target| world.targets.get(target).ok())
        .map(|target| target.translation.xy())
}

fn advance_action(tick: &mut ActorTick, world: &mut ActuationWorld) {
    let Some(mut active) = tick.action.0.take() else { return; };
    let effects = if tick.actor.entered {
        active.advance(world.time.tick(), world.signals.get(tick.gent.e_gfx).ok(), false)
    } else {
        std::mem::take(&mut tick.action.1)
    };
    for _ in 0..effects.releases {
        release_action(tick, world, &mut active);
    }
    if effects.restart_clip {
        if let Ok(mut player) = world.gfx.get_mut(tick.gent.e_gfx) { active.present(&mut player, ActionPresentation::Execute); }
    }
    if effects.outcome.is_some() {
        active.cleanup(&mut world.commands);
    }
    tick.actor.playback = active.playback;
    tick.action.0 = Some(active);
}

fn release_action(tick: &mut ActorTick, world: &mut ActuationWorld, active: &mut ActiveAction) {
    face_target(tick, world);
    if let Some(descriptor) = &active.definition.projectile {
        let target = target_position(tick, world);
        spawn_enemy_projectile_from_descriptor(&mut world.commands, EnemyProjectileSpawn {
            owner: tick.entity, start: tick.transform.translation.xy(), target,
            facing: *tick.facing, descriptor,
            clips: active.definition.projectile_clips.as_ref(),
            damage: active.damage,
            particle_effect: world.particles.0.clone(), cache: tick.cache.as_deref_mut(),
        });
    }
    if let Some(descriptor) = &active.definition.attack {
        let hitbox = spawn_enemy_attack_from_descriptor(&mut world.commands, EnemyAttackSpawn {
            owner: tick.entity, gfx: tick.gent.e_gfx, descriptor,
            damage: active.damage,
        });
        active.owned_entities.push(hitbox);
    }
}

fn bind_walk_playback(tick: &mut ActorTick, world: &ActuationWorld) {
    let Some(ground) = tick.ground.as_deref_mut() else { return; };
    if let Ok(player) = world.gfx.get(tick.gent.e_gfx) {
        ground.walking_playback = PlaybackId(player.execution_id());
    }
}

pub(super) fn finish_actor_tick(mut actors: Query<&mut ActorController, Without<AiPaused>>) {
    for mut actor in &mut actors { actor.entered = false; }
}
