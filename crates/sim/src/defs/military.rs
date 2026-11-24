//! Military balancing (docs/game_design.md §11): unit classes, the defence values of
//! buildings, `rules.ron` → `military` ([`MilitaryRules`]: recruitment, pay, morale, combat,
//! bandits, AI armies, campaigns) and `rules.ron` → `ranks` ([`RankRule`], §1).

use serde::{Deserialize, Serialize};

use super::error::{Loc, Problems};
use super::progression::DifficultyValues;

/// Class of a unit type: its sprite role and how it fights.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum UnitClass {
    /// Foot soldiers fighting in melee.
    Infantry,
    /// Archers and crossbowmen shooting from a distance.
    Ranged,
    /// Riders.
    Cavalry,
    /// Siege engines with their crew.
    Siege,
}

impl UnitClass {
    /// All classes.
    pub const ALL: [UnitClass; 4] = [UnitClass::Infantry, UnitClass::Ranged, UnitClass::Cavalry, UnitClass::Siege];

    /// German display name (plural, for reports).
    pub const fn display_name(self) -> &'static str {
        match self {
            UnitClass::Infantry => "Fußvolk",
            UnitClass::Ranged => "Schützen",
            UnitClass::Cavalry => "Reiter",
            UnitClass::Siege => "Belagerungsgerät",
        }
    }
}

/// Extra attack of a unit type against a class (spearmen against riders).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClassBonus {
    /// Class the bonus applies to.
    pub against: UnitClass,
    /// Extra attack in percent.
    pub percent: u16,
}

/// Defence values of a building type (`defense` in `buildings.ron`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefenseDef {
    /// Ranged attack of the building itself per combat round (towers, archer posts, castle;
    /// 0 = it does not shoot).
    #[serde(default)]
    pub ranged: u16,
    /// Range of its shots in tiles.
    #[serde(default)]
    pub range: u8,
    /// Hit points of the structure against attackers.
    pub hit_points: u32,
}

/// Bandits of the raid event (§12): their combat values and the size of a band.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BanditRules {
    /// Melee attack.
    pub attack: u16,
    /// Defense.
    pub defense: u16,
    /// Hit points.
    pub hit_points: u16,
    /// Morale (0–100).
    pub morale: u16,
    /// Walking speed in permille of a citizen's.
    pub speed_permille: u16,
    /// Damage per combat round against walls and gates.
    pub building_damage: u16,
    /// Smallest band.
    pub min: u16,
    /// One bandit per this many citizens (on top of the minimum).
    pub citizens_per_bandit: u16,
    /// Largest band (before the difficulty).
    pub max: u16,
    /// Size of a band in percent of the normal size, per difficulty.
    pub size_percent: DifficultyValues<u16>,
    /// Bandits shun a defended town: one bandit fewer per this many defenders (ready soldiers
    /// on the map, finished towers, posts and castles with a ranged defence) …
    pub defenders_per_bandit: u16,
    /// … but at most this share of the band fewer (percent).
    pub max_deterrence_percent: u8,
    /// Fair pacing: no raid event before this day (0-based), per difficulty …
    pub grace_days: DifficultyValues<u16>,
    /// … and before the kingdom has this many inhabitants.
    pub grace_population: u16,
    /// Game hours between the scouts' first report (the raid event) and the band's arrival at
    /// the map edge …
    pub warning_hours: u8,
    /// … but at least this many hours after the player's decision.
    pub delay_hours: u8,
    /// Combat rounds the band (and a plundering army) needs at its target before it has the
    /// plunder.
    pub plunder_rounds: u8,
    /// Walking speed of a fighter carrying plunder, in permille of its normal speed.
    pub loaded_speed_permille: u16,
    /// Goods one bandit carries off from a storage.
    pub steal_units: u16,
    /// Gold one bandit takes from the treasury.
    pub steal_gold: u16,
    /// Chance in percent that the band sets the plundered storage on fire.
    pub fire_percent: u8,
    /// The band flees once it has lost this share of its members (percent).
    pub flee_losses_percent: u8,
}

/// Armies of AI kingdoms at war with the player.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArmyRules {
    /// Share of the kingdom's soldiers sent (percent).
    pub share_percent: u8,
    /// A kingdom with fewer soldiers sends no army.
    pub min_soldiers: u16,
    /// Most soldiers of one army.
    pub max_size: u16,
    /// Unit types of AI armies by weight (unit ids of `units.ron`).
    pub mix: Vec<(String, u16)>,
    /// Kingdoms at least this aggressive march on the headquarters (conquest); the others
    /// plunder the richest storage.
    pub conquest_aggression: u8,
    /// The army retreats once it has lost this share of its soldiers (percent).
    pub retreat_losses_percent: u8,
    /// Soldiers of an army attack the player's soldiers within this many tiles.
    pub aggro_radius: u8,
    /// Game hours an army withdrawing after a peace waits for a way off the map (walled in
    /// without a gate) before it disbands and its soldiers slip home one by one.
    pub withdraw_stuck_hours: u8,
    /// War exhaustion the kingdom gains when its army is beaten.
    pub exhaustion_defeat: u16,
    /// Relation points the kingdom remembers for a beaten army (negative).
    pub relation_defeat: i8,
}

/// Campaigns and sieges of the player's companies against kingdoms.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CampaignRules {
    /// Game hours the company has to gather at the map edge; whoever is not there stays home.
    pub muster_hours: u8,
    /// Share of the kingdom's soldiers in its field army (percent); the rest guards the seat.
    pub field_share_percent: u8,
    /// Most combat rounds of an abstract battle.
    pub max_rounds: u16,
    /// Share of the kingdom's treasury taken after a won field battle (percent).
    pub loot_percent: u8,
    /// War exhaustion the kingdom gains when it loses a field battle.
    pub exhaustion_victory: u16,
    /// Hit points of a kingdom's seat (walls and gates) per inhabitant.
    pub seat_hp_per_inhabitant: u16,
    /// Defense bonus of the seat's garrison behind its walls (percent).
    pub wall_defense_percent: u16,
    /// Game hours a siege takes.
    pub siege_hours: u16,
    /// Gold per inhabitant of the kingdom that it pays after a successful siege.
    pub siege_reparations_per_inhabitant: u16,
}

/// Recruitment, pay, morale, combat and threats (`rules.ron` → `military`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MilitaryRules {
    /// Recruits training at one building at the same time.
    pub training_slots: u8,
    /// Training speed without trainers (the building's workers), in percent.
    pub training_without_staff_percent: u8,
    /// Most companies.
    pub max_companies: u8,
    /// Most soldiers per company.
    pub max_company_size: u16,
    /// Most waypoints of a patrol route.
    pub max_patrol_points: u8,
    /// Ready soldiers react to enemies within this many tiles of their post or area.
    pub alert_radius: u8,
    /// Patrols react to enemies within this many tiles of themselves.
    pub patrol_alert_radius: u8,
    /// Morale points a soldier's morale moves towards its target per day.
    pub morale_change_per_day: u8,
    /// Morale target while the pay is not paid (bankrupt treasury).
    pub morale_unpaid: i8,
    /// Morale target while the soldier's household is not fed.
    pub morale_hungry: i8,
    /// Morale target per 10 happiness points above (or below) 50.
    pub morale_happiness_per_10: i8,
    /// Morale after a won fight (remembered, fades with the daily change).
    pub morale_victory: i8,
    /// Morale after a lost fight.
    pub morale_defeat: i8,
    /// Morale target below half equipment.
    pub morale_low_equipment: i8,
    /// Soldiers below this morale leave the fight.
    pub flee_morale: u8,
    /// Soldiers below this share of their health leave the fight (percent).
    pub retreat_health_percent: u8,
    /// Equipment lost per combat round fought (permille).
    pub wear_per_round_permille: u16,
    /// Equipment one refit restores (permille).
    pub refit_permille: u16,
    /// Health points regained per game hour at the post (doubled with a hospital in reach).
    pub heal_per_hour: u8,
    /// Food per soldier and day from the storages while the kingdom is at war or the soldier
    /// is on a campaign (thousandths of a unit).
    pub war_ration_milli: u32,
    /// Ticks between two combat rounds.
    pub round_ticks: u16,
    /// Damage of one hit: attack × `damage_factor` / (defense + `defense_offset`).
    pub damage_factor: u16,
    /// See `damage_factor`.
    pub defense_offset: u16,
    /// Random spread of every hit (± percent).
    pub variance_percent: u8,
    /// Defense bonus on forest tiles (percent).
    pub forest_defense_percent: u16,
    /// Defense bonus against attackers on lower ground (percent).
    pub height_defense_percent: u16,
    /// Defense bonus of defenders next to a wall or gate (percent).
    pub wall_defense_percent: u16,
    /// Defense bonus of soldiers inside a building (percent).
    pub garrison_defense_percent: u16,
    /// Extra range of shooters inside a building (tiles).
    pub garrison_range_bonus: u8,
    /// Experience per combat round fought.
    pub experience_per_round: u8,
    /// Experience per enemy killed.
    pub experience_per_kill: u8,
    /// Attack bonus at full experience (100), in percent.
    pub experience_attack_percent: u8,
    /// Hit points per footprint tile of buildings without own defence values.
    pub building_hp_per_tile: u32,
    /// Damage to structures repaired per day without enemies on the map (percent of their
    /// hit points).
    pub repair_percent_per_day: u8,
    /// Game hours an enemy army must hold the broken headquarters to conquer it.
    pub hq_capture_hours: u8,
    /// Own soldiers within this many tiles of the headquarters prevent its conquest.
    pub hq_defense_radius: u8,
    /// Bandits.
    pub bandits: BanditRules,
    /// AI armies.
    pub armies: ArmyRules,
    /// Campaigns and sieges.
    pub campaigns: CampaignRules,
}

impl MilitaryRules {
    /// Checks value ranges.
    pub(crate) fn validate(&self, p: &mut Problems, loc: &Loc<'_>) {
        p.check(self.training_slots > 0, loc, || "military.training_slots must be positive".into());
        p.check(self.training_without_staff_percent <= 100, loc, || "military.training_without_staff_percent must be <= 100".into());
        p.check(self.max_companies > 0 && self.max_company_size > 0, loc, || {
            "military: max_companies and max_company_size must be positive".into()
        });
        p.check((2..=32).contains(&self.max_patrol_points), loc, || "military.max_patrol_points must be 2..=32".into());
        p.check(self.alert_radius > 0 && self.patrol_alert_radius > 0, loc, || "military: alert radii must be positive".into());
        p.check(self.morale_change_per_day > 0, loc, || "military.morale_change_per_day must be positive".into());
        p.check(self.flee_morale < 100 && self.retreat_health_percent < 100, loc, || {
            "military: flee_morale and retreat_health_percent must be below 100".into()
        });
        p.check(self.wear_per_round_permille <= 1000 && self.refit_permille > 0 && self.refit_permille <= 1000, loc, || {
            "military: wear_per_round_permille <= 1000, refit_permille in 1..=1000".into()
        });
        p.check(self.round_ticks > 0 && self.damage_factor > 0 && self.defense_offset > 0, loc, || {
            "military: round_ticks, damage_factor and defense_offset must be positive".into()
        });
        p.check(self.variance_percent < 100, loc, || "military.variance_percent must be below 100".into());
        p.check(self.building_hp_per_tile > 0, loc, || "military.building_hp_per_tile must be positive".into());
        p.check(self.repair_percent_per_day <= 100, loc, || "military.repair_percent_per_day must be <= 100".into());
        p.check(self.hq_capture_hours > 0, loc, || "military.hq_capture_hours must be positive".into());
        let b = &self.bandits;
        p.check(b.attack > 0 && b.hit_points > 0 && (1..=100).contains(&b.morale) && b.speed_permille > 0, loc, || {
            "military.bandits: attack, hit_points, speed must be positive, morale 1..=100".into()
        });
        p.check(b.min > 0 && b.min <= b.max && b.citizens_per_bandit > 0 && b.max <= 60, loc, || {
            "military.bandits: 0 < min <= max <= 60, citizens_per_bandit > 0".into()
        });
        p.check(b.fire_percent <= 100 && (1..=100).contains(&b.flee_losses_percent), loc, || {
            "military.bandits: fire_percent <= 100, flee_losses_percent 1..=100".into()
        });
        let sizes = [b.size_percent.easy, b.size_percent.normal, b.size_percent.hard];
        p.check(sizes.iter().all(|v| (10..=200).contains(v)), loc, || "military.bandits.size_percent must be 10..=200".into());
        p.check(b.defenders_per_bandit > 0 && b.max_deterrence_percent <= 90, loc, || {
            "military.bandits: defenders_per_bandit > 0, max_deterrence_percent <= 90".into()
        });
        p.check(b.delay_hours > 0 && b.warning_hours >= b.delay_hours, loc, || {
            "military.bandits: delay_hours > 0 and warning_hours >= delay_hours".into()
        });
        let a = &self.armies;
        p.check(
            (1..=100).contains(&a.share_percent) && a.max_size > 0 && a.max_size <= 120 && a.min_soldiers > 0,
            loc,
            || "military.armies: share_percent 1..=100, 0 < max_size <= 120, min_soldiers > 0".into(),
        );
        p.check(!a.mix.is_empty() && a.mix.iter().map(|(_, w)| u32::from(*w)).sum::<u32>() > 0, loc, || {
            "military.armies.mix needs a positive total weight".into()
        });
        p.check(a.conquest_aggression <= 100 && (1..=100).contains(&a.retreat_losses_percent) && a.aggro_radius > 0, loc, || {
            "military.armies: conquest_aggression <= 100, retreat_losses_percent 1..=100, aggro_radius > 0".into()
        });
        p.check(a.withdraw_stuck_hours > 0, loc, || "military.armies.withdraw_stuck_hours must be positive".into());
        let c = &self.campaigns;
        p.check(c.muster_hours > 0 && c.field_share_percent <= 100 && c.max_rounds > 0 && c.loot_percent <= 100, loc, || {
            "military.campaigns: muster_hours > 0, field_share_percent <= 100, max_rounds > 0, loot_percent <= 100".into()
        });
        p.check(c.seat_hp_per_inhabitant > 0 && c.siege_hours > 0, loc, || {
            "military.campaigns: seat_hp_per_inhabitant and siege_hours must be positive".into()
        });
    }
}

/// A ruler rank (§1) with the requirements to reach it (`rules.ron` → `ranks`, in order; the
/// first rank is the start).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RankRule {
    /// Canonical id.
    pub id: String,
    /// German name.
    pub name: String,
    /// Inhabitants at least.
    #[serde(default)]
    pub population: u32,
    /// Wealth (treasury plus the stock at base prices) at least.
    #[serde(default)]
    pub wealth: u32,
    /// Finished buildings (without walls and gates) at least.
    #[serde(default)]
    pub buildings: u32,
    /// Territory tiles at least.
    #[serde(default)]
    pub territory: u32,
    /// A finished building of this type (building id) is required.
    #[serde(default)]
    pub building: Option<String>,
    /// Prestige: relation points with every AI kingdom (§10).
    #[serde(default)]
    pub prestige: i8,
    /// Court upkeep (Hofhaltung): gold per day the ruler of this rank spends on officials,
    /// envoys and the court (§7; booked as upkeep).
    #[serde(default)]
    pub court_upkeep: u32,
}

/// Checks the rank list: at least one rank, unique ids, requirements and prestige never fall.
pub(crate) fn validate_ranks(ranks: &[RankRule], p: &mut Problems, loc: &Loc<'_>) {
    p.check(!ranks.is_empty(), loc, || "ranks: at least the start rank is required".into());
    for (i, r) in ranks.iter().enumerate() {
        p.check(!r.id.is_empty() && !r.name.trim().is_empty(), loc, || format!("ranks[{i}]: id and name must not be empty"));
        p.check(ranks.iter().filter(|o| o.id == r.id).count() == 1, loc, || format!("ranks: id '{}' is used twice", r.id));
        if let Some(prev) = i.checked_sub(1).and_then(|j| ranks.get(j)) {
            let rising = r.population >= prev.population
                && r.wealth >= prev.wealth
                && r.buildings >= prev.buildings
                && r.territory >= prev.territory
                && r.prestige >= prev.prestige
                && r.court_upkeep >= prev.court_upkeep;
            p.check(rising, loc, || format!("ranks: '{}' must not require less than '{}'", r.id, prev.id));
        }
    }
    if let Some(first) = ranks.first() {
        let empty = first.population == 0 && first.wealth == 0 && first.buildings == 0 && first.territory == 0 && first.building.is_none();
        p.check(empty, loc, || "ranks: the first rank is the start and has no requirements".into());
    }
}

/// A ruler rank with its required building resolved (see [`RankRule`]); `Defs::ranks`.
#[derive(Clone, Debug, PartialEq)]
pub struct RankDef {
    /// Position in the rank list (0 = start).
    pub index: u8,
    /// Canonical id.
    pub key: String,
    /// German name.
    pub name: String,
    /// Inhabitants at least.
    pub population: u32,
    /// Wealth at least.
    pub wealth: u32,
    /// Finished buildings (without walls and gates) at least.
    pub buildings: u32,
    /// Territory tiles at least.
    pub territory: u32,
    /// Required finished building type.
    pub building: Option<super::ids::BuildingTypeId>,
    /// Relation points with every AI kingdom.
    pub prestige: i8,
    /// Court upkeep (Hofhaltung) in gold per day while this rank is held.
    pub court_upkeep: u32,
}

/// Resolves the rank list and the unit mix of AI armies (`rules.ron`); unknown building and
/// unit ids are reported.
pub(crate) fn resolve_military(
    ctx: &mut super::loader::Ctx,
    rules: &super::rules::GameRules,
    file: &str,
) -> (Vec<RankDef>, Vec<(super::ids::UnitTypeId, u16)>) {
    let loc = Loc { file, kind: "rules", key: "ranks" };
    let ranks = rules
        .ranks
        .iter()
        .enumerate()
        .map(|(i, r)| RankDef {
            index: u8::try_from(i).unwrap_or(u8::MAX),
            key: r.id.clone(),
            name: r.name.clone(),
            population: r.population,
            wealth: r.wealth,
            buildings: r.buildings,
            territory: r.territory,
            building: r.building.as_deref().and_then(|b| ctx.building(&loc, b, "ranks")),
            prestige: r.prestige,
            court_upkeep: r.court_upkeep,
        })
        .collect::<Vec<_>>();
    ctx.problems.check(ranks.len() <= 16, &loc, || "ranks: at most 16 ranks".into());
    let loc = Loc { file, kind: "rules", key: "military.armies.mix" };
    let mix = rules
        .military
        .armies
        .mix
        .iter()
        .filter_map(|(unit, weight)| ctx.unit(&loc, unit, "army mix").map(|u| (u, *weight)))
        .collect();
    (ranks, mix)
}
