//! Consequences of event choices: applying them and describing them in German beforehand.

use glam::IVec2;
use rand::RngExt;

use super::conditions::{is_wooden, near_river};
use super::{ActiveModifier, ModifierKind, ModifierSource, TaxCap};
use crate::buildings::BuildingId;
use crate::defs::{BuildingFilter, DamageCause, Defs, EventDef, EventDefId, EventEffect, ResourceAmount, ResourceCategory, ServiceKind};
use crate::economy::{LedgerCategory, storage};
use crate::event::{Notification, Severity, SimEvent};
use crate::population::{CENTI, ServiceIndex};
use crate::state::GameState;
use crate::time::TICKS_PER_DAY;
use crate::world::{Deposit, DepositKind, Vegetation};

/// What effects of one choice share: the event, its focus building and map position, the
/// tick it occurred and the arrival of the bandits a `Raiders` effect sent (an `Alarm` after
/// it lasts from their arrival on).
pub(super) struct EffectCtx {
    pub event: EventDefId,
    pub focus: Option<BuildingId>,
    pub position: Option<IVec2>,
    pub started_tick: u64,
    pub raid_arrival: Option<u64>,
}

fn resource_name(defs: &Defs, resource: crate::defs::ResourceId) -> String {
    defs.resources().get(resource.index()).map_or_else(|| resource.to_string(), |r| r.name.clone())
}

pub(super) fn building_name(state: &GameState, defs: &Defs, id: BuildingId) -> String {
    state
        .buildings
        .get(id)
        .and_then(|b| defs.buildings().get(b.kind().index()))
        .map_or_else(|| "Gebäude".to_owned(), |d| d.name.clone())
}

fn amounts_text(defs: &Defs, list: &[ResourceAmount]) -> String {
    list.iter().map(|m| format!("{} {}", m.amount, resource_name(defs, m.resource))).collect::<Vec<_>>().join(", ")
}

/// Entrance of the main headquarters (where goods arrive and leave), else the start point.
fn hq_entrance(state: &GameState, defs: &Defs) -> IVec2 {
    storage::main_headquarters(state, defs).and_then(|id| state.buildings.get(id)).map_or(state.world.start, |b| b.entrance())
}

/// Buildings `filter` can hit now (finished, idle, not headquarters/walls/gates), in id order.
fn candidates(state: &GameState, defs: &Defs, filter: BuildingFilter, index: &ServiceIndex) -> Vec<BuildingId> {
    state
        .buildings
        .iter()
        .filter(|(_, b)| matches!(b.phase(), crate::buildings::BuildingPhase::Active))
        .filter_map(|(id, b)| {
            let def = defs.buildings().get(b.kind().index())?;
            if !crate::buildings::can_be_damaged(def) {
                return None;
            }
            let rect = b.rect();
            let hit = match filter {
                BuildingFilter::Any => true,
                BuildingFilter::Wooden => is_wooden(def),
                BuildingFilter::WoodenUnprotected => is_wooden(def) && !index.covers(ServiceKind::FireProtection, &rect),
                BuildingFilter::NearRiver { distance } => near_river(&state.world, rect, distance),
                BuildingFilter::Unguarded => !index.covers(ServiceKind::Safety, &rect),
                BuildingFilter::Fields => false,
            };
            hit.then_some(id)
        })
        .collect()
}

/// The building an event starts at: a random candidate of the first building-damaging effect
/// of its default choice (drawn from the events RNG), if any.
pub(super) fn pick_focus(state: &mut GameState, defs: &Defs, def: &EventDef) -> Option<BuildingId> {
    let filter = def.choices.first()?.effects.iter().find_map(|e| match e {
        EventEffect::DamageBuildings { filter, .. } if *filter != BuildingFilter::Fields => Some(*filter),
        _ => None,
    })?;
    let index = ServiceIndex::collect(state, defs);
    let list = candidates(state, defs, filter, &index);
    if list.is_empty() {
        return None;
    }
    let i = state.events.rng.random_range(0..list.len());
    Some(list[i])
}

/// The buildings `filter` hits, `count` at most: the focus first (or a random candidate), then
/// the nearest candidates to it; a fire spreads only within `events.fire_spread_radius`, the
/// wind carries it further downwind and holds it back upwind ([`fire_reach`]).
fn targets(
    state: &mut GameState,
    defs: &Defs,
    filter: BuildingFilter,
    count: u8,
    cause: DamageCause,
    focus: Option<BuildingId>,
) -> Vec<BuildingId> {
    let index = ServiceIndex::collect(state, defs);
    let list = candidates(state, defs, filter, &index);
    if list.is_empty() {
        return Vec::new();
    }
    let first = match focus.filter(|f| list.contains(f)) {
        Some(f) => f,
        None => list[state.events.rng.random_range(0..list.len())],
    };
    let Some(origin) = state.buildings.get(first).map(|b| b.rect()) else { return Vec::new() };
    let spread = i32::from(defs.rules().events.fire_spread_radius);
    let (wind, strength) = (state.weather.wind_vector_permille(), i32::from(state.weather.wind_strength()));
    let mut rest: Vec<(i32, BuildingId)> = list
        .into_iter()
        .filter(|id| *id != first)
        .filter_map(|id| state.buildings.get(id).map(|b| (b.rect(), id)))
        .filter_map(|(rect, id)| {
            let d = origin.distance_to_rect(&rect);
            if cause != DamageCause::Fire {
                return Some((d, id));
            }
            let (reach, rank) = fire_reach(spread, d, rect.center_x2() - origin.center_x2(), wind, strength);
            (d <= reach).then_some((rank, id))
        })
        .collect();
    rest.sort();
    std::iter::once(first).chain(rest.into_iter().map(|(_, id)| id)).take(usize::from(count)).collect()
}

/// How far a fire reaches towards a building `distance` tiles away in direction `offset_x2`
/// (centre to centre, doubled) with the wind (`wind`: unit vector in permille, `strength`
/// 0–100), and the building's rank (lower burns first): downwind (more than 45° off the wind's
/// back) the reach grows by up to two tiles (strength / 34) and the building ranks as nearer;
/// upwind in a strong wind (≥ 50) the reach shrinks by one tile.
pub(crate) fn fire_reach(spread: i32, distance: i32, offset_x2: IVec2, wind: IVec2, strength: i32) -> (i32, i32) {
    let along = i64::from(offset_x2.x) * i64::from(wind.x) + i64::from(offset_x2.y) * i64::from(wind.y);
    let length = i64::from(offset_x2.abs().element_sum().max(1)) * 1000;
    // cos(angle) > 0.7 (within ~45°) using the Manhattan length (conservative).
    let downwind = along * 10 > length * 7;
    let upwind = along * 10 < -length * 7;
    let bonus = strength / 34;
    if downwind {
        (spread + bonus, distance - bonus)
    } else if upwind && strength >= 50 {
        (spread - 1, distance + 1)
    } else {
        (spread, distance)
    }
}

/// Citizens that fall ill for `percent` of the population (researched `DiseaseResistance`
/// lowers the share).
fn sick_count(state: &GameState, defs: &Defs, percent: u8) -> usize {
    let resistance = u64::try_from(100 - state.research.disease_resistance_percent(defs)).unwrap_or(0);
    let n = state.population.len() as u64 * u64::from(percent) * resistance;
    usize::try_from(n.div_ceil(10_000)).unwrap_or(usize::MAX)
}

/// Units of `category` raiders would find in storages without safety coverage.
fn raidable(state: &GameState, defs: &Defs, category: ResourceCategory) -> Vec<(BuildingId, ResourceAmount)> {
    let index = ServiceIndex::collect(state, defs);
    let mut out = Vec::new();
    for (id, b) in state.buildings.iter() {
        if !storage::is_operational_storage(defs, b) || index.covers(ServiceKind::Safety, &b.rect()) {
            continue;
        }
        for m in b.inventory().iter() {
            if defs.resources().get(m.resource.index()).is_some_and(|r| r.category == category) {
                out.push((id, m));
            }
        }
    }
    out
}

/// Applies one effect; appends one German line describing what happened to `outcome`.
pub(super) fn apply(
    state: &mut GameState,
    defs: &Defs,
    effect: &EventEffect,
    ctx: &mut EffectCtx,
    events: &mut Vec<SimEvent>,
    outcome: &mut Vec<String>,
) {
    let tick = state.clock.tick;
    match effect {
        EventEffect::Gold(g) => {
            let gold = if *g >= 0 { i64::from(*g) } else { -i64::from(g.unsigned_abs()).min(state.economy.treasury().max(0)) };
            state.economy.book_gold(LedgerCategory::Events, gold);
            outcome.push(if gold >= 0 { format!("+{gold} Gold.") } else { format!("−{} Gold.", -gold) });
        }
        EventEffect::Resource { resource, amount } => {
            let at = hq_entrance(state, defs);
            let name = resource_name(defs, *resource);
            if *amount > 0 {
                let units = amount.unsigned_abs();
                storage::deposit_or_lose(state, defs, &[ResourceAmount { resource: *resource, amount: units }], at, events);
                outcome.push(format!("+{units} {name}."));
            } else {
                let wanted = amount.unsigned_abs().min(state.economy.available(*resource));
                let taken = storage::withdraw(state, defs, *resource, wanted, at);
                outcome.push(format!("−{taken} {name}."));
            }
        }
        EventEffect::LoseStock { category, percent } => {
            let mut lost = Vec::new();
            let mut robbed = Vec::new();
            for (id, m) in raidable(state, defs, *category) {
                let amount = m.amount * u32::from(*percent) / 100;
                if let Some(b) = state.buildings.get_mut(id) {
                    let taken = b.inventory.remove(m.resource, amount);
                    if taken > 0 {
                        lost.push(ResourceAmount { resource: m.resource, amount: taken });
                        if !robbed.contains(&id) {
                            robbed.push(id);
                        }
                    }
                }
            }
            let lost = crate::buildings::merge_amounts(lost);
            storage::refresh_stock(state, defs);
            if lost.is_empty() {
                outcome.push("Die Räuber fanden in unbewachten Lagern nichts.".to_owned());
            } else {
                outcome.push(format!("Verloren: {}.", amounts_text(defs, &lost)));
                robbed.sort();
                let position = robbed.first().and_then(|b| state.buildings.get(*b)).map(|b| b.entrance());
                if ctx.position.is_none() {
                    ctx.position = position;
                }
                events.push(SimEvent::StockLost { lost, buildings: robbed, position });
            }
        }
        EventEffect::Festival { days } => {
            add_modifier(state, ModifierKind::Festival, 0, *days, ctx.event);
            outcome.push(format!(
                "Festtag für {days} {}: Die Bewohner feiern nachmittags bei der Kirche oder auf dem Markt, statt zu arbeiten.",
                days_word(*days)
            ));
        }
        EventEffect::Satisfaction { amount, days } => {
            add_modifier(state, ModifierKind::Happiness, i16::from(*amount), *days, ctx.event);
            outcome.push(format!("Zufriedenheit {amount:+} für {days} {}.", days_word(*days)));
        }
        EventEffect::FieldYield { percent, days } => {
            add_modifier(state, ModifierKind::FieldYield, *percent, *days, ctx.event);
            outcome.push(format!("Feldertrag {percent:+} % für {days} {}.", days_word(*days)));
        }
        EventEffect::Production { percent, days } => {
            add_modifier(state, ModifierKind::Production, *percent, *days, ctx.event);
            outcome.push(format!("Produktion {percent:+} % für {days} {}.", days_word(*days)));
        }
        EventEffect::Weather { weather, days, intensity } => {
            let intensity = intensity.unwrap_or(defs.rules().events.weather_intensity);
            let today = state.clock.total_days();
            state.weather.force(*weather, intensity, *days, today, Some(ctx.event));
            events.push(SimEvent::WeatherChanged { weather: state.weather.current(), intensity: state.weather.intensity() });
            outcome.push(format!("Wetter: {} für {days} {}.", weather.display_name(), days_word(*days)));
        }
        EventEffect::Settlers(n) => {
            let arrived = crate::population::arrive_refugees(state, defs, *n, events);
            let homeless = arrived
                .iter()
                .filter_map(|h| state.population.household(*h))
                .filter(|h| h.home().is_none())
                .map(|h| u32::from(h.size()))
                .sum::<u32>();
            let mut line = format!("{n} Flüchtlinge ziehen zu.");
            if homeless > 0 {
                line.push_str(&format!(" {homeless} von ihnen finden noch keine Unterkunft."));
            }
            outcome.push(line);
        }
        EventEffect::Disease { percent } => {
            let count = sick_count(state, defs, *percent);
            let mut ids: Vec<_> = state.population.citizens().map(|(id, _)| id).collect();
            // Partial Fisher–Yates shuffle: the first `count` ids are a uniform random choice.
            let count = count.min(ids.len());
            for i in 0..count {
                let j = state.events.rng.random_range(i..ids.len());
                ids.swap(i, j);
            }
            let rules = defs.rules();
            let loss = u32::from(rules.events.plague_health_loss) * u32::from(CENTI);
            let hospital_loss = loss * u32::from(rules.population.hospital_disease_percent.min(100)) / 100;
            let mut sick_homes: std::collections::BTreeMap<BuildingId, u32> = std::collections::BTreeMap::new();
            for id in ids.iter().take(count) {
                if let Some(home) = state.population.citizen(*id).and_then(|c| state.population.household(c.household())).and_then(|h| h.home()) {
                    *sick_homes.entry(home).or_insert(0) += 1;
                }
            }
            for id in ids.into_iter().take(count) {
                let covered = state
                    .population
                    .citizen(id)
                    .and_then(|c| state.population.household(c.household()))
                    .is_some_and(|h| h.supply().has_service(ServiceKind::Health));
                if let Some(c) = state.population.citizens.get_mut(id) {
                    let lost = u16::try_from(if covered { hospital_loss } else { loss }).unwrap_or(u16::MAX);
                    c.health = c.health.saturating_sub(lost);
                }
            }
            let count = u32::try_from(count).unwrap_or(u32::MAX);
            // The focus: the home with the most sick (ties: the lower id).
            let focus = sick_homes.iter().max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0))).map(|(b, _)| *b);
            let position = focus.and_then(|b| state.buildings.get(b)).map(|b| b.entrance());
            if ctx.position.is_none() {
                ctx.position = position;
            }
            events.push(SimEvent::CitizensFellIll { citizens: count, homes: sick_homes.into_keys().collect(), position });
            outcome.push(match count {
                0 => "Niemand ist erkrankt.".to_owned(),
                1 => "Ein Bewohner ist erkrankt.".to_owned(),
                n => format!("{n} Bewohner sind erkrankt."),
            });
        }
        EventEffect::DamageBuildings { filter: BuildingFilter::Fields, count, .. } => {
            let near = ctx.position.unwrap_or_else(|| hq_entrance(state, defs));
            let hit = crate::agriculture::destroy_crops(state, defs, near, u32::from(*count));
            if hit > 0 {
                events.push(SimEvent::FieldsDamaged { fields: hit });
            }
            outcome.push(match hit {
                0 => "Kein Feld wurde getroffen.".to_owned(),
                1 => "Die Ernte eines Feldes ist vernichtet.".to_owned(),
                n => format!("Die Ernte von {n} Feldern ist vernichtet."),
            });
        }
        EventEffect::DamageBuildings { filter, count, cause } => {
            let hit = targets(state, defs, *filter, *count, *cause, ctx.focus);
            let mut names = Vec::new();
            let mut lost_all = Vec::new();
            let mut damaged = Vec::new();
            for id in hit {
                let name = building_name(state, defs, id);
                let at = state.buildings.get(id).map(|b| b.entrance());
                let Some(lost) = crate::buildings::damage_building(state, defs, id, *cause, events) else { continue };
                if ctx.position.is_none() {
                    ctx.position = at;
                }
                let text = format!("Das Gebäude „{name}“ ist {} und muss wiederaufgebaut werden.", cause.participle());
                let mut note = Notification::new(Severity::Warning, text);
                if let Some(at) = at {
                    note = note.at(at);
                }
                events.push(SimEvent::Notification(note));
                names.push(name);
                if !lost.is_empty() {
                    damaged.push(id);
                }
                lost_all.extend(lost);
            }
            let lost_all = crate::buildings::merge_amounts(lost_all);
            // One line per effect: the damaged buildings and the goods lost inside.
            let mut line = match names.as_slice() {
                [] => "Kein Gebäude wurde beschädigt.".to_owned(),
                [one] => format!("{one} ist {}.", cause.participle()),
                [rest @ .., last] => format!("{} und {last} sind {}.", rest.join(", "), cause.participle()),
            };
            if !lost_all.is_empty() {
                line.push_str(&format!(" Verloren: {}.", amounts_text(defs, &lost_all)));
                damaged.sort();
                let position = damaged.first().and_then(|b| state.buildings.get(*b)).map(|b| b.entrance());
                events.push(SimEvent::StockLost { lost: lost_all, buildings: damaged, position });
            }
            outcome.push(line);
        }
        EventEffect::SpawnDeposit { kinds, amount, radius } => match spawn_deposit(state, defs, kinds, *amount, *radius) {
            Some((kind, position, tiles)) => {
                events.push(SimEvent::DepositDiscovered { kind, position, tiles });
                events.push(SimEvent::Notification(
                    Notification::new(Severity::Good, format!("Neues Vorkommen entdeckt: {}.", kind.display_name())).at(position),
                ));
                let fields = if tiles == 1 { "1 Feld".to_owned() } else { format!("{tiles} Felder") };
                outcome.push(format!("Neues Vorkommen: {} ({fields} mit je {amount} Einheiten).", kind.display_name()));
            }
            None => outcome.push("Die Fundstelle erwies sich als wertlos.".to_owned()),
        },
        EventEffect::TaxRateCap { max_percent, days } => {
            let max = (*max_percent).min(defs.rules().max_tax_rate_percent);
            if state.policy.tax_rate_percent > max {
                state.policy.tax_rate_percent = max;
                events.push(SimEvent::TaxRateChanged { percent: max });
            }
            let until_tick = tick + u64::from(*days) * TICKS_PER_DAY;
            state.events.tax_cap = Some(TaxCap { max_percent: max, until_tick, source: ctx.event });
            outcome.push(format!("Steuersatz höchstens {max} % für {days} {}.", days_word(*days)));
        }
        EventEffect::Raiders { percent } => {
            let (size, arrive) = crate::military::spawn_bandits(state, defs, *percent, ctx.started_tick, events);
            if size > 0 {
                ctx.raid_arrival = Some(arrive);
            }
            let when = crate::report::deadline_clock_text(tick, arrive);
            outcome.push(match size {
                0 => "Die Räuber fanden keinen Weg ins Reich.".to_owned(),
                1 => format!("Ein Räuber nähert sich dem Reich (Ankunft gegen {when})."),
                n => format!("{n} Räuber nähern sich dem Reich (Ankunft gegen {when})."),
            });
        }
        EventEffect::Alarm { hours } => {
            // Soldiers sent against announced bandits stand by until they come and hunt them
            // for the hours after their arrival.
            let from = ctx.raid_arrival.unwrap_or(tick).max(tick);
            state.military.alarm_until = state.military.alarm_until.max(from + u64::from(*hours) * crate::time::TICKS_PER_HOUR);
            let n = state.military.available_count();
            let after = if from > tick { " nach ihrer Ankunft" } else { "" };
            outcome.push(format!("{} die Feinde{after} für {}.", at_start(&soldiers_with_verb(n, "jagt", "jagen")), hours_text(*hours)));
        }
        EventEffect::Muster { hours } => {
            state.military.muster_until = state.military.muster_until.max(tick + u64::from(*hours) * crate::time::TICKS_PER_HOUR);
            let n = state.military.available_count();
            outcome.push(format!("{} für {} vor dem Hauptquartier auf.", at_start(&soldiers_with_verb(n, "zieht", "ziehen")), hours_text(*hours)));
        }
    }
}

/// „1 Soldat jagt“, „3 Soldaten jagen“, „kein Soldat jagt“ (subject with its verb, for the
/// middle of a sentence; see [`at_start`]).
fn soldiers_with_verb(n: u32, singular: &str, plural: &str) -> String {
    match n {
        0 => format!("kein Soldat {singular}"),
        1 => format!("1 Soldat {singular}"),
        n => format!("{n} Soldaten {plural}"),
    }
}

/// The working buildings that shoot at the bandits, e.g. „, dazu 2 Wachtürme und 1 Burg“ (empty
/// without any: the warning must not promise towers that do not stand).
fn defences_text(state: &GameState, defs: &Defs) -> String {
    let parts: Vec<String> = crate::military::ranged_defences(state, defs)
        .into_iter()
        .filter_map(|(kind, n)| defs.buildings().get(kind.index()).map(|d| d.count_text(n)))
        .collect();
    match parts.split_last() {
        None => String::new(),
        Some((last, [])) => format!(", dazu {last}"),
        Some((last, rest)) => format!(", dazu {} und {last}", rest.join(", ")),
    }
}

/// `text` with its first letter capitalized (start of a sentence).
fn at_start(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
}

/// „8 h“ (the duration form of the game, [`crate::report::hours_text`]).
fn hours_text(hours: u8) -> String {
    crate::report::hours_text(u64::from(hours))
}

fn days_word(days: u16) -> &'static str {
    if days == 1 { "Tag" } else { "Tage" }
}

fn add_modifier(state: &mut GameState, kind: ModifierKind, amount: i16, days: u16, source: EventDefId) {
    push_modifier(state, kind, amount, days, ModifierSource::Event(source));
}

/// Adds a modifier of `kind` for `days` from now.
pub(super) fn push_modifier(state: &mut GameState, kind: ModifierKind, amount: i16, days: u16, source: ModifierSource) {
    let until_tick = state.clock.tick + u64::from(days) * TICKS_PER_DAY;
    state.events.modifiers.push(ActiveModifier { kind, amount, source, until_tick });
}

/// True if a new deposit may lie on `p`: land without building, road, field, boulder or
/// deposit.
fn deposit_spot(state: &GameState, p: IVec2) -> bool {
    state.world.get(p).is_some_and(|t| !t.is_water() && t.deposit.is_none() && !matches!(t.vegetation, Vegetation::Boulder))
        && state.buildings.at(p).is_none()
        && !state.roads.has_road(p)
        && state.agriculture.owner_of(p).is_none()
}

/// Places a deposit of one of `kinds` near the headquarters (clay only within 2 tiles of fresh
/// water), preferring owned land: a random centre within `radius` and its four neighbours on
/// the same level (`events.deposit_tiles` at most), `amount` units each; trees there are
/// cleared. Returns the kind, the centre and the tile count.
fn spawn_deposit(state: &mut GameState, defs: &Defs, kinds: &[DepositKind], amount: u16, radius: u16) -> Option<(DepositKind, IVec2, u8)> {
    let centre = storage::main_headquarters(state, defs).and_then(|id| state.buildings.get(id)).map_or(state.world.start, |b| {
        let c = b.rect().center_x2();
        IVec2::new(c.x / 2, c.y / 2)
    });
    let r = i32::from(radius);
    let near_water = |state: &GameState, p: IVec2| {
        (-2..=2).any(|dy| (-2..=2).any(|dx| state.world.get(p + IVec2::new(dx, dy)).is_some_and(|t| t.has_fresh_water())))
    };
    let start = state.events.rng.random_range(0..kinds.len().max(1));
    for k in 0..kinds.len() {
        let kind = kinds[(start + k) % kinds.len()];
        let spots: Vec<IVec2> = (-r..=r)
            .flat_map(|dy| (-r..=r).map(move |dx| IVec2::new(dx, dy)))
            .map(|d| centre + d)
            .filter(|p| deposit_spot(state, *p) && (kind != DepositKind::Clay || near_water(state, *p)))
            .collect();
        if spots.is_empty() {
            continue;
        }
        let owned: Vec<IVec2> = spots.iter().copied().filter(|p| state.territory.contains(*p)).collect();
        let pool = if owned.is_empty() { spots } else { owned };
        let at = pool[state.events.rng.random_range(0..pool.len())];
        let level = state.world.get(at).map_or(0, |t| t.elevation);
        let limit = usize::from(defs.rules().events.deposit_tiles.max(1));
        let tiles: Vec<IVec2> = [IVec2::ZERO, IVec2::X, IVec2::Y, IVec2::NEG_X, IVec2::NEG_Y]
            .into_iter()
            .map(|d| at + d)
            .filter(|p| deposit_spot(state, *p) && state.world.get(*p).is_some_and(|t| t.elevation == level))
            .take(limit)
            .collect();
        for p in &tiles {
            let tile = state.world.tile_mut(*p);
            tile.deposit = Some(Deposit { kind, amount });
            if matches!(tile.vegetation, Vegetation::Tree { .. } | Vegetation::Stump { .. } | Vegetation::Bush) {
                tile.vegetation = Vegetation::None;
            }
        }
        return Some((kind, at, u8::try_from(tiles.len()).unwrap_or(u8::MAX)));
    }
    None
}

/// German lines describing what choice `choice` of `def` would do now (estimates for losses
/// and illness from the current state; `focus` names the building an event starts at).
pub fn predicted_effects(state: &GameState, defs: &Defs, def: &EventDef, choice: usize, focus: Option<BuildingId>) -> Vec<String> {
    let Some(option) = def.choices.get(choice) else { return Vec::new() };
    // An alarm sent with announced bandits starts at their arrival.
    let raid = option.effects.iter().any(|e| matches!(e, EventEffect::Raiders { .. }));
    option.effects.iter().map(|e| predict(state, defs, e, focus, raid)).collect()
}

fn predict(state: &GameState, defs: &Defs, effect: &EventEffect, focus: Option<BuildingId>, raid: bool) -> String {
    match effect {
        EventEffect::Gold(g) if *g >= 0 => format!("+{g} Gold"),
        EventEffect::Gold(g) => format!("−{} Gold", g.unsigned_abs()),
        EventEffect::Resource { resource, amount } if *amount >= 0 => format!("+{amount} {}", resource_name(defs, *resource)),
        EventEffect::Resource { resource, amount } => format!("−{} {}", amount.unsigned_abs(), resource_name(defs, *resource)),
        EventEffect::LoseStock { category, percent } => {
            let units: u32 = raidable(state, defs, *category).iter().map(|(_, m)| m.amount * u32::from(*percent) / 100).sum();
            format!(
                "{percent} % der Waren der Art {} aus unbewachten Lagern gehen verloren (derzeit etwa {units} Einheiten)",
                category.display_name()
            )
        }
        EventEffect::Festival { days } => format!("Festtag für {days} {}: nachmittags feiern statt arbeiten", days_word(*days)),
        EventEffect::Satisfaction { amount, days } => format!("Zufriedenheit {amount:+} für {days} {}", days_word(*days)),
        EventEffect::FieldYield { percent, days } => format!("Feldertrag {percent:+} % für {days} {}", days_word(*days)),
        EventEffect::Production { percent, days } => format!("Produktion {percent:+} % für {days} {}", days_word(*days)),
        EventEffect::Weather { weather, days, .. } => format!("Wetter: {} für {days} {}", weather.display_name(), days_word(*days)),
        EventEffect::Settlers(n) => {
            let who = if *n == 1 { "1 Flüchtling zieht".to_owned() } else { format!("{n} Flüchtlinge ziehen") };
            let free = crate::population::free_housing(state, defs);
            let places = if free == 1 { "1 freier Wohnplatz".to_owned() } else { format!("{free} freie Wohnplätze") };
            format!("{who} zu (derzeit {places})")
        }
        EventEffect::Disease { percent } => match sick_count(state, defs, *percent) {
            1 => "Etwa 1 Bewohner erkrankt und verliert Gesundheit".to_owned(),
            n => format!("Etwa {n} Bewohner erkranken und verlieren Gesundheit"),
        },
        EventEffect::DamageBuildings { filter: BuildingFilter::Fields, count, .. } => {
            if *count == 1 {
                "Die Ernte eines Feldes wird vernichtet".to_owned()
            } else {
                format!("Die Ernte von bis zu {count} Feldern wird vernichtet")
            }
        }
        EventEffect::DamageBuildings { filter, count, cause } => {
            let one = *count == 1;
            let what = match (filter, one) {
                (BuildingFilter::Any | BuildingFilter::Fields, _) => "Gebäude",
                (BuildingFilter::Wooden, _) => "Holzgebäude",
                (BuildingFilter::WoodenUnprotected, _) => "Holzgebäude ohne Brunnen in der Nähe",
                (BuildingFilter::NearRiver { .. }, _) => "Gebäude am Fluss",
                (BuildingFilter::Unguarded, true) => "unbewachtes Gebäude",
                (BuildingFilter::Unguarded, false) => "unbewachte Gebäude",
            };
            let verb = match (cause, one) {
                (DamageCause::Fire, true) => "brennt nieder",
                (DamageCause::Fire, false) => "brennen nieder",
                (DamageCause::Flood, true) => "wird überschwemmt",
                (DamageCause::Flood, false) => "werden überschwemmt",
                (DamageCause::Storm | DamageCause::Attack, true) => "wird beschädigt",
                (DamageCause::Storm | DamageCause::Attack, false) => "werden beschädigt",
            };
            let first = focus.filter(|b| state.buildings.get(*b).is_some()).map(|b| format!(", zuerst das Gebäude „{}“", building_name(state, defs, b)));
            let how_many = if one { "Höchstens ein".to_owned() } else { format!("Bis zu {count}") };
            format!("{how_many} {what} {verb}{} – Wiederaufbau durch Bauarbeiter", first.unwrap_or_default())
        }
        EventEffect::SpawnDeposit { kinds, amount, .. } => {
            let names: Vec<&str> = kinds.iter().map(|k| k.display_name()).collect();
            format!("Neues Vorkommen: {} (je Feld {amount} Einheiten)", names.join(" oder "))
        }
        EventEffect::TaxRateCap { max_percent, days } => {
            format!("Steuersatz höchstens {max_percent} % für {days} {}", days_word(*days))
        }
        EventEffect::Raiders { percent } => {
            let size = u32::from(crate::military::bandit_band(state, defs)) * u32::from(*percent) / 100;
            let guarded = state.military.available_count();
            let band = match size.max(1) {
                1 => "Etwa 1 Räuber greift".to_owned(),
                n => format!("Etwa {n} Räuber greifen"),
            };
            // The scouts reported the band when the event occurred.
            let reported = state.events.pending.map_or(state.clock.tick, |p| p.started_tick);
            let arrive = crate::military::bandit_arrival(state, defs, reported);
            format!(
                "{band} gegen {} unbewachte Lager an; {} bereit{}",
                crate::report::deadline_clock_text(state.clock.tick, arrive),
                soldiers_with_verb(guarded, "steht", "stehen"),
                defences_text(state, defs)
            )
        }
        EventEffect::Alarm { hours } => {
            let hunters = match state.military.available_count() {
                0 => "Kein Soldat jagt".to_owned(),
                1 => "Der eine Soldat jagt".to_owned(),
                n => format!("Alle {n} Soldaten jagen"),
            };
            let after = if raid { " nach ihrer Ankunft" } else { "" };
            format!("{hunters} die Räuber{after} für {}", hours_text(*hours))
        }
        EventEffect::Muster { hours } => {
            format!(
                "{} für {} vor dem Hauptquartier auf",
                at_start(&soldiers_with_verb(state.military.available_count(), "zieht", "ziehen")),
                hours_text(*hours)
            )
        }
    }
}
