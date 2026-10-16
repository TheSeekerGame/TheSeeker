use crate::game::player::material_inventory::MaterialCost;
use std::collections::HashSet;

use crate::prelude::*;

use crate::game::player::skills::types::SkillId;
use crate::game::player::weapon::{PlayerRangedWeapon, WeaponInventory};
use crate::game::player::{
    CraftMaterial, Essence, MaterialInventory, Passive, Passives,
    SkillInventory,
};

pub(crate) const CAPSULE_ACTION_ESSENCE_COST: u32 = 20;

/// Capsule recipes the player has discovered through research; persists across respawn.
#[derive(Component, Debug, Default, Clone)]
pub struct CraftingKnowledge {
    pub discovered: HashSet<CapsuleRecipeId>,
}

impl CraftingKnowledge {
    pub fn is_discovered(&self, recipe: CapsuleRecipeId) -> bool {
        self.discovered.contains(&recipe)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum CapsuleRecipeId {
    BowBundle,
    Arrows,
    TetherNode,
    ExplosiveMines,
    QuarkPylons,
    Spinners,
    FrostBombs,
    AmplifiedBells,
    PortableTent,
    BloodShards,
    Bloodstone,
    CriticalRegeneration,
    DeathShroud,
    FlamingHeart,
    Permutator,
    PrimeSygil,
}

impl CapsuleRecipeId {
    pub const ALL: [Self; 16] = [
        Self::BowBundle,
        Self::Arrows,
        Self::TetherNode,
        Self::ExplosiveMines,
        Self::QuarkPylons,
        Self::Spinners,
        Self::FrostBombs,
        Self::AmplifiedBells,
        Self::PortableTent,
        Self::BloodShards,
        Self::Bloodstone,
        Self::CriticalRegeneration,
        Self::DeathShroud,
        Self::FlamingHeart,
        Self::Permutator,
        Self::PrimeSygil,
    ];
}

/// An item the capsule gives the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum PlayerItemGrant {
    BowBundle { arrows: u32 },
    Arrows { amount: u32 },
    SkillStock { skill: SkillId, material: CraftMaterial, amount: u32 },
    Passive(Passive),
}

impl PlayerItemGrant {
    /// Whether the player already owns this one-time item; stock can always be granted again.
    pub(crate) fn is_owned(self, passives: &Passives, weapons: &WeaponInventory) -> bool {
        match self {
            Self::BowBundle { .. } => weapons.has_ranged(PlayerRangedWeapon::Bow),
            Self::Passive(passive) => passives.is_unlocked(passive),
            Self::Arrows { .. } | Self::SkillStock { .. } => false,
        }
    }

    /// Adds the item, unlocking and equipping the skills it provides.
    pub(crate) fn grant(
        self,
        materials: &mut MaterialInventory,
        skills: &mut SkillInventory,
        passives: &mut Passives,
        weapons: &mut WeaponInventory,
    ) {
        let max_slots = SkillInventory::max_slots_for(passives);
        match self {
            Self::BowBundle { arrows } => {
                weapons.grant_bow_bundle(arrows);
                skills.unlock_skill_with_capacity(SkillId::BowAttack, max_slots);
            },
            Self::Arrows { amount } => weapons.grant_arrows(amount),
            Self::SkillStock { skill, material, amount } => {
                materials.add_to_backpack(material, amount);
                grant_skill(skills, skill, max_slots);
            },
            Self::Passive(passive) => passives.add_passive(passive),
        }
    }
}

/// Unlocks `skill` and equips it when a slot is free.
fn grant_skill(skills: &mut SkillInventory, skill: SkillId, max_slots: usize) {
    if !skills.is_unlocked(&skill) {
        skills.unlock_skill(skill);
    }
    if !skills.is_equipped(&skill) {
        skills.auto_equip_skill(skill, max_slots);
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CapsuleRecipe {
    pub id: CapsuleRecipeId,
    pub title: &'static str,
    pub description: &'static str,
    pub icon_path: &'static str,
    pub output: PlayerItemGrant,
    pub ingredients: &'static [MaterialCost],
}

macro_rules! ingredients {
    ($(($material:ident, $amount:expr)),+ $(,)?) => {
        &[$(MaterialCost {
            material: CraftMaterial::$material,
            amount: $amount,
        }),+]
    };
}

pub(crate) const CAPSULE_RECIPES: [CapsuleRecipe; 16] = [
    CapsuleRecipe {
        id: CapsuleRecipeId::BowBundle,
        title: "Bow + 100 Arrows",
        description: "Ranged weapon and ammo",
        icon_path: "items/weapons/BowIcon.png",
        output: PlayerItemGrant::BowBundle { arrows: 100 },
        ingredients: ingredients!((PlantFiber, 5), (BatWing, 2), (Ore1, 2)),
    },
    CapsuleRecipe {
        id: CapsuleRecipeId::Arrows,
        title: "Arrows x100",
        description: "Restocks bow ammunition",
        icon_path: "items/weapons/ArrowsIcon.png",
        output: PlayerItemGrant::Arrows { amount: 100 },
        ingredients: ingredients!((PlantFiber, 4), (Ore1, 2)),
    },
    CapsuleRecipe {
        id: CapsuleRecipeId::TetherNode,
        title: "Tether Node",
        description: "Extends the Alignment network",
        icon_path: "ui/game/TetherSkillIcon.png",
        output: PlayerItemGrant::SkillStock {
            skill: SkillId::TetherNode,
            material: CraftMaterial::Tether,
            amount: 1,
        },
        ingredients: ingredients!((Ore2, 2), (IceShard, 2), (MossFragment, 1)),
    },
    CapsuleRecipe {
        id: CapsuleRecipeId::ExplosiveMines,
        title: "Explosive Mines x100",
        description: "Deploys pressure mines",
        icon_path: "ui/game/MineSkillIcon.png",
        output: PlayerItemGrant::SkillStock {
            skill: SkillId::ExplosiveMine,
            material: CraftMaterial::ExplosiveMine,
            amount: 100,
        },
        ingredients: ingredients!((Ore1, 3), (RedBerry, 2), (MossFragment, 1)),
    },
    CapsuleRecipe {
        id: CapsuleRecipeId::QuarkPylons,
        title: "Quark Pylons x100",
        description: "Paired damage pylons",
        icon_path: "ui/game/PylonSkillIcon.png",
        output: PlayerItemGrant::SkillStock {
            skill: SkillId::QuarkPylon,
            material: CraftMaterial::QuarkPylon,
            amount: 100,
        },
        ingredients: ingredients!((Ore3, 3), (IceShard, 2), (BlueBerry, 1)),
    },
    CapsuleRecipe {
        id: CapsuleRecipeId::Spinners,
        title: "Spinners x100",
        description: "Returning combat drones",
        icon_path: "ui/game/SpinnerSkillIcon.png",
        output: PlayerItemGrant::SkillStock {
            skill: SkillId::Spinner,
            material: CraftMaterial::Spinner,
            amount: 100,
        },
        ingredients: ingredients!((Ore2, 3), (BatWing, 2), (PlantFiber, 2)),
    },
    CapsuleRecipe {
        id: CapsuleRecipeId::FrostBombs,
        title: "Frost Bombs x100",
        description: "Freezes nearby enemies",
        icon_path: "ui/game/IceNovaSkillIcon.png",
        output: PlayerItemGrant::SkillStock {
            skill: SkillId::IceNova,
            material: CraftMaterial::FrostBomb,
            amount: 100,
        },
        ingredients: ingredients!((IceShard, 4), (BlueBerry, 2), (Ore1, 1)),
    },
    CapsuleRecipe {
        id: CapsuleRecipeId::AmplifiedBells,
        title: "Amplified Bells x12",
        description: "Delayed shock rings",
        icon_path: "ui/game/AmplifiedBellSkillIcon.png",
        output: PlayerItemGrant::SkillStock {
            skill: SkillId::AmplifiedBell,
            material: CraftMaterial::AmplifiedBell,
            amount: 12,
        },
        ingredients: ingredients!((Ore2, 3), (Ore3, 1), (BatWing, 2)),
    },
    CapsuleRecipe {
        id: CapsuleRecipeId::PortableTent,
        title: "Portable Tent",
        description: "Portable field shelter",
        icon_path: "ui/game/TentSkillIcon.png",
        output: PlayerItemGrant::SkillStock {
            skill: SkillId::PortableTent,
            material: CraftMaterial::Tent,
            amount: 1,
        },
        ingredients: ingredients!((PlantFiber, 6), (MossFragment, 3), (BatWing, 2)),
    },
    CapsuleRecipe {
        id: CapsuleRecipeId::BloodShards,
        title: "Blood Shards",
        description: "Excess healing makes shards",
        icon_path: "items/passives/BloodShards.png",
        output: PlayerItemGrant::Passive(Passive::BloodShards),
        ingredients: ingredients!((RedBerry, 3), (IceShard, 2), (Ore3, 1)),
    },
    CapsuleRecipe {
        id: CapsuleRecipeId::Bloodstone,
        title: "Bloodstone",
        description: "Steals life from kills",
        icon_path: "items/passives/Bloodstone.png",
        output: PlayerItemGrant::Passive(Passive::Bloodstone),
        ingredients: ingredients!((RedBerry, 4), (MossFragment, 2), (Ore2, 1)),
    },
    CapsuleRecipe {
        id: CapsuleRecipeId::CriticalRegeneration,
        title: "Critical Regeneration",
        description: "Critical hits restore health",
        icon_path: "items/passives/CriticalRegeneration.png",
        output: PlayerItemGrant::Passive(Passive::CriticalRegeneration),
        ingredients: ingredients!((BlueBerry, 3), (RedBerry, 2), (IceShard, 1)),
    },
    CapsuleRecipe {
        id: CapsuleRecipeId::DeathShroud,
        title: "Death Shroud",
        description: "Heals while death is close",
        icon_path: "items/passives/DeathShroud.png",
        output: PlayerItemGrant::Passive(Passive::DeathShroud),
        ingredients: ingredients!((MossFragment, 4), (BatWing, 2), (Ore3, 1)),
    },
    CapsuleRecipe {
        id: CapsuleRecipeId::FlamingHeart,
        title: "Flaming Heart",
        description: "Low health boosts damage",
        icon_path: "items/passives/FlamingHeart.png",
        output: PlayerItemGrant::Passive(Passive::FlamingHeart),
        ingredients: ingredients!((RedBerry, 3), (MossBerry, 3), (Ore2, 1)),
    },
    CapsuleRecipe {
        id: CapsuleRecipeId::Permutator,
        title: "Permutator",
        description: "Rewards alternating skills",
        icon_path: "items/passives/Permutator.png",
        output: PlayerItemGrant::Passive(Passive::Permutator),
        ingredients: ingredients!((Ore1, 2), (Ore2, 2), (Ore3, 2)),
    },
    CapsuleRecipe {
        id: CapsuleRecipeId::PrimeSygil,
        title: "Prime Sygil",
        description: "Hits fresh foes harder",
        icon_path: "items/passives/PrimeSygil.png",
        output: PlayerItemGrant::Passive(Passive::PrimeSygil),
        ingredients: ingredients!((BatWing, 4), (IceShard, 2), (Ore3, 1)),
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CapsuleActionResult {
    Completed,
    MissingEssence,
    MissingIngredients,
    NotDiscovered,
    AlreadyOwned,
    NothingToDiscover,
}

pub(crate) fn recipe(id: CapsuleRecipeId) -> &'static CapsuleRecipe {
    CAPSULE_RECIPES
        .iter()
        .find(|recipe| recipe.id == id)
        .expect("all CapsuleRecipeId values have definitions")
}

pub(crate) fn discoverable_recipes(
    knowledge: &CraftingKnowledge,
    materials: &MaterialInventory,
) -> Vec<CapsuleRecipeId> {
    CAPSULE_RECIPES
        .iter()
        .filter(|recipe| {
            !knowledge.is_discovered(recipe.id)
                && recipe.ingredients.iter().all(|ingredient| {
                    materials.total_count(ingredient.material) > 0
                })
        })
        .map(|recipe| recipe.id)
        .collect()
}

pub(crate) fn research_recipes(
    knowledge: &mut CraftingKnowledge,
    materials: &MaterialInventory,
    essence: &mut Essence,
) -> CapsuleActionResult {
    let discoverable = discoverable_recipes(knowledge, materials);
    if discoverable.is_empty() {
        return CapsuleActionResult::NothingToDiscover;
    }
    if !essence.try_spend(CAPSULE_ACTION_ESSENCE_COST) {
        return CapsuleActionResult::MissingEssence;
    }
    knowledge
        .discovered
        .extend(discoverable);
    CapsuleActionResult::Completed
}

pub(crate) fn purchase_tether(
    essence: &mut Essence,
    materials: &mut MaterialInventory,
    skills: &mut SkillInventory,
    passives: &mut Passives,
    weapons: &mut WeaponInventory,
) -> CapsuleActionResult {
    if !essence.try_spend(CAPSULE_ACTION_ESSENCE_COST) {
        return CapsuleActionResult::MissingEssence;
    }
    recipe(CapsuleRecipeId::TetherNode).output.grant(materials, skills, passives, weapons);
    CapsuleActionResult::Completed
}

pub(crate) fn craft_recipe(
    id: CapsuleRecipeId,
    knowledge: &CraftingKnowledge,
    materials: &mut MaterialInventory,
    skills: &mut SkillInventory,
    passives: &mut Passives,
    weapons: &mut WeaponInventory,
) -> CapsuleActionResult {
    if !knowledge.is_discovered(id) {
        return CapsuleActionResult::NotDiscovered;
    }
    let definition = recipe(id);
    if definition.output.is_owned(passives, weapons) {
        return CapsuleActionResult::AlreadyOwned;
    }
    if !materials.try_pay(definition.ingredients.iter().map(MaterialCost::pair)) {
        return CapsuleActionResult::MissingIngredients;
    }
    definition.output.grant(materials, skills, passives, weapons);
    CapsuleActionResult::Completed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::player::CraftMaterial;

    fn add_materials(
        inventory: &mut MaterialInventory,
        ingredients: &[MaterialCost],
    ) {
        for ingredient in ingredients {
            inventory.add_to_backpack(
                ingredient.material,
                ingredient.amount,
            );
        }
    }

    #[test]
    fn research_uses_types_not_required_quantities() {
        let mut materials = MaterialInventory::empty();
        let recipe = recipe(CapsuleRecipeId::BowBundle);
        for ingredient in recipe.ingredients {
            materials.add_to_storage(ingredient.material, 1);
        }
        let knowledge = CraftingKnowledge::default();

        assert!(discoverable_recipes(&knowledge, &materials)
            .contains(&CapsuleRecipeId::BowBundle));
    }

    #[test]
    fn research_combines_backpack_and_storage_and_charges_once() {
        let definition = recipe(CapsuleRecipeId::BowBundle);
        let mut materials = MaterialInventory::empty();
        materials.add_to_backpack(definition.ingredients[0].material, 1);
        materials.add_to_storage(definition.ingredients[1].material, 1);
        materials.add_to_backpack(definition.ingredients[2].material, 1);
        let mut essence = Essence::default();
        let _ = essence.grant(200);
        let mut knowledge = CraftingKnowledge::default();

        assert_eq!(
            research_recipes(&mut knowledge, &materials, &mut essence),
            CapsuleActionResult::Completed
        );
        assert!(knowledge.is_discovered(CapsuleRecipeId::BowBundle));
        assert_eq!(essence.get(), 180);
        assert_eq!(
            research_recipes(&mut knowledge, &materials, &mut essence),
            CapsuleActionResult::NothingToDiscover
        );
        assert_eq!(essence.get(), 180);
    }

    #[test]
    fn missing_material_type_keeps_recipe_hidden() {
        let mut materials = MaterialInventory::empty();
        materials.add_to_backpack(CraftMaterial::PlantFiber, 99);
        materials.add_to_storage(CraftMaterial::BatWing, 99);

        assert!(!discoverable_recipes(&CraftingKnowledge::default(), &materials)
            .contains(&CapsuleRecipeId::BowBundle));
    }

    #[test]
    fn research_without_new_recipe_does_not_charge() {
        let mut essence = Essence::default();
        let _ = essence.grant(100);
        let mut knowledge = CraftingKnowledge::default();
        let materials = MaterialInventory::empty();

        assert_eq!(
            research_recipes(&mut knowledge, &materials, &mut essence),
            CapsuleActionResult::NothingToDiscover
        );
        assert_eq!(essence.get(), 100);
    }

    #[test]
    fn crafting_removes_backpack_before_storage() {
        let id = CapsuleRecipeId::Arrows;
        let mut knowledge = CraftingKnowledge::default();
        knowledge.discovered.insert(id);
        let mut materials = MaterialInventory::empty();
        materials.add_to_backpack(CraftMaterial::PlantFiber, 2);
        materials.add_to_storage(CraftMaterial::PlantFiber, 2);
        materials.add_to_storage(CraftMaterial::Ore1, 2);
        let mut skills = SkillInventory::default();
        let mut passives = Passives::default();
        let mut weapons = WeaponInventory::default();

        assert_eq!(
            craft_recipe(
                id,
                &knowledge,
                &mut materials,
                &mut skills,
                &mut passives,
                &mut weapons,
            ),
            CapsuleActionResult::Completed
        );
        assert_eq!(materials.backpack_count(CraftMaterial::PlantFiber), 0);
        assert_eq!(materials.storage_count(CraftMaterial::PlantFiber), 0);
    }

    #[test]
    fn tether_shop_is_repeatable_and_outputs_to_backpack() {
        let mut essence = Essence::default();
        let _ = essence.grant(40);
        let mut materials = MaterialInventory::empty();
        let mut skills = SkillInventory::default();
        let mut passives = Passives::default();
        let mut weapons = WeaponInventory::default();

        for _ in 0..2 {
            assert_eq!(
                purchase_tether(
                    &mut essence,
                    &mut materials,
                    &mut skills,
                    &mut passives,
                    &mut weapons,
                ),
                CapsuleActionResult::Completed
            );
        }
        assert_eq!(materials.backpack_count(CraftMaterial::Tether), 2);
        assert_eq!(materials.storage_count(CraftMaterial::Tether), 0);
        assert_eq!(essence.get(), 0);
    }

    #[test]
    fn failed_craft_does_not_mutate_inventory() {
        let id = CapsuleRecipeId::TetherNode;
        let mut knowledge = CraftingKnowledge::default();
        knowledge.discovered.insert(id);
        let mut materials = MaterialInventory::empty();
        add_materials(&mut materials, &recipe(id).ingredients[..2]);
        let before = materials.total_count(CraftMaterial::Ore2);
        let mut skills = SkillInventory::default();
        let mut passives = Passives::default();
        let mut weapons = WeaponInventory::default();

        assert_eq!(
            craft_recipe(id, &knowledge, &mut materials, &mut skills, &mut passives, &mut weapons),
            CapsuleActionResult::MissingIngredients
        );
        assert_eq!(materials.total_count(CraftMaterial::Ore2), before);
    }
}
