use crate::game::player::skills::instant::{ActivationOrigin, InstantUser, InstantWorld};
use crate::game::player::foreground::ActionRef;
use bevy::asset::AssetId;
use bevy::prelude::*;
use theseeker_engine::assets::animation::SpriteAnimation;
use theseeker_engine::gent::Gent;
use theseeker_engine::physics::SpriteShapeMap;
use theseeker_engine::script::ScriptPlayer;
use theseeker_engine::time::GameTime;

use super::{ordered_passives, PassiveOwner};
use crate::game::player::skills::types::{
    flicker_strike_metadata, whirl_metadata, SkillId,
};
use theseeker_engine::profile::TickProfiler;

use super::super::crits::Crits;
use super::super::passives::blood_shards::HealApplied;
use super::super::sensors::GroundSensor;
use super::super::{
    FlickerAbility, MaterialInventory, Passive, Passives, Player, PlayerStatMod,
    SkillInventory, WhirlAbility,
};
use crate::game::combat::{DamageSource, Health};
use crate::game::gentstate::Facing;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum CrystalTint {
    Fire,
    Ice,
}

#[derive(Resource, Default)]
pub struct ElementalSpriteTintCache {
    variants: HashMap<(AssetId<Image>, CrystalTint), Handle<Image>>,
    reverse: HashMap<AssetId<Image>, Handle<Image>>,
}

fn equipped_crystal_tint(passives: &Passives) -> Option<CrystalTint> {
    if passives.contains(&Passive::FireCrystal) {
        Some(CrystalTint::Fire)
    } else if passives.contains(&Passive::IceCrystal) {
        Some(CrystalTint::Ice)
    } else {
        None
    }
}

fn crystal_tint_rgb(tint: CrystalTint) -> [u8; 3] {
    match tint {
        CrystalTint::Fire => [255, 255, 227], // #ffffe3
        CrystalTint::Ice => [225, 248, 255],  // #e1f8ff
    }
}

fn clone_tinted_image(image: &Image, tint: CrystalTint) -> Option<Image> {
    let mut tinted = image.clone();
    let data = tinted.data.as_mut()?;
    let [r, g, b] = crystal_tint_rgb(tint);
    for rgba in data.as_chunks_mut::<4>().0.iter_mut() {
        if rgba[0] == 255 && rgba[1] == 255 && rgba[2] == 255 && rgba[3] != 0 {
            rgba[0] = r;
            rgba[1] = g;
            rgba[2] = b;
        }
    }
    Some(tinted)
}

fn add_energy_clamped(current: &mut f32, amount: f32, max: f32) {
    if amount <= 0.0 {
        return;
    }
    *current = (*current + amount).min(max);
}

/// Bridge XP orb pickups into the passive event stream.
pub fn emit_passive_events_from_xp(
    mut xp_events: MessageReader<
        crate::game::player::spawns::xp_orbs::XpOrbPickup,
    >,
    mut passive_events: MessageWriter<super::PassiveEvent>,
) {
    for _ in xp_events.read() {
        passive_events.write(super::PassiveEvent::XpOrbPickup);
    }
}

/// Update player stat modifiers each tick based on equipped passives.
///
/// This system runs early in PlayerStateSet::Behavior and:
/// 1. Resets PlayerStatMod to baseline (1.0 for multipliers, 0 for additions)
/// 2. Applies all passive effects in priority order
/// 3. Leaves PlayerStatMod ready for further modification by effects (e.g., chilled)
///
/// This runs every tick to ensure stat modifiers are always fresh and
/// correctly reflect the current game state (health %, enemies nearby, etc.)
pub fn process_passive_stats(
    mut query: Query<(&Passives, &mut PlayerStatMod, &Health, PassiveOwner), With<Player>>,
    mut profiler: TickProfiler,
) {
    let _profile = profiler.scope("player_passives");
    for (passives, mut stats, health, owner) in &mut query {
        // Reset all stat modifiers to baseline (these are multiplicative modifiers)
        stats.damage = 1.0;
        stats.defense = 1.0;
        stats.speed = 1.0;
        stats.cdr = 1.0;
        stats.extra_jumps = 0;

        let context = owner.context(Some(health));
        for passive in ordered_passives(passives).iter().flatten() {
            passive.modify_stats(&mut stats, &context);
        }

        // Event-driven actions are handled in `process_passive_events_system`.
    }
}

/// Apply animation slot toggles exported by passives (e.g., cosmetic markers).
pub fn apply_passive_animation_slots(
    player_query: Query<(&Passives, &Gent), With<Player>>,
    mut gfx_query: Query<
        &mut ScriptPlayer<SpriteAnimation>,
        With<super::super::PlayerGfx>,
    >,
    mut profiler: TickProfiler,
) {
    let _profile = profiler.scope("player_passive_slots");
    for (passives, gent) in player_query.iter() {
        let Ok(mut anim) = gfx_query.get_mut(gent.e_gfx) else {
            continue;
        };

        apply_animation_slots_for_passives(&mut anim, passives);
    }
}

fn apply_animation_slots_for_passives(
    anim: &mut ScriptPlayer<SpriteAnimation>,
    passives: &Passives,
) {
    anim.set_slot("SerpentRing", false);
    anim.set_slot("FrenziedAttack", false);
    for passive in super::ordered_passives(passives).into_iter().flatten() {
        for &(name, value) in passive.animation_slots() {
            anim.set_slot(name, value);
        }
    }
}

/// Apply crystal elemental tinting to the player's animated sprite sheets.
///
/// This performs exact pure-white pixel replacement on cached texture variants
/// and swaps the player's current sprite image handle to the matching variant.
pub fn apply_elemental_crystal_player_tint(
    player_query: Query<(&Passives, &Gent), With<Player>>,
    mut gfx_query: Query<&mut Sprite, With<super::super::PlayerGfx>>,
    mut images: ResMut<Assets<Image>>,
    mut shape_map: ResMut<SpriteShapeMap>,
    mut tint_cache: ResMut<ElementalSpriteTintCache>,
) {
    for (passives, gent) in player_query.iter() {
        let Ok(mut sprite) = gfx_query.get_mut(gent.e_gfx) else {
            continue;
        };

        let current_image = sprite.image.clone();
        let current_id = current_image.id();
        let base_image = tint_cache
            .reverse
            .get(&current_id)
            .cloned()
            .unwrap_or(current_image);
        let base_id = base_image.id();

        let desired_tint = equipped_crystal_tint(passives);
        let Some(tint) = desired_tint else {
            // No crystal equipped: ensure we render the original sprite sheet.
            reset_player_tint_if_needed(&mut sprite, current_id, base_id, base_image);
            continue;
        };

        let Some(tinted_image) = tinted_player_image(
            &base_image,
            base_id,
            tint,
            &mut images,
            &mut shape_map,
            &mut tint_cache,
        ) else {
            continue;
        };

        if sprite.image.id() != tinted_image.id() {
            sprite.image = tinted_image;
        }
    }
}

fn reset_player_tint_if_needed(
    sprite: &mut Sprite,
    current_id: AssetId<Image>,
    base_id: AssetId<Image>,
    base_image: Handle<Image>,
) {
    if current_id != base_id {
        sprite.image = base_image;
    }
}

fn tinted_player_image(
    base_image: &Handle<Image>,
    base_id: AssetId<Image>,
    tint: CrystalTint,
    images: &mut Assets<Image>,
    shape_map: &mut SpriteShapeMap,
    tint_cache: &mut ElementalSpriteTintCache,
) -> Option<Handle<Image>> {
    let tint_key = (base_id, tint);
    if let Some(existing) = tint_cache.variants.get(&tint_key) {
        return Some(existing.clone());
    }

    let base_asset = images.get(base_image)?;
    let tinted_asset = clone_tinted_image(base_asset, tint)?;
    let tinted_handle = images.add(tinted_asset);
    copy_tinted_sprite_shape_indices(shape_map, base_id, tinted_handle.id());

    tint_cache
        .reverse
        .insert(tinted_handle.id(), base_image.clone());
    tint_cache.variants.insert(tint_key, tinted_handle.clone());
    Some(tinted_handle)
}

fn copy_tinted_sprite_shape_indices(
    shape_map: &mut SpriteShapeMap,
    base_id: AssetId<Image>,
    tinted_id: AssetId<Image>,
) {
    if let Some(indices) = shape_map.map.get(&base_id).cloned() {
        shape_map.map.insert(tinted_id, indices);
    }
}

/// Process passive events and apply resulting actions (heals, cooldown changes, etc.).
/// Runs after emitters (e.g., damage source pipeline, XP bridge) and before event clear.
#[expect(clippy::too_many_arguments, reason = "Bevy system dependencies are expressed as explicit system parameters.")]
pub fn process_passive_events_system(
    mut world: InstantWorld,
    mut query: Query<
        (
            Entity,
            &Passives,
            &mut Health,
            PassiveOwner,
            Option<ActionRef<crate::game::player::states::Elevator>>,
            Option<&mut WhirlAbility>,
            Option<&mut super::super::FlickerAbility>,
            Option<&mut crate::game::player::crits::Crits>,
        ),
        With<Player>,
    >,
    pose_query: Query<
        (
            &Transform,
            &crate::game::gentstate::Facing,
            &super::super::SkillInventory,
            &crate::game::player::sensors::GroundSensor,
        ),
        With<Player>,
    >,
    mut materials_q: Query<&mut crate::game::player::MaterialInventory, With<Player>>,
    mut passive_events: MessageReader<super::PassiveEvent>,
    mut heal_events: MessageWriter<
        crate::game::player::passives::blood_shards::HealApplied,
    >,
    damage_source_query: Query<&crate::game::combat::DamageSource>,
    time: Res<GameTime>,
    stat_query: Query<&crate::game::player::PlayerStatMod, With<Player>>,
    mut profiler: TickProfiler,
) {
    let _profile = profiler.scope("player_passive_events");
    for (
        entity,
        passives_component,
        mut health,
        owner,
        elevator,
        mut maybe_whirl,
        mut maybe_flicker,
        mut maybe_crits,
    ) in query.iter_mut()
    {
        let Ok((transform, facing, skill_inventory, ground_sensor)) =
            pose_query.get(entity)
        else {
            continue;
        };
        let active_passives = ordered_passives(passives_component);

        let base_context = owner.context(Some(&*health));

        let mut actions_to_apply =
            collect_passive_tick_actions(&active_passives, &base_context, time.tick());
        extend_applicable_passive_event_actions(
            &mut actions_to_apply,
            &active_passives,
            passive_events.read(),
            entity,
            &damage_source_query,
            &base_context,
        );

        let mut action_context = PassiveActionContext {
            world: &mut world,
            entity,
            health: &mut health,
            heal_events: &mut heal_events,
            whirl: maybe_whirl.as_deref_mut(),
            flicker: maybe_flicker.as_deref_mut(),
            crits: maybe_crits.as_deref_mut(),
            elevator: elevator.as_deref(),
            time: &time,
            stat_query: &stat_query,
            skill_inventory,
            transform,
            facing,
            ground_sensor,
            materials_q: &mut materials_q,
        };
        action_context.apply_actions(actions_to_apply);
    }
}

struct PassiveActionContext<
    'a,
    'commands_world,
    'commands_state,
    'events_world,
    'stat_world,
    'stat_state,
    'stat_data,
    'materials_world,
    'materials_state,
    'materials_data,
> {
    world: &'a mut InstantWorld<'commands_world, 'commands_state>,
    entity: Entity,
    health: &'a mut Health,
    heal_events: &'a mut MessageWriter<'events_world, HealApplied>,
    whirl: Option<&'a mut WhirlAbility>,
    flicker: Option<&'a mut FlickerAbility>,
    crits: Option<&'a mut Crits>,
    elevator: Option<&'a crate::game::player::states::Elevator>,
    time: &'a GameTime,
    stat_query:
        &'a Query<'stat_world, 'stat_state, &'stat_data PlayerStatMod, With<Player>>,
    skill_inventory: &'a SkillInventory,
    transform: &'a Transform,
    facing: &'a Facing,
    ground_sensor: &'a GroundSensor,
    materials_q: &'a mut Query<
        'materials_world,
        'materials_state,
        &'materials_data mut MaterialInventory,
        With<Player>,
    >,

}

impl PassiveActionContext<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_> {
    fn apply_actions(&mut self, actions: Vec<super::PassiveAction>) {
        for action in actions {
            self.apply_action(action);
        }
    }

    fn apply_action(&mut self, action: super::PassiveAction) {
        match action {
            super::PassiveAction::Heal(amount) => self.heal(amount),
            super::PassiveAction::ReduceCooldowns(dt_seconds) => {
                self.reduce_cooldowns(dt_seconds);
            },
            super::PassiveAction::ResetCooldowns => self.reset_cooldowns(),
            super::PassiveAction::AddEnergy(amount) => {
                self.add_channelled_energy(amount);
            },
            super::PassiveAction::RefillEnergyFull => {
                self.refill_channelled_energy();
            },
            super::PassiveAction::ScheduleNextCrit => self.schedule_next_crit(),
            super::PassiveAction::TriggerInstantSkills { source_damage } => {
                self.trigger_instant_skills(source_damage);
            },
        }
    }

    fn heal(&mut self, amount: u32) {
        // Always emit the event: Blood Shards trigger on overheal.
        self.heal_events.write(HealApplied::heal(self.entity, self.health, amount));
    }

    fn reduce_cooldowns(&mut self, dt_seconds: f32) {
        // Centralized: reduce all skill cooldowns for this entity by dt (in ticks).
        let dt_ticks = dt_seconds * 96.0;
        self.world.cooldowns.reduce_all(self.entity, dt_ticks);
    }

    fn reset_cooldowns(&mut self) {
        self.world.cooldowns.reset_all(self.entity);
    }

    fn add_channelled_energy(&mut self, amount: f32) {
        if let Some(whirl) = self.whirl.as_deref_mut() {
            add_energy_clamped(&mut whirl.energy, amount, whirl_metadata().max_energy);
        }

        if let Some(flicker) = self.flicker.as_deref_mut() {
            add_energy_clamped(
                &mut flicker.energy,
                amount,
                flicker_strike_metadata().max_energy,
            );
        }
    }

    fn refill_channelled_energy(&mut self) {
        if let Some(whirl) = self.whirl.as_deref_mut() {
            whirl.energy = whirl_metadata().max_energy;
        }

        if let Some(flicker) = self.flicker.as_deref_mut() {
            flicker.energy = flicker_strike_metadata().max_energy;
        }
    }

    fn schedule_next_crit(&mut self) {
        if let Some(crits) = self.crits.as_deref_mut() {
            crits.schedule_next_crit();
        }
    }

    fn trigger_instant_skills(&mut self, source_damage: Option<Entity>) {
        if self.elevator.is_some() {
            return;
        }

        let now_tick = self.time.tick();
        let cdr_snapshot = self
            .stat_query
            .get(self.entity)
            .map(|stats| stats.cdr)
            .unwrap_or(1.0);
        let equipped = self.skill_inventory.equipped.clone();

        for skill in equipped {
            self.trigger_instant_skill(skill, source_damage, now_tick, cdr_snapshot);
        }
    }

    fn trigger_instant_skill(&mut self, skill: SkillId, source_damage: Option<Entity>, now_tick: u64, cdr: f32) {
        if !skill.is_instant() || skill.is_stackable() {
            return;
        }
        let user = InstantUser { entity: self.entity, transform: self.transform, facing: self.facing,
            ground: self.ground_sensor, tick: now_tick, cdr, origin: ActivationOrigin::PulseDrive { source_damage } };
        let mut materials = self.materials_q.get_mut(self.entity).ok();
        self.world.activate(skill, &user, materials.as_deref_mut());
    }

}

fn collect_passive_tick_actions(
    active_passives: &[Option<super::PassiveEvaluation>],
    base_context: &super::PassiveContext,
    tick: u64,
) -> Vec<super::PassiveAction> {
    let mut actions_to_apply = Vec::new();
    for passive in active_passives.iter().flatten() {
        passive.on_tick(base_context, tick, &mut actions_to_apply);
    }
    actions_to_apply
}

fn extend_applicable_passive_event_actions<'event>(
    actions_to_apply: &mut Vec<super::PassiveAction>,
    active_passives: &[Option<super::PassiveEvaluation>],
    events: impl Iterator<Item = &'event super::PassiveEvent>,
    entity: Entity,
    damage_source_query: &Query<&DamageSource>,
    base_context: &super::PassiveContext,
) {
    for event in events {
        if !passive_event_applies_to_entity(event, entity, damage_source_query) {
            continue;
        }

        extend_passive_event_actions(
            actions_to_apply,
            active_passives,
            event,
            base_context,
        );
    }
}

fn passive_event_applies_to_entity(
    event: &super::PassiveEvent,
    entity: Entity,
    damage_source_query: &Query<&DamageSource>,
) -> bool {
    match *event {
        super::PassiveEvent::SkillUsed { owner, .. }
        | super::PassiveEvent::EnemyKilled { owner }
        | super::PassiveEvent::BackstabKill { owner }
        | super::PassiveEvent::HitCountAdvanced { owner, .. } => owner == entity,
        super::PassiveEvent::XpOrbPickup => true,
        super::PassiveEvent::CriticalHit { damage_source, .. }
        | super::PassiveEvent::DamageHit { damage_source } => damage_source_query
            .get(damage_source)
            .map(|ds| ds.owner == entity)
            .unwrap_or(false),
        super::PassiveEvent::DamageDealt(info) => info.owner == entity,
        super::PassiveEvent::DamageTaken(info) => info.target == entity,
    }
}

fn extend_passive_event_actions(
    actions_to_apply: &mut Vec<super::PassiveAction>,
    active_passives: &[Option<super::PassiveEvaluation>],
    event: &super::PassiveEvent,
    base_context: &super::PassiveContext,
) {
    for passive in active_passives.iter().flatten() {
        passive.on_message(event, base_context, actions_to_apply);
    }
}
