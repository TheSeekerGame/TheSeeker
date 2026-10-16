use bevy::ecs::system::Command;
use bevy::ecs::world::EntityWorldMut;
use bevy::prelude::*;
use std::collections::HashMap;

use crate::game::combat::Health;
use crate::game::player::passives::blood_shards::HealApplied;
use crate::game::player::{
    CraftMaterial, MaterialInventory, Passives, Player,
    PlayerStatMod, SkillInventory,
};

use super::types::{
    consumable_metadata, ConsumableEffect, ConsumableMetadata, CooldownMode,
    CooldownSpec, SkillId,
};

pub const CONSUMABLE_COOLDOWN_TICKS: u32 = 24;
pub const CONSUMABLE_REGEN_TICKS: u32 = 96;
pub const CONSUMABLE_BUFF_TICKS: u32 = 192;

pub const RED_BERRY_METADATA: ConsumableMetadata = ConsumableMetadata {
    material: CraftMaterial::RedBerry,
    effect: ConsumableEffect::Regeneration {
        total_heal: 12,
        duration_ticks: CONSUMABLE_REGEN_TICKS,
    },
    cooldown: CooldownSpec {
        min_ticks: 0,
        max_ticks: CONSUMABLE_COOLDOWN_TICKS,
        mode: CooldownMode::RateBased,
    },
};

pub const BLUE_BERRY_METADATA: ConsumableMetadata = ConsumableMetadata {
    material: CraftMaterial::BlueBerry,
    effect: ConsumableEffect::CooldownRateMultiplier {
        multiplier: 1.33,
        duration_ticks: CONSUMABLE_BUFF_TICKS,
    },
    cooldown: CooldownSpec {
        min_ticks: 0,
        max_ticks: CONSUMABLE_COOLDOWN_TICKS,
        mode: CooldownMode::RateBased,
    },
};

pub const MOSS_BERRY_METADATA: ConsumableMetadata = ConsumableMetadata {
    material: CraftMaterial::MossBerry,
    effect: ConsumableEffect::DamageMultiplier {
        multiplier: 1.5,
        duration_ticks: CONSUMABLE_BUFF_TICKS,
    },
    cooldown: CooldownSpec {
        min_ticks: 0,
        max_ticks: CONSUMABLE_COOLDOWN_TICKS,
        mode: CooldownMode::RateBased,
    },
};

#[derive(Debug, Clone, Copy)]
struct ConsumableRegenStack {
    total_heal: u32,
    duration_ticks: u32,
    remaining_ticks: u32,
    carry: u32,
}

impl ConsumableRegenStack {
    fn new(total_heal: u32, duration_ticks: u32) -> Self {
        Self {
            total_heal,
            duration_ticks: duration_ticks.max(1),
            remaining_ticks: duration_ticks.max(1),
            carry: 0,
        }
    }

    fn tick(&mut self) -> u32 {
        if self.remaining_ticks == 0 {
            return 0;
        }

        self.carry = self.carry.saturating_add(self.total_heal);
        let heal = self.carry / self.duration_ticks;
        self.carry %= self.duration_ticks;
        self.remaining_ticks -= 1;
        heal
    }

    fn finished(self) -> bool {
        self.remaining_ticks == 0
    }
}

#[derive(Debug, Clone, Copy)]
struct TimedConsumableBuff {
    multiplier: f32,
    remaining_ticks: u32,
}

impl TimedConsumableBuff {
    fn new(multiplier: f32, duration_ticks: u32) -> Self {
        Self {
            multiplier,
            remaining_ticks: duration_ticks.max(1),
        }
    }

    fn tick(&mut self) {
        self.remaining_ticks = self.remaining_ticks.saturating_sub(1);
    }

    fn expired(self) -> bool {
        self.remaining_ticks == 0
    }
}

#[derive(Component, Debug, Default)]
pub struct ConsumableRegeneration {
    stacks: Vec<ConsumableRegenStack>,
}

impl ConsumableRegeneration {
    fn push(&mut self, stack: ConsumableRegenStack) {
        if let Some(existing) = self.stacks.iter_mut().find(|existing| {
            existing.total_heal == stack.total_heal
                && existing.duration_ticks == stack.duration_ticks
        }) {
            existing.remaining_ticks = existing
                .remaining_ticks
                .saturating_add(stack.remaining_ticks);
        } else {
            self.stacks.push(stack);
        }
    }
}

#[derive(Component, Debug, Default)]
pub struct ConsumableCooldownBuffs {
    stacks: Vec<TimedConsumableBuff>,
}

impl ConsumableCooldownBuffs {
    fn push(&mut self, stack: TimedConsumableBuff) {
        if let Some(existing) = self.stacks.iter_mut().find(|existing| {
            (existing.multiplier - stack.multiplier).abs() <= f32::EPSILON
        }) {
            existing.remaining_ticks = existing
                .remaining_ticks
                .saturating_add(stack.remaining_ticks);
        } else {
            self.stacks.push(stack);
        }
    }
}

#[derive(Component, Debug, Default)]
pub struct ConsumableDamageBuffs {
    stacks: Vec<TimedConsumableBuff>,
}

impl ConsumableDamageBuffs {
    fn push(&mut self, stack: TimedConsumableBuff) {
        if let Some(existing) = self.stacks.iter_mut().find(|existing| {
            (existing.multiplier - stack.multiplier).abs() <= f32::EPSILON
        }) {
            existing.remaining_ticks = existing
                .remaining_ticks
                .saturating_add(stack.remaining_ticks);
        } else {
            self.stacks.push(stack);
        }
    }
}

struct QueueConsumableRegeneration {
    entity: Entity,
    stack: ConsumableRegenStack,
}

impl QueueConsumableRegeneration {
    fn new(entity: Entity, stack: ConsumableRegenStack) -> Self {
        Self { entity, stack }
    }
}

impl Command for QueueConsumableRegeneration {
    type Out = ();
    fn apply(self, world: &mut World) {
        let Ok(mut entity) = world.get_entity_mut(self.entity) else {
            return;
        };

        if let Some(mut regen) = entity.get_mut::<ConsumableRegeneration>() {
            regen.push(self.stack);
        } else {
            entity.insert(ConsumableRegeneration {
                stacks: vec![self.stack],
            });
        }
    }
}

enum ConsumableBuffKind {
    Cooldown,
    Damage,
}

struct QueueConsumableBuff {
    entity: Entity,
    kind: ConsumableBuffKind,
    stack: TimedConsumableBuff,
}

impl QueueConsumableBuff {
    fn cooldown(entity: Entity, stack: TimedConsumableBuff) -> Self {
        Self {
            entity,
            kind: ConsumableBuffKind::Cooldown,
            stack,
        }
    }

    fn damage(entity: Entity, stack: TimedConsumableBuff) -> Self {
        Self {
            entity,
            kind: ConsumableBuffKind::Damage,
            stack,
        }
    }
}

impl Command for QueueConsumableBuff {
    type Out = ();
    fn apply(self, world: &mut World) {
        let Ok(mut entity) = world.get_entity_mut(self.entity) else {
            return;
        };

        match self.kind {
            ConsumableBuffKind::Cooldown => {
                queue_consumable_cooldown_buff(&mut entity, self.stack);
            },
            ConsumableBuffKind::Damage => {
                queue_consumable_damage_buff(&mut entity, self.stack);
            },
        }
    }
}

fn queue_consumable_cooldown_buff(
    entity: &mut EntityWorldMut<'_>,
    stack: TimedConsumableBuff,
) {
    let Some(mut buffs) = entity.get_mut::<ConsumableCooldownBuffs>() else {
        entity.insert(ConsumableCooldownBuffs {
            stacks: vec![stack],
        });
        return;
    };

    buffs.push(stack);
}

fn queue_consumable_damage_buff(
    entity: &mut EntityWorldMut<'_>,
    stack: TimedConsumableBuff,
) {
    let Some(mut buffs) = entity.get_mut::<ConsumableDamageBuffs>() else {
        entity.insert(ConsumableDamageBuffs {
            stacks: vec![stack],
        });
        return;
    };

    buffs.push(stack);
}

#[derive(Component, Debug, Clone, Default)]
pub struct SkillStockTracker {
    counts: HashMap<CraftMaterial, u32>,
}

impl SkillStockTracker {
    pub fn from_inventory(materials: &MaterialInventory) -> Self {
        let counts = CraftMaterial::SKILL_STOCK_MATERIALS
            .into_iter()
            .map(|material| {
                (material, materials.backpack_count(material))
            })
            .filter(|(_, count)| *count > 0)
            .collect();
        Self { counts }
    }

    fn count(&self, material: CraftMaterial) -> u32 {
        self.counts.get(&material).copied().unwrap_or(0)
    }

    fn set(&mut self, material: CraftMaterial, count: u32) {
        if count == 0 {
            self.counts.remove(&material);
        } else {
            self.counts.insert(material, count);
        }
    }
}

pub(super) fn try_start_consumable_slot(
    skill: SkillId,
    ctx: &mut super::registry::SkillActivationContext,
    world: &mut super::activation::ActivationWorld,
) -> bool {
    let commands = &mut world.instant.commands;
    let cooldowns = &mut world.instant.cooldowns;
    if !skill.is_consumable() {
        return false;
    }

    if !ctx.action_state.just_pressed(&ctx.slot) {
        return false;
    }

    let Some(metadata) = consumable_metadata(skill) else {
        return false;
    };

    if !cooldowns.is_ready(ctx.entity, skill, ctx.current_tick) {
        return false;
    }

    if !ctx.materials.remove_from_backpack(metadata.material, 1) {
        return false;
    }

    let cdr_snapshot = ctx.stats.map(|s| s.cdr).unwrap_or(1.0);
    cooldowns.start(
        ctx.entity,
        skill,
        metadata.cooldown,
        cdr_snapshot,
        ctx.current_tick,
    );

    match metadata.effect {
        ConsumableEffect::Regeneration {
            total_heal,
            duration_ticks,
        } => {
            commands.queue(QueueConsumableRegeneration::new(
                ctx.entity,
                ConsumableRegenStack::new(total_heal, duration_ticks),
            ));
        },
        ConsumableEffect::CooldownRateMultiplier {
            multiplier,
            duration_ticks,
        } => {
            commands.queue(QueueConsumableBuff::cooldown(
                ctx.entity,
                TimedConsumableBuff::new(multiplier, duration_ticks),
            ));
        },
        ConsumableEffect::DamageMultiplier {
            multiplier,
            duration_ticks,
        } => {
            commands.queue(QueueConsumableBuff::damage(
                ctx.entity,
                TimedConsumableBuff::new(multiplier, duration_ticks),
            ));
        },
    }

    true
}

pub fn apply_consumable_stat_buffs(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &mut PlayerStatMod,
            Option<&mut ConsumableCooldownBuffs>,
            Option<&mut ConsumableDamageBuffs>,
        ),
        With<Player>,
    >,
) {
    for (entity, mut stats, mut cdr_buffs, mut damage_buffs) in query.iter_mut()
    {
        apply_consumable_cooldown_buffs(
            &mut commands,
            entity,
            &mut stats,
            cdr_buffs.as_deref_mut(),
        );
        apply_consumable_damage_buffs(
            &mut commands,
            entity,
            &mut stats,
            damage_buffs.as_deref_mut(),
        );
    }
}

fn apply_consumable_cooldown_buffs(
    commands: &mut Commands,
    entity: Entity,
    stats: &mut PlayerStatMod,
    cdr_buffs: Option<&mut ConsumableCooldownBuffs>,
) {
    let Some(cdr_buffs) = cdr_buffs else {
        return;
    };

    stats.cdr *= cdr_buffs
        .stacks
        .iter()
        .map(|buff| buff.multiplier)
        .product::<f32>();
    tick_consumable_buffs(&mut cdr_buffs.stacks);

    if cdr_buffs.stacks.is_empty() {
        commands.entity(entity).remove::<ConsumableCooldownBuffs>();
    }
}

fn apply_consumable_damage_buffs(
    commands: &mut Commands,
    entity: Entity,
    stats: &mut PlayerStatMod,
    damage_buffs: Option<&mut ConsumableDamageBuffs>,
) {
    let Some(damage_buffs) = damage_buffs else {
        return;
    };

    stats.damage *= damage_buffs
        .stacks
        .iter()
        .map(|buff| buff.multiplier)
        .product::<f32>();
    tick_consumable_buffs(&mut damage_buffs.stacks);

    if damage_buffs.stacks.is_empty() {
        commands.entity(entity).remove::<ConsumableDamageBuffs>();
    }
}

fn tick_consumable_buffs(stacks: &mut Vec<TimedConsumableBuff>) {
    for buff in stacks.iter_mut() {
        buff.tick();
    }
    stacks.retain(|buff| !buff.expired());
}

pub fn tick_consumable_regeneration(
    mut commands: Commands,
    mut query: Query<
        (Entity, &mut Health, &mut ConsumableRegeneration),
        With<Player>,
    >,
    mut heal_events: MessageWriter<HealApplied>,
) {
    for (entity, mut health, mut regen) in query.iter_mut() {
        let pending_heal: u32 = regen.stacks.iter_mut().map(ConsumableRegenStack::tick).sum();
        regen.stacks.retain(|stack| !stack.finished());

        if regen.stacks.is_empty() {
            commands.entity(entity).remove::<ConsumableRegeneration>();
        }

        if pending_heal == 0 {
            continue;
        }

        heal_events.write(HealApplied::heal(entity, &mut health, pending_heal));
    }
}

pub fn sync_skill_stock_unlocks(
    mut query: Query<
        (
            &MaterialInventory,
            &Passives,
            &mut SkillInventory,
            &mut SkillStockTracker,
        ),
        With<Player>,
    >,
) {
    for (materials, passives, mut skills, mut tracker) in query.iter_mut() {
        let max_slots = SkillInventory::max_slots_for(passives);

        for material in CraftMaterial::SKILL_STOCK_MATERIALS {
            sync_stock_material_unlock(
                material,
                materials,
                &mut skills,
                &mut tracker,
                max_slots,
            );
        }
    }
}

fn sync_stock_material_unlock(
    material: CraftMaterial,
    materials: &MaterialInventory,
    skills: &mut SkillInventory,
    tracker: &mut SkillStockTracker,
    max_slots: usize,
) {
    let Some(skill) = material.stock_skill() else {
        return;
    };

    let current = materials.backpack_count(material);
    let previous = tracker.count(material);
    let retains_unlock = material.retains_skill_unlock_when_empty();

    if current == previous {
        sync_unchanged_stock_skill(skill, current, retains_unlock, skills, max_slots);
        return;
    }

    if current == 0 {
        remove_empty_stock_skill(skill, retains_unlock, skills);
        tracker.set(material, current);
        return;
    }

    let became_available = !skills.is_unlocked(&skill);
    let gained_more = current > previous;
    if became_available {
        skills.unlock_skill(skill);
    }

    auto_equip_stock_skill_if_needed(
        skill,
        became_available || gained_more,
        skills,
        max_slots,
    );
    tracker.set(material, current);
}

fn sync_unchanged_stock_skill(
    skill: SkillId,
    current: u32,
    retains_unlock: bool,
    skills: &mut SkillInventory,
    max_slots: usize,
) {
    if current == 0 {
        remove_empty_stock_skill(skill, retains_unlock, skills);
        return;
    }

    if skills.is_unlocked(&skill) {
        return;
    }

    skills.unlock_skill(skill);
    auto_equip_stock_skill_if_needed(skill, true, skills, max_slots);
}

fn remove_empty_stock_skill(
    skill: SkillId,
    retains_unlock: bool,
    skills: &mut SkillInventory,
) {
    if !retains_unlock && skills.is_unlocked(&skill) {
        skills.remove_skill(skill);
    }
}

fn auto_equip_stock_skill_if_needed(
    skill: SkillId,
    should_auto_equip: bool,
    skills: &mut SkillInventory,
    max_slots: usize,
) {
    if should_auto_equip && !skills.is_equipped(&skill) {
        skills.auto_equip_skill(skill, max_slots);
    }
}
