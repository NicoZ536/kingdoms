//! Balance with the military (docs/game_design.md §1, §10, §11): the reasonable player of the
//! year test plays three years on five seeds with AI kingdoms, events and threats.
//!
//! On top of the economic build order of `reasonable_player` it sows more vegetable fields as
//! the population grows (one farm per 14 citizens) and researches the citizens' militia once
//! the settlement has 30 inhabitants, builds a quarry and a sawmill (planks), saves for barracks
//! with taxes at 15 % (houses for the homeless are always built), then puts a watchtower next
//! to every storage no safety service reaches (bandits go for those; only where the building
//! material reaches), keeps about one militiaman per seven inhabitants (the first four even
//! from workers, then only from adults without work, at most eight) grouped in a company that
//! guards the town – an area over the headquarters and every storage – and, while enemies
//! come for a building, stands at that building; it decides bandit raids by sending the
//! soldiers when it has five, keeps the peace (accepts treaties and peace offers, pays small
//! tributes, offers a non-aggression pact every few days) and asks for peace while at war.
//!
//! The event draw rarely sends bandits to a guarded town (`MaxSafetyCoverage`), so the test
//! sends a band of the size the town draws (`bandit_band`: its size, the difficulty and its
//! defenders) on days 26, 36 and 60 (spring and autumn of the second year, autumn of the third)
//! to exercise the defence every run. The first one comes just after the bandits' grace period
//! (docs/game_design.md §11: no raid before day 24 on „Normal“) – earlier test versions sent it
//! in the first winter, before any town could have soldiers, which the game no longer does. The
//! AI kingdoms keep their grace period (no war before day 36)
//! and rarely attack a kingdom that keeps the peace, so in the winter of the second year
//! (day 42) the player goes to war itself, against the weakest kingdom (cancelling a pact with
//! it first), defends against its armies and asks for peace every three days.
//!
//! After three years the kingdom must live (no defeat), have grown and risen in rank, have kept
//! its soldiers, have fought a war and beaten off the bandits without heavy losses, nobody may
//! have starved or frozen, the treasury must not have collapsed despite the soldiers' pay and
//! the happiness must be sane. The key numbers of every seed are printed.
//!
//! The ambitious player ([`Style::Ambitious`]) plays the same three years aiming for the rank
//! „Stadt“ (docs/game_design.md §1) and must reach it on two seeds.

mod common;

use common::{choice_cost, decide_event, find_spot, find_spot_in, kind, load_defs, next_goal, plan};
use kingdoms_sim::command::Command;
use kingdoms_sim::defs::Defs;
use kingdoms_sim::diplomacy::{DiplomaticAction, PeaceTerms, ProposalKind, TreatyKind};
use kingdoms_sim::event::SimEvent;
use kingdoms_sim::military::{BattleKind, BattleOutcome, DefendTarget, Order};
use kingdoms_sim::population::DeathCause;
use kingdoms_sim::report;
use kingdoms_sim::state::{GameSettings, GameState};
use kingdoms_sim::time::{Season, TICKS_PER_DAY, TICKS_PER_HOUR};
use kingdoms_sim::world::WorldSize;

/// Three years (72 days) plus the first day of the fourth spring.
const DAYS: u64 = 73;

/// Days the test sends bandits (at 10:00): spring of the second year (just after the grace
/// period of `military.bandits.grace_days`, normal difficulty), autumn of the second and of the
/// third year.
const RAID_DAYS: [u64; 3] = [26, 36, 60];

/// Day the player goes to war if no kingdom has attacked it by then (winter, second year).
const WAR_DAY: u64 = 42;

/// Inhabitants from which the player researches the militia and builds up its defence.
const MILITARY_FROM: usize = 30;

/// The build order of the year test, plus vegetable farms that grow with the population
/// (one per 14 citizens, sown in spring), and a quarry, a sawmill (planks) and barracks once
/// the militia is researched. Until the barracks stand, the player saves: food, firewood, water
/// and houses only while 80 gold stay, everything else while 200 gold stay (houses for the
/// homeless always).
fn campaign_goal(state: &GameState, defs: &Defs, blocked: &[&str]) -> Option<&'static str> {
    let count = |key: &str| state.buildings().count_of(kind(defs, key));
    let researched = defs.tech_id("militia_training").is_some_and(|t| state.research().is_researched(t));
    let gold = state.economy().treasury();
    let open = |key: &str| !blocked.contains(&key);
    let stock = |key: &str| defs.resource_id(key).map_or(0, |r| state.economy().stock().get(r));
    let cost = |key: &str| i64::from(defs.building(kind(defs, key)).cost.gold);
    let affordable = |key: &str| gold >= cost(key) + 40;
    let pop = state.population().len();
    let clock = state.clock();
    let sowing = clock.season() == Season::Spring || (clock.season() == Season::Summer && clock.day_of_season() <= 2);
    let saving = researched && pop >= MILITARY_FROM && count("barracks") == 0;
    if sowing
        && count("vegetable_farm") * 14 < pop
        && open("vegetable_farm")
        && affordable("vegetable_farm")
        && pop >= 40
        && (!saving || gold - cost("vegetable_farm") >= 80)
    {
        return Some("vegetable_farm");
    }
    if researched && pop >= MILITARY_FROM {
        for key in ["quarry", "sawmill"] {
            if count(key) == 0 && open(key) && affordable(key) {
                return Some(key);
            }
        }
        if count("barracks") == 0 {
            if affordable("barracks") && stock("stone") >= 25 && stock("planks") >= 15 && open("barracks") {
                return Some("barracks");
            }
            let essential = |g: &str| {
                matches!(g, "house_wood" | "chicken_farm" | "fishery" | "storehouse" | "vegetable_farm" | "woodcutter" | "forester" | "well")
            };
            let homeless = report::population_summary(state, defs).homeless > 0;
            return next_goal(state, defs, blocked).filter(|g| {
                (*g == "house_wood" && homeless) || (essential(g) && gold - cost(g) >= 80) || gold - cost(g) >= 200
            });
        }
    }
    next_goal(state, defs, blocked)
}

/// How the scripted player plays.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Style {
    /// The campaign player described in the module documentation.
    Reasonable,
    /// On top of that: builds two houses at a time while newcomers wait, raises the
    /// construction priority while sites wait for builders, researches the church order and
    /// builds a church, and pushes the borders out with watchtowers – what a player aiming for
    /// the rank „Stadt“ does.
    Ambitious,
    /// The long game (see [`expansive_goal`]): the ambitious player who also keeps the wood
    /// and the food ahead of a growing town, upgrades the houses (loam), researches book-keeping,
    /// grain farming, schooling and trade, builds the school, a warehouse and the trading post,
    /// sells surplus and upgrades its workshops and stores – aiming for „Großstadt“ and beyond.
    Expansive,
}

/// Wood the ambitious player wants in stock: below it woodcutters come first and the raw
/// materials get the workers first.
const AMBITIOUS_WOOD: u32 = 40;

/// The ambitious player's build order: houses ahead (two sites at a time while fewer than six
/// places are free), the church once researched (with 150 gold to spare), then the campaign
/// order.
fn ambitious_goal(state: &GameState, defs: &Defs, blocked: &[&str]) -> Option<&'static str> {
    let count = |key: &str| state.buildings().count_of(kind(defs, key));
    let gold = state.economy().treasury();
    let stock = |key: &str| defs.resource_id(key).map_or(0, |r| state.economy().stock().get(r));
    let summary = report::population_summary(state, defs);
    let house = kind(defs, "house_wood");
    let house_sites = state.buildings().iter().filter(|(_, b)| b.kind() == house && b.construction_site().is_some()).count();
    let barracks = count("barracks") > 0;
    // Food first: newcomers only while the stores hold twelve days and the animals and fish
    // keep up with four fifths of what is eaten (the fields bring the rest).
    let food = report::food_forecast(state, defs);
    let food_site = state.buildings().iter().any(|(_, b)| {
        b.construction_site().is_some() && matches!(defs.building(b.kind()).key.as_str(), "fishery" | "chicken_farm")
    });
    if barracks && !food_site && food.other_production_per_day < 0.8 * food.consumption_per_day && gold >= 70 {
        for key in ["fishery", "chicken_farm"] {
            if !blocked.contains(&key) && (key != "fishery" || count("fishery") < 2) {
                return Some(key);
            }
        }
    }
    let fed = food.in_stock >= 12.0 * food.consumption_per_day;
    // Wood next: houses, towers and the stoves burn it. While the stock runs short a woodcutter
    // (with its forester) for every eight citizens, even if workers have to change jobs for it
    // (the raw-material priority goes up meanwhile, see `ambitious_commands`).
    let pop = summary.population as usize;
    let site = |key: &str| state.buildings().iter().any(|(_, b)| b.kind() == kind(defs, key) && b.construction_site().is_some());
    let wood_cost = |key: &str| i64::from(defs.building(kind(defs, key)).cost.gold) + 40;
    if barracks && stock("wood") < AMBITIOUS_WOOD && !site("woodcutter") && !site("forester") {
        if count("woodcutter") * 8 < pop + 16 && !blocked.contains(&"woodcutter") && gold >= wood_cost("woodcutter") {
            return Some("woodcutter");
        }
        if count("forester") < count("woodcutter") && !blocked.contains(&"forester") && gold >= wood_cost("forester") {
            return Some("forester");
        }
    }
    // Houses first (they are cheap), then the church (other buildings wait while it saves).
    let church = kind(defs, "church");
    let reserve = church_reserve(state, defs);
    if barracks
        && fed
        && summary.immigrant_places < 6
        && house_sites < 2
        && gold >= 60
        && stock("wood") >= 30
        && !blocked.contains(&"house_wood")
    {
        return Some("house_wood");
    }
    if barracks
        && count("church") == 0
        && state.research().building_unlocked(defs, church)
        && gold >= 230
        && stock("stone") >= 30
        && stock("planks") >= 20
        && !blocked.contains(&"church")
    {
        return Some("church");
    }
    // Saving for the church: only food, fuel, water and stores on top.
    let essential = |g: &str| matches!(g, "chicken_farm" | "fishery" | "vegetable_farm" | "storehouse" | "woodcutter" | "forester" | "well");
    let cost = |g: &str| i64::from(defs.building(kind(defs, g)).cost.gold);
    campaign_goal(state, defs, blocked).filter(|g| reserve == 0 || essential(g) || gold - cost(g) >= reserve)
}

/// Firewood and building wood the expansive player keeps: a day of heating for everybody in
/// winter, otherwise half the population (houses, towers, upgrades).
fn wood_floor(state: &GameState) -> u32 {
    let pop = u32::try_from(state.population().len()).unwrap_or(0);
    if state.clock().season() == Season::Winter { pop.max(60) } else { (pop / 2).max(40) }
}

/// The expansive player's build order for the long game, ahead of the ambitious order:
/// woodcutters and foresters while wood runs short (the raw-material priority moves the
/// workers, see [`expansive_commands`]), food producers while the stock covers less than ten
/// days, the grain chain once researched, a clay pit (loam: houses upgrade to farmhouses), the
/// school, a warehouse per 120 citizens and the trading post; then the ambitious order.
fn expansive_goal(state: &GameState, defs: &Defs, blocked: &[&str]) -> Option<&'static str> {
    let count = |key: &str| state.buildings().count_of(kind(defs, key));
    let gold = state.economy().treasury();
    let open = |key: &str| !blocked.contains(&key);
    let cost = |key: &str| i64::from(defs.building(kind(defs, key)).cost.gold);
    let can = |key: &str| open(key) && gold >= cost(key) + 40 && state.research().building_unlocked(defs, kind(defs, key));
    let stock = |key: &str| defs.resource_id(key).map_or(0, |r| state.economy().stock().get(r));
    let site = |key: &str| state.buildings().iter().any(|(_, b)| b.kind() == kind(defs, key) && b.construction_site().is_some());
    let pop = state.population().len();
    if count("barracks") == 0 {
        return ambitious_goal(state, defs, blocked);
    }
    // Wood first: without it no house, tower or upgrade goes up.
    if stock("wood") < wood_floor(state) && count("woodcutter") * 8 < pop + 16 && !site("woodcutter") && can("woodcutter") {
        return Some("woodcutter");
    }
    if count("forester") < count("woodcutter") && !site("forester") && can("forester") {
        return Some("forester");
    }
    // Stone for towers, the church, the school and the upgrades: a quarry per 80 citizens.
    if stock("stone") < 60 && count("quarry") < 1 + pop / 80 && !site("quarry") && can("quarry") {
        return Some("quarry");
    }
    // Food ahead of the growth.
    let food = report::food_forecast(state, defs);
    let short = food.in_stock < 10.0 * food.consumption_per_day || food.other_production_per_day < 0.7 * food.consumption_per_day;
    if short && !site("chicken_farm") && !site("fishery") && !site("wheat_farm") {
        if count("fishery") < 4 && can("fishery") {
            return Some("fishery");
        }
        if count("wheat_farm") > count("mill") * 2 && can("mill") {
            return Some("mill");
        }
        if count("mill") > count("bakery") && can("bakery") {
            return Some("bakery");
        }
        let sowing = state.clock().season() == Season::Spring || (state.clock().season() == Season::Summer && state.clock().day_of_season() <= 2);
        if sowing && count("wheat_farm") < 1 + pop / 60 && can("wheat_farm") {
            return Some("wheat_farm");
        }
        if can("chicken_farm") {
            return Some("chicken_farm");
        }
    }
    // Loam for the farmhouses (houses upgrade by themselves once the market, the well and the
    // food reach them and loam and planks are in stock).
    if count("clay_pit") == 0 && pop >= 50 && can("clay_pit") {
        return Some("clay_pit");
    }
    if count("school") == 0 && count("church") > 0 && can("school") && stock("stone") >= 15 && stock("planks") >= 20 {
        return Some("school");
    }
    if count("warehouse") * 120 < pop && !site("warehouse") && can("warehouse") && stock("stone") >= 10 {
        return Some("warehouse");
    }
    if count("trading_post") == 0 && pop >= 80 && can("trading_post") {
        return Some("trading_post");
    }
    // A second market for the growing town (and so the first one can be upgraded).
    if count("market") + count("grand_market") < 1 + pop / 120 && !site("market") && can("market") {
        return Some("market");
    }
    // Iron for the castle (the kingdoms sell none): ore, charcoal and a smelter once the
    // fortification is researched.
    let fortified = defs.tech_id("fortification").is_some_and(|t| state.research().is_researched(t));
    if fortified && count("castle") == 0 {
        if count("iron_mine") == 0 && !site("iron_mine") && can("iron_mine") {
            return Some("iron_mine");
        }
        if count("iron_mine") > 0 && count("charcoal_kiln") == 0 && !site("charcoal_kiln") && can("charcoal_kiln") {
            return Some("charcoal_kiln");
        }
        if count("iron_mine") > 0 && count("smelter") == 0 && !site("smelter") && can("smelter") {
            return Some("smelter");
        }
    }
    ambitious_goal(state, defs, blocked)
}

/// The expansive player's extras: research in a fixed order once the militia stands, the
/// raw-material priority raised while wood runs short, standing orders selling surplus stone,
/// planks and food, and the upgrades of stores and workshops when gold and material allow.
fn expansive_commands(state: &GameState, defs: &Defs) -> Vec<Command> {
    use kingdoms_sim::defs::JobCategory;
    use kingdoms_sim::policy::JobPriority;
    let mut out = Vec::new();
    let gold = state.economy().treasury();
    let stock = |key: &str| defs.resource_id(key).map_or(0, |r| state.economy().stock().get(r));
    let r = state.research();
    let barracks = state.buildings().count_of(kind(defs, "barracks")) > 0;
    if barracks && r.queue().is_empty() && gold >= 300 {
        let order = [
            "bookkeeping", "grain_farming", "brickmaking", "schooling", "trading", "efficient_markets", "stone_masonry", "city_walls",
            "fortification", "smelting", "civil_service", "guild_system", "extended_storage", "larger_buildings", "trade_agreements",
            "banking",
        ];
        if let Some(tech) = order.iter().filter_map(|k| defs.tech_id(k)).find(|t| !r.is_researched(*t)) {
            out.push(Command::QueueResearchWithPrerequisites { tech });
        }
    }
    let wanted = if stock("wood") < wood_floor(state) { JobPriority::High } else { JobPriority::Normal };
    if state.policy().job_priority(JobCategory::RawMaterials) != wanted {
        out.push(Command::SetWorkforcePriority { category: JobCategory::RawMaterials, priority: wanted });
    }
    if barracks {
        for (key, keep) in [("stone", 300), ("planks", 300)] {
            let Some(resource) = defs.resource_id(key) else { continue };
            if state.economy().trade().order(resource).is_none() {
                let order = kingdoms_sim::economy::TradeOrder { resource, buy_below: None, sell_above: Some(keep), daily_limit: 30 };
                out.push(Command::SetTradeOrder { order });
            }
        }
        // Iron for the castle: bought from the kingdoms once the fortification is researched.
        let fortified = defs.tech_id("fortification").is_some_and(|t| r.is_researched(t));
        if let Some(resource) = defs.resource_id("iron_bars").filter(|_| fortified)
            && state.economy().trade().order(resource).is_none()
        {
            let order = kingdoms_sim::economy::TradeOrder { resource, buy_below: Some(30), sell_above: None, daily_limit: 10 };
            out.push(Command::SetTradeOrder { order });
        }
    }
    // Upgrades: one at a time, with 300 gold to spare.
    let upgrading = state.buildings().iter().any(|(_, b)| b.is_upgrading());
    if !upgrading && gold >= 300 {
        let wanted = ["town_hall", "watchtower", "storehouse", "sawmill", "market", "woodcutter", "school", "church"];
        // Never one that leaves houses without its service meanwhile (the only market: hunger).
        let saving_stone = rank_building_stone(state, defs) > 0;
        let candidate = state.buildings().iter().find(|(id, b)| {
            let key = defs.building(b.kind()).key.as_str();
            wanted.contains(&key)
                && !(saving_stone && key == "watchtower")
                && kingdoms_sim::buildings::upgrade_target(defs, b.kind()).is_some_and(|(up, _)| gold - i64::from(up.cost.gold) >= 300)
                && kingdoms_sim::buildings::upgrade_blockers(state, defs, *id).is_some_and(|list| list.is_empty())
                && kingdoms_sim::buildings::upgrade_service_gaps(state, defs, *id).is_empty()
        });
        if let Some((building, _)) = candidate {
            out.push(Command::UpgradeBuilding { building });
        }
    }
    out
}

/// True if a market (or grand market) serves tile `o` with `spare` tiles of its radius to
/// spare.
fn market_covers(state: &GameState, defs: &Defs, o: glam::IVec2, spare: i32) -> bool {
    state.buildings().iter().filter(|(_, b)| b.is_operational()).any(|(_, b)| {
        defs.building(b.kind())
            .services
            .iter()
            .any(|s| s.kind == kingdoms_sim::defs::ServiceKind::Market && b.rect().distance_to(o) + spare <= i32::from(s.radius))
    })
}

/// The expansive player's extras beyond the old town: a market by every outpost warehouse no
/// market serves (the plan places other buildings out there where storages and a market reach,
/// see `plan_with`), and a festival whenever the treasury holds far more than it needs
/// (docs/game_design.md §7 Feste).
fn expansive_outskirts(state: &GameState, defs: &Defs) -> Vec<Command> {
    let gold = state.economy().treasury();
    let stock = |key: &str| defs.resource_id(key).map_or(0, |id| state.economy().stock().get(id));
    let site_of = |k| state.buildings().iter().any(|(_, b)| b.kind() == k && b.construction_site().is_some());
    // Festival: the gold sink of the late game, while a large reserve stays.
    let cost = i64::from(kingdoms_sim::events::festival::festival_cost(state, defs));
    if gold >= 3000 + cost && kingdoms_sim::events::festival::festival_problem(state, defs).is_none() {
        return vec![Command::HoldFestival];
    }
    // A market by every outpost warehouse.
    let market = kind(defs, "market");
    let warehouse = kind(defs, "warehouse");
    if !site_of(market) && gold >= i64::from(defs.building(market).cost.gold) + 200 && stock("wood") >= 40 && stock("stone") >= 20 {
        let lonely = state
            .buildings()
            .iter()
            .filter(|(_, b)| b.kind() == warehouse && b.is_operational())
            .find(|(_, b)| !market_covers(state, defs, b.entrance(), 1));
        if let Some((_, w)) = lonely
            && let Some((origin, rotation, road)) =
                find_spot(state, defs, market, w.origin(), 1..10, |o| i32::from(supplied(state, defs, o))).filter(|(o, ..)| supplied(state, defs, *o))
        {
            return vec![Command::PlaceBuilding { kind: market, origin, rotation }, road];
        }
    }
    Vec::new()
}

/// True if building material reaches tile `o`: it lies within the carrier radius of a storage
/// that takes all goods (the town hall, warehouses), two tiles to spare.
fn supplied(state: &GameState, defs: &Defs, o: glam::IVec2) -> bool {
    state.buildings().iter().filter(|(_, b)| kingdoms_sim::economy::storage::is_operational_storage(defs, b)).any(|(_, b)| {
        defs.building(b.kind()).storage.as_ref().is_some_and(|st| {
            matches!(st.accepts, kingdoms_sim::defs::StorageFilter::All) && b.rect().distance_to(o) + 2 <= i32::from(st.carrier_radius)
        })
    })
}

/// Stone kept for the building the next rank requires (church, school) while it is missing.
fn rank_building_stone(state: &GameState, defs: &Defs) -> u32 {
    let missing = report::rank_report(state, defs)
        .next
        .is_some_and(|n| n.requirements.iter().any(|q| matches!(q.kind, kingdoms_sim::ranks::RequirementKind::Building(_)) && !q.done));
    if missing { 30 } else { 0 }
}

/// Gold the ambitious player keeps for the church: 230 from 60 inhabitants on, once the church
/// order is researched and until the church stands.
fn church_reserve(state: &GameState, defs: &Defs) -> i64 {
    let church = kind(defs, "church");
    let pending = state.buildings().count_of(church) == 0
        && state.research().building_unlocked(defs, church)
        && state.population().len() >= 60;
    if pending { 230 } else { 0 }
}

/// Unowned tiles within `radius` of `p` (on the map, not foreign).
fn new_land_near(state: &GameState, p: glam::IVec2, radius: i32) -> i32 {
    let mut n = 0;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let q = p + glam::IVec2::new(dx, dy);
            if dx * dx + dy * dy <= radius * radius
                && state.world().in_bounds(q)
                && !state.territory().contains(q)
                && !state.diplomacy().land().is_foreign(q)
            {
                n += 1;
            }
        }
    }
    n
}

/// The ambitious player's extras: the church order researched after the militia, the
/// construction priority raised while sites wait for builders, and a watchtower at the border
/// (where it claims the most new land) while the territory is short of the next rank.
fn ambitious_commands(state: &GameState, defs: &Defs, blocked: &mut Vec<(&'static str, u64)>, style: Style) -> Vec<Command> {
    use kingdoms_sim::defs::JobCategory;
    use kingdoms_sim::policy::JobPriority;
    let mut out = Vec::new();
    let r = state.research();
    if let (Some(militia), Some(clergy)) = (defs.tech_id("militia_training"), defs.tech_id("clergy"))
        && r.is_researched(militia)
        && !r.is_researched(clergy)
        && !r.is_queued(clergy)
        && state.population().len() >= 40
        && state.economy().treasury() >= 150
    {
        out.push(Command::QueueResearch { tech: clergy });
    }
    // Surplus food for gold: standing orders to sell what exceeds a stock of 400 units (the
    // stores hold thousands by the second year), at most 30 a day each.
    if state.buildings().count_of(kind(defs, "barracks")) > 0 {
        for key in ["vegetables", "meat", "fish"] {
            let Some(resource) = defs.resource_id(key) else { continue };
            if state.economy().trade().order(resource).is_none() {
                let order = kingdoms_sim::economy::TradeOrder { resource, buy_below: None, sell_above: Some(400), daily_limit: 30 };
                out.push(Command::SetTradeOrder { order });
            }
        }
    }
    // Short of wood: the woodcutters get their workers first (the expansive player keeps its
    // own wood floor, see `expansive_commands`).
    if style != Style::Expansive {
        let wood = defs.resource_id("wood").map_or(0, |r| state.economy().stock().get(r));
        let wanted = if wood < AMBITIOUS_WOOD && state.buildings().count_of(kind(defs, "barracks")) > 0 { JobPriority::High } else { JobPriority::Normal };
        if state.policy().job_priority(JobCategory::RawMaterials) != wanted {
            out.push(Command::SetWorkforcePriority { category: JobCategory::RawMaterials, priority: wanted });
        }
    }
    // Sites waiting for builders: construction first until they have them.
    let waiting = state.buildings().iter().any(|(_, b)| b.construction_site().is_some() && b.workers_wanted(defs) > 0 && b.staffed() == 0);
    let wanted = if waiting { JobPriority::High } else { JobPriority::Normal };
    if state.policy().job_priority(JobCategory::Construction) != wanted {
        out.push(Command::SetWorkforcePriority { category: JobCategory::Construction, priority: wanted });
    }
    // Borders: a watchtower where it claims the most new land, while the territory is short.
    let tower = kind(defs, "watchtower");
    let short = report::rank_report(state, defs)
        .next
        .is_some_and(|n| n.requirements.iter().any(|q| q.kind == kingdoms_sim::ranks::RequirementKind::Territory && !q.done));
    let stock = |key: &str| defs.resource_id(key).map_or(0, |id| state.economy().stock().get(id));
    let building_tower = state.buildings().iter().any(|(_, b)| b.kind() == tower && b.construction_site().is_some());
    // Houses for newcomers go first (see `ambitious_goal`).
    let food = report::food_forecast(state, defs);
    let house = kind(defs, "house_wood");
    let house_sites = state.buildings().iter().filter(|(_, b)| b.kind() == house && b.construction_site().is_some()).count();
    // (Only while a house could go up: without the wood for one the border comes first.)
    let houses_wanted = house_sites < 2
        && !blocked.iter().any(|(k, _)| *k == "house_wood")
        && report::population_summary(state, defs).immigrant_places < 6
        && food.in_stock >= 12.0 * food.consumption_per_day
        && stock("wood") >= 30
        && state.economy().treasury() >= 60;
    if short
        && !houses_wanted
        && !building_tower
        && state.buildings().count_of(kind(defs, "barracks")) > 0
        && state.research().building_unlocked(defs, tower)
        && state.economy().treasury() >= 90 + church_reserve(state, defs)
        && stock("stone") >= 15 + rank_building_stone(state, defs)
        && stock("planks") >= 10
        && !blocked.iter().any(|(k, _)| *k == "tower_border")
    {
        let center = state.world().start;
        // Within reach of the carriers of a storage that takes building material (so the
        // material arrives): the town hall and warehouses, radius from their footprint.
        let score = |o: glam::IVec2| if supplied(state, defs, o) { new_land_near(state, o, 8) } else { 0 };
        let spot = find_spot_in(state, defs, tower, center, 8..40, true, score).filter(|(o, ..)| score(*o) >= 24);
        match spot {
            Some((origin, rotation, road)) => out.extend([Command::PlaceBuilding { kind: tower, origin, rotation }, road]),
            None => {
                // The border lies beyond the carriers' reach: carry the supply out first – a
                // warehouse at the edge of the territory (what a human player does; the rule
                // that sites are supplied only from storages in reach stays).
                out.extend(outpost_warehouse(state, defs));
                blocked.push(("tower_border", state.clock().tick));
            }
        }
    }
    out
}

/// A warehouse at the edge of the territory, within the reach of the storages that supply its
/// site, where its carriers reach the most land not yet claimed (an outpost for the towers
/// beyond); `None` while one is being built, gold or material are short or no spot reaches
/// enough new land.
fn outpost_warehouse(state: &GameState, defs: &Defs) -> Vec<Command> {
    let warehouse = kind(defs, "warehouse");
    let gold = state.economy().treasury();
    let stock = |key: &str| defs.resource_id(key).map_or(0, |id| state.economy().stock().get(id));
    let building_one = state.buildings().iter().any(|(_, b)| b.kind() == warehouse && b.construction_site().is_some());
    let cost = i64::from(defs.building(warehouse).cost.gold);
    if building_one || gold < cost + 150 + church_reserve(state, defs) || stock("wood") < 40 || stock("stone") < 20 {
        return Vec::new();
    }
    // Its carriers reach about 20 tiles; the towers they supply claim 8 tiles around them. Half
    // a ring of new land is enough: where the land is narrow (coast, neighbours) the town grows
    // in smaller steps.
    let score = |o: glam::IVec2| if supplied(state, defs, o) { new_land_near(state, o, 18) } else { 0 };
    find_spot_in(state, defs, warehouse, state.world().start, 10..40, true, score)
        .filter(|(o, ..)| score(*o) >= 80)
        .map(|(origin, rotation, road)| vec![Command::PlaceBuilding { kind: warehouse, origin, rotation }, road])
        .unwrap_or_default()
}

/// A watchtower next to the first storage that no safety service reaches (bandits go for
/// those), once the barracks stand and if affordable.
fn tower_for_storage(state: &GameState, defs: &Defs, blocked: &mut Vec<(&'static str, u64)>) -> Vec<Command> {
    let tower = kind(defs, "watchtower");
    let barracks = state.buildings().count_of(kind(defs, "barracks")) > 0;
    if !barracks || !state.research().building_unlocked(defs, tower) || state.economy().treasury() < 100 {
        return Vec::new();
    }
    if blocked.iter().any(|(k, _)| *k == "watchtower") || state.buildings().iter().any(|(_, b)| b.kind() == tower && b.construction_site().is_some()) {
        return Vec::new();
    }
    let stock = |key: &str| defs.resource_id(key).map_or(0, |r| state.economy().stock().get(r));
    if stock("stone") < 10 || stock("planks") < 6 {
        return Vec::new();
    }
    let unguarded = state.buildings().iter().find(|(id, b)| {
        kingdoms_sim::economy::storage::is_operational_storage(defs, b) && !kingdoms_sim::events::raid_protected(state, defs, *id)
    });
    let Some((_, storage)) = unguarded else { return Vec::new() };
    // Only where the building material reaches (a site out of every carrier's reach would wait
    // for ever).
    let spot = find_spot(state, defs, tower, storage.origin(), 1..6, |o| i32::from(supplied(state, defs, o)))
        .filter(|(o, ..)| supplied(state, defs, *o));
    match spot {
        Some((origin, rotation, road)) => vec![Command::PlaceBuilding { kind: tower, origin, rotation }, road],
        None => {
            blocked.push(("watchtower", state.clock().tick));
            Vec::new()
        }
    }
}

/// Taxes: 15 % while saving for the barracks (the early economy earns little gold) – the
/// ambitious player also while saving for the church –, 10 % otherwise.
fn tax_commands(state: &GameState, defs: &Defs, style: Style) -> Option<Command> {
    let researched = defs.tech_id("militia_training").is_some_and(|t| state.research().is_researched(t));
    let saving = (researched && state.population().len() >= MILITARY_FROM && state.buildings().count_of(kind(defs, "barracks")) == 0)
        || (style != Style::Reasonable && church_reserve(state, defs) > 0);
    let percent = if saving { 15 } else { 10 };
    (state.policy().tax_rate_percent != percent).then_some(Command::SetTaxRate { percent })
}

/// The sawmills turn wood into planks; a player who has planks enough closes them, so the wood
/// goes to houses and the stoves (docs/game_design.md §6.1 Heizen geht vor), and opens them
/// again when planks run low: above `high` planks closed, below `low` open.
fn sawmill_commands(state: &GameState, defs: &Defs, low: u32, high: u32) -> Vec<Command> {
    let planks = defs.resource_id("planks").map_or(0, |r| state.economy().stock().get(r));
    let mills = [kind(defs, "sawmill"), kind(defs, "large_sawmill")];
    state
        .buildings()
        .iter()
        .filter(|(_, b)| mills.contains(&b.kind()) && b.is_complete())
        .filter_map(|(building, b)| {
            let paused = b.is_paused();
            let want = if planks >= high { true } else if planks < low { false } else { paused };
            (want != paused).then_some(Command::SetBuildingPaused { building, paused: want })
        })
        .collect()
}

/// Research, recruitment and the company.
fn military_commands(state: &GameState, defs: &Defs) -> Vec<Command> {
    let mut out = Vec::new();
    let Some(militia_tech) = defs.tech_id("militia_training") else { return out };
    let r = state.research();
    if !r.is_researched(militia_tech) && !r.is_queued(militia_tech) && state.population().len() >= MILITARY_FROM && state.economy().treasury() >= 150 {
        out.push(Command::QueueResearch { tech: militia_tech });
    }
    let barracks = state
        .buildings()
        .iter()
        .find(|(_, b)| b.kind() == kind(defs, "barracks") && b.is_operational())
        .map(|(id, _)| id);
    let m = state.military();
    let soldiers = u32::try_from(m.soldiers().count()).unwrap_or(0);
    let target = (u32::try_from(state.population().len()).unwrap_or(0) / 7).min(8);
    let unemployed = state.workforce().unemployed();
    let militia = defs.unit_id("militia");
    // The first four even from workers (the recruitment takes the unemployed first), then
    // only while adults are without work.
    if let (Some(b), Some(unit)) = (barracks, militia)
        && soldiers < target
        && (unemployed >= 2 || soldiers < 4)
        && state.economy().treasury() >= 60
    {
        let count = u8::try_from((target - soldiers).min(2)).unwrap_or(1);
        if kingdoms_sim::military::recruit_problem(state, defs, b, unit, count).is_none() {
            out.push(Command::Recruit { building: b, unit, count });
        }
    }
    let ready: Vec<_> = m.soldiers().filter(|(_, s)| s.is_ready() && s.company().is_none() && s.crew_of().is_none()).map(|(id, _)| id).collect();
    if !ready.is_empty() {
        match m.companies().first() {
            None if ready.len() >= 4 => out.push(Command::CreateCompany { soldiers: ready }),
            Some(c) => out.push(Command::AssignSoldiers { soldiers: ready, company: Some(c.id()) }),
            None => {}
        }
    }
    // The company guards the town: the headquarters and the storages (bandits go for those);
    // when enemies come for a building, it stands at that building until they are gone.
    if let Some(c) = m.companies().first()
        && matches!(c.order(), Order::Station | Order::Defend(_))
        && let Some(order) = defence_order(state, defs)
        && c.order() != &order
        && kingdoms_sim::military::order_problem(state, defs, c.id(), &order).is_none()
    {
        out.push(Command::CompanyOrder { company: c.id(), order });
    }
    out
}

/// Where the company stands: around the target of the first enemy group on the map or on its
/// way, else over the town – the headquarters and every storage within the area (radius up to
/// the largest allowed, `DEFEND_RADIUS`).
fn defence_order(state: &GameState, defs: &Defs) -> Option<Order> {
    let threatened = state.military().warbands().iter().filter(|w| w.stage().is_hostile()).find_map(|w| w.target());
    if let Some(b) = threatened.and_then(|b| state.building(b)) {
        return Some(Order::Defend(DefendTarget::Area { center: b.entrance(), radius: 4 }));
    }
    let hq = kingdoms_sim::economy::storage::main_headquarters(state, defs).and_then(|b| state.building(b))?;
    let mut points = vec![hq.entrance()];
    points.extend(
        state
            .buildings()
            .iter()
            .filter(|(_, b)| kingdoms_sim::economy::storage::is_operational_storage(defs, b))
            .map(|(_, b)| b.entrance()),
    );
    let n = i32::try_from(points.len()).unwrap_or(1);
    let center = points.iter().fold(glam::IVec2::ZERO, |a, p| a + *p) / n;
    let max = i32::from(*kingdoms_sim::military::DEFEND_RADIUS.end());
    let reach = points.iter().map(|p| (*p - center).abs().max_element()).max().unwrap_or(0);
    let radius = u8::try_from((reach + 2).clamp(8, max)).unwrap_or(8);
    Some(Order::Defend(DefendTarget::Area { center, radius }))
}

/// Bandit raids: send the soldiers when there are five, else the reasonable player's choice.
fn decide(state: &GameState, defs: &Defs) -> Option<Command> {
    let pending = state.events().pending()?;
    let def = defs.event(pending.event);
    if def.key == "bandit_raid" {
        let send = def.choices.get(2).is_some_and(|c| kingdoms_sim::events::choice_problem(state, defs, c).is_none());
        if send {
            return Some(Command::ResolveEvent { serial: pending.serial, choice: 2 });
        }
        let ransom = def.choices.get(1).is_some_and(|c| {
            kingdoms_sim::events::choice_problem(state, defs, c).is_none() && state.economy().treasury() - choice_cost(c) >= 250
        });
        return Some(Command::ResolveEvent { serial: pending.serial, choice: u8::from(ransom) });
    }
    decide_event(state, defs)
}

/// The weakest kingdom (fewest soldiers, then lowest id).
fn weakest_kingdom(state: &GameState) -> Option<kingdoms_sim::diplomacy::KingdomId> {
    state.diplomacy().kingdoms().min_by_key(|(id, k)| (k.soldiers(), *id)).map(|(id, _)| id)
}

/// Going to war once (if no war happened yet): on [`WAR_DAY`] the pact with the weakest
/// kingdom is cancelled, an hour later war is declared.
fn war_commands(state: &GameState, defs: &Defs, day: u64, hour: u64, wars: u32) -> Vec<Command> {
    if wars > 0 || day != WAR_DAY || !(10..=12).contains(&hour) {
        return Vec::new();
    }
    let Some(target) = weakest_kingdom(state) else { return Vec::new() };
    let Some(k) = state.diplomacy().kingdom(target) else { return Vec::new() };
    let tick = state.clock().tick;
    for treaty in [TreatyKind::Alliance, TreatyKind::NonAggression] {
        if k.has_treaty(treaty, tick) {
            return vec![Command::Diplomacy { kingdom: target, action: DiplomaticAction::CancelTreaty { treaty } }];
        }
    }
    let action = DiplomaticAction::DeclareWar;
    if kingdoms_sim::diplomacy::actions::action_problem(state, defs, target, &action).is_none() {
        return vec![Command::Diplomacy { kingdom: target, action }];
    }
    Vec::new()
}

/// Keeping the peace: answers to proposals, pacts offered, peace asked for.
fn diplomacy_commands(state: &GameState, defs: &Defs, day: u64, hour: u64, style: Style) -> Vec<Command> {
    let mut out = Vec::new();
    let food = report::food_forecast(state, defs);
    for p in state.diplomacy().proposals() {
        let accept = match p.kind {
            ProposalKind::Treaty(_) => true,
            ProposalKind::Peace { terms: PeaceTerms::PlayerPays { gold } } => i64::from(gold) * 2 <= state.economy().treasury(),
            ProposalKind::Peace { .. } => true,
            ProposalKind::Tribute { gold } => i64::from(gold) * 3 <= state.economy().treasury(),
            // The ambitious player sells what it has in plenty: food beyond twenty days of
            // eating, other goods beyond twice the amount asked for.
            ProposalKind::Purchase { resource, amount, .. } if style != Style::Reasonable => {
                let have = state.economy().stock().get(resource) as f32;
                let sold = amount as f32;
                let is_food = defs.resource(resource).category == kingdoms_sim::defs::ResourceCategory::Food;
                let keep = if is_food { 20.0 * food.consumption_per_day } else { 2.0 * sold };
                have - sold >= keep && (!is_food || food.in_stock - sold >= 20.0 * food.consumption_per_day)
            }
            ProposalKind::Purchase { .. } | ProposalKind::Sale { .. } => false,
        };
        let possible = kingdoms_sim::diplomacy::actions::proposal_problem(state, defs, p).is_none();
        out.push(Command::AnswerProposal { serial: p.serial, accept: accept && possible });
    }
    if hour == 11 && day % 3 == 0 && !(WAR_DAY..WAR_DAY + 3).contains(&day) {
        for (id, k) in state.diplomacy().kingdoms() {
            let action = if k.at_war() {
                DiplomaticAction::ProposePeace { terms: PeaceTerms::White }
            } else if !k.has_treaty(TreatyKind::NonAggression, state.clock().tick) {
                DiplomaticAction::ProposeTreaty { treaty: TreatyKind::NonAggression }
            } else {
                continue;
            };
            let promising = kingdoms_sim::diplomacy::actions::judge(state, defs, id, &action).is_some_and(|j| j.accepted);
            if promising && kingdoms_sim::diplomacy::actions::action_problem(state, defs, id, &action).is_none() {
                out.push(Command::Diplomacy { kingdom: id, action });
            }
        }
    }
    out
}

/// What happened in a three-year game.
struct Outcome {
    start_population: usize,
    population: usize,
    soldiers: usize,
    deaths: Vec<(u64, DeathCause)>,
    min_treasury: i64,
    treasury: i64,
    happiness: u8,
    raids: Vec<(BattleOutcome, u32, u32, String)>,
    wars: u32,
    peace: u32,
    rank: u8,
    rank_name: String,
    battles: usize,
    defeat: bool,
    /// Ranks reached after the start with the day.
    reached: Vec<(String, u64)>,
}

/// Access roads for the buildings and sites whose entrance has no road (the road was rejected
/// for want of material when they were placed): what a player does on the „kein
/// Straßenanschluss“ warning. One road per call, only when it can be paid now.
fn reconnect_roads(state: &GameState, defs: &Defs) -> Vec<Command> {
    let Some(path) = defs.road_id("path") else { return Vec::new() };
    state
        .buildings()
        .iter()
        .filter(|(_, b)| defs.building(b.kind()).needs_road() && state.roads().get(b.entrance()).is_none())
        .find_map(|(_, b)| {
            kingdoms_sim::roads::plan_access_road(state, defs, b.kind(), b.origin(), b.rotation(), path)
                .filter(|plan| plan.tiles.iter().any(|t| t.is_new))
                .and_then(|plan| plan.to_command())
        })
        .into_iter()
        .collect()
}

fn play(defs: &Defs, seed: u64, style: Style) -> Outcome {
    play_for(defs, seed, style, DAYS)
}

/// [`play`] for `days` game days.
fn play_for(defs: &Defs, seed: u64, style: Style, days: u64) -> Outcome {
    let mut state = GameState::new(defs, GameSettings::new(defs, seed, WorldSize::Small));
    let start_population = state.population().len();
    let mut blocked = Vec::new();
    let mut deaths = Vec::new();
    let mut min_treasury = state.economy().treasury();
    let (mut wars, mut peace) = (0, 0);
    let report_every = if days > DAYS { 12 } else { 4 };
    for tick in 0..days * TICKS_PER_DAY {
        let day = tick / TICKS_PER_DAY;
        let hour = (tick % TICKS_PER_DAY) / TICKS_PER_HOUR;
        let mut commands = Vec::new();
        if tick % TICKS_PER_HOUR == 1 && hour == 7 {
            // Every morning: a building whose access road could not be paid when it was placed
            // („kein Straßenanschluss“) gets its road now.
            commands.extend(reconnect_roads(&state, defs));
        }
        if tick % TICKS_PER_HOUR == 1 {
            let goal = match style {
                Style::Reasonable => campaign_goal,
                Style::Ambitious => ambitious_goal,
                Style::Expansive => expansive_goal,
            };
            if style == Style::Expansive {
                // The town grows beyond the old rings where storages and a market reach.
                let outskirts = |goal: &str, o: glam::IVec2| -> bool {
                    supplied(&state, defs, o)
                        && match goal {
                            "woodcutter" => common::trees_near(&state, o, 8) >= 8,
                            "forester" | "iron_mine" | "coal_mine" | "quarry" | "fishery" | "clay_pit" => true,
                            // Houses well inside a market's reach (the runners walk there daily).
                            "house_wood" => market_covers(&state, defs, o, 6),
                            // The school serves the children in its radius wherever it stands:
                            // once the old town is full it goes to the edge.
                            "school" => true,
                            _ => market_covers(&state, defs, o, 1),
                        }
                };
                commands.extend(common::plan_with(&state, defs, &mut blocked, goal, Some(&outskirts)));
            } else {
                commands.extend(plan(&state, defs, &mut blocked, goal));
            }
            if commands.is_empty() {
                commands.extend(tower_for_storage(&state, defs, &mut blocked));
            }
            if style != Style::Reasonable {
                commands.extend(ambitious_commands(&state, defs, &mut blocked, style));
                let (low, high) = if style == Style::Expansive { (80, 160) } else { (30, 60) };
                commands.extend(sawmill_commands(&state, defs, low, high));
            }
            if style == Style::Expansive {
                commands.extend(expansive_commands(&state, defs));
                if commands.is_empty() {
                    commands.extend(expansive_outskirts(&state, defs));
                }
            }
            commands.extend(decide(&state, defs));
            commands.extend(military_commands(&state, defs));
            commands.extend(diplomacy_commands(&state, defs, day, hour, style));
            commands.extend(war_commands(&state, defs, day, hour, wars));
            commands.extend(tax_commands(&state, defs, style));
            if RAID_DAYS.contains(&day) && hour == 10 {
                let size = kingdoms_sim::military::bandit_band(&state, defs);
                commands.push(Command::DebugSpawnThreat { kingdom: None, size });
            }
        }
        for e in state.tick(defs, commands) {
            match e {
                SimEvent::CitizenDied { cause, .. } => deaths.push((day, cause)),
                SimEvent::WarDeclared { .. } => wars += 1,
                SimEvent::PeaceMade { .. } => peace += 1,
                _ => {}
            }
        }
        min_treasury = min_treasury.min(state.economy().treasury());
        if state.game_over().is_some() {
            break;
        }
        if tick % (report_every * TICKS_PER_DAY) == 0 && tick > 0 {
            let s = report::population_summary(&state, defs);
            let mil = report::military_report(&state, defs);
            let count = |key: &str| state.buildings().count_of(kind(defs, key));
            let stock = |key: &str| defs.resource_id(key).map_or(0, |r| state.economy().stock().get(r));
            let f = report::food_forecast(&state, defs);
            eprintln!(
                "seed {seed} day {day:>2}: food stock {:.0} barns {:.0} per day {:.0} other prod {:.0} veg farms {} chicken {} fishery {} storehouses {}",
                f.in_stock, f.in_barns, f.consumption_per_day, f.other_production_per_day, count("vegetable_farm"), count("chicken_farm"), count("fishery"), count("storehouse"),
            );
            eprintln!(
                "seed {seed} day {day:>2}: militia {} woodcutters {} foresters {} quarry {} sawmill {} barracks {} towers {} stone {} planks {} tools {} blocked {:?} unempl {}",
                defs.tech_id("militia_training").is_some_and(|t| state.research().is_researched(t)),
                count("woodcutter") + count("lumber_camp"),
                count("forester"),
                count("quarry"),
                count("sawmill"),
                count("barracks"),
                count("watchtower"),
                stock("stone"),
                stock("planks"),
                stock("tools"),
                blocked,
                state.workforce().unemployed(),
            );
            eprintln!(
                "seed {seed} day {day:>2}: housing cap {} housed {} homeless {} free {} wood {} food {} goal {:?} buildings {} church {} school {}",
                s.housing_capacity,
                s.housed,
                s.homeless,
                s.immigrant_places,
                stock("wood"),
                f.in_stock,
                {
                    let names: Vec<&str> = blocked.iter().map(|(k, _)| *k).collect();
                    match style {
                        Style::Reasonable => campaign_goal(&state, defs, &names),
                        Style::Ambitious => ambitious_goal(&state, defs, &names),
                        Style::Expansive => expansive_goal(&state, defs, &names),
                    }
                },
                state.buildings().len(),
                count("church"),
                count("school"),
            );
            if let Some(y) = state.economy().ledger().yesterday() {
                eprintln!(
                    "seed {seed} day {day:>2}: ledger {:?}",
                    kingdoms_sim::economy::LedgerCategory::ALL.iter().map(|c| (*c, y.gold(*c))).filter(|(_, g)| *g != 0).collect::<Vec<_>>()
                );
            }
            eprintln!(
                "seed {seed} day {day:>2}: pop {:>3}, gold {:>5}, happy {:>2}, soldiers {} ({} ready), companies {}, threats {}, rank {}, wars {:?}",
                s.population,
                state.economy().treasury(),
                s.average_happiness,
                mil.soldiers,
                mil.ready,
                mil.companies.len(),
                mil.threats.len(),
                report::rank_report(&state, defs).name,
                mil.wars.iter().map(|w| w.text.clone()).collect::<Vec<_>>(),
            );
        }
    }
    for b in report::battle_reports(&state) {
        eprintln!("seed {seed} battle {} {} {}: {}", b.date_text, b.kind_name, b.outcome_name, b.text);
    }
    if let Some(next) = report::rank_report(&state, defs).next {
        eprintln!(
            "seed {seed} next rank {}: {:?}",
            next.name,
            next.requirements.iter().map(|r| format!("{} {}/{}", r.label, r.current, r.required)).collect::<Vec<_>>()
        );
    }
    let rank_buildings: Vec<(String, String)> = state
        .buildings()
        .iter()
        .filter(|(_, b)| {
            matches!(defs.building(b.kind()).key.as_str(), "church" | "cathedral" | "school" | "academy") || b.construction_site().is_some()
        })
        .map(|(id, b)| {
            let work = kingdoms_sim::buildings::site_work(&state, defs, id).map(|w| w.text()).unwrap_or("");
            (defs.building(b.kind()).key.clone(), format!("{:?} {} at {}", b.status(), work, b.origin()))
        })
        .collect();
    eprintln!("seed {seed} rank buildings: {rank_buildings:?}");
    eprintln!("seed {seed} events: {:?}", state.events().history().map(|r| defs.event(r.event).key.clone()).collect::<Vec<_>>());
    eprintln!("seed {seed} diplomacy: {:?}", state.diplomacy().history().map(|r| r.text.clone()).collect::<Vec<_>>());
    if let Some(over) = state.game_over() {
        eprintln!("seed {seed} GAME OVER: {}", kingdoms_sim::ranks::defeat_text(&state, over.reason));
    } else {
        state.validate(defs).unwrap_or_else(|e| panic!("seed {seed}: consistent state: {e}"));
    }
    let raids = state
        .military()
        .battles()
        .filter(|b| b.kind == BattleKind::Raid)
        .map(|b| (b.outcome, b.tally.own_losses, b.tally.goods_lost.iter().map(|m| m.amount).sum::<u32>() + b.tally.gold_lost, b.text.clone()))
        .collect();
    let happiness = report::population_summary(&state, defs).average_happiness;
    Outcome {
        start_population,
        population: state.population().len(),
        soldiers: state.military().soldiers().count(),
        deaths,
        min_treasury,
        treasury: state.economy().treasury(),
        happiness,
        raids,
        wars,
        peace,
        rank: state.progress().rank(),
        rank_name: report::rank_report(&state, defs).name,
        battles: state.military().battles().count(),
        defeat: state.game_over().is_some(),
        reached: state
            .progress()
            .reached()
            .iter()
            .map(|(r, tick)| (defs.ranks().get(usize::from(*r)).map_or_else(String::new, |d| d.name.clone()), tick / TICKS_PER_DAY))
            .collect(),
    }
}

fn check(seed: u64) {
    let defs = load_defs();
    let o = play(&defs, seed, Style::Reasonable);
    let mut causes: Vec<(DeathCause, usize)> = Vec::new();
    for (_, c) in &o.deaths {
        match causes.iter_mut().find(|(k, _)| k == c) {
            Some((_, n)) => *n += 1,
            None => causes.push((*c, 1)),
        }
    }
    eprintln!(
        "METRICS seed {seed}: population {} → {}, rank {} ({}), treasury {} (lowest {}), happiness {}, soldiers {}, wars {} (peace {}), battles {}, deaths {:?}, raids {:?}",
        o.start_population,
        o.population,
        o.rank,
        o.rank_name,
        o.treasury,
        o.min_treasury,
        o.happiness,
        o.soldiers,
        o.wars,
        o.peace,
        o.battles,
        causes,
        o.raids.iter().map(|r| (r.0, r.1, r.2)).collect::<Vec<_>>()
    );
    assert!(!o.defeat, "seed {seed}: the kingdom survived three years");
    assert!(o.wars >= 1, "seed {seed}: the kingdom fought a war");
    assert!(o.rank >= 1, "seed {seed}: the ruler rose in rank");
    assert!(o.population >= o.start_population + 15, "seed {seed}: the kingdom grew ({} → {})", o.start_population, o.population);
    assert!(o.soldiers >= 4, "seed {seed}: the kingdom keeps soldiers ({})", o.soldiers);
    assert!(
        o.deaths.iter().all(|(_, c)| !matches!(c, DeathCause::Starvation | DeathCause::Freezing)),
        "seed {seed}: nobody starved or froze: {:?}",
        o.deaths
    );
    // Both bands sent by the test were met; the defence held with modest losses.
    assert!(o.raids.len() >= RAID_DAYS.len(), "seed {seed}: the bandits came ({:?})", o.raids);
    assert!(o.raids.iter().any(|r| r.0 == BattleOutcome::Victory), "seed {seed}: at least one raid was beaten off: {:?}", o.raids);
    let losses: u32 = o.raids.iter().map(|r| r.1).sum();
    assert!(losses <= 6, "seed {seed}: modest losses against bandits ({losses})");
    // Bandits may empty the treasury, but the pay of the soldiers never drives it into debt.
    assert!(o.min_treasury >= 0, "seed {seed}: the treasury did not collapse into bankruptcy (lowest {})", o.min_treasury);
    assert!((50..=95).contains(&o.happiness), "seed {seed}: average happiness {}", o.happiness);
}

#[test]
fn reasonable_player_defends_the_kingdom_for_three_years_seed_7() {
    check(7);
}

#[test]
fn reasonable_player_defends_the_kingdom_for_three_years_seed_11() {
    check(11);
}

#[test]
fn reasonable_player_defends_the_kingdom_for_three_years_seed_23() {
    check(23);
}

#[test]
fn reasonable_player_defends_the_kingdom_for_three_years_seed_31() {
    check(31);
}

#[test]
fn reasonable_player_defends_the_kingdom_for_three_years_seed_42() {
    check(42);
}

/// The ambitious player (see [`Style::Ambitious`]) reaches the rank „Stadt“ within the three
/// years, keeps everybody fed and warm and the treasury out of debt.
fn check_ambitious(seed: u64) {
    let defs = load_defs();
    let o = play(&defs, seed, Style::Ambitious);
    eprintln!(
        "AMBITIOUS seed {seed}: population {} → {}, rank {} ({}, reached {:?}), treasury {} (lowest {}), happiness {}, soldiers {}, raids {:?}",
        o.start_population,
        o.population,
        o.rank,
        o.rank_name,
        o.reached,
        o.treasury,
        o.min_treasury,
        o.happiness,
        o.soldiers,
        o.raids.iter().map(|r| (r.0, r.1, r.2)).collect::<Vec<_>>()
    );
    assert!(!o.defeat, "seed {seed}: the kingdom survived three years");
    let town = defs.ranks().iter().position(|r| r.key == "town").expect("rank town");
    assert!(usize::from(o.rank) >= town, "seed {seed}: the ambitious player became a town (rank {})", o.rank_name);
    assert!(
        o.deaths.iter().all(|(_, c)| !matches!(c, DeathCause::Starvation | DeathCause::Freezing)),
        "seed {seed}: nobody starved or froze: {:?}",
        o.deaths
    );
    assert!(o.min_treasury >= 0, "seed {seed}: the treasury did not collapse (lowest {})", o.min_treasury);
    assert!((50..=95).contains(&o.happiness), "seed {seed}: average happiness {}", o.happiness);
}

#[test]
fn ambitious_player_becomes_a_town_within_three_years_seed_7() {
    check_ambitious(7);
}

#[test]
fn ambitious_player_becomes_a_town_within_three_years_seed_23() {
    check_ambitious(23);
}

/// The ambitious player on the other campaign seeds, for the balance notes: it prints its key
/// numbers and must survive and reach at least „Marktflecken“ (it does not reach „Stadt“ on
/// every map in three years).
#[test]
#[ignore = "balance survey over more seeds; run with --ignored --nocapture"]
fn ambitious_player_on_more_seeds() {
    let defs = load_defs();
    for seed in [11, 31, 42] {
        let o = play(&defs, seed, Style::Ambitious);
        eprintln!("AMBITIOUS seed {seed}: population {} → {}, rank {} ({:?}), treasury {}", o.start_population, o.population, o.rank_name, o.reached, o.treasury);
        assert!(!o.defeat && o.rank >= 2, "seed {seed}: survived and became at least a market town");
    }
}

/// The top ranks stay within reach on the smallest map (docs/game_design.md §1): on every
/// campaign seed the land a player can claim (dry, not owned by an AI kingdom) is well above
/// the territory of „Imperium“, so a long game (towers, warehouses, the castle, land ceded
/// after sieges) can get there.
#[test]
fn the_top_ranks_fit_the_smallest_map() {
    let defs = load_defs();
    let empire = defs.ranks().last().expect("ranks");
    for seed in [7, 11, 23, 31, 42] {
        let state = GameState::new(&defs, GameSettings::new(&defs, seed, WorldSize::Small));
        let (w, h) = (state.world().width(), state.world().height());
        let mut free = 0u32;
        for y in 0..h {
            for x in 0..w {
                let p = glam::IVec2::new(x, y);
                if !state.diplomacy().land().is_foreign(p) && !state.world().tile(p).terrain.is_water() {
                    free += 1;
                }
            }
        }
        eprintln!("seed {seed}: {free} tiles of free land, {} needed for {}", empire.territory, empire.name);
        assert!(free >= 2 * empire.territory, "seed {seed}: {free} free tiles for {} needed", empire.territory);
    }
}

/// Eight game years (192 days) plus a day: the long game of the expansive player.
const LONG_DAYS: u64 = 8 * 24 + 1;

/// The long game (docs/game_design.md §1 Fortschrittskurve): the expansive player
/// ([`Style::Expansive`]) plays eight years on three seeds; the town keeps growing past 100
/// inhabitants and at least one seed reaches „Großstadt“ (in polish round 11 all three did:
/// outpost warehouses are restocked from the town and supply the border, heating comes before
/// the workshops, the town grows beyond the old rings, festivals spend the surplus). „Königreich“ is not expected within eight years: it needs the
/// castle and its 20 iron bars. Prints the progression of every seed (the rank and the day it
/// was reached, the season samples of the long-term statistics).
#[test]
#[ignore = "long-game survey (eight years on three seeds); run with --release --ignored --nocapture"]
fn expansive_player_grows_beyond_the_town_in_eight_years() {
    let defs = load_defs();
    let city = defs.ranks().iter().position(|r| r.key == "city").expect("rank city");
    let mut best = 0;
    // `LONG_SEEDS=7,23` limits the survey (balance work).
    let seeds: Vec<u64> = std::env::var("LONG_SEEDS")
        .ok()
        .map(|v| v.split(',').filter_map(|s| s.trim().parse().ok()).collect())
        .unwrap_or_else(|| vec![7, 23, 31]);
    for seed in seeds {
        let o = play_for(&defs, seed, Style::Expansive, LONG_DAYS);
        let starved: Vec<u64> = o.deaths.iter().filter(|(_, c)| *c == DeathCause::Starvation).map(|(d, _)| *d).collect();
        let froze: Vec<u64> = o.deaths.iter().filter(|(_, c)| *c == DeathCause::Freezing).map(|(d, _)| *d).collect();
        eprintln!(
            "LONG seed {seed}: population {} → {}, rank {} ({:?}), treasury {} (lowest {}), happiness {}, starved on days {starved:?}, froze on days {froze:?}",
            o.start_population, o.population, o.rank_name, o.reached, o.treasury, o.min_treasury, o.happiness
        );
        assert!(!o.defeat, "seed {seed}: survived eight years");
        assert!(o.population > 100, "seed {seed}: grew past 100 inhabitants ({})", o.population);
        assert!(starved.len() * 100 <= o.population, "seed {seed}: hardly anybody starved ({starved:?})");
        best = best.max(usize::from(o.rank));
    }
    assert!(best >= city, "at least one seed reached „Großstadt“ (best rank {best})");
}
