//! Placement, payment and effect execution shared by toolbar input and Pulse Drive.
use bevy::{ecs::system::SystemParam, prelude::*};
use theseeker_engine::physics::{PhysicsWorld, TileCollisionWorld};
use crate::game::{effects::stealthed::StealthEffect, gentstate::Facing};
use super::super::{MaterialInventory, sensors::GroundSensor, spawns};
use super::{amplified_bell, explosive_mine, quark_pylon, cooldowns::Cooldowns, types::*};

#[derive(Clone, Copy)]
pub(crate) enum ActivationOrigin {
    Input,
    PulseDrive { source_damage: Option<Entity> },
}

pub(crate) struct InstantUser<'a> {
    pub entity: Entity,
    pub transform: &'a Transform,
    pub facing: &'a Facing,
    pub ground: &'a GroundSensor,
    pub tick: u64,
    pub cdr: f32,
    pub origin: ActivationOrigin,
}

#[derive(SystemParam)]
pub(crate) struct InstantWorld<'w, 's> {
    pub commands: Commands<'w, 's>,
    pub cooldowns: ResMut<'w, Cooldowns>,
    pub physics: PhysicsWorld<'w>,
    pub tiles: Option<Res<'w, TileCollisionWorld>>,
    pub pylons: Query<'w, 's, &'static spawns::quark_pylon::QuarkPylon>,
}

#[derive(PartialEq, Eq)]
pub(crate) enum InstantResult {
    Used,
    Rejected,
    InvalidPlacement,
}

impl InstantWorld<'_, '_> {
    pub fn activate(
        &mut self,
        skill: SkillId,
        user: &InstantUser,
        materials: Option<&mut MaterialInventory>,
    ) -> InstantResult {
        if skill == SkillId::Stealth {
            return self.stealth(user);
        }
        // Mine ground admission precedes cooldown admission, as in its input path.
        let mine_position = explosive_mine::resolve_spawn_position(user.transform, user.ground);
        if skill == SkillId::ExplosiveMine && mine_position.is_none() {
            return InstantResult::InvalidPlacement;
        }
        if !self.cooldowns.is_ready(user.entity, skill, user.tick) {
            return InstantResult::Rejected;
        }
        let Some(position) = self.placement(skill, user, mine_position) else {
            return InstantResult::InvalidPlacement;
        };
        let Some(material) = skill.stock_material() else { return InstantResult::Rejected; };
        let Some(materials) = materials else { return InstantResult::Rejected; };
        let Some(stock) = super::item_use::ItemReservation::reserve(materials, material, 1) else {
            return InstantResult::Rejected;
        };
        stock.consume();
        let cooldown = self.spawn(skill, user, position);
        if let Some(cooldown) = cooldown {
            self.cooldowns.start(user.entity, skill, cooldown, user.cdr, user.tick);
        }
        InstantResult::Used
    }

    fn stealth(&mut self, user: &InstantUser) -> InstantResult {
        let mut effect = StealthEffect::new(user.tick);
        match user.origin {
            ActivationOrigin::Input if !self.cooldowns.is_ready(user.entity, SkillId::Stealth, user.tick) => {
                return InstantResult::Rejected;
            }
            ActivationOrigin::PulseDrive { source_damage } => effect.ignore_damage_from = source_damage,
            ActivationOrigin::Input => {}
        }
        // Stealth's cooldown begins when its effect ends.
        self.commands.entity(user.entity).insert(effect);
        InstantResult::Used
    }

    fn placement(&self, skill: SkillId, user: &InstantUser, mine: Option<Vec2>) -> Option<Vec2> {
        match skill {
            SkillId::Spinner => Some(user.transform.translation.truncate() + Vec2::new(10.0 * user.facing.direction(), 0.0)),
            SkillId::IceNova => Some(user.transform.translation.truncate()),
            SkillId::ExplosiveMine => mine,
            SkillId::QuarkPylon => quark_pylon::find_quark_pylon_placement(user.transform, user.facing),
            SkillId::AmplifiedBell => amplified_bell::find_amplified_bell_placement(
                user.transform, user.facing, &self.physics, self.tiles.as_deref(), user.entity),
            _ => None,
        }
    }

    fn spawn(&mut self, skill: SkillId, user: &InstantUser, position: Vec2) -> Option<CooldownSpec> {
        let direction = user.facing.direction();
        match skill {
            SkillId::Spinner => {
                spawns::spinner::spawn_spinner(&mut self.commands, user.entity, position, direction);
                Some(spinner_metadata().cooldown)
            }
            SkillId::IceNova => {
                spawns::ice_nova::spawn_ice_nova(&mut self.commands, user.entity, position);
                Some(ice_nova_metadata().cooldown)
            }
            SkillId::AmplifiedBell => {
                spawns::amplified_bell::spawn_bell(&mut self.commands, user.entity, position, direction);
                Some(amplified_bell_metadata().cooldown)
            }
            SkillId::ExplosiveMine => {
                let direction = mine_direction(user.origin, direction);
                spawns::mine::spawn_mine(&mut self.commands, user.entity, position, direction);
                Some(explosive_mine_metadata().cooldown)
            }
            SkillId::QuarkPylon => {
                let count = quark_pylon::active_pylon_count(user.entity, &self.pylons) + 1;
                spawns::quark_pylon::spawn_quark_pylon(&mut self.commands, user.entity, position, true);
                count.is_multiple_of(2).then_some(quark_pylon_metadata().cooldown)
            }
            _ => None,
        }
    }
}

fn mine_direction(origin: ActivationOrigin, facing: f32) -> f32 {
    match origin {
        ActivationOrigin::Input => 0.0,
        ActivationOrigin::PulseDrive { .. } => facing,
    }
}

#[cfg(test)]
#[path = "instant_tests.rs"]
mod tests;
