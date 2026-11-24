//! Companies, orders and the duty of every soldier (docs/game_design.md §11).
//!
//! **Duty** (derived every combat round): soldiers in training go to their training building
//! during the working hours; ready soldiers follow their company's order during the working
//! hours and otherwise only while enemies threaten (the alert): at their post (the training building, else the
//! headquarters), defending a building or an area, on patrol, gathering for a campaign. Inside
//! a building fit as many as its garrison allows, the others stand on the free tiles around it
//! (nearest first). Wounded or demoralised soldiers return to their post. Off duty a soldier
//! follows the normal daily routine of a citizen.
//!
//! **Engagement:** a ready soldier whose post, area or own position (patrols) lies within the
//! alert radius of an enemy goes for the nearest one: melee soldiers walk up to it, shooters
//! stop within range; garrisoned shooters and soldiers fighting out of a building stay inside.
//! During an event alarm every ready soldier hunts the enemies anywhere on the map; during a
//! muster they gather at the headquarters.

use std::collections::BTreeMap;

use glam::IVec2;

use super::threats::standing_tile;
use super::{Company, DefendTarget, Order, SoldierStage, count_text, tile_distance};
use crate::buildings::BuildingId;
use crate::defs::Defs;
use crate::diplomacy::KingdomId;
use crate::economy::storage;
use crate::event::SimEvent;
use crate::grid::TileRect;
use crate::population::{Activity, CitizenId, Destination, Purpose, TravelerId, TripPhase, routine};
use crate::state::GameState;
use crate::time::TICKS_PER_HOUR;

/// The destination soldier `id` must follow instead of the routine: `Some(None)` while away on
/// a campaign (hidden), `Some(Some(_))` on duty, `None` off duty (normal routine).
pub(crate) fn soldier_destination(state: &GameState, id: CitizenId) -> Option<Option<Destination>> {
    let s = state.military.soldiers.get(&id)?;
    if s.away.is_some() {
        return Some(None);
    }
    s.duty.map(Some)
}

/// Activity of soldier `id` on duty (`None`: off duty or walking without a fight).
pub(crate) fn soldier_activity(state: &GameState, id: CitizenId) -> Option<Activity> {
    let s = state.military.soldiers.get(&id)?;
    let c = state.population.citizen(id)?;
    if s.away.is_some() {
        return Some(Activity::Campaign);
    }
    if s.engage.is_some() {
        return Some(Activity::Fighting);
    }
    let still = c.motion().phase() == TripPhase::Still && c.motion().is_at_destination();
    match s.stage {
        SoldierStage::Training { building, .. } if c.motion().inside() == Some(building) => Some(Activity::Training),
        _ if s.duty.is_some() && still => Some(Activity::OnDuty),
        _ => None,
    }
}

fn building_name(state: &GameState, defs: &Defs, id: BuildingId) -> String {
    state
        .buildings
        .get(id)
        .and_then(|b| defs.buildings().get(b.kind().index()))
        .map_or_else(|| "Gebäude".to_owned(), |d| d.name.clone())
}

/// German description of what soldier `id` does (`None`: off duty, the normal text applies).
pub(crate) fn soldier_activity_text(state: &GameState, defs: &Defs, id: CitizenId) -> Option<String> {
    let s = state.military.soldiers.get(&id)?;
    let c = state.population.citizen(id)?;
    if let Some(camp) = s.away.and_then(|a| state.military.campaign(a)) {
        let name = state.diplomacy.kingdom(camp.kingdom).map_or("?", |k| k.name.as_str());
        return Some(match camp.stage {
            super::CampaignStage::Sieging { .. } => format!("Belagert {name}"),
            super::CampaignStage::Returning { .. } => format!("Kehrt vom Feldzug gegen {name} zurück"),
            _ => format!("Auf Feldzug gegen {name}"),
        });
    }
    if s.engage.is_some() {
        return Some(Activity::Fighting.display_name().to_owned());
    }
    let walking = matches!(c.motion().phase(), TripPhase::Walking | TripPhase::Queued);
    if let SoldierStage::Training { building, done_milli } = s.stage {
        let total = defs.units().get(s.unit.index()).map_or(1, |u| u.training_ticks.max(1)) * 1000;
        let percent = done_milli * 100 / total;
        let name = building_name(state, defs, building);
        return Some(if walking {
            format!("Auf dem Weg zur Ausbildung ({name})")
        } else if c.motion().inside() == Some(building) {
            format!("In Ausbildung ({name}, {percent} %)")
        } else {
            return None;
        });
    }
    let patrol = s.company.and_then(|co| state.military.company(co)).is_some_and(|co| matches!(co.order, Order::Patrol { .. }));
    Some(match (s.duty?, walking) {
        (_, true) if patrol => "Auf Patrouille".to_owned(),
        (Destination::Building { building, .. }, true) => format!("Auf dem Weg zum Dienst ({})", building_name(state, defs, building)),
        (Destination::Building { building, .. }, false) => format!("Im Dienst ({})", building_name(state, defs, building)),
        (_, true) => "Auf dem Weg zum Posten".to_owned(),
        (_, false) if patrol => "Auf Patrouille".to_owned(),
        (_, false) => "Hält Wache".to_owned(),
    })
}

/// Why `order` cannot be given to company `company` now (German), or `None`.
pub fn order_problem(state: &GameState, defs: &Defs, company: u32, order: &Order) -> Option<String> {
    let Some(c) = state.military.company(company) else { return Some("Diese Kompanie gibt es nicht.".to_owned()) };
    let rules = &defs.rules().military;
    let campaigning = state.military.campaigns.iter().any(|camp| camp.company == company);
    if campaigning && *order != Order::Retreat {
        return Some(format!("{} ist auf einem Feldzug; befehlt zuerst den Rückzug.", c.name));
    }
    match order {
        Order::Station | Order::Retreat => None,
        Order::Defend(DefendTarget::Building(b)) => match state.buildings.get(*b) {
            None => Some("Dieses Gebäude gibt es nicht.".to_owned()),
            Some(bb) if !bb.is_complete() => Some(format!("Das Gebäude „{}“ ist noch nicht fertig.", building_name(state, defs, *b))),
            Some(_) => None,
        },
        Order::Defend(DefendTarget::Area { center, radius }) => {
            if !state.world.in_bounds(*center) {
                Some("Dieser Ort liegt außerhalb der Karte.".to_owned())
            } else if !super::DEFEND_RADIUS.contains(radius) {
                Some(format!("Der Umkreis muss {} bis {} Felder betragen.", super::DEFEND_RADIUS.start(), super::DEFEND_RADIUS.end()))
            } else {
                None
            }
        }
        Order::Patrol { route } => {
            let max = usize::from(rules.max_patrol_points);
            if route.len() < 2 || route.len() > max {
                Some(format!("Eine Patrouille braucht 2 bis {max} Wegpunkte."))
            } else if route.iter().any(|p| !state.world.in_bounds(*p) || !state.navigation.is_walkable(*p)) {
                Some("Ein Wegpunkt der Patrouille ist nicht begehbar.".to_owned())
            } else {
                None
            }
        }
        Order::Campaign { kingdom, siege } => super::campaign_problem(state, defs, company, *kingdom, *siege),
    }
}

/// Next company name: „1. Kompanie“, „2. Kompanie“ … (the lowest number not in use).
fn company_name(state: &GameState) -> String {
    let used: Vec<&str> = state.military.companies.iter().map(|c| c.name.as_str()).collect();
    (1..).map(|n| format!("{n}. Kompanie")).find(|n| !used.contains(&n.as_str())).unwrap_or_default()
}

/// Why `soldiers` cannot join company `company` (German), or `None`.
fn assign_problem(state: &GameState, defs: &Defs, soldiers: &[CitizenId], company: Option<u32>) -> Option<String> {
    if soldiers.is_empty() && company.is_none() {
        return Some("Es wurden keine Soldaten gewählt.".to_owned());
    }
    for id in soldiers {
        let Some(s) = state.military.soldiers.get(id) else { return Some("Dieser Bewohner ist kein Soldat.".to_owned()) };
        if !s.is_ready() {
            return Some("Soldaten in Ausbildung können noch keiner Kompanie angehören.".to_owned());
        }
        if s.crew_of.is_some() {
            return Some("Die Bedienmannschaft gehört zu ihrem Belagerungsgerät.".to_owned());
        }
        if s.away.is_some() {
            return Some("Dieser Soldat ist auf einem Feldzug.".to_owned());
        }
        if s.company.is_some_and(|c| state.military.campaigns.iter().any(|camp| camp.company == c)) {
            return Some("Die Kompanie dieses Soldaten ist auf einem Feldzug.".to_owned());
        }
    }
    if let Some(c) = company {
        let Some(co) = state.military.company(c) else { return Some("Diese Kompanie gibt es nicht.".to_owned()) };
        if state.military.campaigns.iter().any(|camp| camp.company == c) {
            return Some(format!("{} ist auf einem Feldzug.", co.name));
        }
        let joining = soldiers.iter().filter(|s| co.members.binary_search(s).is_err()).count();
        let max = usize::from(defs.rules().military.max_company_size);
        if co.members.len() + joining > max {
            return Some(format!("Eine Kompanie hat höchstens {max} Soldaten."));
        }
    }
    None
}

/// Moves `id` into company `company` (or out of any).
fn set_company(state: &mut GameState, id: CitizenId, company: Option<u32>) {
    let Some(old) = state.military.soldiers.get(&id).map(|s| s.company) else { return };
    if old == company {
        return;
    }
    if let Some(c) = old.and_then(|c| state.military.company_mut(c)) {
        c.members.retain(|m| *m != id);
    }
    if let Some(c) = company.and_then(|c| state.military.company_mut(c))
        && let Err(pos) = c.members.binary_search(&id)
    {
        c.members.insert(pos, id);
    }
    if let Some(s) = state.military.soldiers.get_mut(&id) {
        s.company = company;
        s.waypoint = 0;
    }
}

/// `CreateCompany`: a new company with `soldiers`.
pub(crate) fn create_company(
    state: &mut GameState,
    defs: &Defs,
    soldiers: &[CitizenId],
    events: &mut Vec<SimEvent>,
) -> Result<u32, String> {
    if state.military.companies.len() >= usize::from(defs.rules().military.max_companies) {
        return Err(format!("Es gibt höchstens {} Kompanien.", defs.rules().military.max_companies));
    }
    if let Some(p) = assign_problem(state, defs, soldiers, None).filter(|_| !soldiers.is_empty()) {
        return Err(p);
    }
    if soldiers.len() > usize::from(defs.rules().military.max_company_size) {
        return Err(format!("Eine Kompanie hat höchstens {} Soldaten.", defs.rules().military.max_company_size));
    }
    let id = state.military.next_company;
    state.military.next_company = id.wrapping_add(1).max(1);
    let name = company_name(state);
    state.military.companies.push(Company { id, name: name.clone(), members: Vec::new(), order: Order::Station });
    for s in soldiers {
        set_company(state, *s, Some(id));
    }
    events.push(SimEvent::CompanyChanged { company: id });
    Ok(id)
}

/// `AssignSoldiers`: soldiers join company `company` (or leave their company).
pub(crate) fn assign(
    state: &mut GameState,
    defs: &Defs,
    soldiers: &[CitizenId],
    company: Option<u32>,
    events: &mut Vec<SimEvent>,
) -> Result<(), String> {
    if let Some(p) = assign_problem(state, defs, soldiers, company) {
        return Err(p);
    }
    let mut touched: Vec<u32> = company.into_iter().collect();
    for s in soldiers {
        if let Some(old) = state.military.soldiers.get(s).and_then(|x| x.company) {
            touched.push(old);
        }
        set_company(state, *s, company);
    }
    touched.sort_unstable();
    touched.dedup();
    for c in touched {
        events.push(SimEvent::CompanyChanged { company: c });
    }
    Ok(())
}

/// `DisbandCompany`: its soldiers stay soldiers without a company.
pub(crate) fn disband(state: &mut GameState, company: u32, events: &mut Vec<SimEvent>) -> Result<(), String> {
    let Some(c) = state.military.company(company) else { return Err("Diese Kompanie gibt es nicht.".to_owned()) };
    if state.military.campaigns.iter().any(|camp| camp.company == company) {
        return Err(format!("{} ist auf einem Feldzug.", c.name));
    }
    let members = c.members.clone();
    for m in members {
        set_company(state, m, None);
    }
    state.military.companies.retain(|c| c.id != company);
    events.push(SimEvent::CompanyChanged { company });
    Ok(())
}

/// `CompanyOrder`: a new order (campaigns start here, a retreat turns a campaign back).
pub(crate) fn give_order(
    state: &mut GameState,
    defs: &Defs,
    company: u32,
    order: &Order,
    events: &mut Vec<SimEvent>,
) -> Result<(), String> {
    if let Some(p) = order_problem(state, defs, company, order) {
        return Err(p);
    }
    let campaigning = state.military.campaigns.iter().position(|camp| camp.company == company);
    match (order, campaigning) {
        (Order::Retreat, Some(i)) => super::campaign::turn_back(state, defs, i, events),
        (Order::Campaign { kingdom, siege }, _) => super::campaign::start(state, defs, company, *kingdom, *siege, events)?,
        _ => {}
    }
    if let Some(c) = state.military.company_mut(company) {
        c.order = order.clone();
    }
    for s in state.military.soldiers.values_mut().filter(|s| s.company == Some(company)) {
        s.waypoint = 0;
    }
    events.push(SimEvent::CompanyChanged { company });
    assign_duties(state, defs);
    Ok(())
}

/// Garrison capacity of a building.
fn garrison_of(state: &GameState, defs: &Defs, b: BuildingId) -> u16 {
    state
        .buildings
        .get(b)
        .filter(|bb| bb.is_complete())
        .and_then(|bb| defs.buildings().get(bb.kind().index()))
        .map_or(0, |d| u16::from(d.garrison))
}

/// Free walkable tiles around `rect` within 3 tiles, nearest first (ties: row, column).
fn ring_tiles(state: &GameState, rect: TileRect) -> Vec<IVec2> {
    let mut tiles: Vec<(i32, i32, i32, IVec2)> = rect
        .expanded(3)
        .tiles()
        .filter(|p| !rect.contains(*p) && state.navigation.is_walkable(*p) && state.buildings.at(*p).is_none())
        .map(|p| (rect.distance_to(p), p.y, p.x, p))
        .collect();
    tiles.sort_by_key(|(d, y, x, _)| (*d, *y, *x));
    tiles.into_iter().map(|(.., p)| p).collect()
}

/// Free walkable tiles within `radius` of `center`, nearest first.
fn area_tiles(state: &GameState, center: IVec2, radius: u8) -> Vec<IVec2> {
    let r = i32::from(radius);
    let mut tiles: Vec<(i32, i32, i32, IVec2)> = TileRect::tile(center)
        .expanded(r)
        .tiles()
        .filter(|p| state.navigation.is_walkable(*p) && state.buildings.at(*p).is_none())
        .map(|p| (tile_distance(p, center), p.y, p.x, p))
        .collect();
    tiles.sort_by_key(|(d, y, x, _)| (*d, *y, *x));
    tiles.into_iter().map(|(.., p)| p).collect()
}

/// Hands out places at buildings and in areas during one assignment pass.
#[derive(Default)]
struct Slots {
    garrison: BTreeMap<BuildingId, u16>,
    ring: BTreeMap<BuildingId, (Vec<IVec2>, usize)>,
    area: BTreeMap<(i32, i32, u8), (Vec<IVec2>, usize)>,
}

impl Slots {
    /// A place at building `b`: inside while its garrison has room, else around it.
    fn at_building(&mut self, state: &GameState, defs: &Defs, b: BuildingId) -> Option<Destination> {
        let bb = state.buildings.get(b)?;
        let used = self.garrison.entry(b).or_insert(0);
        if *used < garrison_of(state, defs, b) && bb.entrance_reachable(&state.world) {
            *used += 1;
            return Some(Destination::Building { building: b, purpose: Purpose::Work });
        }
        let (tiles, next) = self.ring.entry(b).or_insert_with(|| (ring_tiles(state, bb.rect()), 0));
        if tiles.is_empty() {
            return Some(Destination::Building { building: b, purpose: Purpose::Work });
        }
        let p = tiles[*next % tiles.len()];
        *next += 1;
        Some(Destination::Tile(p))
    }

    /// A place in an area.
    fn in_area(&mut self, state: &GameState, center: IVec2, radius: u8) -> Option<Destination> {
        let (tiles, next) = self.area.entry((center.x, center.y, radius)).or_insert_with(|| (area_tiles(state, center, radius), 0));
        if tiles.is_empty() {
            return None;
        }
        let p = tiles[*next % tiles.len()];
        *next += 1;
        Some(Destination::Tile(p))
    }
}

/// The building a soldier stands at without other orders: his post, else the headquarters.
fn post_of(state: &GameState, defs: &Defs, post: Option<BuildingId>) -> Option<BuildingId> {
    post.filter(|b| state.buildings.get(*b).is_some_and(|bb| bb.is_complete())).or_else(|| storage::main_headquarters(state, defs))
}

/// Where the soldier stands (inside a building: its entrance).
fn soldier_tile(state: &GameState, id: CitizenId) -> Option<(IVec2, Option<BuildingId>)> {
    let c = state.population.citizen(id)?;
    let m = c.motion();
    match m.inside().and_then(|b| state.buildings.get(b).map(|bb| (b, bb))) {
        Some((b, bb)) => Some((bb.entrance(), Some(b))),
        None => Some((standing_tile(m), None)),
    }
}

/// Derives the duty (and the enemy engaged) of every soldier and sends those whose duty
/// changed on their way (see the module documentation).
pub(crate) fn assign_duties(state: &mut GameState, defs: &Defs) {
    if state.military.soldiers.is_empty() {
        return;
    }
    crate::navigation::prepare(state, defs);
    let now = state.clock.tick;
    let r = &defs.rules().walking.routine;
    let rules = &defs.rules().military;
    let tod = state.clock.tick_of_day();
    // Soldiers serve during the working hours (in the evening they run errands and rest like
    // everybody else); while enemies threaten they are on duty day and night.
    let day = tod >= u64::from(r.work_start_hour) * TICKS_PER_HOUR && tod < u64::from(r.work_end_hour) * TICKS_PER_HOUR;
    let alert = super::alert(state);
    let hunt = now < state.military.alarm_until;
    let muster = now < state.military.muster_until;
    let hq = storage::main_headquarters(state, defs);
    let enemies: Vec<(TravelerId, IVec2)> = state
        .military
        .warbands
        .iter()
        .filter(|w| w.stage.is_hostile())
        .flat_map(|w| w.fighters.iter())
        .filter_map(|f| state.population.walking.traveler(f.walker).map(|t| (f.walker, t.motion().pos().tile())))
        .collect();
    // Patrols: a soldier at his waypoint heads for the next one.
    let ids: Vec<CitizenId> = state.military.soldiers.keys().copied().collect();
    for id in &ids {
        let Some(s) = state.military.soldiers.get(id) else { continue };
        let Some(Order::Patrol { route }) = s.company.and_then(|c| state.military.company(c)).map(|c| c.order.clone()) else {
            continue;
        };
        let at_point = state.population.citizen(*id).is_some_and(|c| c.motion().phase() == TripPhase::Still && c.motion().is_at_destination());
        if s.engage.is_none() && at_point && s.duty == route.get(usize::from(s.waypoint) % route.len()).map(|p| Destination::Tile(*p))
            && let Some(s) = state.military.soldiers.get_mut(id)
        {
            s.waypoint = u16::try_from((usize::from(s.waypoint) + 1) % route.len()).unwrap_or(0);
        }
    }
    let mustering: BTreeMap<CitizenId, IVec2> = state
        .military
        .campaigns
        .iter()
        .filter(|c| matches!(c.stage, super::CampaignStage::Mustering { .. }))
        .flat_map(|c| c.soldiers.iter().map(move |s| (*s, c.entry)))
        .collect();
    let mut slots = Slots::default();
    let mut plan: Vec<(CitizenId, Option<Destination>, Option<TravelerId>)> = Vec::with_capacity(ids.len());
    for id in &ids {
        let Some(s) = state.military.soldiers.get(id) else { continue };
        if s.away.is_some() || s.crew_of.is_some() {
            continue;
        }
        let max_hp = defs.units().get(s.unit.index()).map_or(100, |u| u.hit_points);
        let unit = defs.units().get(s.unit.index());
        let ranged = unit.is_some_and(|u| u.ranged > 0);
        let range = unit.map_or(0, |u| i32::from(u.range));
        let order = s.company.and_then(|c| state.military.company(c)).map(|c| c.order.clone());
        let wounded = u32::from(s.hp) * 100 < u32::from(max_hp) * u32::from(rules.retreat_health_percent) || s.morale < u16::from(rules.flee_morale);
        // Regular place.
        let regular = |slots: &mut Slots| -> Option<Destination> {
            if let SoldierStage::Training { building, .. } = s.stage {
                return (day || alert).then_some(Destination::Building { building, purpose: Purpose::Work });
            }
            if let Some(entry) = mustering.get(id) {
                return Some(Destination::Tile(*entry));
            }
            if !day && !alert {
                return None;
            }
            if wounded {
                return post_of(state, defs, s.post).and_then(|b| slots.at_building(state, defs, b));
            }
            if muster && let Some(hq) = hq {
                return slots.at_building(state, defs, hq);
            }
            match &order {
                Some(Order::Defend(DefendTarget::Building(b))) if state.buildings.get(*b).is_some() => slots.at_building(state, defs, *b),
                Some(Order::Defend(DefendTarget::Area { center, radius })) => slots.in_area(state, *center, *radius),
                Some(Order::Patrol { route }) if !route.is_empty() => {
                    Some(Destination::Tile(route[usize::from(s.waypoint) % route.len()]))
                }
                _ => post_of(state, defs, s.post).and_then(|b| slots.at_building(state, defs, b)),
            }
        };
        let base = regular(&mut slots);
        // Engagement.
        let mut duty = base;
        let mut engage = None;
        let can_fight = s.is_ready() && !wounded && !mustering.contains_key(id) && !matches!(order, Some(Order::Retreat)) && (day || alert);
        if can_fight && !enemies.is_empty() && let Some((own, inside)) = soldier_tile(state, *id) {
            let anchor: Option<(TileRect, i32)> = if hunt {
                Some((TileRect::tile(own), i32::MAX))
            } else {
                match &order {
                    Some(Order::Defend(DefendTarget::Area { center, radius })) => {
                        Some((TileRect::tile(*center), i32::from(rules.alert_radius) + i32::from(*radius)))
                    }
                    Some(Order::Patrol { .. }) => Some((TileRect::tile(own), i32::from(rules.patrol_alert_radius))),
                    Some(Order::Defend(DefendTarget::Building(b))) => {
                        state.buildings.get(*b).map(|bb| (bb.rect(), i32::from(rules.alert_radius)))
                    }
                    _ => post_of(state, defs, s.post).and_then(|b| state.buildings.get(b)).map(|bb| (bb.rect(), i32::from(rules.alert_radius))),
                }
            };
            if let Some((rect, radius)) = anchor {
                // Enemies in the guarded area; an enemy already engaged is pursued while it
                // stays within the patrol radius of the soldier.
                let pursuit = i32::from(rules.patrol_alert_radius);
                let foe = enemies
                    .iter()
                    .filter(|(t, p)| rect.distance_to(*p) <= radius || (s.engage == Some(*t) && tile_distance(*p, own) <= pursuit))
                    .map(|(t, p)| (tile_distance(*p, own), *t, *p))
                    .min_by_key(|(d, t, _)| (*d, *t));
                if let Some((d, t, p)) = foe {
                    engage = Some(t);
                    let inside_rect = inside.and_then(|b| state.buildings.get(b)).map(|bb| bb.rect());
                    let garrisoned = inside.is_some_and(|b| garrison_of(state, defs, b) > 0);
                    let reach = inside_rect.map_or(d, |r| r.distance_to(p));
                    if garrisoned && (ranged || reach <= 1) {
                        // Shoots or fights out of the building.
                    } else if (ranged && reach <= range) || reach <= 1 {
                        // In reach: hold the ground (out of a non-garrison building first).
                        duty = Some(Destination::Tile(if inside.is_some() { p } else { own }));
                    } else {
                        duty = Some(Destination::Tile(p));
                    }
                }
            }
        }
        plan.push((*id, duty, engage));
    }
    // Crews follow their engine.
    let leads: BTreeMap<CitizenId, Option<Destination>> = plan.iter().map(|(id, d, _)| (*id, *d)).collect();
    for id in &ids {
        if let Some(s) = state.military.soldiers.get(id)
            && s.away.is_none()
            && let Some(lead) = s.crew_of
        {
            let duty = match leads.get(&lead) {
                Some(Some(Destination::Building { building, purpose })) => Some(Destination::Building { building: *building, purpose: *purpose }),
                Some(Some(Destination::Tile(_))) => {
                    soldier_tile(state, lead).map(|(p, inside)| inside.map_or(Destination::Tile(p), |b| Destination::Building { building: b, purpose: Purpose::Work }))
                }
                Some(other) => *other,
                None => None,
            };
            plan.push((*id, duty, None));
        }
    }
    let mut venues = routine::Venues::default();
    for (id, duty, engage) in plan {
        let Some(s) = state.military.soldiers.get_mut(&id) else { continue };
        let changed = s.duty != duty || s.engage != engage;
        s.duty = duty;
        s.engage = engage;
        if changed {
            routine::update(state, defs, id, &mut venues);
        }
    }
}

/// German summary of a company's order („Verteidigt Wachturm“, „Patrouilliert (4 Wegpunkte)“).
pub fn order_text(state: &GameState, defs: &Defs, company: &Company) -> String {
    let kingdom = |k: KingdomId| state.diplomacy.kingdom(k).map_or_else(|| "?".to_owned(), |kk| kk.name.clone());
    match &company.order {
        Order::Station => "In Bereitschaft an ihren Posten".to_owned(),
        Order::Defend(DefendTarget::Building(b)) => format!("Verteidigt das Gebäude „{}“", building_name(state, defs, *b)),
        Order::Defend(DefendTarget::Area { center, radius }) => {
            format!("Hält das Gebiet um Feld {}, {} ({})", center.x, center.y, count_text(u32::from(*radius), "Feld", "Felder"))
        }
        Order::Patrol { route } => format!("Patrouilliert ({})", count_text(u32::try_from(route.len()).unwrap_or(0), "Wegpunkt", "Wegpunkte")),
        Order::Campaign { kingdom: k, siege: false } => format!("Feldzug gegen {}", kingdom(*k)),
        Order::Campaign { kingdom: k, siege: true } => format!("Belagerung von {}", kingdom(*k)),
        Order::Retreat => "Rückzug zu den Posten".to_owned(),
    }
}
