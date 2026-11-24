//! Canonical content IDs from `docs/game_design.md` (§4 resources, §5 buildings) that the
//! generated sprites must cover. Keep this list in sync with the design document.

/// Building category (selects the visual vocabulary and the icon background).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Residential,
    Production,
    Agriculture,
    Public,
    Military,
}

/// One building ID with its footprint `B×T` as written in the design document.
#[derive(Clone, Copy, Debug)]
pub struct BuildingInfo {
    pub id: &'static str,
    /// Width `B` (first number of "B×T").
    pub b: u32,
    /// Depth `T` (second number of "B×T").
    pub t: u32,
    pub category: Category,
    /// Drawn as auto-connecting segments in the `roads` atlas (`wall_<mask>`, `gate_<x|y>`).
    pub road_like: bool,
}

const fn bld(id: &'static str, b: u32, t: u32, category: Category) -> BuildingInfo {
    BuildingInfo { id, b, t, category, road_like: false }
}

const fn seg(id: &'static str) -> BuildingInfo {
    BuildingInfo { id, b: 1, t: 1, category: Category::Military, road_like: true }
}

use Category::{Agriculture, Military, Production, Public, Residential};

/// Every building ID of §5 (weaponsmith appears once, under production), including the
/// upgrade levels reached only by upgrading (§5 Ausbauketten).
pub const BUILDINGS: &[BuildingInfo] = &[
    bld("house_wood", 2, 2, Residential),
    bld("farmhouse", 2, 2, Residential),
    bld("citizen_house", 2, 2, Residential),
    bld("townhouse", 2, 2, Residential),
    bld("residence", 3, 3, Residential),
    bld("manor", 4, 4, Residential),
    bld("woodcutter", 2, 2, Production),
    bld("lumber_camp", 2, 2, Production),
    bld("forester", 2, 2, Production),
    bld("sawmill", 2, 3, Production),
    bld("large_sawmill", 2, 3, Production),
    bld("quarry", 3, 3, Production),
    bld("large_quarry", 3, 3, Production),
    bld("clay_pit", 2, 2, Production),
    bld("sand_pit", 2, 2, Production),
    bld("iron_mine", 2, 2, Production),
    bld("coal_mine", 2, 2, Production),
    bld("charcoal_kiln", 2, 2, Production),
    bld("gold_mine", 2, 2, Production),
    bld("smelter", 2, 2, Production),
    bld("blast_furnace", 2, 2, Production),
    bld("smithy", 2, 2, Production),
    bld("master_smithy", 2, 2, Production),
    bld("weaponsmith", 2, 2, Production),
    bld("brickworks", 2, 3, Production),
    bld("large_brickworks", 2, 3, Production),
    bld("pottery", 2, 2, Production),
    bld("large_pottery", 2, 2, Production),
    bld("weaver", 2, 2, Production),
    bld("large_weaver", 2, 2, Production),
    bld("tailor", 2, 2, Production),
    bld("large_tailor", 2, 2, Production),
    bld("tannery", 2, 2, Production),
    bld("large_tannery", 2, 2, Production),
    bld("carpenter", 2, 2, Production),
    bld("large_carpenter", 2, 2, Production),
    bld("mill", 2, 2, Production),
    bld("large_mill", 2, 2, Production),
    bld("bakery", 2, 2, Production),
    bld("large_bakery", 2, 2, Production),
    bld("brewery", 2, 3, Production),
    bld("large_brewery", 2, 3, Production),
    bld("cheese_dairy", 2, 2, Production),
    bld("winery", 2, 3, Production),
    bld("jeweler", 2, 2, Production),
    bld("artisan", 2, 2, Production),
    bld("wheat_farm", 3, 3, Agriculture),
    bld("vegetable_farm", 3, 3, Agriculture),
    bld("orchard", 3, 3, Agriculture),
    bld("cattle_farm", 3, 3, Agriculture),
    bld("chicken_farm", 2, 2, Agriculture),
    bld("sheep_farm", 3, 3, Agriculture),
    bld("fishery", 2, 2, Agriculture),
    bld("large_fishery", 2, 2, Agriculture),
    bld("apiary", 2, 2, Agriculture),
    bld("large_apiary", 2, 2, Agriculture),
    bld("town_hall", 4, 4, Public),
    bld("market", 4, 4, Public),
    bld("grand_market", 4, 4, Public),
    bld("church", 3, 4, Public),
    bld("cathedral", 3, 4, Public),
    bld("school", 3, 3, Public),
    bld("academy", 3, 3, Public),
    bld("hospital", 3, 3, Public),
    bld("infirmary", 3, 3, Public),
    bld("well", 1, 1, Public),
    bld("tavern", 3, 3, Public),
    bld("inn", 3, 3, Public),
    bld("warehouse", 3, 3, Public),
    bld("storehouse", 2, 2, Public),
    bld("trading_post", 3, 3, Public),
    bld("barracks", 3, 3, Military),
    bld("garrison", 3, 3, Military),
    bld("watchtower", 1, 1, Military),
    bld("stone_tower", 1, 1, Military),
    bld("archer_post", 2, 2, Military),
    bld("archer_tower", 2, 2, Military),
    bld("stables", 3, 3, Military),
    bld("stud_farm", 3, 3, Military),
    seg("wall"),
    seg("gate"),
    bld("castle", 5, 5, Military),
];

/// Every resource ID of §4 (including the armour of the military, §11).
pub const RESOURCES: &[&str] = &[
    "wood",
    "stone",
    "loam",
    "clay",
    "iron_ore",
    "coal",
    "sand",
    "wool",
    "grain",
    "hides",
    "gold_ore",
    "wheat",
    "flour",
    "bread",
    "meat",
    "fish",
    "milk",
    "cheese",
    "vegetables",
    "fruit",
    "honey",
    "planks",
    "tools",
    "bricks",
    "iron_bars",
    "cloth",
    "clothes",
    "furniture",
    "weapons",
    "armor",
    "ceramics",
    "beer",
    "leather",
    "jewelry",
    "wine",
    "spices",
    "silk",
    "art",
];

/// Distinct footprints `(B, T)` of all non-segment buildings, sorted; construction-site and
/// rubble sprites exist for each of them.
pub fn footprints() -> Vec<(u32, u32)> {
    let mut v: Vec<(u32, u32)> = BUILDINGS.iter().filter(|b| !b.road_like).map(|b| (b.b, b.t)).collect();
    v.sort_unstable();
    v.dedup();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_match_design_document() {
        let count = |c: Category| BUILDINGS.iter().filter(|b| b.category == c).count();
        assert_eq!(count(Residential), 6);
        // 26 + the 14 level-2 workshops and gatherers reached by upgrading.
        assert_eq!(count(Production), 40);
        // 8 + large fishery and large apiary.
        assert_eq!(count(Agriculture), 10);
        // 10 + grand market, cathedral, academy, infirmary and inn.
        assert_eq!(count(Public), 15);
        // 7 + stone tower, garrison, archer tower and stud farm.
        assert_eq!(count(Military), 11);
        assert_eq!(RESOURCES.len(), 38);
    }

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<&str> = BUILDINGS.iter().map(|b| b.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), BUILDINGS.len());
        let mut res = RESOURCES.to_vec();
        res.sort_unstable();
        res.dedup();
        assert_eq!(res.len(), RESOURCES.len());
    }

    #[test]
    fn footprint_sizes() {
        assert_eq!(footprints(), vec![(1, 1), (2, 2), (2, 3), (3, 3), (3, 4), (4, 4), (5, 5)]);
    }
}
