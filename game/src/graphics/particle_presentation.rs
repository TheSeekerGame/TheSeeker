//! Gameplay owns emitter descriptors; Hanabi owns delayed render instances.
use bevy::camera::visibility::RenderLayers;
use bevy_hanabi::{EffectProperties, EffectSimulation, ParticleEffect};
use theseeker_engine::presentation::{PresentationSet, TickHistory, interpolate_transform};
use crate::prelude::*;

#[derive(Component)]
pub struct PresentedEffect(pub ParticleEffect);

#[derive(Clone)]
struct EmitterPose {
    transform: Transform,
    properties: EffectProperties,
    layers: RenderLayers,
    visible: bool,
}

struct EmitterTrack {
    effect: ParticleEffect,
    proxy: Option<Entity>,
    history: TickHistory<EmitterPose>,
    dead_at: Option<u64>,
}

#[derive(Resource, Default)]
struct Emitters(HashMap<Entity, EmitterTrack>);

pub struct ParticlePresentationPlugin;
impl Plugin for ParticlePresentationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Emitters>();
        app.add_systems(GameTickPost, capture_emitters.in_set(PresentationSet::Capture));
        app.add_systems(Update, sync_particle_time.in_set(GameTickSet::Post));
        app.add_systems(PostUpdate, present_emitters.in_set(PresentationSet::Present));
        app.add_systems(OnExit(AppState::InGame), clear_emitters);
    }
}

fn sync_particle_time(
    presentation: Res<PresentationTime>, mut effect: ResMut<Time<EffectSimulation>>,
    mut clock: Local<Time<EffectSimulation>>,
) {
    clock.advance_to(presentation.elapsed());
    *effect = *clock;
}

type EmitterQuery<'w, 's> = Query<'w, 's, (Entity, &'static PresentedEffect,
    Option<&'static EffectProperties>, Option<&'static RenderLayers>, Option<&'static Visibility>)>;

fn capture_emitters(
    time: Res<GameTime>, query: EmitterQuery, transforms: TransformHelper,
    mut emitters: ResMut<Emitters>,
) {
    for track in emitters.0.values_mut() {
        if track.dead_at.is_none() { track.dead_at = Some(time.tick()); }
    }
    for (entity, effect, properties, layers, visibility) in &query {
        let Ok(transform) = transforms.compute_global_transform(entity) else { continue; };
        let track = emitters.0.entry(entity).or_insert_with(|| EmitterTrack {
            effect: effect.0.clone(), proxy: None, history: TickHistory::default(), dead_at: None,
        });
        track.dead_at = None;
        track.history.push(time.tick(), EmitterPose {
            transform: transform.compute_transform(), properties: properties.cloned().unwrap_or_default(),
            layers: layers.cloned().unwrap_or_default(), visible: visibility != Some(&Visibility::Hidden),
        });
    }
}

fn present_emitters(mut commands: Commands, cursor: Res<RenderInterpolation>, mut emitters: ResMut<Emitters>) {
    emitters.0.retain(|_, track| {
        if track.dead_at.is_none_or(|tick| cursor.position() < tick as f64) { return true; }
        if let Some(proxy) = track.proxy { commands.entity(proxy).try_despawn(); }
        false
    });
    for track in emitters.0.values_mut() {
        let Some((previous, next, alpha)) = track.history.pair(cursor.position()) else { continue; };
        let proxy = *track.proxy.get_or_insert_with(|| commands.spawn(track.effect.clone()).id());
        commands.entity(proxy).insert((interpolate_transform(&previous.transform, &next.transform, alpha),
            previous.properties.clone(), previous.layers.clone(),
            if previous.visible { Visibility::Visible } else { Visibility::Hidden }));
        track.history.discard_before(cursor.position());
    }
}

fn clear_emitters(mut commands: Commands, mut emitters: ResMut<Emitters>) {
    for (_, track) in emitters.0.drain() {
        if let Some(proxy) = track.proxy { commands.entity(proxy).try_despawn(); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use theseeker_engine::time::{MasterClock, GameTickPre, run_gametickupdate_schedule};
    #[test]
    fn parent_death_removes_the_source_and_proxy_death_waits_for_the_presentation_cursor() {
        let mut world = World::new();
        world.init_resource::<Emitters>();
        world.init_resource::<GameTime>();
        world.insert_resource(RenderInterpolation::at(1.0));
        let parent = world.spawn(Transform::from_xyz(10.0, 20.0, 0.0)).id();
        let source = world.spawn((PresentedEffect(ParticleEffect::new(Handle::default())),
            Transform::from_xyz(1.0, 2.0, 0.0), ChildOf(parent))).id();
        world.add_schedule(Schedule::new(GameTickPre));
        world.add_schedule(Schedule::new(GameTickUpdate));
        let mut capture = Schedule::new(GameTickPost);
        capture.add_systems(capture_emitters);
        world.add_schedule(capture);
        let mut present = Schedule::default();
        present.add_systems(present_emitters);
        for tick in 1..=2 {
            world.resource_mut::<GameTime>().update_from_master(&MasterClock { tick, ..default() });
            run_gametickupdate_schedule(&mut world);
        }
        present.run(&mut world);
        let proxy = world.resource::<Emitters>().0[&source].proxy.unwrap();
        assert_eq!(world.get::<Transform>(proxy).unwrap().translation, Vec3::new(11.0, 22.0, 0.0));
        world.despawn(parent);
        assert!(world.get_entity(source).is_err());
        world.resource_mut::<GameTime>().update_from_master(&MasterClock { tick: 3, ..default() });
        run_gametickupdate_schedule(&mut world);
        world.insert_resource(RenderInterpolation::at(1.99));
        present.run(&mut world);
        assert!(world.get_entity(proxy).is_ok());
        world.insert_resource(RenderInterpolation::at(2.0));
        present.run(&mut world);
        assert!(world.get_entity(proxy).is_err());
        assert!(world.resource::<Emitters>().0.is_empty());
    }
}
