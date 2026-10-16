use super::*;
use crate::game::gentstate::Facing;
use crate::game::player::weapon::{PlayerMeleeWeapon, WeaponType};

fn actor(world: &mut World) -> (Entity, Entity) {
    let gfx = world.spawn(ScriptPlayer::<SpriteAnimation>::default()).id();
    let actor = world.spawn(Gent { e_gfx: gfx, e_effects_gfx: gfx }).id();
    start_action(world, actor, Ready);
    (actor, gfx)
}

fn play(world: &mut World, actor: Entity, key: &str) -> u64 {
    let gfx = world.get::<Gent>(actor).unwrap().e_gfx;
    let mut state = bevy::ecs::system::SystemState::<(
        Query<&mut ForegroundAction>, Query<&mut ScriptPlayer<SpriteAnimation>>,
    )>::new(world);
    let (mut actions, mut scripts) = state.get_mut(world).expect("fixture system parameters are valid");
    let mut action = actions.get_mut(actor).unwrap();
    action.play(&mut scripts.get_mut(gfx).unwrap(), key);
    action.id
}

#[test]
fn replacing_a_mechanic_preserves_one_execution_and_query_index() {
    let mut world = World::new();
    let (actor, _) = actor(&mut world);
    start_action(&mut world, actor, Attacking::new(AttackVariant::Forward, WeaponType::Sword));
    let first = world.get::<ForegroundAction>(actor).unwrap().id;
    start_action(&mut world, actor, Spitting::new(Facing::Left));
    world.flush();
    let action = world.get::<ForegroundAction>(actor).unwrap();
    assert_ne!(first, action.id);
    assert!(action.state::<Spitting>().is_some());
    assert!(world.get::<ActionTag<Attacking>>(actor).is_none());
    assert!(world.get::<ActionTag<Ready>>(actor).is_none());
    let mut query = world.query::<ActionRef<Spitting>>();
    assert_eq!(query.single(&world).unwrap().facing, Facing::Left);
}

#[test]
fn stale_completion_cannot_finish_a_restart_of_the_same_mechanic() {
    let mut world = World::new();
    let (actor, _) = actor(&mut world);
    start_action(&mut world, actor, Dashing::new());
    let first = world.get::<ForegroundAction>(actor).unwrap().id;
    start_action(&mut world, actor, Dashing::new());
    let second = world.get::<ForegroundAction>(actor).unwrap().id;
    finish_action(&mut world.commands(), actor, first);
    world.flush();
    assert_eq!(world.get::<ForegroundAction>(actor).unwrap().id, second);
    assert!(world.get::<GhostingSource>(actor).is_some());
    finish_action(&mut world.commands(), actor, second);
    world.flush();
    assert!(world.get::<ForegroundAction>(actor).unwrap().state::<Ready>().is_some());
    assert!(world.get::<GhostingSource>(actor).is_none());
    assert!(world.get::<InAir>(actor).is_none());
}

#[test]
fn direct_removal_cleans_hitboxes_tags_and_overrides_but_keeps_independent_objects() {
    let mut world = World::new();
    let (actor, _) = actor(&mut world);
    start_action(&mut world, actor, BurningDashing::new());
    let hitbox = world.spawn_empty().id();
    let projectile = world.spawn_empty().id();
    world.get_mut::<ForegroundAction>(actor).unwrap().own(hitbox);
    world.entity_mut(actor).remove::<ForegroundAction>();
    world.flush();
    assert!(world.get_entity(hitbox).is_err());
    assert!(world.get_entity(projectile).is_ok());
    assert!(world.get::<ActionTag<BurningDashing>>(actor).is_none());
    assert!(world.get::<ActionTag<Ready>>(actor).is_some());
    assert!(!world.get::<ForegroundAction>(actor).unwrap().owns_motion());
    assert!(world.get::<ChanneledSkill>(actor).is_none());
    assert!(world.get::<GhostingSource>(actor).is_none());
    assert!(world.get::<InAir>(actor).is_none());
}

#[test]
fn presentation_and_cues_belong_to_the_execution_that_started_them() {
    let mut world = World::new();
    let (actor, gfx) = actor(&mut world);
    world.get_mut::<ScriptPlayer<SpriteAnimation>>(gfx).unwrap().request_background("Idle");
    start_action(&mut world, actor, Dashing::new());
    let old = play(&mut world, actor, "copied");
    let old_playback = world.get::<ForegroundAction>(actor).unwrap().playback;
    world.get_mut::<ScriptPlayer<SpriteAnimation>>(gfx).unwrap().request_background("Run");
    start_action(&mut world, actor, Dashing::new());
    let current = play(&mut world, actor, "copied");
    let stale = PlaybackState { playback: old_playback, finished: true, ..Default::default() };
    assert!(world.get::<ForegroundAction>(actor).unwrap().playback(Some(&stale)).is_none());
    finish_action(&mut world.commands(), actor, old);
    world.flush();
    assert!(!world.get_mut::<ScriptPlayer<SpriteAnimation>>(gfx).unwrap().release_presentation(old));
    finish_action(&mut world.commands(), actor, current);
    world.flush();
    assert_eq!(world.get::<ScriptPlayer<SpriteAnimation>>(gfx).unwrap().current_key(), Some("Run"));
}

#[test]
fn death_interrupts_foreground_before_another_action_can_start() {
    let mut world = World::new();
    world.add_observer(interrupt_on_death);
    let (actor, _) = actor(&mut world);
    start_action(&mut world, actor, Whirling::new(PlayerMeleeWeapon::Sword));
    let hitbox = world.spawn_empty().id();
    world.get_mut::<ForegroundAction>(actor).unwrap().state_mut::<Whirling>().unwrap().damage_entity = Some(hitbox);
    world.entity_mut(actor).insert(Dead::default());
    world.flush();
    start_action(&mut world, actor, Attacking::new(AttackVariant::Forward, WeaponType::Sword));
    assert!(world.get::<ForegroundAction>(actor).is_none());
    assert!(world.get_entity(hitbox).is_err());
    assert!(world.get::<ChanneledSkill>(actor).is_none());
}

#[test]
fn interrupted_playback_releases_motion_even_when_another_clip_has_started() {
    use theseeker_engine::animation::{PlaybackEvent, PlaybackEventKind};
    let mut world = World::new();
    let (actor, gfx) = actor(&mut world);
    start_action(&mut world, actor, Dashing::new());
    play(&mut world, actor, "dash");
    let owned_playback = world.get::<ForegroundAction>(actor).unwrap().playback;
    world.entity_mut(gfx).insert(PlaybackState {
        playback: PlaybackId(owned_playback.0 + 1), ordered: vec![PlaybackEvent {
            playback: owned_playback, kind: PlaybackEventKind::Interrupted,
        }], ..Default::default()
    });
    let mut schedule = Schedule::default();
    schedule.add_systems(interrupt_stopped_playback);
    schedule.run(&mut world);
    assert!(world.get::<ActionTag<Ready>>(actor).is_some());
    assert!(!world.get::<ForegroundAction>(actor).unwrap().owns_motion());
    assert!(world.get::<GhostingSource>(actor).is_none());
}

#[test]
fn removal_followed_by_replacement_cannot_leave_a_stale_query_index() {
    let mut world = World::new();
    let (actor, _) = actor(&mut world);
    start_action(&mut world, actor, Dashing::new());
    world.entity_mut(actor).remove::<ForegroundAction>();
    start_action(&mut world, actor, Spitting::new(Facing::Right));
    world.flush();
    assert!(world.get::<ActionTag<Dashing>>(actor).is_none());
    assert!(world.get::<ActionTag<Ready>>(actor).is_none());
    assert_eq!(world.query::<ActionRef<Spitting>>().single(&world).unwrap().facing, Facing::Right);
}

#[test]
fn presentation_completion_restores_the_base_anchor_without_touching_a_replacement() {
    let mut world = World::new();
    let (actor, gfx) = actor(&mut world);
    let base = crate::game::player::PLAYER_GFX_OFFSET;
    world.entity_mut(gfx).insert((
        crate::game::player::PlayerGfx { e_gent: actor, base_offset: base },
        TransformGfxFromGent { gent: actor, pixel_aligned: true, offset: Some(base) },
    ));
    start_action(&mut world, actor, Spitting::new(Facing::Right));
    let old = play(&mut world, actor, "spit");
    world.get_mut::<TransformGfxFromGent>(gfx).unwrap().offset = Some(Vec3::Y * 4.0);
    finish_action(&mut world.commands(), actor, old);
    world.flush();
    assert_eq!(world.get::<TransformGfxFromGent>(gfx).unwrap().offset, Some(base));

    start_action(&mut world, actor, Dashing::new());
    let current = play(&mut world, actor, "dash");
    let replacement_anchor = Vec3::Y * 7.0;
    world.get_mut::<TransformGfxFromGent>(gfx).unwrap().offset = Some(replacement_anchor);
    finish_action(&mut world.commands(), actor, old);
    world.flush();
    assert_eq!(world.get::<TransformGfxFromGent>(gfx).unwrap().offset, Some(replacement_anchor));
    finish_action(&mut world.commands(), actor, current);
    world.flush();
    assert_eq!(world.get::<TransformGfxFromGent>(gfx).unwrap().offset, Some(base));
}
