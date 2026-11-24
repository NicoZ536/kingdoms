//! Event conditions and choice requirements evaluated against the world state.

use crate::buildings::{Building, BuildingId};
use crate::defs::{BuildingDef, BuildingMaterial, Defs, EventCondition, ServiceKind};
use crate::grid::TileRect;
use crate::military::count_text;
use crate::population::ServiceIndex;
use crate::state::GameState;
use crate::time::DAYS_PER_SEASON;
use crate::world::WorldMap;

/// Finished buildings with their type.
fn finished<'a>(state: &'a GameState, defs: &'a Defs) -> impl Iterator<Item = (BuildingId, &'a Building, &'a BuildingDef)> + 'a {
    state
        .buildings
        .iter()
        .filter(|(_, b)| b.is_complete())
        .filter_map(|(id, b)| defs.buildings().get(b.kind().index()).map(|d| (id, b, d)))
}

/// True if a river tile lies within `distance` tiles (Chebyshev) of `rect`.
pub(super) fn near_river(world: &WorldMap, rect: TileRect, distance: u8) -> bool {
    let d = i32::from(distance);
    let min = rect.origin - glam::IVec2::splat(d);
    let max = rect.origin + rect.size + glam::IVec2::splat(d);
    (min.y..max.y).any(|y| (min.x..max.x).any(|x| world.get(glam::IVec2::new(x, y)).is_some_and(|t| t.river)))
}

/// True if a building of type `def` burns: wooden and damageable (not the headquarters, walls
/// or gates).
pub(super) fn is_wooden(def: &BuildingDef) -> bool {
    def.material == BuildingMaterial::Wood && crate::buildings::can_be_damaged(def)
}

/// Wooden buildings in total and those without fire protection (wells in reach).
pub fn wooden_buildings(state: &GameState, defs: &Defs) -> (u64, u64) {
    let index = ServiceIndex::collect(state, defs);
    let mut wooden = 0;
    let mut unprotected = 0;
    for (_, b, def) in finished(state, defs) {
        if is_wooden(def) {
            wooden += 1;
            if !index.covers(ServiceKind::FireProtection, &b.rect()) {
                unprotected += 1;
            }
        }
    }
    (wooden, unprotected)
}

/// True if raiders cannot reach the goods of `building`: a safety service (watchtower,
/// barracks, castle) covers it. This is the hook for the military defence of the next phase.
pub fn raid_protected(state: &GameState, defs: &Defs, building: BuildingId) -> bool {
    let index = ServiceIndex::collect(state, defs);
    state.buildings.get(building).is_some_and(|b| index.covers(ServiceKind::Safety, &b.rect()))
}

/// Share of finished houses covered by a safety service, in percent (0 without houses).
pub(super) fn safety_coverage_percent(state: &GameState, defs: &Defs) -> u32 {
    let index = ServiceIndex::collect(state, defs);
    let (mut houses, mut covered) = (0u32, 0u32);
    for (_, b, def) in finished(state, defs) {
        if def.housing.is_some() {
            houses += 1;
            if index.covers(ServiceKind::Safety, &b.rect()) {
                covered += 1;
            }
        }
    }
    (covered * 100).checked_div(houses).unwrap_or(0)
}

/// Average happiness of all citizens in points (`None` without citizens).
pub(super) fn average_happiness(state: &GameState) -> Option<u32> {
    let n = state.population.len() as u64;
    let sum: u64 = state.population.citizens().map(|(_, c)| u64::from(c.happiness())).sum();
    (n > 0).then(|| u32::try_from(sum / n).unwrap_or(u32::MAX))
}

/// Soldiers available for event choices: ready soldiers on the map (docs/game_design.md §11).
pub(crate) fn soldiers(state: &GameState) -> u32 {
    // Ready soldiers on the map (docs/game_design.md §11).
    state.military.available_count()
}

/// True if `condition` holds now.
pub fn condition_holds(state: &GameState, defs: &Defs, condition: &EventCondition) -> bool {
    use EventCondition as C;
    let population = u32::try_from(state.population.len()).unwrap_or(u32::MAX);
    match condition {
        C::Season(s) => state.clock.season() == *s,
        C::MinDay(d) => state.clock.total_days() >= u64::from(*d),
        C::MinPopulation(n) => population >= *n,
        C::MaxPopulation(n) => population <= *n,
        C::HasAnyBuilding(kinds) => finished(state, defs).any(|(_, b, _)| kinds.contains(&b.kind())),
        C::LacksBuildings(kinds) => !finished(state, defs).any(|(_, b, _)| kinds.contains(&b.kind())),
        C::MinBuildingsInCategory { category, count } => {
            finished(state, defs).filter(|(_, _, d)| d.category == *category).count() as u64 >= u64::from(*count)
        }
        C::MinWoodenBuildings { count, unprotected } => {
            let (wooden, open) = wooden_buildings(state, defs);
            (if *unprotected { open } else { wooden }) >= u64::from(*count)
        }
        C::MinBuildingsNearRiver { count, distance } => {
            finished(state, defs)
                .filter(|(_, b, d)| crate::buildings::can_be_damaged(d) && near_river(&state.world, b.rect(), *distance))
                .count() as u64
                >= u64::from(*count)
        }
        C::MaxSatisfaction(v) => average_happiness(state).is_some_and(|h| h <= u32::from(*v)),
        C::MinSatisfaction(v) => average_happiness(state).is_some_and(|h| h >= u32::from(*v)),
        C::MinTaxRate(v) => state.policy.tax_rate_percent >= *v,
        C::MinFreeHousing(n) => crate::population::free_housing(state, defs) >= *n,
        C::MinTreasury(n) => state.economy.treasury() >= i64::from(*n),
        C::MinStock { resource, amount } => state.economy.available(*resource) >= *amount,
        C::MinSoldiers(n) => soldiers(state) >= *n,
        C::MaxSafetyCoverage(v) => safety_coverage_percent(state, defs) <= u32::from(*v),
        C::Weather(w) => state.weather.current() == *w,
        C::MinWeatherIntensity(v) => state.weather.intensity() >= *v,
        C::WarNearby => crate::diplomacy::any_war(state),
        C::RaidGraceOver => crate::military::raid_grace_over(state, defs),
        C::MinDryDays(n) => state.weather.dry_days() >= *n,
        C::MinFieldCondition(v) => state.agriculture.average_field_condition().is_some_and(|c| c >= *v),
        C::MaxFieldCondition(v) => state.agriculture.average_field_condition().is_some_and(|c| c <= *v),
        C::NoEventThisSeason(list) => {
            let season = state.clock.total_seasons();
            list.iter().all(|e| state.events.last_occurred(*e).is_none_or(|day| day / DAYS_PER_SEASON != season))
        }
        C::AnyOf(list) => list.iter().any(|c| condition_holds(state, defs, c)),
    }
}

/// Why the choice requirement `condition` is not met (German), or `None` if it is.
pub fn requirement_problem(state: &GameState, defs: &Defs, condition: &EventCondition) -> Option<String> {
    if condition_holds(state, defs, condition) {
        return None;
    }
    Some(match condition {
        EventCondition::MinTreasury(n) => {
            format!("{}.", crate::report::missing_gold(i64::from(*n) - state.economy.treasury().max(0)))
        }
        EventCondition::MinStock { resource, amount } => {
            let name = defs.resources().get(resource.index()).map_or_else(|| resource.to_string(), |r| r.name.clone());
            format!("{}.", crate::report::missing_goods(amount.saturating_sub(state.economy.available(*resource)), &name))
        }
        EventCondition::MinSoldiers(n) => match soldiers(state) {
            0 => format!("Es steht kein Soldat bereit (nötig: {}).", count_text(*n, "Soldat", "Soldaten")),
            1 => format!("Es steht nur 1 von {n} Soldaten bereit."),
            have => format!("Es stehen nur {have} von {n} Soldaten bereit."),
        },
        other => format!("Voraussetzung nicht erfüllt: {}.", condition_text(defs, other)),
    })
}

/// German description of a condition, e.g. „mindestens 60 Bewohner“.
pub(crate) fn condition_text(defs: &Defs, condition: &EventCondition) -> String {
    use EventCondition as C;
    let names = |kinds: &[crate::defs::BuildingTypeId]| -> String {
        kinds.iter().filter_map(|k| defs.buildings().get(k.index())).map(|d| d.name.as_str()).collect::<Vec<_>>().join(" oder ")
    };
    match condition {
        C::Season(s) => format!("Jahreszeit {}", s.display_name()),
        C::MinDay(d) => format!("ab dem {}. Tag", d + 1),
        C::MinPopulation(n) => format!("mindestens {n} Bewohner"),
        C::MaxPopulation(n) => format!("höchstens {n} Bewohner"),
        C::HasAnyBuilding(kinds) => format!("ein fertiges Gebäude: {}", names(kinds)),
        C::LacksBuildings(kinds) => format!("kein Gebäude: {}", names(kinds)),
        C::MinBuildingsInCategory { category, count } => format!("mindestens {count} Gebäude der Art {}", category.display_name()),
        C::MinWoodenBuildings { count, unprotected: true } => format!("mindestens {count} Holzgebäude ohne Brunnen in der Nähe"),
        C::MinWoodenBuildings { count, unprotected: false } => format!("mindestens {count} Holzgebäude"),
        C::MinBuildingsNearRiver { count, distance } => {
            format!("mindestens {count} Gebäude höchstens {} vom Fluss", count_text(u32::from(*distance), "Feld", "Felder"))
        }
        C::MaxSatisfaction(v) => format!("Zufriedenheit höchstens {v}"),
        C::MinSatisfaction(v) => format!("Zufriedenheit mindestens {v}"),
        C::MinTaxRate(v) => format!("Steuersatz mindestens {v} %"),
        C::MinFreeHousing(n) => format!("mindestens {}", count_text(*n, "freier Wohnplatz", "freie Wohnplätze")),
        C::MinTreasury(n) => format!("mindestens {n} Gold"),
        C::MinStock { resource, amount } => {
            let name = defs.resources().get(resource.index()).map_or_else(|| resource.to_string(), |r| r.name.clone());
            format!("mindestens {amount} {name} im Lager")
        }
        C::MinSoldiers(n) => format!("mindestens {}", count_text(*n, "Soldat", "Soldaten")),
        C::MaxSafetyCoverage(v) => format!("höchstens {v} % der Häuser bewacht"),
        C::Weather(w) => format!("Wetter {}", w.display_name()),
        C::MinWeatherIntensity(v) => format!("Wetterstärke mindestens {v} %"),
        C::WarNearby => "ein Krieg in der Nähe".to_owned(),
        C::RaidGraceOver => format!(
            "nach der Schonfrist für Räuber (mindestens {} Bewohner)",
            defs.rules().military.bandits.grace_population
        ),
        C::MinDryDays(n) => format!("seit mindestens {} kein Regen", count_text(u32::from(*n), "Tag", "Tagen")),
        C::MinFieldCondition(v) => format!("Zustand der bestellten Felder im Schnitt mindestens {v} %"),
        C::MaxFieldCondition(v) => format!("Zustand der bestellten Felder im Schnitt höchstens {v} %"),
        C::NoEventThisSeason(list) => {
            let titles: Vec<String> =
                list.iter().filter_map(|e| defs.events().get(e.index())).map(|d| format!("„{}“", d.title)).collect();
            format!("in dieser Jahreszeit noch kein Ereignis {}", titles.join(" oder "))
        }
        C::AnyOf(list) => list.iter().map(|c| condition_text(defs, c)).collect::<Vec<_>>().join(" oder "),
    }
}
