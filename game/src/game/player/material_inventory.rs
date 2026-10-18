use crate::game::player::skills::types::SkillId;
use crate::prelude::*;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CraftMaterial {
    BatWing,
    IceShard,
    MossFragment,
    PlantFiber,
    Ink,
    AcidGland,
    TentacleTip,
    NullPearl,
    RedBerry,
    BlueBerry,
    MossBerry,
    Ore1,
    Ore2,
    Ore3,
    AmplifiedBell,
    ExplosiveMine,
    QuarkPylon,
    Spinner,
    FrostBomb,
    Tent,
    Tether,
}

struct MaterialDefinition {
    name: &'static str,
    description: &'static str,
    icon_path: &'static str,
    stock_skill: Option<SkillId>,
    retain_unlock: bool,
}

impl CraftMaterial {
    fn definition(self) -> MaterialDefinition {
        let (name, description, icon_path, stock_skill, retain_unlock) = match self {
            Self::BatWing => ("Bat Wing",
                "Brittle membrane from cave bats.",
                "items/drops/BatWing.png", None, false),
            Self::IceShard => ("Ice Shard",
                "It never quite melts.",
                "items/drops/IceShard.png", None, false),
            Self::MossFragment => ("Moss Fragment",
                "Dense tundra growth.",
                "items/drops/MossFragment.png", None, false),
            Self::PlantFiber => ("Plant Fiber",
                "Tough, fibrous strands.",
                "items/drops/PlantFiber.png", None, false),
            Self::Ink => ("Ink",
                "Squid ink, darker than the dark.",
                "items/drops/Ink.png", None, false),
            Self::AcidGland => ("Acid Gland",
                "A buzzer's orb gland, still fizzing.",
                "items/drops/AcidGland.png", None, false),
            Self::TentacleTip => ("Tentacle Tip",
                "Severed, and still trying to strike.",
                "items/drops/TentacleTip.png", None, false),
            Self::NullPearl => ("Null Pearl",
                "Left where a Void came apart.",
                "items/drops/NullPearl.png", None, false),
            Self::RedBerry => ("Red Berry",
                "Tart and restorative.",
                "items/drops/RedBerry.png", Some(SkillId::RedBerry), false),
            Self::BlueBerry => ("Blue Berry",
                "Cool and quickening.",
                "items/drops/BlueBerry.png", Some(SkillId::BlueBerry), false),
            Self::MossBerry => ("Moss Berry",
                "Dense and fierce.",
                "items/drops/MossBerry.png", Some(SkillId::MossBerry), false),
            Self::Ore1 => ("Ore 1",
                "Common tundra ore.",
                "items/drops/Ore1.png", None, false),
            Self::Ore2 => ("Ore 2",
                "Rarer tundra ore.",
                "items/drops/Ore2.png", None, false),
            Self::Ore3 => ("Ore 3",
                "Ore from deep ice.",
                "items/drops/Ore3.png", None, false),
            Self::AmplifiedBell => ("Amplified Bell",
                "A tuned bell, waiting to ring.",
                "ui/game/AmplifiedBellSkillIcon.png", Some(SkillId::AmplifiedBell), true),
            Self::ExplosiveMine => ("Explosive Mine",
                "Handle with care.",
                "ui/game/MineSkillIcon.png", Some(SkillId::ExplosiveMine), true),
            Self::QuarkPylon => ("Quark Pylon",
                "It hums faintly.",
                "ui/game/PylonSkillIcon.png", Some(SkillId::QuarkPylon), true),
            Self::Spinner => ("Spinner",
                "A drone, folded tight.",
                "ui/game/SpinnerSkillIcon.png", Some(SkillId::Spinner), true),
            Self::FrostBomb => ("Frost Bomb",
                "Cold to the touch.",
                "ui/game/IceNovaSkillIcon.png", Some(SkillId::IceNova), true),
            Self::Tent => ("Portable Tent",
                "A shelter, folded small.",
                "ui/game/TentSkillIcon.png", Some(SkillId::PortableTent), false),
            Self::Tether => ("Tether Node",
                "Extends the Alignment network.",
                "items/drops/Tether.png", Some(SkillId::TetherNode), true),
        };
        MaterialDefinition { name, description, icon_path, stock_skill, retain_unlock }
    }

    pub const STORAGE: [CraftMaterial; 21] = [
        CraftMaterial::BatWing,
        CraftMaterial::IceShard,
        CraftMaterial::MossFragment,
        CraftMaterial::PlantFiber,
        CraftMaterial::Ink,
        CraftMaterial::AcidGland,
        CraftMaterial::TentacleTip,
        CraftMaterial::NullPearl,
        CraftMaterial::RedBerry,
        CraftMaterial::BlueBerry,
        CraftMaterial::MossBerry,
        CraftMaterial::Ore1,
        CraftMaterial::Ore2,
        CraftMaterial::Ore3,
        CraftMaterial::AmplifiedBell,
        CraftMaterial::ExplosiveMine,
        CraftMaterial::QuarkPylon,
        CraftMaterial::Spinner,
        CraftMaterial::FrostBomb,
        CraftMaterial::Tent,
        CraftMaterial::Tether,
    ];

    pub const SKILL_STOCK_MATERIALS: [CraftMaterial; 10] = [
        CraftMaterial::RedBerry,
        CraftMaterial::BlueBerry,
        CraftMaterial::MossBerry,
        CraftMaterial::AmplifiedBell,
        CraftMaterial::ExplosiveMine,
        CraftMaterial::QuarkPylon,
        CraftMaterial::Spinner,
        CraftMaterial::FrostBomb,
        CraftMaterial::Tent,
        CraftMaterial::Tether,
    ];

    pub fn name(self) -> &'static str { self.definition().name }

    pub fn description(self) -> &'static str { self.definition().description }

    pub fn icon_path(self) -> &'static str { self.definition().icon_path }

    pub fn stock_skill(self) -> Option<SkillId> { self.definition().stock_skill }

    pub fn retains_skill_unlock_when_empty(self) -> bool { self.definition().retain_unlock }
}

#[derive(Component, Debug, Clone)]
pub struct MaterialInventory {
    storage: HashMap<CraftMaterial, u32>,
    backpack: HashMap<CraftMaterial, u32>,
}

impl Default for MaterialInventory {
    fn default() -> Self {
        let mut inventory = Self {
            storage: HashMap::new(),
            backpack: HashMap::new(),
        };
        inventory.add_to_backpack(CraftMaterial::Tether, 5);
        inventory
    }
}

impl MaterialInventory {
    /// Pays the whole cost, backpack stock first and then storage, or nothing when any
    /// material (counting repeats) is short.
    pub(crate) fn try_pay(&mut self, costs: impl IntoIterator<Item = (CraftMaterial, u32)>) -> bool {
        let totals = cost_totals(costs);
        if !totals.iter().all(|(&material, &amount)| self.total_count(material) >= amount) {
            return false;
        }
        for (material, amount) in totals {
            self.remove_combined(material, amount);
        }
        true
    }

    #[cfg(test)]
    pub fn empty() -> Self {
        Self {
            storage: HashMap::new(),
            backpack: HashMap::new(),
        }
    }

    pub fn add_to_storage(&mut self, material: CraftMaterial, amount: u32) {
        add_count(&mut self.storage, material, amount);
    }

    pub fn add_to_backpack(&mut self, material: CraftMaterial, amount: u32) {
        add_count(&mut self.backpack, material, amount);
    }

    pub fn total_count(&self, key: CraftMaterial) -> u32 {
        self.storage_count(key).saturating_add(self.backpack_count(key))
    }

    pub fn storage_count(&self, key: CraftMaterial) -> u32 {
        self.storage.get(&key).copied().unwrap_or(0)
    }

    pub fn backpack_count(&self, key: CraftMaterial) -> u32 {
        self.backpack.get(&key).copied().unwrap_or(0)
    }

    pub fn total_backpack(&self) -> u32 {
        self.backpack
            .values()
            .copied()
            .fold(0, u32::saturating_add)
    }

    pub fn has_total(&self, key: CraftMaterial, amount: u32) -> bool {
        self.total_count(key) >= amount
    }

    pub fn remove_combined(
        &mut self,
        key: CraftMaterial,
        amount: u32,
    ) -> bool {
        if amount == 0 {
            return true;
        }
        if self.total_count(key) < amount {
            return false;
        }
        let from_backpack = self.backpack_count(key).min(amount);
        if from_backpack > 0 {
            let _ = self.remove_from_backpack(key, from_backpack);
        }
        let remaining = amount - from_backpack;
        if remaining > 0 {
            let _ = self.remove_from_storage(key, remaining);
        }
        true
    }

    pub fn remove_from_storage(&mut self, material: CraftMaterial, amount: u32) -> bool {
        take_count(&mut self.storage, material, amount)
    }

    pub fn remove_from_backpack(&mut self, material: CraftMaterial, amount: u32) -> bool {
        take_count(&mut self.backpack, material, amount)
    }

    pub fn move_backpack_to_storage(
        &mut self,
        key: CraftMaterial,
        amount: u32,
    ) -> bool {
        if amount == 0 {
            return true;
        }
        if self.backpack_count(key) < amount {
            return false;
        }
        let _ = self.remove_from_backpack(key, amount);
        self.add_to_storage(key, amount);
        true
    }

    pub fn move_storage_to_backpack(
        &mut self,
        key: CraftMaterial,
        amount: u32,
    ) -> bool {
        if amount == 0 {
            return true;
        }
        if self.storage_count(key) < amount {
            return false;
        }
        let _ = self.remove_from_storage(key, amount);
        self.add_to_backpack(key, amount);
        true
    }

    pub fn storage_only(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            backpack: HashMap::new(),
        }
    }

    pub fn display_count(count: u32) -> String {
        if count > 999 {
            "999+".to_string()
        } else {
            count.to_string()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MaterialCost {
    pub material: CraftMaterial,
    pub amount: u32,
}

impl MaterialCost {
    pub(crate) fn pair(&self) -> (CraftMaterial, u32) {
        (self.material, self.amount)
    }
}

fn cost_totals(costs: impl IntoIterator<Item = (CraftMaterial, u32)>) -> HashMap<CraftMaterial, u32> {
    let mut totals = HashMap::new();
    for (material, amount) in costs {
        add_count(&mut totals, material, amount);
    }
    totals
}

fn add_count(counts: &mut HashMap<CraftMaterial, u32>, material: CraftMaterial, amount: u32) {
    if amount > 0 {
        let count = counts.entry(material).or_default();
        *count = count.saturating_add(amount);
    }
}

fn take_count(counts: &mut HashMap<CraftMaterial, u32>, material: CraftMaterial, amount: u32) -> bool {
    if amount == 0 {
        return true;
    }
    let Some(count) = counts.get_mut(&material) else { return false; };
    if *count < amount {
        return false;
    }
    *count -= amount;
    if *count == 0 {
        counts.remove(&material);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_inventory_starts_with_five_tethers() {
        let inventory = MaterialInventory::default();

        assert_eq!(inventory.backpack_count(CraftMaterial::Tether), 5);
        assert_eq!(inventory.storage_count(CraftMaterial::Tether), 0);
    }

    #[test]
    fn combined_removal_uses_backpack_before_storage() {
        let mut inventory = MaterialInventory::default();
        let key = CraftMaterial::Ore1;
        inventory.add_to_backpack(key, 2);
        inventory.add_to_storage(key, 4);

        assert!(inventory.remove_combined(key, 5));
        assert_eq!(inventory.backpack_count(key), 0);
        assert_eq!(inventory.storage_count(key), 1);
    }

    #[test]
    fn full_stack_transfer_preserves_total() {
        let mut inventory = MaterialInventory::default();
        let key = CraftMaterial::IceShard;
        inventory.add_to_backpack(key, 7);

        assert!(inventory.move_backpack_to_storage(key, 7));
        assert_eq!(inventory.backpack_count(key), 0);
        assert_eq!(inventory.storage_count(key), 7);
        assert_eq!(inventory.total_count(key), 7);
    }

    #[test]
    fn storage_is_excluded_from_backpack_queries_and_survives_death_copy() {
        let mut inventory = MaterialInventory::empty();
        let key = CraftMaterial::Ore3;
        inventory.add_to_storage(key, 4);
        inventory.add_to_backpack(key, 2);

        assert!(inventory.backpack_count(key) < 3);
        assert!(inventory.has_total(key, 6));
        let persisted = inventory.storage_only();
        assert_eq!(persisted.storage_count(key), 4);
        assert_eq!(persisted.backpack_count(key), 0);
    }
    #[test]
    fn repeated_costs_are_paid_atomically_from_backpack_then_storage() {
        let mut inventory = MaterialInventory::empty();
        inventory.add_to_backpack(CraftMaterial::Ore1, 3);
        inventory.add_to_storage(CraftMaterial::Ore1, 4);
        inventory.add_to_backpack(CraftMaterial::IceShard, 1);
        let costs = [(CraftMaterial::Ore1, 2), (CraftMaterial::Ore1, 3), (CraftMaterial::IceShard, 1)];
        assert!(inventory.try_pay(costs));
        assert_eq!(inventory.backpack_count(CraftMaterial::Ore1), 0);
        assert_eq!(inventory.storage_count(CraftMaterial::Ore1), 2);
        assert_eq!(inventory.backpack_count(CraftMaterial::IceShard), 0);
        assert!(!inventory.try_pay(costs));
        assert_eq!(inventory.storage_count(CraftMaterial::Ore1), 2);
        assert!(inventory.try_pay([]));
    }
    #[test]
    fn each_stock_material_binds_its_own_skill() {
        for material in CraftMaterial::SKILL_STOCK_MATERIALS {
            let skill = material.stock_skill().unwrap();
            assert_eq!(skill.stock_material(), Some(material));
        }
        assert!(!SkillId::AmplifiedBell.is_stackable());
        assert!(SkillId::PortableTent.is_stackable());
    }

}
