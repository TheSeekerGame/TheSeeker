use crate::game::player::foreground::ActionMut;
use bevy::camera::visibility::RenderLayers;
use theseeker_engine::animation::{PlaybackState, PlaybackId};
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::{Gent, TransformGfxFromGent};
use theseeker_engine::physics::{groups, LinearVelocity};
use theseeker_engine::script::ScriptPlayer;
use crate::game::actors::action::{ActiveAction, ActionRequest, ActionPresentation, TickDisplacement};
use crate::game::actors::catalog::ActorCatalog;
use crate::game::actors::projectile::{emit_projectile_shot, ProjectileShot, ProjectilePolicy};
use crate::game::enemy::Enemy;
use crate::game::gentstate::{Dead, Facing};
use crate::game::player::skills::types::SkillId;
use crate::game::player::states::{HorizontalInputIntent, Spitting};
use crate::game::player::{Passive, Passives, Player, PlayerGfx, PlayerStateSet};
use crate::graphics::projectile_particles::ArcParticleEffectHandle;
use crate::prelude::*;

const SPIT_TARGET_RANGE: f32 = 200.0;
type LiveEnemyQuery<'w, 's> = Query<'w, 's, (Entity, &'static GlobalTransform), (With<Enemy>, Without<Dead>)>;
type PlayerSpitGfxQuery<'w, 's> = Query<'w, 's, (
    &'static mut ScriptPlayer<SpriteAnimation>, &'static mut TransformGfxFromGent,
), With<PlayerGfx>>;

pub struct SpitPlugin;
impl Plugin for SpitPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(GameTickUpdate, tick_spitting
            .run_if(in_state(AppState::InGame))
            .after(PlayerStateSet::Behavior).before(PlayerStateSet::Collisions)
            .after(theseeker_engine::animation::AnimationSet::Signals)
            .before(crate::game::combat::damage_source::determine_damage_targets));
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct SpitContext<'w, 's> {
    commands: Commands<'w, 's>,
    gfx: PlayerSpitGfxQuery<'w, 's>,
    signals: Query<'w, 's, &'static PlaybackState>,
    enemies: LiveEnemyQuery<'w, 's>,
    particles: Option<Res<'w, ArcParticleEffectHandle>>,
    catalog: Res<'w, ActorCatalog>,
    time: Res<'w, GameTime>,
}

fn tick_spitting(
    mut players: Query<(Entity, ActionMut<Spitting>, &Transform, &Gent, &mut LinearVelocity,
        &mut Facing, &HorizontalInputIntent, &Passives), (With<Player>, Without<Dead>, Without<PlayerGfx>)>,
    mut context: SpitContext,
) {
    for (entity, mut spitting, transform, gent, mut velocity, mut facing, input, passives) in &mut players {
        let position = transform.translation.xy();
        if spitting.action.is_none() {
            let execution = spitting.execution().id;
            begin_spit(execution, entity, &mut spitting, &mut facing, gent, position, &mut context);
        }
        let Some(action) = &mut spitting.action else { continue; };
        action.motion_intent(TickDisplacement::default()).apply(action.id, &mut velocity);
        let effects = action.advance(context.time.tick(), context.signals.get(gent.e_gfx).ok(), input.movement != 0.0);
        if effects.releases > 0 {
            release_spit(action, entity, position, passives, &mut context);
        }
        if effects.outcome.is_some() {
            crate::game::player::foreground::finish_action(&mut context.commands, entity, action.id);
        }
    }
}

fn begin_spit(execution: u64, entity: Entity, spitting: &mut Spitting, facing: &mut Facing, gent: &Gent, position: Vec2, context: &mut SpitContext) {
    let definition = context.catalog.actions.get("player_spit").expect("validated player_spit action").clone();
    let binding = definition.presentation.as_ref().expect("Spit presentation");
    let nearest = nearest_enemy_facing(position, spitting.facing, &context.enemies);
    let Ok((mut player, mut offset)) = context.gfx.get_mut(gent.e_gfx) else { return; };
    let mut action = ActionRequest { forward: nearest.direction(), actor: entity, target: None, damage: definition.damage.expect("Spit damage"), definition: definition.clone() }.activate(PlaybackId::default());
    action.id = execution;
    crate::game::player::player_anim::set_direction_slots(&mut player, &nearest);
    action.present(&mut player, ActionPresentation::Execute);
    action.claim_facing(&mut context.commands);
    offset.offset = Some(Vec2::from_array(binding.offset).extend(0.0));
    spitting.facing = nearest;
    *facing = nearest;
    spitting.action = Some(action);
}

fn release_spit(action: &ActiveAction, owner: Entity, position: Vec2, passives: &Passives, context: &mut SpitContext) {
    emit_projectile_shot(&mut context.commands, ProjectileShot {
        owner, start: position, target: find_nearest_enemy_position(position, &context.enemies, SPIT_TARGET_RANGE),
        forward: action.forward, descriptor: action.definition.projectile.as_ref().expect("Spit projectile"),
        damage: action.damage,
        particle_effect: context.particles.as_ref().map(|effect| effect.0.clone()),
        split_angle: passives.contains(&Passive::Prism).then_some(5.0_f32.to_radians()),
        policy: ProjectilePolicy {
            groups: groups::player_attack(), skill: Some(SkillId::Spit), max_targets: Some(1),
            descriptor_hit_effects: false, z: 0.000_025, layers: Some(RenderLayers::layer(2)), state_lifetime: true,
        },
    }, None);
}

fn nearest_enemy_facing(
    player_pos: Vec2,
    fallback: Facing,
    enemy_query: &LiveEnemyQuery,
) -> Facing {
    let Some(enemy_pos) = find_nearest_enemy_position_any_range(
        player_pos,
        enemy_query,
    ) else {
        return fallback;
    };

    if enemy_pos.x > player_pos.x {
        Facing::Right
    } else {
        Facing::Left
    }
}

/// Find the position of the nearest enemy within range
fn find_nearest_enemy_position(
    from: Vec2,
    enemy_query: &LiveEnemyQuery,
    max_range: f32,
) -> Option<Vec2> {
    let max_range_squared = max_range * max_range;

    nearest_enemy_position_by_distance(from, enemy_query)
        .filter(|(dist_squared, _pos)| *dist_squared <= max_range_squared)
        .map(|(_dist_squared, pos)| pos)
}

fn find_nearest_enemy_position_any_range(
    from: Vec2,
    enemy_query: &LiveEnemyQuery,
) -> Option<Vec2> {
    nearest_enemy_position_by_distance(from, enemy_query)
        .map(|(_dist_squared, pos)| pos)
}

fn nearest_enemy_position_by_distance(
    from: Vec2,
    enemy_query: &LiveEnemyQuery,
) -> Option<(f32, Vec2)> {
    enemy_query
        .iter()
        .map(|(_entity, enemy_tf)| {
            let enemy_pos = enemy_tf.translation().truncate();
            (from.distance_squared(enemy_pos), enemy_pos)
        })
        .min_by(|(dist_a, _pos_a), (dist_b, _pos_b)| {
            dist_a.total_cmp(dist_b)
        })
}
