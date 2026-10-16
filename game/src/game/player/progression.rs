use leafwing_input_manager::prelude::ActionState;
use std::collections::HashMap;

use crate::game::capsule_commerce::{CapsuleRecipeId, CraftingKnowledge};
use crate::game::player::skills::types::SkillId;
use crate::game::player::spawns::xp_orbs::XpOrbPickup;
use crate::game::player::weapon::{PlayerAmmo, PlayerRangedWeapon, WeaponInventory};
use crate::game::player::{
    Passives, Player, PlayerAction, PlayerGfx, PlayerStateSet,
    SkillInventory,
};
use crate::graphics::progression_particles::{
    spawn_progression_particle_burst,
    ProgressionBurstParticleEffectHandle,
};
use crate::prelude::*;
use bevy::camera::visibility::RenderLayers;
use theseeker_engine::gent::Gent;

pub struct PlayerProgressionPlugin;

impl Plugin for PlayerProgressionPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AdaptationLevelUpEvent>();
        app.init_resource::<UnlockedSkillPresentationState>();
        app.add_systems(
            GameTickUpdate,
            tick_unlocked_skill_presentation_state
                .before(apply_xp_orb_pickups)
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            GameTickUpdate,
            apply_xp_orb_pickups
                .after(crate::game::player::spawns::xp_orbs::XpOrbSet)
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            GameTickUpdate,
            apply_essence_from_xp_orbs
                .after(crate::game::player::spawns::xp_orbs::XpOrbSet)
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            GameTickUpdate,
            (
                apply_level_up_world_feedback,
                tick_player_level_up_flash,
            )
                .chain()
                .after(apply_xp_orb_pickups)
                .run_if(in_state(AppState::InGame)),
        );
        app.add_systems(
            GameTickUpdate,
            handle_debug_unlock_all_progression
                .after(PlayerStateSet::Behavior)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

#[derive(Message, Debug, Clone, Copy)]
pub struct AdaptationLevelUpEvent {
    pub unlocked_skill: Option<SkillId>,
}

#[derive(Debug, Clone, Copy)]
pub struct SkillUnlockPresentationEntry {
    pub remaining_ticks: u32,
}

#[derive(Resource, Debug, Default)]
pub struct UnlockedSkillPresentationState {
    entries: HashMap<SkillId, SkillUnlockPresentationEntry>,
}

impl UnlockedSkillPresentationState {
    pub const FLASH_TICKS: u32 = 40;

    pub fn trigger(&mut self, skill: SkillId) {
        let entry = SkillUnlockPresentationEntry {
            remaining_ticks: Self::FLASH_TICKS,
        };
        self.entries.insert(skill, entry);
    }

    pub fn entry(
        &self,
        skill: SkillId,
    ) -> Option<SkillUnlockPresentationEntry> {
        self.entries.get(&skill).copied()
    }

    fn tick(&mut self) {
        self.entries.retain(|_, entry| {
            entry.remaining_ticks = entry.remaining_ticks.saturating_sub(1);
            entry.remaining_ticks > 0
        });
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct AdaptationProgress {
    pub adaptation: u32,
    pub current_xp: u32,
    pub xp_to_next: u32,
}

#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Essence(u32);

impl Essence {
    pub const PER_XP_ORB: u32 = 1;
    pub const MAX: u32 = 999;

    pub const fn get(self) -> u32 {
        self.0
    }

    pub const fn can_spend(self, amount: u32) -> bool {
        self.0 >= amount
    }

    pub fn try_spend(&mut self, amount: u32) -> bool {
        if !self.can_spend(amount) {
            return false;
        }
        self.0 -= amount;
        true
    }

    pub(crate) fn grant(&mut self, amount: u32) -> bool {
        let next = self.0.saturating_add(amount).min(Self::MAX);
        let changed = next != self.0;
        self.0 = next;
        changed
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EssenceOrbCadence {
    next_pickup_grants: bool,
}

impl Default for EssenceOrbCadence {
    fn default() -> Self {
        Self {
            next_pickup_grants: true,
        }
    }
}

impl EssenceOrbCadence {
    fn consume_pickups(&mut self, pickup_count: u32) -> u32 {
        let has_unpaired_pickup = !pickup_count.is_multiple_of(2);
        let grants = pickup_count / 2
            + u32::from(self.next_pickup_grants && has_unpaired_pickup);
        if has_unpaired_pickup {
            self.next_pickup_grants = !self.next_pickup_grants;
        }
        grants
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct Alignment {
    current: f64,
    zero_seconds: f64,
    damage_carry: f64,
}

const ALIGNMENT_DEPLETION_SECONDS: f64 = 32.0;
const ALIGNMENT_REGENERATION_SECONDS: f64 = 1.0;
const ALIGNMENT_INITIAL_DAMAGE_PER_SECOND: f64 = 1.0;
const ALIGNMENT_DAMAGE_DOUBLING_SECONDS: f64 = 3.0;
const ALIGNMENT_FACTOR_EPSILON: f64 = 1.0e-9;

impl Default for Alignment {
    fn default() -> Self {
        Self {
            current: 1.0,
            zero_seconds: 0.0,
            damage_carry: 0.0,
        }
    }
}

impl Alignment {
    pub fn factor(&self) -> f32 {
        self.current.clamp(0.0, 1.0) as f32
    }

    pub fn tick(&mut self, connected: bool, tick_seconds: f64) -> u32 {
        let tick_seconds = tick_seconds.max(0.0);
        if connected {
            self.current = (self.current
                + tick_seconds / ALIGNMENT_REGENERATION_SECONDS)
                .min(1.0);
        } else {
            self.current = (self.current
                - tick_seconds / ALIGNMENT_DEPLETION_SECONDS)
                .max(0.0);
        }
        if self.current <= ALIGNMENT_FACTOR_EPSILON {
            self.current = 0.0;
        } else if 1.0 - self.current <= ALIGNMENT_FACTOR_EPSILON {
            self.current = 1.0;
        }

        if self.current > 0.0 {
            self.zero_seconds = 0.0;
            self.damage_carry = 0.0;
            return 0;
        }

        self.damage_carry +=
            alignment_damage_per_second(self.zero_seconds) * tick_seconds;
        self.zero_seconds += tick_seconds;
        let whole_damage = self.damage_carry.floor() as u32;
        self.damage_carry -= f64::from(whole_damage);
        whole_damage
    }
}

fn alignment_damage_per_second(zero_seconds: f64) -> f64 {
    ALIGNMENT_INITIAL_DAMAGE_PER_SECOND
        * 2.0_f64.powf(
            zero_seconds / ALIGNMENT_DAMAGE_DOUBLING_SECONDS,
        )
}

impl Default for AdaptationProgress {
    fn default() -> Self {
        Self {
            adaptation: 0,
            current_xp: 0,
            xp_to_next: Self::initial_xp_to_next(),
        }
    }
}

impl AdaptationProgress {
    pub const XP_PER_ORB: u32 = 1;

    pub const fn initial_xp_to_next() -> u32 {
        70
    }

    pub const fn max_normal_adaptation() -> u32 {
        ADAPTATION_SKILL_UNLOCKS.len() as u32
    }

    pub fn advance_adaptation(&mut self) {
        self.adaptation = self.adaptation.saturating_add(1);
        self.xp_to_next = xp_required_for_adaptation(self.adaptation);
    }

    pub fn ensure_debug_floor(&mut self) {
        let floor = Self::max_normal_adaptation();
        if self.adaptation < floor {
            self.adaptation = floor;
        }
        self.current_xp = 0;
        self.xp_to_next = xp_required_for_adaptation(self.adaptation);
    }
}

const ADAPTATION_SKILL_UNLOCKS: [SkillId; 5] = [
    SkillId::Dash,
    SkillId::Whirl,
    SkillId::Stealth,
    SkillId::DashStrike,
    SkillId::FlickerStrike,
];

fn xp_required_for_adaptation(adaptation: u32) -> u32 {
    let mut required = AdaptationProgress::initial_xp_to_next();
    for _ in 0..adaptation {
        required = required.saturating_mul(2);
    }
    required
}

fn skill_for_adaptation(adaptation: u32) -> Option<SkillId> {
    let index = adaptation.checked_sub(1)? as usize;
    ADAPTATION_SKILL_UNLOCKS.get(index).copied()
}

fn unlock_skill_for_adaptation(
    adaptation: u32,
    skills: &mut SkillInventory,
    max_slots: usize,
    unlock_presentations: &mut UnlockedSkillPresentationState,
) -> Option<SkillId> {
    let skill = skill_for_adaptation(adaptation)?;
    if !skills.unlock_skill_with_capacity(skill, max_slots) {
        return None;
    }

    unlock_presentations.trigger(skill);
    info!(
        "Adaptation {} unlocked {}",
        adaptation,
        skill.name(),
    );
    Some(skill)
}

fn apply_xp_orb_pickups(
    mut players: Query<
        (
            &mut AdaptationProgress,
            &Passives,
            &mut SkillInventory,
        ),
        With<Player>,
    >,
    mut xp_events: MessageReader<XpOrbPickup>,
    mut level_up_events: MessageWriter<AdaptationLevelUpEvent>,
    mut unlock_presentations: ResMut<UnlockedSkillPresentationState>,
) {
    let gained_xp: u32 = xp_events.read().map(|event| event.amount).sum();
    if gained_xp == 0 {
        return;
    }

    let Ok((mut progression, passives, mut skills)) = players.single_mut()
    else {
        return;
    };

    progression.current_xp = progression.current_xp.saturating_add(gained_xp);
    let max_slots = SkillInventory::max_slots_for(passives);

    while progression.current_xp >= progression.xp_to_next {
        progression.current_xp -= progression.xp_to_next;
        progression.advance_adaptation();
        info!(
            "Adaptation increased to {}",
            progression.adaptation
        );

        let unlocked_skill = unlock_skill_for_adaptation(
            progression.adaptation,
            &mut skills,
            max_slots,
            &mut unlock_presentations,
        );

        level_up_events.write(AdaptationLevelUpEvent {
            unlocked_skill,
        });
    }
}

pub(crate) fn apply_essence_from_xp_orbs(
    mut players: Query<(&mut Essence, &mut EssenceOrbCadence), With<Player>>,
    mut xp_events: MessageReader<XpOrbPickup>,
) {
    let pickup_count = xp_events.read().fold(0_u32, |count, _| {
        count.saturating_add(1)
    });
    if pickup_count == 0 {
        return;
    }

    let Ok((mut essence, mut cadence)) = players.single_mut() else {
        return;
    };
    let gained = cadence
        .consume_pickups(pickup_count)
        .saturating_mul(Essence::PER_XP_ORB);
    essence.grant(gained);
}

#[derive(Component, Debug, Clone, Copy)]
struct PlayerLevelUpFlash {
    remaining_ticks: u32,
    total_ticks: u32,
    base_color: Color,
}

impl PlayerLevelUpFlash {
    const TICKS: u32 = 24;

    fn new(base_color: Color) -> Self {
        Self {
            remaining_ticks: Self::TICKS,
            total_ticks: Self::TICKS,
            base_color,
        }
    }
}

fn tick_unlocked_skill_presentation_state(
    mut presentations: ResMut<UnlockedSkillPresentationState>,
) {
    presentations.tick();
}

fn apply_level_up_world_feedback(
    mut events: MessageReader<AdaptationLevelUpEvent>,
    player_q: Query<&Gent, With<Player>>,
    player_gfx_q: Query<&Sprite, With<PlayerGfx>>,
    burst_particles: Option<Res<ProgressionBurstParticleEffectHandle>>,
    mut commands: Commands,
) {
    let Ok(player_gent) = player_q.single() else {
        return;
    };

    for _ in events.read() {
        let base_color = player_gfx_q
            .get(player_gent.e_gfx)
            .map(|sprite| sprite.color)
            .unwrap_or(Color::WHITE);
        commands
            .entity(player_gent.e_gfx)
            .insert(PlayerLevelUpFlash::new(base_color));

        if let Some(handle) = burst_particles.as_deref() {
            spawn_progression_particle_burst(
                &mut commands,
                Some(player_gent.e_gfx),
                handle.0.clone(),
                Transform::from_translation(Vec3::new(0.0, 0.0, 0.02)),
                Some(RenderLayers::layer(2)),
            );
        }
    }
}

fn tick_player_level_up_flash(
    mut query: Query<
        (Entity, &mut Sprite, &mut PlayerLevelUpFlash),
        With<PlayerGfx>,
    >,
    mut commands: Commands,
) {
    for (entity, mut sprite, mut flash) in query.iter_mut() {
        let t = flash.remaining_ticks as f32 / flash.total_ticks as f32;
        let boost = 1.0 + 1.8 * t.powf(0.45);
        sprite.color = brighten_color(flash.base_color, boost);

        flash.remaining_ticks = flash.remaining_ticks.saturating_sub(1);
        if flash.remaining_ticks == 0 {
            sprite.color = flash.base_color;
            commands.entity(entity).remove::<PlayerLevelUpFlash>();
        }
    }
}

fn brighten_color(base_color: Color, boost: f32) -> Color {
    let base = base_color.to_linear();
    Color::linear_rgba(
        base.red * boost,
        base.green * boost,
        base.blue * boost,
        base.alpha,
    )
}

fn handle_debug_unlock_all_progression(
    mut players: Query<
        (
            &ActionState<PlayerAction>,
            &mut Passives,
            &mut crate::game::player::MaterialInventory,
            &mut SkillInventory,
            &mut WeaponInventory,
            &mut AdaptationProgress,
            Option<&mut CraftingKnowledge>,
        ),
        With<Player>,
    >,
) {
    for (
        actions,
        mut passives,
        mut materials,
        mut skills,
        mut weapons,
        mut progression,
        maybe_knowledge,
    ) in players.iter_mut()
    {
        if !actions.just_pressed(&PlayerAction::DebugUnlockAllProgression) {
            continue;
        }

        passives.unlock_all_max();
        if let Some(mut knowledge) = maybe_knowledge {
            discover_all_craft_recipes(&mut knowledge);
        }

        let max_slots = SkillInventory::max_slots_for(&passives);
        skills.unlock_all_skills(max_slots);
        weapons.unlock_ranged(PlayerRangedWeapon::Bow);
        weapons.equip_ranged(PlayerRangedWeapon::Bow);
        weapons.add_ammo(PlayerAmmo::Arrows, 100);
        weapons.equip_ammo(PlayerAmmo::Arrows);
        materials.add_to_backpack(
            crate::game::player::CraftMaterial::AmplifiedBell,
            12,
        );
        materials.add_to_backpack(
            crate::game::player::CraftMaterial::ExplosiveMine,
            100,
        );
        materials.add_to_backpack(
            crate::game::player::CraftMaterial::QuarkPylon,
            100,
        );
        materials.add_to_backpack(
            crate::game::player::CraftMaterial::Tether,
            100,
        );
        materials.add_to_backpack(
            crate::game::player::CraftMaterial::Spinner,
            100,
        );
        materials.add_to_backpack(
            crate::game::player::CraftMaterial::FrostBomb,
            100,
        );
        progression.ensure_debug_floor();

        info!("Debug unlock: all skills and passives unlocked");
    }
}

fn discover_all_craft_recipes(knowledge: &mut CraftingKnowledge) {
    knowledge.discovered.extend(CapsuleRecipeId::ALL);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn depleted_alignment() -> Alignment {
        Alignment {
            current: 0.0,
            zero_seconds: 0.0,
            damage_carry: 0.0,
        }
    }

    fn assert_depletes_in_32_seconds(tick_rate: u32) {
        let tick_seconds = 1.0 / f64::from(tick_rate);
        let depletion_ticks =
            (ALIGNMENT_DEPLETION_SECONDS * f64::from(tick_rate)) as u32;
        let mut alignment = Alignment::default();

        for _ in 0..depletion_ticks - 1 {
            alignment.tick(false, tick_seconds);
        }
        assert!(alignment.factor() > 0.0);

        alignment.tick(false, tick_seconds);
        assert_eq!(alignment.factor(), 0.0);
    }

    fn apply_death_shroud_regeneration(health: u32, tick: u32) -> u32 {
        if health == 0 || health >= 60 || !tick.is_multiple_of(8) {
            return health;
        }
        health.saturating_add(1).min(120)
    }

    #[test]
    fn alignment_depletion_uses_real_time_across_tick_rates() {
        assert_depletes_in_32_seconds(96);
        assert_depletes_in_32_seconds(144);
    }

    #[test]
    fn alignment_regenerates_fully_in_one_second() {
        let tick_rate = 96;
        let tick_seconds = 1.0 / f64::from(tick_rate);
        let mut alignment = depleted_alignment();

        for _ in 0..tick_rate - 1 {
            alignment.tick(true, tick_seconds);
        }
        assert!(alignment.factor() < 1.0);

        alignment.tick(true, tick_seconds);
        assert_eq!(alignment.factor(), 1.0);
    }

    #[test]
    fn depleted_alignment_damage_doubles_every_three_seconds() {
        let checkpoints = [(0.0, 1.0), (3.0, 2.0), (6.0, 4.0)];
        for (seconds, expected_damage_per_second) in checkpoints {
            let actual = alignment_damage_per_second(seconds);
            assert!((actual - expected_damage_per_second).abs() < 0.000_01);
        }
    }

    #[test]
    fn depleted_alignment_overwhelms_death_shroud_regeneration() {
        let tick_rate = 96_u32;
        let tick_seconds = 1.0 / f64::from(tick_rate);
        let mut alignment = depleted_alignment();
        let mut health = 120_u32;

        for tick in 0..20 * tick_rate {
            let damage = alignment.tick(false, tick_seconds);
            health = health.saturating_sub(damage);
            health = apply_death_shroud_regeneration(health, tick);
        }

        assert_eq!(health, 0);
    }

    #[test]
    fn essence_grants_one_per_qualifying_orb_and_caps_at_three_digits() {
        let mut essence = Essence::default();

        assert!(essence.grant(Essence::PER_XP_ORB));
        assert_eq!(essence.get(), 1);
        assert!(essence.grant(u32::MAX));
        assert_eq!(essence.get(), Essence::MAX);
        assert!(!essence.grant(Essence::PER_XP_ORB));
    }

    #[test]
    fn essence_spending_is_atomic() {
        let mut essence = Essence::default();
        let _ = essence.grant(100);

        assert!(!essence.try_spend(101));
        assert_eq!(essence.get(), 100);
        assert!(essence.try_spend(100));
        assert_eq!(essence.get(), 0);
    }

    #[test]
    fn essence_orb_cadence_grants_on_first_third_and_fifth_pickups() {
        let mut cadence = EssenceOrbCadence::default();

        assert_eq!(cadence.consume_pickups(1), 1);
        assert_eq!(cadence.consume_pickups(1), 0);
        assert_eq!(cadence.consume_pickups(1), 1);
        assert_eq!(cadence.consume_pickups(2), 1);
        assert_eq!(cadence.consume_pickups(2), 1);
    }
}
