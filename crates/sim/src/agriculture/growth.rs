//! Farm bookkeeping, claims, frost and hourly growth (pipeline step 4).

use glam::IVec2;

use super::{
    Agriculture, FarmKind, FieldTile, GROWTH_PER_HOUR, MAX_STAGE, Farm, farm_building, farm_crops, farmland_around,
    is_ripe, required_fruit, required_growth,
};
use crate::buildings::BuildingId;
use crate::defs::{CropDef, Defs, GameRules};
use crate::event::{Notification, Severity, SimEvent};
use crate::state::GameState;
use crate::time::{DAYS_PER_SEASON, GameClock, HOURS_PER_DAY, Season, TICKS_PER_HOUR};
use crate::world::Vegetation;

/// Pipeline step 4 (agriculture part): farm bookkeeping, frost, hourly growth of every farm in
/// its slot and the farm work of this tick.
pub(crate) fn run_agriculture(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    // Buildings and roads only matter when they changed (at the end of every tick the farms
    // match the buildings, also right after loading).
    let revision = state.structure_revision();
    let activations = state.buildings.activations();
    if revision != state.agriculture.seen_revision || activations != state.agriculture.seen_activations {
        state.agriculture.seen_revision = revision;
        state.agriculture.seen_activations = activations;
        sync_farms(state, defs, events);
        release_built_over(state, events);
    }
    let clock = state.clock;
    // Frost: at the start of winter, and on every frost day (docs/game_design.md §8.1).
    let winter_starts = clock.is_new_season() && clock.season() == Season::Winter;
    if winter_starts || (clock.is_new_day() && state.weather.is_frost(defs.rules(), clock)) {
        frost(state, defs, winter_starts, events);
    }
    if clock.is_new_season() && matches!(clock.season(), Season::Winter | Season::Spring) {
        // Orchard trees drop or grow their leaves.
        state.agriculture.mark_perennials_dirty(defs);
    }
    let slot = clock.tick % TICKS_PER_HOUR;
    let due: Vec<BuildingId> =
        state.agriculture.farms.keys().copied().filter(|id| u64::from(id.index()) % TICKS_PER_HOUR == slot).collect();
    for id in due {
        farm_hour(state, defs, id, events);
    }
    super::work::run_work(state, defs, events);
}

/// Creates farms for finished farm buildings (claiming their tiles at once) and removes farms
/// whose building is gone, unfinished (moving) or has another footprint.
fn sync_farms(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    let stale: Vec<BuildingId> = state
        .agriculture
        .farms
        .iter()
        .filter(|(id, farm)| {
            state.buildings.get(**id).is_none_or(|b| {
                !b.is_complete() || b.rect() != farm.rect || farm_building(defs, b).is_none_or(|(_, k, ..)| k != farm.kind)
            })
        })
        .map(|(id, _)| *id)
        .collect();
    for id in stale {
        let had = state.agriculture.farms.get(&id).is_some_and(|f| !f.tiles.is_empty());
        state.agriculture.remove_farm(id);
        if had && state.buildings.get(id).is_some() {
            events.push(SimEvent::FieldsChanged { building: id, fields: 0 });
        }
    }
    let new: Vec<(BuildingId, FarmKind, crate::grid::TileRect, Option<crate::defs::CropId>)> = state
        .buildings
        .iter()
        .filter(|(id, b)| b.is_complete() && !state.agriculture.farms.contains_key(id))
        .filter_map(|(id, b)| {
            let (def, kind, ..) = farm_building(defs, b)?;
            let crop = if kind == FarmKind::Fields { farm_crops(def).first().copied() } else { None };
            Some((id, kind, b.rect(), crop))
        })
        .collect();
    for (id, kind, rect, crop) in new {
        state.agriculture.farms.insert(
            id,
            Farm {
                kind,
                rect,
                crop,
                tiles: Vec::new(),
                task: None,
                carry: Default::default(),
                round: None,
                last_harvest: None,
                herd: 0,
                unreachable_hours: Default::default(),
            },
        );
        claim_tiles(state, defs, id, events);
        // A new pasture farm buys part of its herd.
        if let Some(f) = state.agriculture.farms.get_mut(&id)
            && f.kind == FarmKind::Pasture
        {
            let start = u32::from(f.herd_capacity(defs)) * u32::from(defs.rules().agriculture.herd_start_percent) / 100;
            f.herd = u16::try_from(start).unwrap_or(u16::MAX);
        }
    }
}

/// The herd of pasture farm `id` follows its pasture: never more animals than it feeds (the
/// rest are sold at once), and once a day (in the farm's first slot of the day, not in
/// winter) it grows by `herd_growth_percent_per_day` of the capacity, at least one animal.
fn herd_hour(state: &mut GameState, defs: &Defs, id: BuildingId) {
    let rules = &defs.rules().agriculture;
    let grow = state.clock.hour() == 0 && state.clock.season() != Season::Winter;
    let Some(f) = state.agriculture.farms.get_mut(&id) else { return };
    let capacity = f.herd_capacity(defs);
    if grow && f.herd < capacity {
        let step = (u32::from(capacity) * u32::from(rules.herd_growth_percent_per_day) / 100).max(1);
        f.herd = f.herd.saturating_add(u16::try_from(step).unwrap_or(u16::MAX));
    }
    f.herd = f.herd.min(capacity);
}

/// Hours a claimed field tile may stay out of the farmers' reach before it is released.
const UNREACHABLE_RELEASE_HOURS: u8 = 24;

/// Field tiles of farm `id` the farmers cannot reach on foot (walled in, cut off by water) for
/// [`UNREACHABLE_RELEASE_HOURS`] are released, so the farm claims reachable land instead.
fn release_unreachable(state: &mut GameState, defs: &Defs, id: BuildingId, events: &mut Vec<SimEvent>) {
    if state.agriculture.farms.get(&id).is_none_or(|f| f.tiles.is_empty()) {
        return;
    }
    crate::navigation::prepare(state, defs);
    let Some(f) = state.agriculture.farms.get(&id) else { return };
    let positions: Vec<IVec2> = f.tiles.iter().map(|t| t.pos).collect();
    let cut_off: Vec<IVec2> = positions.into_iter().filter(|p| !crate::population::routine::field_reachable(state, id, *p)).collect();
    let Some(f) = state.agriculture.farms.get_mut(&id) else { return };
    let mut hours = std::collections::BTreeMap::new();
    let mut lost = Vec::new();
    for p in cut_off {
        let h = f.unreachable_hours.get(&(p.y, p.x)).copied().unwrap_or(0).saturating_add(1);
        if h >= UNREACHABLE_RELEASE_HOURS {
            lost.push(p);
        } else {
            hours.insert((p.y, p.x), h);
        }
    }
    f.unreachable_hours = hours;
    if !lost.is_empty() {
        release_all(&mut state.agriculture, &lost, events);
    }
}

/// After roads or footprints changed: releases claimed tiles that are now built over or paved.
fn release_built_over(state: &mut GameState, events: &mut Vec<SimEvent>) {
    let lost: Vec<IVec2> = state
        .agriculture
        .claims
        .keys()
        .map(|(y, x)| IVec2::new(*x, *y))
        .filter(|p| state.buildings.id_at(*p).is_some() || state.roads.has_road(*p))
        .collect();
    release_all(&mut state.agriculture, &lost, events);
}

/// Releases `tiles` and reports the new tile count of every affected farm.
fn release_all(agriculture: &mut Agriculture, tiles: &[IVec2], events: &mut Vec<SimEvent>) {
    let mut farms: Vec<BuildingId> = tiles.iter().filter_map(|p| agriculture.release(*p)).collect();
    farms.sort();
    farms.dedup();
    for id in farms {
        let fields = agriculture.farms.get(&id).map_or(0, |f| f.tiles.len());
        events.push(SimEvent::FieldsChanged { building: id, fields: u16::try_from(fields).unwrap_or(u16::MAX) });
    }
}

/// Tops up the claims of farm `id` to its maximum (nearest eligible tiles first). Fields clear
/// bushes and stumps.
fn claim_tiles(state: &mut GameState, defs: &Defs, id: BuildingId, events: &mut Vec<SimEvent>) {
    let Some(b) = state.buildings.get(id) else { return };
    let Some((_, kind, max, radius)) = farm_building(defs, b) else { return };
    let rect = b.rect();
    let have = state.agriculture.farms.get(&id).map_or(0, |f| f.tiles.len());
    if have >= usize::from(max) {
        return;
    }
    if kind == FarmKind::Fields {
        // Farmers only work fields they can reach on foot.
        crate::navigation::prepare(state, defs);
    }
    let candidates: Vec<IVec2> = farmland_around(state, defs, rect, kind, radius, Some(id), None)
        .into_iter()
        .filter(|p| state.agriculture.owner_of(*p).is_none())
        .filter(|p| kind != FarmKind::Fields || crate::population::routine::field_reachable(state, id, *p))
        .take(usize::from(max) - have)
        .collect();
    if candidates.is_empty() {
        return;
    }
    for p in &candidates {
        if kind == FarmKind::Fields && state.world.tile(*p).vegetation != Vegetation::None {
            state.world.tile_mut(*p).vegetation = Vegetation::None;
        }
        let tile = FieldTile::new(*p, state.world.tile(*p));
        state.agriculture.claim(id, tile);
    }
    let fields = state.agriculture.farms.get(&id).map_or(0, |f| f.tiles.len());
    events.push(SimEvent::FieldsChanged { building: id, fields: u16::try_from(fields).unwrap_or(u16::MAX) });
}

/// First tick of winter and every frost day: frost kills unripe annual crops (if the crop is
/// frost-sensitive). Only the start of winter spoils the fruit left on the trees (§8.3); a
/// frost day in spring or autumn leaves the orchards alone.
fn frost(state: &mut GameState, defs: &Defs, winter_starts: bool, events: &mut Vec<SimEvent>) {
    let season = state.clock.season();
    let mut killed = 0u32;
    let mut first_killed: Option<IVec2> = None;
    let mut changed: Vec<IVec2> = Vec::new();
    for farm in state.agriculture.farms.values_mut() {
        for tile in &mut farm.tiles {
            let Some(crop) = tile.crop.and_then(|c| defs.crops().get(c.index())) else { continue };
            if crop.perennial {
                if winter_starts {
                    // Unharvested fruit spoils; the trees recover from storms over winter.
                    tile.fruit = 0;
                    tile.condition = 100;
                }
            } else if crop.frost_kills && tile.growth < required_growth(crop) {
                tile.crop = None;
                tile.growth = 0;
                tile.condition = 100;
                killed += 1;
                first_killed.get_or_insert(tile.pos);
            }
            if refresh_stage(tile, defs, season) {
                changed.push(tile.pos);
            }
        }
    }
    for p in changed {
        state.agriculture.mark_dirty(p);
    }
    if killed > 0 {
        events.push(SimEvent::CropsFrozen { fields: killed });
        let text = if killed == 1 {
            "Frost hat ein Feld mit unreifer Saat vernichtet.".to_owned()
        } else {
            format!("Frost hat {killed} Felder mit unreifer Saat vernichtet.")
        };
        let mut note = Notification::new(Severity::Warning, text);
        if let Some(at) = first_killed {
            note = note.at(at);
        }
        events.push(SimEvent::Notification(note));
    }
}

/// Destroys the crops of up to `count` sown field tiles nearest to `near` (events: a flood;
/// ties by row, then column): annual crops are lost, orchards keep their trees but lose their
/// fruit. Returns the tiles hit.
pub(crate) fn destroy_crops(state: &mut GameState, defs: &Defs, near: IVec2, count: u32) -> u32 {
    let mut candidates: Vec<(i32, i32, i32, BuildingId)> = state
        .agriculture
        .farms
        .iter()
        .flat_map(|(id, farm)| {
            farm.tiles.iter().filter(|t| t.crop.is_some()).map(move |t| {
                let d = (t.pos - near).abs();
                (d.x.max(d.y), t.pos.y, t.pos.x, *id)
            })
        })
        .collect();
    candidates.sort();
    let season = state.clock.season();
    let mut hit = 0u32;
    for (_, y, x, id) in candidates.into_iter().take(usize::try_from(count).unwrap_or(usize::MAX)) {
        let pos = IVec2::new(x, y);
        let Some(farm) = state.agriculture.farms.get_mut(&id) else { continue };
        let Some(tile) = farm.tiles.iter_mut().find(|t| t.pos == pos) else { continue };
        let perennial = tile.crop.and_then(|c| defs.crops().get(c.index())).is_some_and(|c| c.perennial);
        if perennial {
            tile.fruit = 0;
        } else {
            tile.crop = None;
            tile.growth = 0;
        }
        tile.condition = 100;
        refresh_stage(tile, defs, season);
        hit += 1;
        // The sprite changes even when the stage number does not (crop → bare soil).
        state.agriculture.mark_dirty(pos);
    }
    hit
}

/// Visual stage of a field tile in `season`: fallow 0; annuals 0–3 while growing, 4 when ripe;
/// orchards 0 (sapling) and 1 (young) until bearing, then 3 in spring (blossom), 4 with ripe
/// fruit and 2 otherwise.
pub fn visual_stage(tile: &FieldTile, defs: &Defs, season: Season) -> u8 {
    let Some(crop) = tile.crop.and_then(|c| defs.crops().get(c.index())) else { return 0 };
    let required = required_growth(crop).max(1);
    if crop.perennial {
        if tile.growth < required / 2 {
            0
        } else if tile.growth < required {
            1
        } else if is_ripe(tile, crop, season) {
            MAX_STAGE
        } else if season == Season::Spring {
            3
        } else {
            2
        }
    } else if tile.growth >= required {
        MAX_STAGE
    } else {
        (u64::from(tile.growth) * 4 / u64::from(required)).min(3) as u8
    }
}

/// Recomputes the published stage; true if it changed.
pub(super) fn refresh_stage(tile: &mut FieldTile, defs: &Defs, season: Season) -> bool {
    let stage = visual_stage(tile, defs, season);
    if stage == tile.stage {
        return false;
    }
    tile.stage = stage;
    true
}

/// Soil quality 0–1000 permille from a fertility value.
fn soil_quality_permille(fertility: u8, rules: &GameRules) -> u64 {
    let full = u64::from(rules.agriculture.full_fertility.max(1));
    u64::from(fertility).min(full) * 1000 / full
}

/// Growth factor of soil and moisture in permille (without season and weather).
fn soil_and_moisture_permille(tile: &FieldTile, rules: &GameRules) -> u64 {
    let a = &rules.agriculture;
    let q = soil_quality_permille(tile.fertility, rules);
    let soil = u64::from(a.min_growth_percent) * 10 + (1000 - u64::from(a.min_growth_percent) * 10) * q / 1000;
    let full = u64::from(a.full_moisture.max(1));
    let wet = u64::from(tile.moisture).min(full) * 1000 / full;
    let moisture =
        u64::from(a.min_moisture_growth_percent) * 10 + (1000 - u64::from(a.min_moisture_growth_percent) * 10) * wet / 1000;
    soil * moisture / 1000
}

fn season_growth_permille(rules: &GameRules, season: Season) -> u64 {
    (f64::from(*rules.season_growth.get(season)) * 1000.0).round().max(0.0) as u64
}

/// Growth factor of a field tile now in permille: season × weather × moisture × soil.
pub fn growth_factor_permille(state: &GameState, defs: &Defs, tile: &FieldTile) -> u32 {
    let rules = defs.rules();
    let season = season_growth_permille(rules, state.clock.season());
    let weather = u64::from(state.weather.growth_percent(rules));
    let factor = season * weather / 100 * soil_and_moisture_permille(tile, rules) / 1000;
    u32::try_from(factor).unwrap_or(u32::MAX)
}

/// Hours left in the current season including the current hour.
fn hours_left_in_season(clock: GameClock) -> u64 {
    (DAYS_PER_SEASON - clock.day_of_season()) * HOURS_PER_DAY + (HOURS_PER_DAY - clock.hour())
}

/// Growth points a tile with these soil and moisture conditions is expected to gain from now
/// until the season `until` starts (normal weather), at most one year ahead.
fn expected_growth_until(clock: GameClock, rules: &GameRules, tile: &FieldTile, until: impl Fn(Season) -> bool) -> u64 {
    let base = soil_and_moisture_permille(tile, rules);
    let mut season = clock.season();
    if until(season) {
        return 0;
    }
    let mut hours = hours_left_in_season(clock);
    let mut total = 0u64;
    for _ in 0..4 {
        total += hours * u64::from(GROWTH_PER_HOUR) * season_growth_permille(rules, season) / 1000 * base / 1000;
        season = season.next();
        if until(season) {
            break;
        }
        hours = DAYS_PER_SEASON * HOURS_PER_DAY;
    }
    total
}

/// True if an annual crop sown on `tile` now is expected to ripen before winter with the
/// safety margin (`agriculture.sowing_margin_percent`); always true for frost-hardy crops.
pub(super) fn ripens_before_winter(clock: GameClock, rules: &GameRules, tile: &FieldTile, crop: &CropDef) -> bool {
    if !crop.frost_kills {
        return true;
    }
    let needed = u64::from(required_growth(crop)).saturating_sub(u64::from(tile.growth));
    let needed = needed * (100 + u64::from(rules.agriculture.sowing_margin_percent)) / 100;
    expected_growth_until(clock, rules, tile, |s| s == Season::Winter) >= needed
}

/// Yield of harvesting `tile` (growing `crop`) now, in milli-units of the crop's output: base
/// yield × soil (`min_yield_percent` … 100 %) × condition × (research + irrigation bonus +
/// event modifiers, docs/game_design.md §12) ×
/// (perennials) fruit growth / `fruit_days`.
pub(super) fn harvest_milli(state: &GameState, defs: &Defs, tile: &FieldTile, crop: &CropDef, fruit: u32) -> u64 {
    let rules = defs.rules();
    let a = &rules.agriculture;
    let q = soil_quality_permille(tile.fertility, rules);
    let soil = u64::from(a.min_yield_percent) * 10 + (1000 - u64::from(a.min_yield_percent) * 10) * q / 1000;
    let mut bonus = 100
        + state.research.field_yield_bonus_percent(defs, crop.id)
        + state.events.field_yield_modifier_percent(state.clock.tick);
    if let Some((distance, percent)) = state.research.irrigation(defs)
        && near_fresh_water(state, tile.pos, distance)
    {
        bonus += percent;
    }
    let bonus = u64::try_from(bonus.max(0)).unwrap_or(0);
    let base = (f64::from(crop.yield_per_tile) * 1000.0).round().max(0.0) as u64;
    let mut milli = base * soil / 1000 * u64::from(tile.condition) / 100 * bonus / 100;
    if crop.perennial {
        let needed = u64::from(required_fruit(crop).max(1));
        milli = milli * u64::from(fruit).min(needed) / needed;
    }
    milli
}

fn near_fresh_water(state: &GameState, p: IVec2, distance: u8) -> bool {
    let r = i32::from(distance);
    (-r..=r).any(|dy| (-r..=r).any(|dx| state.world.get(p + IVec2::new(dx, dy)).is_some_and(|t| t.has_fresh_water())))
}

/// Expected yield of a tile in milli-units if it is harvested when ripe: annuals that ripen
/// before winter (or are ripe), bearing perennials with the fruit expected until the harvest
/// season; 0 otherwise.
pub(crate) fn expected_harvest_milli(state: &GameState, defs: &Defs, tile: &FieldTile) -> u64 {
    let Some(crop) = tile.crop.and_then(|c| defs.crops().get(c.index())) else { return 0 };
    let rules = defs.rules();
    let season = state.clock.season();
    if crop.perennial {
        if tile.growth < required_growth(crop) {
            return 0;
        }
        let fruit = if crop.harvest_seasons.contains(&season) {
            u64::from(tile.fruit)
        } else {
            u64::from(tile.fruit) + expected_growth_until(state.clock, rules, tile, |s| crop.harvest_seasons.contains(&s))
        };
        return harvest_milli(state, defs, tile, crop, u32::try_from(fruit).unwrap_or(u32::MAX));
    }
    let ripe = tile.growth >= required_growth(crop);
    if ripe || !crop.frost_kills || {
        let needed = u64::from(required_growth(crop) - tile.growth);
        expected_growth_until(state.clock, rules, tile, |s| s == Season::Winter) >= needed
    } {
        harvest_milli(state, defs, tile, crop, 0)
    } else {
        0
    }
}

/// The hourly update of farm `id` in its slot: claims and, for field farms, one hour of growth
/// (plus the daily soil update in the first hour of the day).
fn farm_hour(state: &mut GameState, defs: &Defs, id: BuildingId, events: &mut Vec<SimEvent>) {
    // Tiles that left the territory are released; free tiles are claimed.
    let lost: Vec<IVec2> = state
        .agriculture
        .farms
        .get(&id)
        .map(|f| f.tiles.iter().map(|t| t.pos).filter(|p| !state.territory.contains(*p)).collect())
        .unwrap_or_default();
    if !lost.is_empty() {
        release_all(&mut state.agriculture, &lost, events);
    }
    claim_tiles(state, defs, id, events);
    if state.agriculture.farms.get(&id).is_some_and(|f| f.kind == FarmKind::Pasture) {
        herd_hour(state, defs, id);
        return;
    }
    if !state.agriculture.farms.contains_key(&id) {
        return;
    }
    release_unreachable(state, defs, id, events);
    let rules = defs.rules();
    let clock = state.clock;
    let season = clock.season();
    let daily = clock.hour() == 0;
    let moisture_change = state.weather.moisture_per_day(rules);
    let damage = u64::from(state.weather.field_damage_percent(rules));
    let regeneration = {
        let bonus = u64::try_from((100 + state.research.soil_regeneration_percent(defs)).max(0)).unwrap_or(0);
        u64::from(rules.agriculture.fallow_regeneration_per_day) * bonus / 100
    };
    let season_permille = season_growth_permille(rules, season);
    let weather_percent = u64::from(state.weather.growth_percent(rules));
    let Some(mut farm) = state.agriculture.farms.remove(&id) else { return };
    let mut changed: Vec<IVec2> = Vec::new();
    for tile in &mut farm.tiles {
        let natural = state.world.tile(tile.pos);
        let crop = tile.crop.and_then(|c| defs.crops().get(c.index()));
        if daily {
            let a = &rules.agriculture;
            let m = i32::from(tile.moisture) + moisture_change;
            let m = m + (i32::from(natural.moisture) - m) * i32::from(a.moisture_recovery_percent) / 100;
            tile.moisture = m.clamp(0, 255) as u8;
            match crop {
                None => {
                    let f = (u64::from(tile.fertility) + regeneration).min(u64::from(natural.fertility.max(tile.fertility)));
                    tile.fertility = f.min(255) as u8;
                }
                Some(_) => {
                    let mut condition = u64::from(tile.condition) * (100 - damage.min(100)) / 100;
                    if tile.moisture < a.dry_moisture {
                        condition = condition.saturating_sub(u64::from(a.wither_percent_per_day));
                    }
                    tile.condition = condition.min(100) as u8;
                }
            }
        }
        if let Some(crop) = crop {
            let factor = season_permille * weather_percent / 100 * soil_and_moisture_permille(tile, rules) / 1000;
            let step = u32::try_from(u64::from(GROWTH_PER_HOUR) * factor / 1000).unwrap_or(u32::MAX);
            let required = required_growth(crop);
            if tile.growth < required {
                tile.growth = tile.growth.saturating_add(step).min(required);
            } else if crop.perennial && !crop.harvest_seasons.contains(&season) {
                tile.fruit = tile.fruit.saturating_add(step).min(required_fruit(crop));
            }
        }
        if refresh_stage(tile, defs, season) {
            changed.push(tile.pos);
        }
    }
    state.agriculture.farms.insert(id, farm);
    for p in changed {
        state.agriculture.mark_dirty(p);
    }
}
