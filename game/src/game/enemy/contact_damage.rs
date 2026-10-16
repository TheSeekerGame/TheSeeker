use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct ContactPushback {
    pub(super) horizontal: f32,
    pub(super) vertical: f32,
    pub(super) ticks: u32,
}

#[derive(Component, Clone, Copy, Debug)]
pub(super) struct ContactDamage {
    damage: f32,
    cooldown_ticks: u32,
    cooldown: u32,
    pushback: Option<ContactPushback>,
}

impl ContactDamage {
    pub(super) const fn new(damage: f32, cooldown_ticks: u32, pushback: Option<ContactPushback>) -> Self {
        Self { damage, cooldown_ticks, cooldown: 0, pushback }
    }
}

pub(super) fn contact_damage_system(
    mut attackers: Query<
        (
            Entity,
            &Transform,
            &Collider,
            &mut ContactDamage,
        ),
        (
            With<Enemy>,
            Without<Dead>,
            Without<crate::game::effects::frozen::Frozen>,
        ),
    >,
    mut player_q: Query<(Entity, &Transform, &mut Health),
        (With<Player>, Without<Dead>, Without<crate::game::spikes::SpikeRespawnPending>)>,
    spatial_query: PhysicsWorld,
    mut commands: Commands,
    mut damage_events: MessageWriter<DamageInfo>,
) {
    let Some((player_entity, player_tf, mut player_health)) =
        player_q.iter_mut().next()
    else {
        return;
    };
    if player_health.current == 0 { return; }

    for (entity, transform, collider, mut contact) in attackers.iter_mut() {
        contact.cooldown = contact.cooldown.saturating_sub(1);
        if contact.cooldown > 0 { continue; }

        let player_colliding = spatial_query
            .intersect(
                transform.translation.xy(),
                collider,
                CollisionGroups::new(Group::all(), Group::all()),
                Some(entity),
            )
            .contains(&player_entity);
        if !player_colliding { continue; }

        if let Some(pushback) = contact.pushback {
            let push_dir =
                (player_tf.translation.xy() - transform.translation.xy())
                    .normalize_or_zero();
            commands.entity(player_entity).insert(Knockback::new(
                contact_pushback_velocity(push_dir, pushback),
                pushback.ticks,
            ));
        }

        player_health.current =
            player_health.current.saturating_sub(contact.damage as u32);
        damage_events.write(DamageInfo {
            owner: entity,
            source: entity,
            target: player_entity,
            amount: contact.damage,
            crit: false,
            stealthed: false,
            backstab: false,
        });

        contact.cooldown = contact.cooldown_ticks;
    }
}

fn contact_pushback_velocity(
    push_dir: Vec2,
    pushback: ContactPushback,
) -> Vec2 {
    Vec2::new(
        contact_pushback_x(push_dir.x, pushback.horizontal),
        pushback.vertical,
    )
}

fn contact_pushback_x(push_direction_x: f32, horizontal: f32) -> f32 {
    if push_direction_x >= 0.0 {
        horizontal
    } else {
        -horizontal
    }
}
