//! Military, threats, battles, wars and ranks for the UI (docs/game_design.md §1, §11), German
//! labels throughout.

use glam::IVec2;
use serde::{Deserialize, Serialize};

use super::walking::{minutes_text, ticks_to_minutes};
use crate::buildings::BuildingId;
use crate::defs::{Defs, ResourceAmount, UnitTypeId};
use crate::diplomacy::KingdomId;
use crate::military::{
    self, BattleKind, BattleOutcome, CampaignStage, Company, FighterKind, Order, Soldier, SoldierStage, WarbandKind,
    WarbandStage,
};
use crate::population::CitizenId;
use crate::ranks::{self, RankRequirement};
use crate::state::GameState;
use crate::time::GameClock;

/// „1 Soldat“ / „3 Soldaten“.
fn count(n: u32, one: &str, many: &str) -> String {
    if n == 1 { format!("1 {one}") } else { format!("{n} {many}") }
}

fn unit_name(defs: &Defs, unit: UnitTypeId) -> String {
    defs.units().get(unit.index()).map_or_else(|| unit.to_string(), |u| u.name.clone())
}

fn kingdom_name(state: &GameState, id: KingdomId) -> String {
    state.diplomacy().kingdom(id).map_or_else(|| "?".to_owned(), |k| k.name.clone())
}

fn building_name(state: &GameState, defs: &Defs, id: BuildingId) -> String {
    state
        .building(id)
        .and_then(|b| defs.buildings().get(b.kind().index()))
        .map_or_else(|| "Gebäude".to_owned(), |d| d.name.clone())
}

fn date(tick: u64) -> String {
    GameClock::at_tick(tick).display()
}

/// German time until `tick` („in 2 h 30 min“).
fn eta_text(state: &GameState, tick: u64) -> String {
    let left = tick.saturating_sub(state.clock().tick);
    if left == 0 { "jetzt".to_owned() } else { format!("in {}", minutes_text(ticks_to_minutes(left).max(1))) }
}

/// One soldier.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoldierRow {
    /// The citizen.
    pub citizen: CitizenId,
    /// Full name.
    pub name: String,
    /// Unit type.
    pub unit: UnitTypeId,
    /// Unit name.
    pub unit_name: String,
    /// German state („Bereit“, „In Ausbildung (40 %)“, „Auf Feldzug“, „Bedienmannschaft“).
    pub stage_text: String,
    /// Health in percent.
    pub health_percent: u8,
    /// Morale 0–100.
    pub morale: u16,
    /// Equipment in percent.
    pub equipment_percent: u8,
    /// Experience 0–100.
    pub experience: u16,
    /// Enemies killed.
    pub kills: u16,
    /// Company.
    pub company: Option<u32>,
    /// What he does now (German).
    pub activity_text: String,
    /// Map position (tile; inside a building: its door; away: `None`).
    pub position: Option<IVec2>,
}

/// Soldiers of one unit type.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitRow {
    /// Unit type.
    pub unit: UnitTypeId,
    /// Name.
    pub name: String,
    /// Class name („Fußvolk“, „Schützen“ …).
    pub class_name: String,
    /// Ready and on the map.
    pub ready: u32,
    /// In training.
    pub training: u32,
    /// Away on a campaign.
    pub away: u32,
    /// Average health in percent.
    pub health_percent: u8,
    /// Average morale.
    pub morale: u16,
    /// Average equipment in percent.
    pub equipment_percent: u8,
    /// Pay of all of them per day (gold).
    pub pay_per_day: u32,
}

/// A company.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanyRow {
    /// Id.
    pub id: u32,
    /// Name.
    pub name: String,
    /// Order.
    pub order: Order,
    /// German order text („Verteidigt Wachturm“).
    pub order_text: String,
    /// Members.
    pub members: Vec<SoldierRow>,
    /// German composition („5 Schwertkämpfer, 2 Bogenschützen“).
    pub units_text: String,
    /// Centre of the members on the map (`None`: none visible).
    pub position: Option<IVec2>,
    /// Average morale.
    pub morale: u16,
    /// Average equipment in percent.
    pub equipment_percent: u8,
    /// Average health in percent.
    pub health_percent: u8,
    /// Pay per day (gold).
    pub pay_per_day: u32,
    /// Its campaign, if any.
    pub campaign: Option<u32>,
}

/// An enemy group approaching or on the map.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreatRow {
    /// Id.
    pub warband: u32,
    /// German kind („Räuberbande“, „Heer von Eschenwald“).
    pub kind_text: String,
    /// The kingdom of an army.
    pub kingdom: Option<KingdomId>,
    /// Fighters.
    pub size: u16,
    /// German composition („6 Räuber“, „5 Miliz, 3 Speerträger“).
    pub composition_text: String,
    /// German stage („Im Anmarsch“, „Greift an“, „Auf der Flucht“).
    pub stage_text: String,
    /// Ticks until it arrives (`None`: on the map).
    pub eta_ticks: Option<u64>,
    /// German arrival („in 1 h 30 min“, „auf der Karte“).
    pub eta_text: String,
    /// Its target building.
    pub target: Option<BuildingId>,
    /// German target („Rathaus“).
    pub target_name: String,
    /// Where it is (entry tile before arrival, else the centre of its fighters).
    pub position: IVec2,
    /// Plunder carried.
    pub loot_units: u32,
}

/// A finished battle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BattleRow {
    /// Serial number.
    pub serial: u32,
    /// Kind.
    pub kind: BattleKind,
    /// German kind.
    pub kind_name: String,
    /// Outcome.
    pub outcome: BattleOutcome,
    /// German outcome.
    pub outcome_name: String,
    /// Enemy („Räuberbande“).
    pub enemy: String,
    /// German date of the end.
    pub date_text: String,
    /// Where.
    pub position: IVec2,
    /// Own soldiers that fought.
    pub own_units: u32,
    /// Own fallen.
    pub own_losses: u32,
    /// Enemies at the start.
    pub enemy_units: u32,
    /// Enemies fallen.
    pub enemy_losses: u32,
    /// Buildings damaged.
    pub buildings_damaged: u32,
    /// Goods and gold carried off.
    pub goods_lost: Vec<ResourceAmount>,
    /// Gold carried off.
    pub gold_lost: u32,
    /// German summary.
    pub text: String,
}

/// A campaign under way.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CampaignRow {
    /// Id.
    pub id: u32,
    /// Company.
    pub company: u32,
    /// Company name.
    pub company_name: String,
    /// Enemy kingdom.
    pub kingdom: KingdomId,
    /// Its name.
    pub kingdom_name: String,
    /// True for a siege.
    pub siege: bool,
    /// German stage.
    pub stage_text: String,
    /// Ticks until the stage ends.
    pub eta_ticks: u64,
    /// German time until the stage ends.
    pub eta_text: String,
    /// Fighting soldiers taking part.
    pub soldiers: u32,
}

/// The war with one kingdom.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WarStatusRow {
    /// The kingdom.
    pub kingdom: KingdomId,
    /// Its name.
    pub name: String,
    /// Days of war.
    pub days: u64,
    /// True if the player declared it.
    pub declared_by_player: bool,
    /// The kingdom's war exhaustion.
    pub exhaustion: u16,
    /// Exhaustion at which it offers peace.
    pub exhaustion_peace: u16,
    /// Armies it sent.
    pub armies_sent: u16,
    /// Its soldiers at home.
    pub enemy_soldiers: u32,
    /// Its armies approaching or on the map.
    pub threats: u32,
    /// Own campaigns against it.
    pub campaigns: u32,
    /// Battles against it won and lost.
    pub battles_won: u32,
    /// Battles lost.
    pub battles_lost: u32,
    /// German summary.
    pub text: String,
}

/// The military window (see the field docs).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MilitaryReport {
    /// Fighting soldiers (without crew).
    pub soldiers: u32,
    /// Crew of siege engines.
    pub crew: u32,
    /// Ready on the map.
    pub ready: u32,
    /// In training.
    pub in_training: u32,
    /// Away on campaigns.
    pub away: u32,
    /// Below the retreat health.
    pub wounded: u32,
    /// Pay per day (gold).
    pub pay_per_day: u32,
    /// The pay is not paid (bankrupt).
    pub unpaid: bool,
    /// Strength for diplomacy (soldiers and inhabitants).
    pub strength: u32,
    /// German strength summary.
    pub strength_text: String,
    /// Soldiers on alert (enemies near).
    pub alert: bool,
    /// German alarm or muster note.
    pub alarm_text: Option<String>,
    /// Soldiers by unit type.
    pub units: Vec<UnitRow>,
    /// Companies.
    pub companies: Vec<CompanyRow>,
    /// Ready soldiers without a company and recruits in training.
    pub unassigned: Vec<SoldierRow>,
    /// Enemy groups.
    pub threats: Vec<ThreatRow>,
    /// Battles, newest first.
    pub battles: Vec<BattleRow>,
    /// Wars of the player.
    pub wars: Vec<WarStatusRow>,
    /// Campaigns under way.
    pub campaigns: Vec<CampaignRow>,
}

fn soldier_row(state: &GameState, defs: &Defs, id: CitizenId, s: &Soldier) -> SoldierRow {
    let max = defs.units().get(s.unit().index()).map_or(100, |u| u.hit_points).max(1);
    let stage_text = if s.crew_of().is_some() {
        "Bedienmannschaft".to_owned()
    } else if s.away().is_some() {
        "Auf Feldzug".to_owned()
    } else {
        match s.stage() {
            SoldierStage::Training { done_milli, .. } => {
                let total = defs.units().get(s.unit().index()).map_or(1, |u| u.training_ticks.max(1)) * 1000;
                format!("In Ausbildung ({} %)", done_milli * 100 / total)
            }
            SoldierStage::Ready => "Bereit".to_owned(),
        }
    };
    let c = state.population().citizen(id);
    let position = c.filter(|_| s.away().is_none()).map(|c| c.motion().pos().tile());
    SoldierRow {
        citizen: id,
        name: state.population().full_name(id).unwrap_or_default(),
        unit: s.unit(),
        unit_name: unit_name(defs, s.unit()),
        stage_text,
        health_percent: u8::try_from(u32::from(s.hp()) * 100 / u32::from(max)).unwrap_or(100),
        morale: s.morale(),
        equipment_percent: u8::try_from(s.equipment_permille() / 10).unwrap_or(100),
        experience: s.experience(),
        kills: s.kills(),
        company: s.company(),
        activity_text: crate::population::activity_text(state, defs, id).unwrap_or_default(),
        position,
    }
}

/// German composition of a list of units.
fn composition(defs: &Defs, units: impl Iterator<Item = UnitTypeId>) -> String {
    let mut counts: Vec<(UnitTypeId, u32)> = Vec::new();
    for u in units {
        match counts.iter_mut().find(|(x, _)| *x == u) {
            Some(c) => c.1 += 1,
            None => counts.push((u, 1)),
        }
    }
    counts.sort();
    counts.iter().map(|(u, n)| military::unit_count_text(defs, *u, *n)).collect::<Vec<_>>().join(", ")
}

fn company_row(state: &GameState, defs: &Defs, c: &Company) -> CompanyRow {
    let m = state.military();
    let members: Vec<SoldierRow> =
        c.members().iter().filter_map(|id| m.soldier(*id).map(|s| soldier_row(state, defs, *id, s))).collect();
    let n = u32::try_from(members.len()).unwrap_or(0).max(1);
    let visible: Vec<IVec2> = members.iter().filter_map(|r| r.position).collect();
    let position = (!visible.is_empty()).then(|| visible.iter().copied().fold(IVec2::ZERO, |a, b| a + b) / visible.len() as i32);
    let pay: u32 = c
        .members()
        .iter()
        .filter_map(|id| m.soldier(*id))
        .map(|s| defs.units().get(s.unit().index()).map_or(0, |u| u.upkeep_per_day))
        .sum();
    CompanyRow {
        id: c.id(),
        name: c.name().to_owned(),
        order: c.order().clone(),
        order_text: military::order_text(state, defs, c),
        units_text: composition(defs, members.iter().map(|r| r.unit)),
        position,
        morale: u16::try_from(members.iter().map(|r| u32::from(r.morale)).sum::<u32>() / n).unwrap_or(0),
        equipment_percent: u8::try_from(members.iter().map(|r| u32::from(r.equipment_percent)).sum::<u32>() / n).unwrap_or(0),
        health_percent: u8::try_from(members.iter().map(|r| u32::from(r.health_percent)).sum::<u32>() / n).unwrap_or(0),
        pay_per_day: pay,
        campaign: m.campaigns().iter().find(|camp| camp.company() == c.id()).map(|camp| camp.id()),
        members,
    }
}

/// The enemy groups approaching or on the map.
pub fn threats_report(state: &GameState, defs: &Defs) -> Vec<ThreatRow> {
    state
        .military()
        .warbands()
        .iter()
        .map(|w| {
            let kind_text = match w.kind() {
                WarbandKind::Bandits => "Räuberbande".to_owned(),
                WarbandKind::Army { kingdom, .. } => format!("Heer von {}", kingdom_name(state, kingdom)),
            };
            let composition_text = match w.stage() {
                WarbandStage::Approaching { .. } => w
                    .planned()
                    .iter()
                    .map(|(k, n)| military::fighter_count_text(defs, *k, u32::from(*n)))
                    .collect::<Vec<_>>()
                    .join(", "),
                _ => {
                    let mut kinds: Vec<(FighterKind, u32)> = Vec::new();
                    for f in w.fighters() {
                        match kinds.iter_mut().find(|(k, _)| *k == f.kind()) {
                            Some(e) => e.1 += 1,
                            None => kinds.push((f.kind(), 1)),
                        }
                    }
                    kinds.sort();
                    kinds.iter().map(|(k, n)| military::fighter_count_text(defs, *k, *n)).collect::<Vec<_>>().join(", ")
                }
            };
            let (stage_text, eta_ticks, eta) = match w.stage() {
                WarbandStage::Approaching { arrive_tick } => {
                    ("Im Anmarsch".to_owned(), military::warband_eta(state, w), eta_text(state, arrive_tick))
                }
                WarbandStage::Attacking => ("Greift an".to_owned(), None, "auf der Karte".to_owned()),
                WarbandStage::Fleeing => ("Auf der Flucht".to_owned(), None, "auf der Karte".to_owned()),
                WarbandStage::Withdrawing => ("Zieht ab".to_owned(), None, "auf der Karte".to_owned()),
            };
            let tiles: Vec<IVec2> = w
                .fighters()
                .iter()
                .filter_map(|f| state.population().walking().traveler(f.walker()).map(|t| t.motion().pos().tile()))
                .collect();
            let position =
                if tiles.is_empty() { w.entry() } else { tiles.iter().copied().fold(IVec2::ZERO, |a, b| a + b) / tiles.len() as i32 };
            let target_name = match (w.kind(), w.target()) {
                (_, Some(t)) => building_name(state, defs, t),
                (WarbandKind::Army { conquest: true, .. }, None) => "Hauptquartier".to_owned(),
                (WarbandKind::Army { conquest: false, .. } | WarbandKind::Bandits, None) => "Lager".to_owned(),
            };
            ThreatRow {
                warband: w.id(),
                kind_text,
                kingdom: w.kingdom(),
                size: w.strength(),
                composition_text,
                stage_text,
                eta_ticks,
                eta_text: eta,
                target: w.target(),
                target_name,
                position,
                loot_units: w.fighters().iter().map(|f| f.loot().iter().map(|m| m.amount).sum::<u32>()).sum(),
            }
        })
        .collect()
}

/// Finished battles, newest first.
pub fn battle_reports(state: &GameState) -> Vec<BattleRow> {
    state
        .military()
        .battles()
        .map(|b| BattleRow {
            serial: b.serial,
            kind: b.kind,
            kind_name: b.kind.display_name().to_owned(),
            outcome: b.outcome,
            outcome_name: b.outcome.display_name().to_owned(),
            enemy: b.enemy.clone(),
            date_text: date(b.end_tick),
            position: b.position,
            own_units: b.tally.own_units,
            own_losses: b.tally.own_losses,
            enemy_units: b.enemy_units,
            enemy_losses: b.tally.enemy_losses,
            buildings_damaged: b.tally.buildings_damaged,
            goods_lost: b.tally.goods_lost.clone(),
            gold_lost: b.tally.gold_lost,
            text: b.text.clone(),
        })
        .collect()
}

/// Campaigns under way.
pub fn campaigns_report(state: &GameState, _defs: &Defs) -> Vec<CampaignRow> {
    let m = state.military();
    m.campaigns()
        .iter()
        .map(|c| {
            let until = c.stage().until_tick();
            CampaignRow {
                id: c.id(),
                company: c.company(),
                company_name: m.company(c.company()).map_or_else(String::new, |co| co.name().to_owned()),
                kingdom: c.kingdom(),
                kingdom_name: kingdom_name(state, c.kingdom()),
                siege: c.is_siege(),
                stage_text: c.stage().display_name().to_owned(),
                eta_ticks: until.saturating_sub(state.clock().tick),
                eta_text: eta_text(state, until),
                soldiers: u32::try_from(c.soldiers().iter().filter(|s| m.soldier(**s).is_some_and(|x| x.crew_of().is_none())).count())
                    .unwrap_or(0),
            }
        })
        .collect()
}

/// The player's wars.
pub fn war_status(state: &GameState, defs: &Defs) -> Vec<WarStatusRow> {
    let m = state.military();
    let peace = u16::from(defs.rules().diplomacy.war.exhaustion_peace);
    state
        .diplomacy()
        .kingdoms()
        .filter_map(|(id, k)| {
            let war = k.war()?;
            let days = state.clock().tick.saturating_sub(war.since_tick) / crate::time::TICKS_PER_DAY;
            let won = u32::try_from(m.battles().filter(|b| b.kingdom == Some(id) && b.outcome == BattleOutcome::Victory).count()).unwrap_or(0);
            let lost = u32::try_from(m.battles().filter(|b| b.kingdom == Some(id) && b.outcome == BattleOutcome::Defeat).count()).unwrap_or(0);
            let threats = u32::try_from(m.warbands().iter().filter(|w| w.kingdom() == Some(id)).count()).unwrap_or(0);
            let campaigns = u32::try_from(m.campaigns().iter().filter(|c| c.kingdom() == id).count()).unwrap_or(0);
            let text = format!(
                "Krieg mit {} {}; Kriegsmüdigkeit {} von {}; {} gewonnen, {} verloren.",
                k.name(),
                super::since_days_text(days),
                war.exhaustion,
                peace,
                count(won, "Schlacht", "Schlachten"),
                lost
            );
            Some(WarStatusRow {
                kingdom: id,
                name: k.name().to_owned(),
                days,
                declared_by_player: war.declared_by_player,
                exhaustion: war.exhaustion,
                exhaustion_peace: peace,
                armies_sent: war.raids,
                enemy_soldiers: k.soldiers(),
                threats,
                campaigns,
                battles_won: won,
                battles_lost: lost,
                text,
            })
        })
        .collect()
}

/// The military window.
pub fn military_report(state: &GameState, defs: &Defs) -> MilitaryReport {
    let m = state.military();
    let rules = &defs.rules().military;
    let mut units: Vec<UnitRow> = Vec::new();
    let (mut soldiers, mut crew, mut ready, mut training, mut away, mut wounded) = (0, 0, 0, 0, 0, 0);
    for (_, s) in m.soldiers() {
        if s.crew_of().is_some() {
            crew += 1;
            continue;
        }
        soldiers += 1;
        let def = defs.units().get(s.unit().index());
        let max = def.map_or(100, |u| u.hit_points).max(1);
        if u32::from(s.hp()) * 100 < u32::from(max) * u32::from(rules.retreat_health_percent) {
            wounded += 1;
        }
        let index = match units.iter().position(|r| r.unit == s.unit()) {
            Some(i) => i,
            None => {
                units.push(UnitRow {
                    unit: s.unit(),
                    name: unit_name(defs, s.unit()),
                    class_name: def.map_or("", |u| u.class.display_name()).to_owned(),
                    ready: 0,
                    training: 0,
                    away: 0,
                    health_percent: 0,
                    morale: 0,
                    equipment_percent: 0,
                    pay_per_day: 0,
                });
                units.len() - 1
            }
        };
        let row = &mut units[index];
        row.pay_per_day += def.map_or(0, |u| u.upkeep_per_day);
        if s.away().is_some() {
            row.away += 1;
            away += 1;
        } else if s.is_ready() {
            row.ready += 1;
            ready += 1;
        } else {
            row.training += 1;
            training += 1;
        }
    }
    for row in &mut units {
        let list: Vec<&Soldier> = m.soldiers().map(|(_, s)| s).filter(|s| s.unit() == row.unit && s.crew_of().is_none()).collect();
        let n = u32::try_from(list.len()).unwrap_or(0).max(1);
        let max = u32::from(defs.units().get(row.unit.index()).map_or(100, |u| u.hit_points).max(1));
        row.health_percent = u8::try_from(list.iter().map(|s| u32::from(s.hp()) * 100 / max).sum::<u32>() / n).unwrap_or(100);
        row.morale = u16::try_from(list.iter().map(|s| u32::from(s.morale())).sum::<u32>() / n).unwrap_or(0);
        row.equipment_percent = u8::try_from(list.iter().map(|s| u32::from(s.equipment_permille()) / 10).sum::<u32>() / n).unwrap_or(0);
    }
    units.sort_by_key(|r| r.unit);
    let pay_per_day = u32::try_from(-crate::military::upkeep_daily_pay_milli(state, defs) / crate::economy::MILLI_PER_GOLD).unwrap_or(0);
    let strength = crate::diplomacy::player_strength(state, defs);
    let strongest = state.diplomacy().kingdoms().map(|(_, k)| crate::diplomacy::kingdom_strength(k, defs)).max();
    let strength_text = match strongest {
        Some(s) => format!("Stärke {strength} (das stärkste Königreich: {s})"),
        None => format!("Stärke {strength}"),
    };
    let now = state.clock().tick;
    let alarm_text = if now < m.alarm_until() {
        Some(format!("Alarm: Alle Soldaten jagen die Feinde (bis {}).", date(m.alarm_until())))
    } else if now < m.muster_until() {
        Some(format!("Die Soldaten stehen vor dem Hauptquartier (bis {}).", date(m.muster_until())))
    } else {
        None
    };
    let unassigned: Vec<SoldierRow> = m
        .soldiers()
        .filter(|(_, s)| s.company().is_none() && s.crew_of().is_none())
        .map(|(id, s)| soldier_row(state, defs, id, s))
        .collect();
    MilitaryReport {
        soldiers,
        crew,
        ready,
        in_training: training,
        away,
        wounded,
        pay_per_day,
        unpaid: m.is_unpaid(),
        strength,
        strength_text,
        alert: military::alert_active(state),
        alarm_text,
        units,
        companies: m.companies().iter().map(|c| company_row(state, defs, c)).collect(),
        unassigned,
        threats: threats_report(state, defs),
        battles: battle_reports(state),
        wars: war_status(state, defs),
        campaigns: campaigns_report(state, defs),
    }
}

/// One unit type a military building can train.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecruitOption {
    /// Unit type.
    pub unit: UnitTypeId,
    /// Name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Class name.
    pub class_name: String,
    /// Gold per unit.
    pub cost_gold: u32,
    /// Equipment per unit.
    pub materials: Vec<ResourceAmount>,
    /// Citizens per unit.
    pub population: u8,
    /// Pay per day.
    pub upkeep: u32,
    /// Training time in game hours.
    pub training_hours: f32,
    /// Combat values (attack, defense, ranged, range).
    pub attack: u16,
    /// Defense.
    pub defense: u16,
    /// Ranged attack.
    pub ranged: u16,
    /// Range in tiles.
    pub range: u8,
    /// How many could be recruited now at once (0: see `problem`).
    pub max_now: u8,
    /// Why one cannot be recruited now (German), `None` if possible.
    pub problem: Option<String>,
}

/// The units a military building trains, with costs and what stops a recruitment.
pub fn recruit_options(state: &GameState, defs: &Defs, building: BuildingId) -> Vec<RecruitOption> {
    let Some(b) = state.building(building) else { return Vec::new() };
    let Some(def) = defs.buildings().get(b.kind().index()) else { return Vec::new() };
    let slots = defs.rules().military.training_slots;
    def.trains
        .iter()
        .filter_map(|u| defs.units().get(u.index()))
        .map(|u| {
            let problem = military::recruit_problem(state, defs, building, u.id, 1).map(|p| p.to_string());
            let max_now = if problem.is_some() {
                0
            } else {
                (1..=slots).rev().find(|n| military::recruit_problem(state, defs, building, u.id, *n).is_none()).unwrap_or(0)
            };
            RecruitOption {
                unit: u.id,
                name: u.name.clone(),
                description: u.description.clone(),
                class_name: u.class.display_name().to_owned(),
                cost_gold: u.cost.gold,
                materials: u.cost.materials.clone(),
                population: u.population,
                upkeep: u.upkeep_per_day,
                training_hours: u.training_hours,
                attack: u.attack,
                defense: u.defense,
                ranged: u.ranged,
                range: u.range,
                max_now,
                problem,
            }
        })
        .collect()
}

/// The next rank.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NextRankRow {
    /// Index.
    pub rank: u8,
    /// Name.
    pub name: String,
    /// Prestige it brings.
    pub prestige: i8,
    /// Requirements with progress (the „Aufgaben“).
    pub requirements: Vec<RankRequirement>,
    /// Overall progress 0–100 (the weakest requirement counts).
    pub percent: u8,
}

/// The end of a lost game.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameOverRow {
    /// Why.
    pub reason: ranks::DefeatReason,
    /// German text.
    pub text: String,
    /// German date.
    pub date_text: String,
}

/// Rank, tasks and the end of the game (the „Aufgaben“ panel).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RankReport {
    /// Current rank index.
    pub rank: u8,
    /// Its name („Dorf“).
    pub name: String,
    /// Prestige with every kingdom.
    pub prestige: i32,
    /// Wealth (treasury and stock at base prices).
    pub wealth: u64,
    /// The next rank and its requirements (`None` at the highest rank).
    pub next: Option<NextRankRow>,
    /// Ranks reached (name, German date).
    pub reached: Vec<(String, String)>,
    /// Set once the game is lost.
    pub game_over: Option<GameOverRow>,
}

/// Rank, tasks and the end of the game.
pub fn rank_report(state: &GameState, defs: &Defs) -> RankReport {
    let p = state.progress();
    let current = ranks::current_rank(state, defs);
    let next = defs.ranks().get(usize::from(p.rank()) + 1).map(|r| {
        let requirements = ranks::requirements(state, defs, r);
        let percent = requirements.iter().map(|q| q.percent).min().unwrap_or(100);
        NextRankRow { rank: r.index, name: r.name.clone(), prestige: r.prestige, requirements, percent }
    });
    RankReport {
        rank: p.rank(),
        name: current.map_or_else(String::new, |r| r.name.clone()),
        prestige: ranks::prestige(state, defs),
        wealth: ranks::wealth(state, defs),
        next,
        reached: p
            .reached()
            .iter()
            .filter_map(|(r, t)| defs.ranks().get(usize::from(*r)).map(|d| (d.name.clone(), date(*t))))
            .collect(),
        game_over: p.game_over().map(|g| GameOverRow { reason: g.reason, text: ranks::defeat_text(state, g.reason), date_text: date(g.tick) }),
    }
}

/// German stage of a campaign (for feeds).
pub fn campaign_stage_text(stage: CampaignStage) -> &'static str {
    stage.display_name()
}
