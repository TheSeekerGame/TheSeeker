use super::*;
use crate::game::player::{foreground::ActionTag, states::{Dashing, Ready}};
use theseeker_engine::animation::{PlaybackEvent, PlaybackEventKind, PlaybackId};

#[derive(Component)]
struct Deployed { owner: Entity, position: Vec2 }

#[derive(Resource)]
struct PlacementAllowed(bool);

fn spawn_item(commands: &mut Commands, owner: Entity, position: Vec2) {
    commands.spawn(Deployed { owner, position });
}

fn valid_placement(world: &World, _: Entity, _: Vec2) -> bool {
    world.resource::<PlacementAllowed>().0
}

fn definition() -> Arc<DeploymentDefinition> {
    let clip: SpriteAnimation = toml::from_str(r#"
[playback]
start = 1
end = 4
ticks_per_frame = 2
mode = "once"
[settings]
time_base = "Relative"
frame_start = 1
frame_min = 1
frame_max = 4
ticks_per_frame = 2
[[events]]
on = { frame_enter = 3 }
emit = "place_item"
"#).unwrap();
    Arc::new(DeploymentDefinition {
        material: CraftMaterial::ExplosiveMine, amount: 1,
        animation_key: "anim.test.Deploy".to_owned(), clip: Arc::new(clip),
        release: "place_item".to_owned(), spawn: spawn_item, validate: valid_placement,
    })
}

fn fixture(stock: u32) -> (World, Entity, Entity, Schedule) {
    let mut world = World::new();
    world.insert_resource(PlacementAllowed(true));
    world.add_observer(foreground::interrupt_on_death);
    let gfx = world.spawn(ScriptPlayer::<SpriteAnimation>::default()).id();
    let mut inventory = MaterialInventory::empty();
    inventory.add_to_backpack(CraftMaterial::ExplosiveMine, stock);
    let player = world.spawn((Player, inventory, Gent { e_gfx: gfx, e_effects_gfx: gfx },
        Health { current: 10, max: 10 }, LinearVelocity(Vec2::new(3.0, -2.0)))).id();
    foreground::start_action(&mut world, player, Ready);
    let mut schedule = Schedule::default();
    schedule.add_systems(tick_deployment);
    (world, player, gfx, schedule)
}

fn remaining(world: &World, player: Entity) -> u32 {
    world.get::<MaterialInventory>(player).unwrap().backpack_count(CraftMaterial::ExplosiveMine)
}

fn action_id(world: &World, player: Entity) -> u64 { world.get::<ForegroundAction>(player).unwrap().id }

fn emit(world: &mut World, gfx: Entity, playback: PlaybackId, finished: bool) {
    world.entity_mut(gfx).insert(PlaybackState {
        playback, finished, ordered: vec![PlaybackEvent { playback,
            kind: PlaybackEventKind::Cue(cue_id("place_item")) }], ..Default::default()
    });
}

#[test]
fn delayed_item_reserves_stock_releases_at_its_own_cue_and_survives_completion() {
    let (mut world, player, gfx, mut schedule) = fixture(2);
    let definition = definition();
    assert!(start_deployment(&mut world, player, definition.clone(), Vec2::new(8.0, 4.0)));
    assert_eq!(remaining(&world, player), 1);
    assert!(Arc::ptr_eq(&world.get::<ForegroundAction>(player).unwrap().state::<Deploying>().unwrap().definition, &definition));
    schedule.run(&mut world);
    assert_eq!(world.get::<LinearVelocity>(player).unwrap().0, Vec2::ZERO);
    let playback = world.get::<ForegroundAction>(player).unwrap().playback;
    emit(&mut world, gfx, PlaybackId(playback.0 + 1), false);
    schedule.run(&mut world);
    assert_eq!(world.query::<&Deployed>().iter(&world).count(), 0);
    emit(&mut world, gfx, playback, false);
    schedule.run(&mut world);
    schedule.run(&mut world);
    assert_eq!(world.query::<&Deployed>().iter(&world).count(), 1);
    assert_eq!(remaining(&world, player), 1);
    let deployed = world.query::<&Deployed>().single(&world).unwrap();
    assert_eq!(deployed.owner, player);
    assert_eq!(deployed.position, Vec2::new(8.0, 4.0));
    emit(&mut world, gfx, playback, true);
    schedule.run(&mut world);
    assert!(world.get::<ActionTag<Ready>>(player).is_some());
    assert_eq!(remaining(&world, player), 1);
    assert_eq!(world.query::<&Deployed>().iter(&world).count(), 1);
}

#[test]
fn pre_release_cancellation_refunds_once_and_stale_release_cannot_spend_again() {
    let (mut world, player, _, _) = fixture(1);
    assert!(start_deployment(&mut world, player, definition(), Vec2::ZERO));
    let old = action_id(&world, player);
    assert!(ItemReservation::reserve(&mut world.get_mut::<MaterialInventory>(player).unwrap(), CraftMaterial::ExplosiveMine, 1).is_none());
    foreground::start_action(&mut world, player, Dashing::new());
    world.flush();
    release_deployment(&mut world, player, old);
    assert_eq!(remaining(&world, player), 1);
    assert!(world.get::<ActionTag<Dashing>>(player).is_some());
    assert_eq!(world.query::<&Deployed>().iter(&world).count(), 0);
}

#[test]
fn placement_is_revalidated_at_release_and_invalid_placement_refunds() {
    let (mut world, player, _, _) = fixture(1);
    assert!(start_deployment(&mut world, player, definition(), Vec2::ZERO));
    let id = action_id(&world, player);
    world.resource_mut::<PlacementAllowed>().0 = false;
    release_deployment(&mut world, player, id);
    world.flush();
    assert_eq!(remaining(&world, player), 1);
    assert!(world.get::<ActionTag<Ready>>(player).is_some());
    assert_eq!(world.query::<&Deployed>().iter(&world).count(), 0);
}

#[test]
fn rejected_deployment_leaves_inventory_and_foreground_untouched() {
    let (mut world, player, _, _) = fixture(1);
    let id = action_id(&world, player);
    world.resource_mut::<PlacementAllowed>().0 = false;
    assert!(!start_deployment(&mut world, player, definition(), Vec2::ZERO));
    world.resource_mut::<PlacementAllowed>().0 = true;
    let mut invalid = definition();
    Arc::get_mut(&mut invalid).unwrap().release = "missing".to_owned();
    assert!(!start_deployment(&mut world, player, invalid, Vec2::ZERO));
    assert!(!start_deployment(&mut world, player, definition(), Vec2::NAN));
    assert_eq!(remaining(&world, player), 1);
    assert_eq!(action_id(&world, player), id);
}

#[test]
fn death_and_component_removal_cancel_pending_deployment() {
    for dead in [false, true] { assert_cancelled_on_removal(dead); }
}

fn assert_cancelled_on_removal(dead: bool) {
    let (mut world, player, _, _) = fixture(1);
    assert!(start_deployment(&mut world, player, definition(), Vec2::ZERO));
    let id = action_id(&world, player);
    if dead { world.entity_mut(player).insert(Dead::default()); }
    else { world.entity_mut(player).remove::<ForegroundAction>(); }
    world.flush();
    release_deployment(&mut world, player, id);
    world.flush();
    assert_eq!(remaining(&world, player), 1);
    assert_eq!(world.query::<&Deployed>().iter(&world).count(), 0);
    assert!(world.get::<ActionTag<Deploying>>(player).is_none());
}

#[test]
fn instant_item_cost_does_not_take_motion_or_foreground_ownership() {
    let (mut world, player, _, _) = fixture(2);
    foreground::start_action(&mut world, player, Dashing::new());
    let id = action_id(&world, player);
    let reservation = ItemReservation::reserve(&mut world.get_mut::<MaterialInventory>(player).unwrap(), CraftMaterial::ExplosiveMine, 1).unwrap();
    reservation.consume();
    spawn_item(&mut world.commands(), player, Vec2::X);
    world.flush();
    assert_eq!(action_id(&world, player), id);
    assert_eq!(remaining(&world, player), 1);
    assert_eq!(world.get::<LinearVelocity>(player).unwrap().0, Vec2::new(3.0, -2.0));
    assert_eq!(world.query::<&Deployed>().iter(&world).count(), 1);
}

#[test]
fn release_cue_must_be_reachable_and_deployment_must_complete() {
    let mut definition = definition();
    let definition = Arc::get_mut(&mut definition).unwrap();
    let clip = Arc::make_mut(&mut definition.clip);
    clip.playback.as_mut().unwrap().end = theseeker_engine::assets::animation::FrameId(2);
    assert!(definition.validate().unwrap_err().contains("place_item"));
    let clip = Arc::make_mut(&mut definition.clip);
    clip.playback.as_mut().unwrap().mode = theseeker_engine::assets::animation::ClipMode::Loop;
    assert!(definition.validate().unwrap_err().contains("complete"));
}
