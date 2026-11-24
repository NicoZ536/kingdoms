//! Kingdoms and diplomacy (docs/game_design.md §10): the balancing numbers in `rules.ron` →
//! `diplomacy` ([`DiplomacyRules`]) and the kingdom data in `kingdoms.ron` ([`KingdomData`]:
//! realm names and rulers, the abstract economy profile, the town plan of AI settlements).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::error::{Loc, Problems};
use super::ids::{BuildingTypeId, ResourceId};
use super::loader::Ctx;
use crate::world::WorldSize;

/// Character of an AI kingdom: drives its initiatives, its trade interest and how it judges
/// proposals.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Personality {
    /// Händlerisch: seeks trade, brings far goods (spices, silk).
    Trader,
    /// Expansiv: grows its territory, quarrels at borders.
    Expansive,
    /// Friedlich: prefers pacts and gifts.
    Peaceful,
    /// Aggressiv: demands tribute and declares war when it feels stronger.
    Aggressive,
}

impl Personality {
    /// All personalities.
    pub const ALL: [Personality; 4] = [Personality::Trader, Personality::Expansive, Personality::Peaceful, Personality::Aggressive];

    /// German adjective (lower case, as in „händlerisch“).
    pub const fn display_name(self) -> &'static str {
        match self {
            Personality::Trader => "händlerisch",
            Personality::Expansive => "expansiv",
            Personality::Peaceful => "friedlich",
            Personality::Aggressive => "aggressiv",
        }
    }
}

/// One value per personality.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonalityValues<T> {
    /// Händlerisch.
    pub trader: T,
    /// Expansiv.
    pub expansive: T,
    /// Friedlich.
    pub peaceful: T,
    /// Aggressiv.
    pub aggressive: T,
}

impl<T> PersonalityValues<T> {
    /// Value for `personality`.
    pub fn get(&self, personality: Personality) -> &T {
        match personality {
            Personality::Trader => &self.trader,
            Personality::Expansive => &self.expansive,
            Personality::Peaceful => &self.peaceful,
            Personality::Aggressive => &self.aggressive,
        }
    }
}

/// One value per map size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldSizeValues<T> {
    /// Klein.
    pub small: T,
    /// Mittel.
    pub medium: T,
    /// Groß.
    pub large: T,
    /// Riesig.
    pub huge: T,
}

impl<T: Copy> WorldSizeValues<T> {
    /// Value for `size`.
    pub fn get(&self, size: WorldSize) -> T {
        match size {
            WorldSize::Small => self.small,
            WorldSize::Medium => self.medium,
            WorldSize::Large => self.large,
            WorldSize::Huge => self.huge,
        }
    }
}

/// Behaviour values of a personality (percentages 0–100 unless noted).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonalityRules {
    /// Weight when a new kingdom draws its personality.
    pub weight: u32,
    /// Attitude towards the player from the start (relation points, −100…+100).
    pub base_relation: i8,
    /// Soldiers per 100 inhabitants the kingdom keeps.
    pub soldiers_per_100: u16,
    /// Readiness to demand, threaten and attack.
    pub aggression: u8,
    /// Readiness to give gifts.
    pub generosity: u8,
    /// Interest in trade (agreements, caravans, offers).
    pub trade_interest: u8,
    /// Urge to grow territory (border incidents).
    pub expansion: u8,
}

/// Relation points and memories (docs/game_design.md §10).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationRules {
    /// Daily decay of remembered actions towards 0, in percent of their value.
    pub memory_decay_percent: u8,
    /// Daily decay of a remembered action at least, in points.
    pub memory_decay_min: f32,
    /// Remembered actions per kingdom at most (the weakest is forgotten first).
    pub max_memories: u8,
    /// While territories touch: relation points (expansive and aggressive kingdoms).
    pub border_tension_hard: i8,
    /// While territories touch: relation points (other kingdoms).
    pub border_tension_soft: i8,
    /// While at war with the player.
    pub war: i8,
    /// At war with a kingdom the player is at war with as well.
    pub common_enemy: i8,
    /// Gift: relation points per 100 gold of value.
    pub gift_points_per_100_gold: f32,
    /// Gifts are remembered up to this many points in total.
    pub gift_max: i8,
    /// Trade by caravan: points per caravan that traded.
    pub trade_points: f32,
    /// Trade is remembered up to this many points in total.
    pub trade_max: i8,
    /// The player paid a demanded tribute.
    pub tribute_paid: i8,
    /// The player refused a demand.
    pub demand_refused: i8,
    /// The player declined a proposal or an offer.
    pub proposal_declined: i8,
    /// The player demanded tribute from the kingdom.
    pub demanded: i8,
    /// The player declared war on the kingdom.
    pub war_declared: i8,
    /// The player declared war on a kingdom this one is friendly with or allied to.
    pub war_on_friend: i8,
    /// Peace was made.
    pub peace_made: i8,
    /// A border incident.
    pub border_incident: i8,
    /// The player broke a treaty with another kingdom.
    pub broken_word: i8,
    /// Price bonus per 10 relation points in percent (negative relations make trade dearer).
    pub price_percent_per_10: u8,
}

/// Treaties with the player.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreatyRules {
    /// Duration of a trade agreement in days.
    pub trade_days: u16,
    /// Duration of a non-aggression pact in days.
    pub pact_days: u16,
    /// Duration of an alliance in days.
    pub alliance_days: u16,
    /// Relation points while a trade agreement holds.
    pub trade_relation: i8,
    /// Relation points while a non-aggression pact holds.
    pub pact_relation: i8,
    /// Relation points while an alliance holds.
    pub alliance_relation: i8,
    /// Better prices with a trade agreement, in percent.
    pub trade_price_bonus_percent: u8,
    /// Better prices with an alliance, in percent.
    pub alliance_price_bonus_percent: u8,
    /// Relation points when the player cancels a running trade agreement.
    pub break_trade: i8,
    /// Relation points when the player cancels a running non-aggression pact.
    pub break_pact: i8,
    /// Relation points when the player cancels a running alliance.
    pub break_alliance: i8,
}

/// Initiatives and judgements of the AI kingdoms.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiRules {
    /// Hour of the day the kingdoms act.
    pub initiative_hour: u8,
    /// No initiative before this day (0-based).
    pub first_initiative_day: u16,
    /// Chance of an initiative per kingdom and day, in percent.
    pub initiative_chance_percent: u8,
    /// Game hours the player has to answer; then the proposal counts as declined.
    pub decision_hours: u16,
    /// Proposals, demands and offers waiting in the player's inbox at most (one per kingdom
    /// in any case); a peace offer of a kingdom at war always comes.
    pub max_open_proposals: u8,
    /// No proposal, demand or offer before this day (0-based; gifts and border incidents may
    /// come earlier).
    pub first_proposal_day: u16,
    /// Game hours between two new proposals of all kingdoms together at least …
    pub proposal_gap_hours: u16,
    /// … and in the first `early_days` days of a game this many.
    pub early_proposal_gap_hours: u16,
    /// Days (0-based) the longer gap applies.
    pub early_days: u16,
    /// No demand for tribute before this day.
    pub demand_first_day: u16,
    /// No AI kingdom declares war on the player before this day (grace period of a new
    /// settlement).
    pub war_grace_days: u16,
    /// No war between AI kingdoms before this day.
    pub ai_war_first_day: u16,
    /// A proposal is accepted at this score (trade agreement).
    pub trade_threshold: i16,
    /// … non-aggression pact.
    pub pact_threshold: i16,
    /// … alliance.
    pub alliance_threshold: i16,
    /// … paying a demanded tribute.
    pub tribute_threshold: i16,
    /// … peace.
    pub peace_threshold: i16,
    /// Strength ratio (percent of the player's) from which a kingdom demands or attacks.
    pub superiority_percent: u16,
    /// A demand asks for this share of the player's treasury, in percent …
    pub demand_percent: u8,
    /// … at least this much gold …
    pub demand_min: u32,
    /// … and at most this much.
    pub demand_max: u32,
    /// An AI gift is this share of its treasury, in percent …
    pub gift_percent: u8,
    /// … between this much …
    pub gift_min: u32,
    /// … and this much gold.
    pub gift_max: u32,
    /// Purchase offers pay this much above the kingdom's price, in percent.
    pub offer_premium_percent: u8,
    /// Sale offers are this much below the kingdom's price, in percent.
    pub sale_discount_percent: u8,
    /// Units of a purchase or sale offer at most.
    pub offer_units: u32,
}

/// War (abstract until the military phase).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WarRules {
    /// Strength per soldier.
    pub strength_per_soldier: u16,
    /// Inhabitants per point of strength (militia).
    pub inhabitants_per_strength: u16,
    /// An enemy kingdom raids the player every this many days.
    pub raid_interval_days: u8,
    /// Share of food and goods raiders take from every storage without safety coverage, in
    /// percent.
    pub raid_percent: u8,
    /// War exhaustion of an AI kingdom per day of war.
    pub exhaustion_per_day: u8,
    /// Extra exhaustion when a raid finds nothing to take.
    pub exhaustion_failed_raid: u8,
    /// From this exhaustion an AI kingdom offers peace.
    pub exhaustion_peace: u8,
    /// War weariness of the player's citizens: happiness points when a war starts …
    pub weariness_base: u8,
    /// … one more point every this many days of war …
    pub weariness_days_per_point: u8,
    /// … at most this many points.
    pub weariness_max: u8,
    /// Daily military losses of a kingdom at war with another, in permille of the enemy's
    /// soldiers.
    pub ai_losses_permille: u16,
    /// A war between AI kingdoms lasts at least this many days.
    pub ai_war_min_days: u8,
    /// Daily chance of peace between AI kingdoms after the minimum, in percent.
    pub ai_peace_chance_percent: u8,
    /// Territory radius the loser of a war cedes, in tiles.
    pub border_ceded: u8,
    /// Share of the loser's treasury paid as reparations, in percent.
    pub reparations_percent: u8,
}

/// Trade caravans (docs/game_design.md §7, §10).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaravanRules {
    /// Margin of trade with kingdoms in percent (sales earn the price minus it, purchases cost
    /// the price plus it).
    pub margin_percent: u8,
    /// Units a caravan carries at most.
    pub capacity: u32,
    /// An AI caravan carries at most this share of its kingdom's treasury, in percent.
    pub purse_percent: u8,
    /// Game hours an AI caravan stays at the trading post.
    pub trade_hours: u8,
    /// Hours between two caravans of a neutral kingdom …
    pub interval_neutral_hours: u16,
    /// … of a friendly or allied kingdom …
    pub interval_friendly_hours: u16,
    /// … with a trade agreement.
    pub interval_treaty_hours: u16,
    /// Hours until a kingdom without goods to trade checks again.
    pub retry_hours: u16,
    /// Off-map travel per leg: hours per 100 tiles between the trading post and the seat …
    pub travel_hours_per_100_tiles: u16,
    /// … at least this many hours.
    pub min_travel_hours: u16,
    /// Robbery risk per leg from a kingdom near the route at war with the player, in percent …
    pub robbery_war_percent: u8,
    /// … hostile to the player …
    pub robbery_hostile_percent: u8,
    /// … rival …
    pub robbery_rival_percent: u8,
    /// … at most this much in total.
    pub robbery_max_percent: u8,
    /// A kingdom lies near a route if its seat is at most its territory radius plus this many
    /// tiles from the line between the trading post and the partner's seat.
    pub route_margin: u16,
    /// The cart follows the merchant this many ticks later.
    pub cart_delay_ticks: u16,
    /// Hours a caravan walker may wait without a path before the caravan gives up.
    pub stuck_hours: u16,
}

/// All diplomacy numbers (`rules.ron` → `diplomacy`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiplomacyRules {
    /// AI kingdoms per map size (the scenario's `rival_kingdoms` is an upper limit).
    pub kingdoms: WorldSizeValues<u8>,
    /// Inhabitants of a new kingdom: drawn between these values.
    pub start_population_min: u32,
    /// See `start_population_min`.
    pub start_population_max: u32,
    /// Most inhabitants of a kingdom.
    pub max_population: u32,
    /// Territory radius in tiles: `base_radius + inhabitants / inhabitants_per_radius` …
    pub base_radius: u16,
    /// See `base_radius`.
    pub inhabitants_per_radius: u16,
    /// … at most this.
    pub max_radius: u16,
    /// Daily growth with full food supply, in permille.
    pub growth_permille: u16,
    /// Daily decline with less than half the food, in permille.
    pub starvation_permille: u16,
    /// Gold income per inhabitant and day.
    pub tax_per_inhabitant: f32,
    /// Gold per soldier and day.
    pub soldier_upkeep: f32,
    /// Starting treasury per inhabitant.
    pub start_gold_per_inhabitant: u32,
    /// A kingdom wants goods it has less than this many days of consumption of …
    pub reserve_days: u16,
    /// … and offers what exceeds this many days.
    pub surplus_days: u16,
    /// Stock beyond this many days of production or consumption leaves for distant markets.
    pub stock_limit_days: u16,
    /// Farm and pasture output in winter, in percent.
    pub winter_farm_percent: u8,
    /// Land tiles a kingdom needs of a source before it practises the crafts requiring it.
    pub craft_min_tiles: u16,
    /// Behaviour per personality.
    pub personalities: PersonalityValues<PersonalityRules>,
    /// Relations and memories.
    pub relations: RelationRules,
    /// Treaties.
    pub treaties: TreatyRules,
    /// AI initiatives and judgements.
    pub ai: AiRules,
    /// War.
    pub war: WarRules,
    /// Caravans.
    pub caravans: CaravanRules,
}

impl DiplomacyRules {
    /// Checks value ranges.
    pub(crate) fn validate(&self, p: &mut Problems, loc: &Loc<'_>) {
        for size in WorldSize::ALL {
            p.check(self.kingdoms.get(size) <= 7, loc, || "diplomacy.kingdoms: at most 7 kingdoms per map".into());
        }
        p.check(
            self.start_population_min > 0
                && self.start_population_min <= self.start_population_max
                && self.start_population_max <= self.max_population,
            loc,
            || "diplomacy: start population must be positive, min <= max <= max_population".into(),
        );
        p.check(self.inhabitants_per_radius > 0 && self.base_radius >= 6 && self.base_radius <= self.max_radius, loc, || {
            "diplomacy: territory radius needs base_radius >= 6, base_radius <= max_radius and inhabitants_per_radius > 0".into()
        });
        p.check(self.max_radius <= 40, loc, || "diplomacy.max_radius must be <= 40".into());
        for (name, v) in [("tax_per_inhabitant", self.tax_per_inhabitant), ("soldier_upkeep", self.soldier_upkeep)] {
            p.check(v.is_finite() && v >= 0.0, loc, || format!("diplomacy.{name} must be >= 0"));
        }
        p.check(self.reserve_days > 0 && self.reserve_days < self.surplus_days && self.surplus_days < self.stock_limit_days, loc, || {
            "diplomacy: 0 < reserve_days < surplus_days < stock_limit_days".into()
        });
        p.check(self.winter_farm_percent <= 100, loc, || "diplomacy.winter_farm_percent must be <= 100".into());
        p.check(Personality::ALL.iter().map(|k| u64::from(self.personalities.get(*k).weight)).sum::<u64>() > 0, loc, || {
            "diplomacy.personalities need a positive total weight".into()
        });
        for k in Personality::ALL {
            let r = self.personalities.get(k);
            p.check(
                [r.aggression, r.generosity, r.trade_interest, r.expansion].iter().all(|v| *v <= 100) && r.base_relation.abs() <= 100,
                loc,
                || format!("diplomacy.personalities.{k:?}: percentages must be <= 100"),
            );
        }
        let rel = &self.relations;
        p.check(rel.memory_decay_percent <= 100 && rel.memory_decay_min.is_finite() && rel.memory_decay_min > 0.0, loc, || {
            "diplomacy.relations: memory decay must be in (0, 100] %".into()
        });
        p.check(rel.max_memories > 0, loc, || "diplomacy.relations.max_memories must be positive".into());
        p.check(rel.gift_points_per_100_gold.is_finite() && rel.gift_points_per_100_gold >= 0.0, loc, || {
            "diplomacy.relations.gift_points_per_100_gold must be >= 0".into()
        });
        p.check(rel.trade_points.is_finite() && rel.trade_points >= 0.0, loc, || "diplomacy.relations.trade_points must be >= 0".into());
        let t = &self.treaties;
        p.check(t.trade_days > 0 && t.pact_days > 0 && t.alliance_days > 0, loc, || "diplomacy.treaties: durations must be positive".into());
        p.check(t.trade_price_bonus_percent <= 50 && t.alliance_price_bonus_percent <= 50, loc, || {
            "diplomacy.treaties: price bonuses must be <= 50 %".into()
        });
        let ai = &self.ai;
        p.check(ai.initiative_hour < 24, loc, || "diplomacy.ai.initiative_hour must be < 24".into());
        p.check(ai.initiative_chance_percent <= 100, loc, || "diplomacy.ai.initiative_chance_percent must be <= 100".into());
        p.check(ai.decision_hours > 0, loc, || "diplomacy.ai.decision_hours must be positive".into());
        p.check(ai.max_open_proposals > 0, loc, || "diplomacy.ai.max_open_proposals must be positive".into());
        p.check(ai.early_proposal_gap_hours >= ai.proposal_gap_hours, loc, || {
            "diplomacy.ai.early_proposal_gap_hours must not be shorter than proposal_gap_hours".into()
        });
        p.check(ai.demand_min <= ai.demand_max && ai.gift_min <= ai.gift_max, loc, || {
            "diplomacy.ai: demand and gift minimums must not exceed their maximums".into()
        });
        p.check(ai.demand_percent <= 100 && ai.gift_percent <= 100 && ai.sale_discount_percent < 100, loc, || {
            "diplomacy.ai: percentages must be <= 100".into()
        });
        p.check(ai.superiority_percent >= 100 && ai.offer_units > 0, loc, || {
            "diplomacy.ai: superiority_percent must be >= 100 and offer_units positive".into()
        });
        let w = &self.war;
        p.check(w.strength_per_soldier > 0 && w.inhabitants_per_strength > 0, loc, || "diplomacy.war: strength factors must be positive".into());
        p.check(w.raid_interval_days > 0 && w.raid_percent <= 100, loc, || "diplomacy.war: raid interval > 0, raid_percent <= 100".into());
        p.check(w.weariness_days_per_point > 0 && w.weariness_base <= w.weariness_max, loc, || {
            "diplomacy.war: weariness_days_per_point > 0 and weariness_base <= weariness_max".into()
        });
        p.check(w.exhaustion_peace > 0 && w.ai_peace_chance_percent <= 100 && w.reparations_percent <= 100, loc, || {
            "diplomacy.war: exhaustion_peace > 0, chances and percentages <= 100".into()
        });
        let c = &self.caravans;
        p.check(c.margin_percent < 100 && c.capacity > 0 && c.purse_percent <= 100 && c.trade_hours > 0, loc, || {
            "diplomacy.caravans: margin < 100, capacity > 0, purse_percent <= 100, trade_hours > 0".into()
        });
        p.check(
            c.interval_treaty_hours > 0 && c.interval_treaty_hours <= c.interval_friendly_hours && c.interval_friendly_hours <= c.interval_neutral_hours,
            loc,
            || "diplomacy.caravans: 0 < interval_treaty_hours <= interval_friendly_hours <= interval_neutral_hours".into(),
        );
        p.check(c.retry_hours > 0 && c.min_travel_hours > 0 && c.stuck_hours > 0, loc, || {
            "diplomacy.caravans: retry, travel and stuck hours must be positive".into()
        });
        p.check(c.robbery_max_percent <= 100, loc, || "diplomacy.caravans.robbery_max_percent must be <= 100".into());
    }
}

/// What a kingdom's land (or people) produce from (docs/game_design.md §10).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LandSource {
    /// Fertile grass and meadow tiles.
    Farmland,
    /// Other grass, meadow and steppe tiles.
    Pasture,
    /// Tiles with trees.
    Forest,
    /// Shallow and fresh water tiles.
    Water,
    /// Stone deposit tiles.
    Stone,
    /// Clay deposit tiles.
    Clay,
    /// Iron deposit tiles.
    Iron,
    /// Coal deposit tiles.
    Coal,
    /// Gold deposit tiles.
    Gold,
    /// Per 100 inhabitants (crafts).
    Inhabitants,
    /// Per 100 inhabitants of trading kingdoms only (goods from distant lands).
    FarTrade,
}

impl LandSource {
    /// All sources.
    pub const ALL: [LandSource; 11] = [
        LandSource::Farmland,
        LandSource::Pasture,
        LandSource::Forest,
        LandSource::Water,
        LandSource::Stone,
        LandSource::Clay,
        LandSource::Iron,
        LandSource::Coal,
        LandSource::Gold,
        LandSource::Inhabitants,
        LandSource::FarTrade,
    ];

    /// Position in [`LandSource::ALL`].
    pub const fn index(self) -> usize {
        self as usize
    }

    /// True for sources counted in tiles of the territory.
    pub const fn is_land(self) -> bool {
        !matches!(self, LandSource::Inhabitants | LandSource::FarTrade)
    }

    /// True for farm and pasture land (less output in winter).
    pub const fn is_farming(self) -> bool {
        matches!(self, LandSource::Farmland | LandSource::Pasture)
    }
}

/// A realm an AI kingdom can be: name and ruler.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RealmTemplate {
    /// Name of the realm („Eschenwald“).
    pub name: String,
    /// Name of the ruler („Adalbert“).
    pub ruler: String,
    /// Title of the ruler („König“, „Herzogin“).
    pub title: String,
}

/// Daily output of a kingdom from one source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProductionSource {
    /// Where it comes from.
    pub source: LandSource,
    /// The good.
    pub resource: ResourceId,
    /// Milli-units per day per tile (land) or per 100 inhabitants.
    pub per_unit_milli: u32,
    /// Only if the territory holds at least `craft_min_tiles` of this land source.
    pub requires: Option<LandSource>,
}

/// Daily consumption of a kingdom per 100 inhabitants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Consumption {
    /// The good.
    pub resource: ResourceId,
    /// Milli-units per 100 inhabitants and day.
    pub per_100_milli: u32,
}

/// Where in an AI settlement a structure stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TownZone {
    /// At the seat.
    Center,
    /// In the town around the seat (up to half the territory radius).
    Town,
    /// Between town and border (farms, mines).
    Fields,
    /// Close to the border (watchtowers).
    Border,
}

/// One kind of structure of the town plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TownPlanEntry {
    /// The building type whose sprite and footprint the structure uses.
    pub building: BuildingTypeId,
    /// Structures at least.
    pub min: u16,
    /// One more per this many inhabitants (0: only `min`).
    pub per_inhabitants: u32,
    /// Structures at most.
    pub max: u16,
    /// Only if the territory holds at least `craft_min_tiles` of this land source.
    pub needs: Option<LandSource>,
    /// Where it stands.
    pub zone: TownZone,
}

impl TownPlanEntry {
    /// Structures of this kind a kingdom of `inhabitants` wants.
    pub fn target(&self, inhabitants: u32) -> u16 {
        let extra = inhabitants.checked_div(self.per_inhabitants).unwrap_or(0);
        u16::try_from(u32::from(self.min) + extra).unwrap_or(u16::MAX).min(self.max)
    }
}

/// Resolved `kingdoms.ron`.
#[derive(Clone, Debug, PartialEq)]
pub struct KingdomData {
    /// Realms new kingdoms are drawn from (at least as many as kingdoms on the largest map).
    pub realms: Vec<RealmTemplate>,
    /// Production profile.
    pub production: Vec<ProductionSource>,
    /// Consumption per 100 inhabitants, in resource id order.
    pub consumption: Vec<Consumption>,
    /// Structures of AI settlements in building order.
    pub town_plan: Vec<TownPlanEntry>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawProduction {
    source: LandSource,
    resource: String,
    per_unit: f32,
    #[serde(default)]
    requires: Option<LandSource>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawTownPlanEntry {
    building: String,
    #[serde(default)]
    min: u16,
    #[serde(default)]
    per_inhabitants: u32,
    max: u16,
    #[serde(default)]
    needs: Option<LandSource>,
    zone: TownZone,
}

/// `kingdoms.ron` as written.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawKingdomData {
    realms: Vec<RealmTemplate>,
    production: Vec<RawProduction>,
    consumption: BTreeMap<String, f32>,
    town_plan: Vec<RawTownPlanEntry>,
}

/// Milli-units of a non-negative amount (rounded).
fn milli(v: f32) -> u32 {
    (f64::from(v) * 1000.0).round().clamp(0.0, f64::from(u32::MAX)) as u32
}

impl RawKingdomData {
    /// Resolves references and checks ranges; `max_kingdoms` is the most kingdoms of any map.
    pub(crate) fn resolve(&self, ctx: &mut Ctx, file: &str, max_kingdoms: u8) -> KingdomData {
        let loc = Loc { file, kind: "kingdoms", key: "" };
        ctx.problems.check(self.realms.len() >= usize::from(max_kingdoms), &loc, || {
            format!("at least {max_kingdoms} realms are needed (diplomacy.kingdoms)")
        });
        for r in &self.realms {
            ctx.problems.check(
                !r.name.trim().is_empty() && !r.ruler.trim().is_empty() && !r.title.trim().is_empty(),
                &loc,
                || "realms need a name, a ruler and a title".into(),
            );
        }
        let mut names: Vec<&str> = self.realms.iter().map(|r| r.name.as_str()).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        ctx.problems.check(names.len() == before, &loc, || "realm names must be unique".into());
        let mut production = Vec::with_capacity(self.production.len());
        for p in &self.production {
            ctx.problems.check(p.per_unit.is_finite() && p.per_unit > 0.0, &loc, || {
                format!("production of '{}' must be positive", p.resource)
            });
            ctx.problems.check(p.requires.is_none_or(LandSource::is_land), &loc, || {
                format!("production of '{}': `requires` must be a land source", p.resource)
            });
            if let Some(resource) = ctx.resource(&loc, &p.resource, "production") {
                production.push(ProductionSource { source: p.source, resource, per_unit_milli: milli(p.per_unit), requires: p.requires });
            }
        }
        let mut consumption = Vec::with_capacity(self.consumption.len());
        for (key, &v) in &self.consumption {
            ctx.problems.check(v.is_finite() && v > 0.0, &loc, || format!("consumption of '{key}' must be positive"));
            if let Some(resource) = ctx.resource(&loc, key, "consumption") {
                consumption.push(Consumption { resource, per_100_milli: milli(v) });
            }
        }
        consumption.sort_by_key(|c| c.resource);
        let mut town_plan = Vec::with_capacity(self.town_plan.len());
        for (i, e) in self.town_plan.iter().enumerate() {
            ctx.problems.check(e.max >= e.min && e.max > 0, &loc, || format!("town_plan entry '{}': max must be >= min and > 0", e.building));
            ctx.problems.check((i == 0) == (e.zone == TownZone::Center), &loc, || {
                "the first town_plan entry (and only it) stands in the zone Center: the seat".into()
            });
            ctx.problems.check(e.needs.is_none_or(LandSource::is_land), &loc, || {
                format!("town_plan entry '{}': `needs` must be a land source", e.building)
            });
            if let Some(building) = ctx.building(&loc, &e.building, "town_plan") {
                town_plan.push(TownPlanEntry {
                    building,
                    min: e.min,
                    per_inhabitants: e.per_inhabitants,
                    max: e.max,
                    needs: e.needs,
                    zone: e.zone,
                });
            }
        }
        if let Some(seat) = self.town_plan.first() {
            ctx.problems.check(seat.min == 1 && seat.max == 1, &loc, || "the seat (first town_plan entry) is exactly one structure".into());
        }
        KingdomData { realms: self.realms.clone(), production, consumption, town_plan }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_targets_grow_with_inhabitants_up_to_the_maximum() {
        let e = TownPlanEntry {
            building: BuildingTypeId::from_index(0).unwrap(),
            min: 2,
            per_inhabitants: 10,
            max: 6,
            needs: None,
            zone: TownZone::Town,
        };
        assert_eq!(e.target(0), 2);
        assert_eq!(e.target(25), 4);
        assert_eq!(e.target(1000), 6);
        assert_eq!(TownPlanEntry { per_inhabitants: 0, ..e }.target(1000), 2);
        assert_eq!(Personality::ALL.map(Personality::display_name), ["händlerisch", "expansiv", "friedlich", "aggressiv"]);
        assert!(LandSource::Farmland.is_farming() && !LandSource::Inhabitants.is_land());
    }
}
