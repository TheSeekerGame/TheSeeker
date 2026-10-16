use super::*;
use bevy::prelude::*;
use crate::time::GameTickUpdate;

#[derive(Actionlike, Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum TestAction { Act }

#[derive(Resource, Default)]
struct TickCount(usize);
#[derive(Resource, Default)]
struct Delivered(Vec<(bool, bool, bool)>);

fn run_ticks(world: &mut World) {
    for _ in 0..world.resource::<TickCount>().0 {
        world.run_schedule(GameTickUpdate);
        world.run_schedule(GameTickPost);
    }
}

fn record_input(actions: Query<&ActionState<TestAction>>, mut delivered: ResMut<Delivered>) {
    let action = actions.single().unwrap();
    delivered.0.push((action.pressed(&TestAction::Act),
        action.just_pressed(&TestAction::Act), action.just_released(&TestAction::Act)));
}

#[test]
fn input_edges_survive_zero_tick_frames_and_are_delivered_once_during_catchup() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, bevy::input::InputPlugin, InputManagerPlugin::<TestAction>::default()));
    app.init_resource::<TickCount>().init_resource::<Delivered>();
    app.add_systems(Update, run_ticks.after(GameTickSet::Pre).before(GameTickSet::Post));
    app.add_systems(GameTickUpdate, record_input);
    app.world_mut().spawn((ActionState::<TestAction>::default(), InputMap::new([(TestAction::Act, KeyCode::Space)])));
    app.update();
    app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Space);
    app.update();
    app.update();
    app.world_mut().resource_mut::<TickCount>().0 = 3;
    app.update();
    assert_eq!(app.world().resource::<Delivered>().0,
        [(true, true, false), (true, false, false), (true, false, false)]);

    app.world_mut().resource_mut::<TickCount>().0 = 0;
    app.world_mut().resource_mut::<ButtonInput<KeyCode>>().release(KeyCode::Space);
    app.update();
    app.update();
    app.world_mut().resource_mut::<TickCount>().0 = 2;
    app.update();
    assert_eq!(&app.world().resource::<Delivered>().0[3..],
        [(false, false, true), (false, false, false)]);
}
