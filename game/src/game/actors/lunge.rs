//! Shared lunge motion, contact results, and hit memory.

use bevy::prelude::*;
use std::collections::HashSet;
use super::action::{LungeMotion, TickDisplacement};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionContact { Ground, Tile, ConfirmedHit }

#[derive(Debug)]
pub struct LungeRuntime {
    displacement: TickDisplacement,
    pub contact: Option<MotionContact>,
    pub last_position: Vec2,
    pub damaged_entities: HashSet<Entity>,
    impact_age: u32,
}

impl LungeRuntime {
    pub fn new(motion: LungeMotion, direction: Vec2, position: Vec2) -> Self {
        Self { displacement: motion.displacement(direction), contact: None, last_position: position,
            damaged_entities: HashSet::new(), impact_age: 0 }
    }

    pub fn displacement(&self) -> TickDisplacement {
        if self.contact.is_some() { TickDisplacement::default() } else { self.displacement }
    }

    pub fn impact(&mut self, contact: MotionContact) -> bool {
        if self.contact.is_some() { return false; }
        self.contact = Some(contact);
        self.impact_age = 0;
        true
    }

    pub fn tick_impact(&mut self, duration: Option<u32>) -> bool {
        if self.contact.is_none() { return false; }
        self.impact_age = self.impact_age.saturating_add(1);
        duration.is_some_and(|duration| self.impact_age >= duration)
    }

    pub fn sweep(&mut self, position: Vec2) -> (Vec2, Vec2) {
        let origin = self.last_position;
        self.last_position = position;
        (origin, position - origin)
    }
}

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LungeHits {
    Contact,
    Sweep { radius: f32, impact_radius: f32, impact_max_targets: u32 },
}

pub fn swept_hit_shape(travel: Vec2, radius: f32) -> theseeker_engine::physics::Collider {
    use theseeker_engine::physics::Collider;
    if travel.length_squared() <= f32::EPSILON { Collider::ball(radius) }
    else { Collider::capsule(Vec2::ZERO, travel, radius) }
}

pub struct LungeHitRequest {
    pub owner: Entity,
    pub position: Vec3,
    pub groups: theseeker_engine::physics::CollisionGroups,
    pub skill: Option<crate::game::player::skills::types::SkillId>,
    pub stealthed: bool,
    pub max_targets: u32,
}

pub fn emit_lunge_hits(
    commands: &mut Commands,
    action: &mut super::action::ActiveAction,
    request: LungeHitRequest,
    targets: impl IntoIterator<Item = Entity>,
) {
    use crate::game::combat::{DamageSource, Stealthed};
    use crate::game::combat::damage_source::DamageSourceSkill;
    use crate::game::combat::damage_pipeline::NoOnHitShake;
    let Some(lunge) = &mut action.lunge else { return; };
    let mut damage = DamageSource::new(2, request.owner, action.damage)
        .with_max_targets(request.max_targets);
    damage.damaged_set.clone_from(&lunge.damaged_entities);
    damage.target_set.extend(targets.into_iter().filter(|target| lunge.damaged_entities.insert(*target)));
    if damage.target_set.is_empty() { return; }
    let mut entity = commands.spawn((damage, theseeker_engine::physics::Collider::Empty, request.groups,
        Transform::from_translation(request.position), GlobalTransform::from_translation(request.position), NoOnHitShake));
    if let Some(skill) = request.skill { entity.insert(DamageSourceSkill(skill)); }
    if request.stealthed { entity.insert(Stealthed); }
}
