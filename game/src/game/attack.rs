use theseeker_engine::physics::{update_query_pipeline, Collider, PhysicsWorld};
use theseeker_engine::{assets::animation::SpriteAnimation, script::ScriptPlayer};

use super::player::{PlayerGent, PlayerGfx};
use crate::prelude::*;

pub struct AttackPlugin;

impl Plugin for AttackPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            GameTickUpdate,
            (
                attack_damage,
                attack_tick,
                attack_cleanup,
            )
                .chain(),
        );
    }
}

#[derive(Component)]
pub struct Attack {
    current_lifetime: u32,
    max_lifetime: u32,
    damage: u32,
    damaged: Vec<Entity>,
    //target: Entity
    //or
    //damage_team: Team::Player
}

#[derive(Component)]
pub struct Health {
    pub current: u32,
    pub max: u32,
}
impl Attack {
    pub fn new(lifetime: u32) -> Self {
        Attack {
            current_lifetime: 0,
            max_lifetime: lifetime,
            damage: 20,
            damaged: Vec::new(),
        }
    }
}
fn attack_damage(
    spatial_query: Res<PhysicsWorld>,
    mut query: Query<(
        Entity,
        &GlobalTransform,
        &mut Attack,
        &Collider,
    )>,
    mut player_query: Query<(
        Entity,
        &mut Health,
        &Collider,
        &PlayerGent,
    )>,
    mut gfx_query: Query<&mut ScriptPlayer<SpriteAnimation>, With<PlayerGfx>>,
    //animation query to flash red?
) {
    for (entity, pos, mut attack, attack_collider) in query.iter_mut() {
        let colliding_entities = spatial_query.intersect(
            pos.translation().xy(),
            attack_collider.0.shape(),
            Some(entity),
        );
        for (entity, mut health, player_collider, player_gent) in player_query.iter_mut() {
            if colliding_entities.contains(&entity) {
                if !attack.damaged.contains(&entity) {
                    health.current = health.current.saturating_sub(attack.damage);
                    attack.damaged.push(entity);
                    println!("player health, {:?}", health.current);
                    if let Ok(mut anim_player) = gfx_query.get_mut(player_gent.e_gfx) {
                        anim_player.set_slot("Damage", true);
                    }
                    //unset damage flash after certain time
                    if health.current == 0 {
                        println!("player dead");
                    }
                }
                println!("colliding, attack with player");
            }
            // spatial_query.shape_intersections(attack_collider, shape_position, shape_rotation, query_filter)
        }
    }
}

fn attack_tick(mut query: Query<&mut Attack>) {
    for mut attack in query.iter_mut() {
        attack.current_lifetime += 1;
    }
}

fn attack_cleanup(query: Query<(Entity, &Attack)>, mut commands: Commands) {
    for (entity, attack) in query.iter() {
        if attack.current_lifetime >= attack.max_lifetime {
            println!("despawned attack collider");
            commands.entity(entity).despawn();
        }
    }
}
