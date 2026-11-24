//! The population as individual citizens (docs/game_design.md §6, pipeline step 7).
//!
//! ## Entities
//! * [`Citizen`]s live in an [`Arena`] with typed [`CitizenId`]s: German name, sex, birth day
//!   (age advances one year per season, §2), life stage, household, partner and parents, job,
//!   needs, health, happiness, education and the current [`Activity`].
//! * [`Household`]s (families) group citizens. A household lives in one house ([`Home`]) or is
//!   homeless (camping at the main headquarters), keeps a *pantry* of food, fuel and goods and
//!   inherits its family name to children.
//! * Houses ([`Home`]) list their households; their tier requirements are judged from the
//!   residents' needs and supplies (upgrades, and downgrades after
//!   `PopulationRules::downgrade_after_hours` of unmet requirements, §5.1).
//!
//! ## Simulation LOD (§6)
//! * every citizen is updated every [`CITIZEN_UPDATE_TICKS`] ticks, spread by arena index
//!   (needs, health, happiness, starvation and freezing; the routine's destination and the
//!   activity), walkers move every tick ([`movement`]),
//! * every household once per game hour, spread by arena index (eating, restocking the pantry
//!   from markets or storages, goods and fuel use, service coverage), and once per day at an
//!   hour given by its index (births, old age, disease, education, coming of age, emigration),
//! * every house once per game hour, spread by building index (tier requirements, downgrade),
//! * immigration once per hour, marriages once per day (midnight).
//!
//! Economic results do not depend on the spread: all rates are per game hour or day.
//!
//! ## Walking (§6 "Bewegung", step 3a)
//! Every citizen has a map position ([`movement`]): inside a building (hidden) or walking
//! between tile centres on paths from [`crate::navigation`]. The daily routine
//! ([`routine`]) picks destinations — home, workplace, school, market, tavern or church —
//! and citizens walk there; nobody teleports. Immigrants walk in from the map edge,
//! emigrants walk out as travellers, newborns appear inside their home, the dead are removed
//! where they are. Workers count as present only while at the workplace during their
//! working time ([`is_at_work`]). Renderers read walkers through [`citizens_in`] (spatial
//! index by chunk, [`spatial`]). Carriers, builders and farmers walking with goods and to
//! fields follow in step 3b on top of these types.

pub(crate) mod demography;
mod housing;
pub mod movement;
mod needs;
pub mod routine;
pub mod spatial;
mod stats;
mod supply;
pub mod wealth;
pub mod friends;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod life_tests;
#[cfg(test)]
mod walking_tests;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::buildings::BuildingId;
use crate::defs::{Defs, JobCategory, NeedKind, PopulationRules, ResourceAmount, ResourceId};
use crate::event::{Notification, SimEvent};
use crate::ids::{Arena, Id};
use crate::state::GameState;
use crate::time::{DAYS_PER_SEASON, TICKS_PER_HOUR};

pub(crate) use demography::{arrive_households, arrive_refugees, create_settlers, release_job};

/// At midnight every employed citizen who is not sick counts a day of work in the category of
/// the job (docs §6 Qualifikation; at most 255 days).
fn gain_experience(state: &mut GameState) {
    for (_, c) in state.population.citizens.iter_mut() {
        if let Some(job) = c.job
            && c.sick_since.is_none()
        {
            let days = &mut c.experience[job.category.index()];
            *days = days.saturating_add(1);
        }
    }
}

/// A soldier falls in battle: statistics (cause „Kampf“), notification, removal.
pub(crate) fn die_in_battle(state: &mut GameState, defs: &Defs, id: CitizenId, events: &mut Vec<SimEvent>) {
    demography::die(state, defs, id, DeathCause::Battle, events);
}
#[cfg(test)]
pub(crate) use demography::remove_citizen;
pub use housing::{
    HomeRequirements, NeedRequirement, RequirementStatus, free_housing, home_requirements, immigrant_places, previous_tier,
};
pub(crate) use housing::{home_capacity, sync_homes};
pub use needs::{HappinessFactor, HappinessPart, citizen_happiness_parts, training_permille, work_efficiency_permille};
pub(crate) use stats::close_day as close_stats_day;
pub use stats::{DayCounts, MAX_DAY_COUNT, PopulationDay, PopulationStats};
pub(crate) use supply::ServiceIndex;
pub use wealth::{WealthClass, wealth_class};
pub use movement::{
    Destination, Motion, NO_PATH_NOTICE_TICKS, Purpose, SUBTILE, SubPos, Traveler, TravelerId, TripPhase, WalkerId, Walking,
    entry_point,
};
pub(crate) use movement::run_movement;
pub use routine::{
    WorkArea, WorkWindow, absence_status, activity_text, building_work_window, current_activity, is_at_work, is_sick,
    on_duty_workers, present_workers, work_area, work_window,
};
pub use spatial::{CitizenRole, CitizenView, WalkerAction, citizen_view, citizens_in, role_of, walker_view};

/// Heating of a house in the last hourly update ([`Population::heating`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heating {
    /// Nobody lives there.
    Empty,
    /// Not winter: no fire needed.
    NotNeeded,
    /// Winter: the fire burns (every household had its fuel for the hour).
    Burning,
    /// Winter: a household ran out of firewood or coal and freezes.
    OutOfFuel,
}

impl Heating {
    /// True if smoke rises from the chimney.
    pub fn smoking(self) -> bool {
        self == Heating::Burning
    }

    /// German status line.
    pub fn text(self) -> &'static str {
        match self {
            Heating::Empty => "Unbewohnt",
            Heating::NotNeeded => "Heizen nicht nötig",
            Heating::Burning => "Der Ofen brennt",
            Heating::OutOfFuel => "Kein Brennstoff – die Bewohner frieren",
        }
    }
}

/// Handle of a citizen.
pub type CitizenId = Id<Citizen>;
/// Handle of a household.
pub type HouseholdId = Id<Household>;

/// Ticks between two updates of the same citizen (docs/game_design.md §6: every 20 ticks).
pub const CITIZEN_UPDATE_TICKS: u64 = 20;
/// Need, health and happiness values are stored in hundredths of a point (0–10 000).
pub const CENTI: u16 = 100;
/// Largest stored need, health or happiness value (100 points).
pub const MAX_VALUE: u16 = 100 * CENTI;

/// Sex of a citizen (relevant for families).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Sex {
    /// Male.
    Male,
    /// Female.
    Female,
}

impl Sex {
    /// German display name.
    pub const fn display_name(self) -> &'static str {
        match self {
            Sex::Male => "männlich",
            Sex::Female => "weiblich",
        }
    }
}

/// Life stage by age (docs/game_design.md §6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LifeStage {
    /// Younger than `adult_age`: goes to school, does not work.
    Child,
    /// Works, marries, has children.
    Adult,
    /// From `elder_age`: does not work.
    Elder,
}

impl LifeStage {
    /// All stages in order.
    pub const ALL: [LifeStage; 3] = [LifeStage::Child, LifeStage::Adult, LifeStage::Elder];

    /// Stage for an age in years.
    pub fn for_age(age: u32, rules: &PopulationRules) -> LifeStage {
        if age < u32::from(rules.adult_age) {
            LifeStage::Child
        } else if age < u32::from(rules.elder_age) {
            LifeStage::Adult
        } else {
            LifeStage::Elder
        }
    }

    /// German display name (singular).
    pub const fn display_name(self) -> &'static str {
        match self {
            LifeStage::Child => "Kind",
            LifeStage::Adult => "Erwachsen",
            LifeStage::Elder => "Alt",
        }
    }

    /// German display name (plural, for statistics).
    pub const fn plural_name(self) -> &'static str {
        match self {
            LifeStage::Child => "Kinder",
            LifeStage::Adult => "Erwachsene",
            LifeStage::Elder => "Alte",
        }
    }
}

/// Why a citizen died (statistics).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DeathCause {
    /// Old age.
    OldAge,
    /// Hunger.
    Starvation,
    /// Cold in winter.
    Freezing,
    /// Disease.
    Disease,
    /// Fell in battle (soldiers, docs/game_design.md §11).
    Battle,
}

impl DeathCause {
    /// All causes in statistics order.
    pub const ALL: [DeathCause; 5] =
        [DeathCause::OldAge, DeathCause::Starvation, DeathCause::Freezing, DeathCause::Disease, DeathCause::Battle];

    /// Position in [`DeathCause::ALL`].
    pub const fn index(self) -> usize {
        self as usize
    }

    /// German display name.
    pub const fn display_name(self) -> &'static str {
        match self {
            DeathCause::OldAge => "Alter",
            DeathCause::Starvation => "Hunger",
            DeathCause::Freezing => "Kälte",
            DeathCause::Disease => "Krankheit",
            DeathCause::Battle => "Kampf",
        }
    }
}

/// What a citizen is doing now: where it actually is within the daily routine of
/// docs/game_design.md §6 ([`routine`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Activity {
    /// At home at night (22:00–06:00).
    #[default]
    Sleeping,
    /// At the workplace during working time.
    Working,
    /// Pupils at school (08:00–13:00).
    School,
    /// Meals and free time at home, in a tavern or church.
    Leisure,
    /// Adult without a job at home during working hours.
    Idle,
    /// Lunch break at the workplace (12:00–13:00).
    LunchBreak,
    /// Errands at the market (18:00).
    Errand,
    /// On the way (or waiting for a path).
    Walking,
    /// No path to the destination; waits and tries again.
    NoPath,
    /// Soldier in training at a military building.
    Training,
    /// Soldier on duty (post, garrison, guard, patrol).
    OnDuty,
    /// Soldier fighting or chasing an enemy.
    Fighting,
    /// Soldier away on a campaign.
    Campaign,
    /// Sick at home (health below `sick_health`).
    Sick,
    /// Sick, cared for in a hospital.
    Treatment,
    /// Sheltering from enemies at home or in the headquarters.
    Sheltering,
    /// Celebrating a festival at the church or market.
    Festival,
}

impl Activity {
    /// German display name.
    pub const fn display_name(self) -> &'static str {
        match self {
            Activity::Sleeping => "Schläft",
            Activity::Working => "Arbeitet",
            Activity::School => "In der Schule",
            Activity::Leisure => "Freizeit",
            Activity::Idle => "Ohne Arbeit",
            Activity::LunchBreak => "Mittagspause",
            Activity::Errand => "Besorgungen",
            Activity::Walking => "Unterwegs",
            Activity::NoPath => "Findet keinen Weg",
            Activity::Training => "In Ausbildung",
            Activity::OnDuty => "Im Dienst",
            Activity::Fighting => "Im Kampf",
            Activity::Campaign => "Auf Feldzug",
            Activity::Sick => "Krank zu Hause",
            Activity::Treatment => "Im Krankenhaus",
            Activity::Sheltering => "In Sicherheit",
            Activity::Festival => "Feiert",
        }
    }
}

/// A job: the building a citizen works at (or builds) and the job category it was given for.
/// The category changes when a construction site becomes a building, which ends the job.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Employment {
    /// Workplace or construction site.
    pub building: BuildingId,
    /// Category of the job (builders: [`JobCategory::Construction`]).
    pub category: JobCategory,
}

/// The nine needs of a citizen, 0–100 points each (stored in hundredths).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Needs([u16; NeedKind::ALL.len()]);

impl Needs {
    /// Every need at `points` (0–100).
    pub fn uniform(points: u8) -> Self {
        Needs([u16::from(points.min(100)) * CENTI; NeedKind::ALL.len()])
    }

    /// Value of `need` in points (0–100).
    pub fn get(&self, need: NeedKind) -> u8 {
        (self.0[need_index(need)] / CENTI) as u8
    }

    /// Value of `need` in hundredths of a point.
    pub fn centi(&self, need: NeedKind) -> u16 {
        self.0[need_index(need)]
    }

    pub(crate) fn set_centi(&mut self, need: NeedKind, value: u16) {
        self.0[need_index(need)] = value.min(MAX_VALUE);
    }

    fn is_valid(&self) -> bool {
        self.0.iter().all(|v| *v <= MAX_VALUE)
    }
}

/// Position of `need` in [`NeedKind::ALL`].
pub const fn need_index(need: NeedKind) -> usize {
    need as usize
}

/// One inhabitant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Citizen {
    pub(crate) first_name: String,
    pub(crate) sex: Sex,
    /// Day (since game start, may be negative) of birth; one season of days = one year of age.
    pub(crate) birth_day: i64,
    pub(crate) household: HouseholdId,
    pub(crate) partner: Option<CitizenId>,
    pub(crate) mother: Option<CitizenId>,
    pub(crate) father: Option<CitizenId>,
    pub(crate) job: Option<Employment>,
    pub(crate) needs: Needs,
    pub(crate) health: u16,
    pub(crate) happiness: u16,
    /// School progress 0–100.
    pub(crate) education: u8,
    pub(crate) educated: bool,
    pub(crate) activity: Activity,
    /// Life stage at the last daily check (detects coming of age and retirement).
    pub(crate) stage: LifeStage,
    /// Day of the last birth given (mothers).
    pub(crate) last_birth_day: Option<i64>,
    /// Map position, trip and walking state.
    pub(crate) motion: Motion,
    /// Transport the citizen is doing (carriers, gatherers bringing their yield back).
    pub(crate) carry: Option<CarryTrip>,
    /// The unit type the citizen serves in as a soldier ([`crate::military`]): no civilian
    /// job, paid by the military, still a member of the household.
    pub(crate) soldier: Option<crate::defs::UnitTypeId>,
    /// Friends (mutual, sorted, at most `PopulationRules::max_friends`; [`friends`]).
    pub(crate) friends: Vec<CitizenId>,
    /// Days worked in each job category ([`JobCategory::ALL`] order, at most 255): skilled and
    /// educated jobs reach full productivity after `population.training.full_after_days`
    /// ([`training_permille`]).
    pub(crate) experience: [u8; JobCategory::ALL.len()],
    /// Tick the citizen fell ill (health below `sick_health`); `None` while healthy.
    pub(crate) sick_since: Option<u64>,
    /// On sick leave: ill for `sick_leave_after_hours`, the job is given up and no new one is
    /// taken until the citizen is healthy again.
    pub(crate) sick_leave: bool,
}

/// A transport a citizen is doing: the job of [`crate::logistics`] it belongs to and the goods
/// on the citizen (docs/game_design.md §7). Goods in transit live here, nowhere else.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CarryTrip {
    /// Id of the transport job ([`crate::logistics::TransportJob::id`]).
    pub(crate) job: u64,
    /// Goods carried (picked up at the source, handed over at the target).
    pub(crate) load: Option<ResourceAmount>,
    /// Tick loading or unloading at a building ends, or the next attempt to find a storage with
    /// room is due.
    pub(crate) ready_at: Option<u64>,
}

impl CarryTrip {
    /// Id of the transport job.
    pub fn job(&self) -> u64 {
        self.job
    }

    /// Goods carried right now.
    pub fn load(&self) -> Option<ResourceAmount> {
        self.load
    }
}

impl Citizen {
    /// First name.
    pub fn first_name(&self) -> &str {
        &self.first_name
    }

    /// Sex.
    pub fn sex(&self) -> Sex {
        self.sex
    }

    /// Day of birth (days since game start; negative for settlers born before).
    pub fn birth_day(&self) -> i64 {
        self.birth_day
    }

    /// Age in years on `day` (one year per season, docs/game_design.md §2).
    pub fn age_on(&self, day: u64) -> u32 {
        let days = i64::try_from(day).unwrap_or(i64::MAX).saturating_sub(self.birth_day);
        u32::try_from(days.div_euclid(DAYS_PER_SEASON as i64)).unwrap_or(0)
    }

    /// Life stage stored at the last daily check.
    pub fn stage(&self) -> LifeStage {
        self.stage
    }

    /// Household.
    pub fn household(&self) -> HouseholdId {
        self.household
    }

    /// Spouse.
    pub fn partner(&self) -> Option<CitizenId> {
        self.partner
    }

    /// Friends (sorted).
    pub fn friends(&self) -> &[CitizenId] {
        &self.friends
    }

    /// Mother.
    pub fn mother(&self) -> Option<CitizenId> {
        self.mother
    }

    /// Father.
    pub fn father(&self) -> Option<CitizenId> {
        self.father
    }

    /// Job, if employed.
    pub fn job(&self) -> Option<Employment> {
        self.job
    }

    /// Needs.
    pub fn needs(&self) -> &Needs {
        &self.needs
    }

    /// Health in points (0–100).
    pub fn health(&self) -> u8 {
        (self.health / CENTI) as u8
    }

    /// Happiness in points (0–100).
    pub fn happiness(&self) -> u8 {
        (self.happiness / CENTI) as u8
    }

    /// School progress (0–100).
    pub fn education(&self) -> u8 {
        self.education
    }

    /// True if the citizen may take educated jobs.
    pub fn is_educated(&self) -> bool {
        self.educated
    }

    /// Current activity.
    pub fn activity(&self) -> Activity {
        self.activity
    }

    /// True for adults of working age (children and elders do not work, §6).
    pub fn can_work(&self) -> bool {
        self.stage == LifeStage::Adult
    }

    /// True for adults of working age who may take a civilian job: soldiers serve in the
    /// military instead (docs/game_design.md §11), the sick on sick leave wait until they are
    /// healthy (§6).
    pub fn can_take_job(&self) -> bool {
        self.stage == LifeStage::Adult && self.soldier.is_none() && !self.sick_leave
    }

    /// Days worked in job category `category` (at most 255).
    pub fn experience_days(&self, category: JobCategory) -> u8 {
        self.experience[category.index()]
    }

    /// Tick the citizen fell ill (`None` while healthy).
    pub fn sick_since(&self) -> Option<u64> {
        self.sick_since
    }

    /// True while the citizen is on sick leave (gave the job up after a day of illness).
    pub fn is_on_sick_leave(&self) -> bool {
        self.sick_leave
    }

    /// True while the citizen serves as a soldier.
    pub fn is_soldier(&self) -> bool {
        self.soldier.is_some()
    }

    /// The unit type the citizen serves in (`None`: a civilian).
    pub fn soldier_unit(&self) -> Option<crate::defs::UnitTypeId> {
        self.soldier
    }

    /// Map position, trip and walking state.
    pub fn motion(&self) -> &Motion {
        &self.motion
    }

    /// Transport in progress (carriers, gatherers bringing their yield back).
    pub fn carry(&self) -> Option<&CarryTrip> {
        self.carry.as_ref()
    }

    /// Goods the citizen carries right now.
    pub fn carried(&self) -> Option<ResourceAmount> {
        self.carry.and_then(|t| t.load)
    }
}

/// Goods a household keeps: resource and amount in thousandths of a unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PantryItem {
    /// The resource.
    pub resource: ResourceId,
    /// Thousandths of a unit.
    pub milli: u32,
}

/// What a household was supplied with at its last hourly update (read by the citizen updates).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupplyState {
    /// Services reaching the home (bit per [`crate::defs::ServiceKind`] in declaration order).
    pub services: u16,
    /// Fresh water (river, lake) near the home.
    pub fresh_water: bool,
    /// Everybody could eat in the last hour.
    pub fed: bool,
    /// Kinds of food in the pantry.
    pub food_variety: u8,
    /// Fuel for the winter in the pantry.
    pub fuel: bool,
    /// Clothes in the pantry.
    pub clothes: bool,
    /// Goods requirements of the home's tier that are not supplied.
    pub missing_goods: u8,
    /// A tavern in reach has its goods (beer) in stock, or the pantry holds some.
    pub tavern_goods: bool,
}

impl SupplyState {
    /// True if `service` reaches the home.
    pub fn has_service(&self, service: crate::defs::ServiceKind) -> bool {
        self.services & service_bit(service) != 0
    }
}

/// Bit of a service in [`SupplyState::services`].
pub const fn service_bit(service: crate::defs::ServiceKind) -> u16 {
    1 << (service as u16)
}

/// A family living together.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Household {
    pub(crate) family_name: String,
    /// Members in the order they joined.
    pub(crate) members: Vec<CitizenId>,
    /// House lived in (`None` = homeless, camping at the main headquarters).
    pub(crate) home: Option<BuildingId>,
    /// Food, fuel and goods kept at home, sorted by resource id.
    pub(crate) pantry: Vec<PantryItem>,
    pub(crate) supply: SupplyState,
    /// Consecutive days with an average happiness below the emigration threshold.
    pub(crate) unhappy_days: u8,
    /// Savings, earnings and spending ([`wealth`]).
    pub(crate) purse: Purse,
}

/// Money of a household in milli-gold (docs/game_design.md §6 Wohlstand).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Purse {
    /// Savings, never negative and at most [`MAX_PURSE_MILLI`].
    pub(crate) savings_milli: i64,
    /// Earned so far today (earnings and wages).
    pub(crate) earned_today: i64,
    /// Spent so far today (taxes, market fee, purchases).
    pub(crate) spent_today: i64,
    /// Earned on the previous day.
    pub(crate) earned_yesterday: i64,
    /// Spent on the previous day.
    pub(crate) spent_yesterday: i64,
}

/// Upper bound of every sum in a [`Purse`] (10¹² gold in milli-gold): far beyond any real game,
/// low enough that sums over all households of a kingdom cannot overflow. Loading rejects
/// larger values.
pub const MAX_PURSE_MILLI: i64 = 1_000_000_000_000_000;

impl Purse {
    /// A purse with `savings_milli` saved.
    pub(crate) fn with_savings(savings_milli: i64) -> Self {
        Self { savings_milli: savings_milli.clamp(0, MAX_PURSE_MILLI), ..Self::default() }
    }

    /// Savings in milli-gold.
    pub fn savings_milli(&self) -> i64 {
        self.savings_milli
    }

    /// Savings in whole gold (rounded down).
    pub fn savings(&self) -> i64 {
        self.savings_milli / crate::economy::MILLI_PER_GOLD
    }

    /// Earned on the previous day, in milli-gold.
    pub fn earned_yesterday_milli(&self) -> i64 {
        self.earned_yesterday
    }

    /// Spent on the previous day, in milli-gold.
    pub fn spent_yesterday_milli(&self) -> i64 {
        self.spent_yesterday
    }

    /// Earned so far today, in milli-gold.
    pub fn earned_today_milli(&self) -> i64 {
        self.earned_today
    }

    /// Spent so far today, in milli-gold.
    pub fn spent_today_milli(&self) -> i64 {
        self.spent_today
    }

    /// Adds income.
    pub(crate) fn earn(&mut self, milli: i64) {
        let milli = milli.max(0);
        self.savings_milli = self.savings_milli.saturating_add(milli).min(MAX_PURSE_MILLI);
        self.earned_today = self.earned_today.saturating_add(milli).min(MAX_PURSE_MILLI);
    }

    /// Pays up to `milli` from the savings; returns the amount paid.
    pub(crate) fn pay(&mut self, milli: i64) -> i64 {
        let paid = milli.clamp(0, self.savings_milli);
        self.savings_milli -= paid;
        self.spent_today = self.spent_today.saturating_add(paid).min(MAX_PURSE_MILLI);
        paid
    }

    /// Closes the day: today's sums become yesterday's.
    pub(crate) fn close_day(&mut self) {
        self.earned_yesterday = std::mem::take(&mut self.earned_today);
        self.spent_yesterday = std::mem::take(&mut self.spent_today);
    }

    /// Splits off the share of `part` of `of` members (a member moving out takes it along).
    pub(crate) fn split_off(&mut self, part: usize, of: usize) -> Purse {
        let of = i64::try_from(of.max(1)).unwrap_or(i64::MAX);
        let part = i64::try_from(part).unwrap_or(0).min(of);
        let share = self.savings_milli / of * part + self.savings_milli % of * part / of;
        self.savings_milli -= share;
        Purse::with_savings(share)
    }
}

impl Household {
    /// Family name of the household.
    pub fn family_name(&self) -> &str {
        &self.family_name
    }

    /// Members.
    pub fn members(&self) -> &[CitizenId] {
        &self.members
    }

    /// Number of members.
    pub fn size(&self) -> u16 {
        u16::try_from(self.members.len()).unwrap_or(u16::MAX)
    }

    /// House, if any.
    pub fn home(&self) -> Option<BuildingId> {
        self.home
    }

    /// Pantry contents.
    pub fn pantry(&self) -> &[PantryItem] {
        &self.pantry
    }

    /// Thousandths of a unit of `resource` in the pantry.
    pub fn pantry_milli(&self, resource: ResourceId) -> u32 {
        self.pantry.iter().find(|p| p.resource == resource).map_or(0, |p| p.milli)
    }

    /// Supply at the last hourly update.
    pub fn supply(&self) -> &SupplyState {
        &self.supply
    }

    /// Consecutive unhappy days (emigration after `PopulationRules::emigration_days`).
    pub fn unhappy_days(&self) -> u8 {
        self.unhappy_days
    }

    /// Savings, earnings and spending.
    pub fn purse(&self) -> &Purse {
        &self.purse
    }

    pub(crate) fn add_to_pantry(&mut self, resource: ResourceId, milli: u32) {
        match self.pantry.binary_search_by_key(&resource, |p| p.resource) {
            Ok(i) => self.pantry[i].milli = self.pantry[i].milli.saturating_add(milli),
            Err(i) => self.pantry.insert(i, PantryItem { resource, milli }),
        }
    }

    /// Removes up to `milli` of `resource`; returns the amount removed.
    pub(crate) fn take_from_pantry(&mut self, resource: ResourceId, milli: u32) -> u32 {
        let Ok(i) = self.pantry.binary_search_by_key(&resource, |p| p.resource) else { return 0 };
        let taken = self.pantry[i].milli.min(milli);
        self.pantry[i].milli -= taken;
        if self.pantry[i].milli == 0 {
            self.pantry.remove(i);
        }
        taken
    }
}

/// A finished house and the households living in it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Home {
    /// Households in the order they moved in.
    pub(crate) households: Vec<HouseholdId>,
    /// Consecutive hours the house missed the requirements of its tier.
    pub(crate) unmet_hours: u16,
}

impl Home {
    /// Households living here.
    pub fn households(&self) -> &[HouseholdId] {
        &self.households
    }

    /// Consecutive hours with unmet tier requirements.
    pub fn unmet_hours(&self) -> u16 {
        self.unmet_hours
    }
}

/// Rate-limited notification topics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) enum Notice {
    Famine,
    NoFuel,
    Homeless,
    Immigration,
    Emigration,
    Birth,
    Death,
    Downgrade,
    Starvation,
}

impl Notice {
    const COUNT: usize = 9;

    /// Minimum game hours between two notifications of the topic.
    const fn cooldown_hours(self) -> u64 {
        match self {
            Notice::Famine | Notice::NoFuel | Notice::Homeless => 6,
            Notice::Immigration | Notice::Birth | Notice::Death => 3,
            Notice::Emigration | Notice::Downgrade | Notice::Starvation => 2,
        }
    }
}

/// All citizens, households and homes plus the population statistics.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Population {
    pub(crate) citizens: Arena<Citizen>,
    pub(crate) households: Arena<Household>,
    /// Every finished house (also while upgrading), by building id.
    pub(crate) homes: BTreeMap<BuildingId, Home>,
    /// Workers and builders per building (mirror of [`Citizen::job`]).
    pub(crate) jobs: BTreeMap<BuildingId, Vec<CitizenId>>,
    pub(crate) stats: PopulationStats,
    /// Tick of the last notification per [`Notice`] topic (`u64::MAX` = never).
    notices: Vec<u64>,
    /// Path requests, travellers and the derived spatial index of walkers.
    pub(crate) walking: Walking,
    /// Building revision the movement step last looked at (derived, not saved).
    #[serde(skip)]
    pub(crate) walking_seen_revision: u64,
    /// People inside each building (citizens and travellers) by building slot index:
    /// (generation, count). Derived at the end of every tick and after loading; not saved.
    #[serde(skip)]
    occupancy: Vec<(u32, u16)>,
    /// Errand runners that arrived at their errand this tick (household, place); restocked
    /// together at the end of the movement step.
    #[serde(skip)]
    pub(crate) errands_arrived: Vec<(HouseholdId, BuildingId)>,
    /// Guests that arrived at a selling leisure venue (tavern) this tick (household, venue);
    /// they buy at the end of the movement step.
    #[serde(skip)]
    pub(crate) guests_arrived: Vec<(HouseholdId, BuildingId)>,
}

impl Population {
    /// Number of citizens.
    pub fn len(&self) -> usize {
        self.citizens.len()
    }

    /// People (citizens and travellers such as merchants) inside `building` at the end of the
    /// last tick — for lit windows and building views; O(1).
    pub fn occupants(&self, building: BuildingId) -> u32 {
        self.occupancy
            .get(building.index() as usize)
            .filter(|(generation, _)| *generation == building.generation())
            .map_or(0, |(_, n)| u32::from(*n))
    }

    /// Recounts [`Population::occupants`] (end of every tick, after loading).
    pub(crate) fn refresh_occupancy(&mut self) {
        self.occupancy.iter_mut().for_each(|slot| *slot = (0, 0));
        let inside = self.citizens.iter().filter_map(|(_, c)| c.motion.inside).chain(self.walking.travelers.iter().filter_map(|(_, t)| t.motion.inside));
        for b in inside {
            let i = b.index() as usize;
            if self.occupancy.len() <= i {
                self.occupancy.resize(i + 1, (0, 0));
            }
            let slot = &mut self.occupancy[i];
            if slot.0 != b.generation() {
                *slot = (b.generation(), 0);
            }
            slot.1 = slot.1.saturating_add(1);
        }
    }

    /// True without citizens.
    pub fn is_empty(&self) -> bool {
        self.citizens.is_empty()
    }

    /// A citizen by id.
    pub fn citizen(&self, id: CitizenId) -> Option<&Citizen> {
        self.citizens.get(id)
    }

    /// All citizens in id order.
    pub fn citizens(&self) -> impl DoubleEndedIterator<Item = (CitizenId, &Citizen)> + '_ {
        self.citizens.iter()
    }

    /// A household by id.
    pub fn household(&self, id: HouseholdId) -> Option<&Household> {
        self.households.get(id)
    }

    /// All households in id order.
    pub fn households(&self) -> impl DoubleEndedIterator<Item = (HouseholdId, &Household)> + '_ {
        self.households.iter()
    }

    /// The house state of a finished house.
    pub fn home(&self, building: BuildingId) -> Option<&Home> {
        self.homes.get(&building)
    }

    /// Whether house `building` burned fuel in its last hourly update (chimney smoke): only on
    /// cold days (`cold`: [`crate::weather::WeatherState::needs_heating`] today), only with
    /// residents; `OutOfFuel` when a household had too little (`None`: not a finished house).
    pub fn heating(&self, building: BuildingId, cold: bool) -> Option<Heating> {
        let home = self.homes.get(&building)?;
        let households: Vec<&Household> = home.households.iter().filter_map(|h| self.households.get(*h)).collect();
        let heating = if households.iter().all(|h| h.members.is_empty()) {
            Heating::Empty
        } else if !cold {
            Heating::NotNeeded
        } else if households.iter().all(|h| h.supply.fuel) {
            Heating::Burning
        } else {
            Heating::OutOfFuel
        };
        Some(heating)
    }

    /// All finished houses in id order.
    pub fn homes(&self) -> impl Iterator<Item = (BuildingId, &Home)> + '_ {
        self.homes.iter().map(|(id, h)| (*id, h))
    }

    /// Citizens working at (or building) `building`.
    pub fn workers_of(&self, building: BuildingId) -> &[CitizenId] {
        self.jobs.get(&building).map_or(&[], Vec::as_slice)
    }

    /// Residents of a house (all members of its households).
    pub fn residents_of(&self, building: BuildingId) -> impl Iterator<Item = CitizenId> + '_ {
        self.homes
            .get(&building)
            .into_iter()
            .flat_map(|h| h.households.iter())
            .filter_map(|hh| self.households.get(*hh))
            .flat_map(|hh| hh.members.iter().copied())
    }

    /// Number of residents of a house.
    pub fn resident_count(&self, building: BuildingId) -> u16 {
        self.homes.get(&building).map_or(0, |h| self.home_residents(h))
    }

    pub(crate) fn home_residents(&self, home: &Home) -> u16 {
        home.households.iter().filter_map(|hh| self.households.get(*hh)).map(Household::size).sum()
    }

    /// Full name ("Vorname Familienname").
    pub fn full_name(&self, id: CitizenId) -> Option<String> {
        let c = self.citizens.get(id)?;
        let family = self.households.get(c.household).map_or("", |h| h.family_name.as_str());
        Some(format!("{} {family}", c.first_name))
    }

    /// Daily counters and history.
    pub fn stats(&self) -> &PopulationStats {
        &self.stats
    }

    /// Path requests and travellers leaving the kingdom.
    pub fn walking(&self) -> &Walking {
        &self.walking
    }

    /// Children of a citizen (citizens naming them as mother or father), in id order.
    pub fn children_of(&self, id: CitizenId) -> Vec<CitizenId> {
        self.citizens.iter().filter(|(_, c)| c.mother == Some(id) || c.father == Some(id)).map(|(cid, _)| cid).collect()
    }

    /// Average happiness of all citizens in points (`None` without citizens).
    pub fn average_happiness(&self) -> Option<f32> {
        let n = self.citizens.len();
        (n > 0).then(|| {
            self.citizens.values().map(|c| f32::from(c.happiness)).sum::<f32>() / n as f32 / f32::from(CENTI)
        })
    }

    /// Mean of `f` over the members of a household (0 for an empty household).
    pub(crate) fn household_mean(&self, household: &Household, f: impl Fn(&Citizen) -> u16) -> u16 {
        let values: Vec<u32> =
            household.members.iter().filter_map(|m| self.citizens.get(*m)).map(|c| u32::from(f(c))).collect();
        if values.is_empty() { 0 } else { (values.iter().sum::<u32>() / values.len() as u32) as u16 }
    }

    /// Emits `notification` unless a notification of the topic was sent within its cooldown.
    pub(crate) fn notify(&mut self, topic: Notice, tick: u64, notification: Notification, events: &mut Vec<SimEvent>) {
        if self.notices.len() != Notice::COUNT {
            self.notices = vec![u64::MAX; Notice::COUNT];
        }
        let slot = &mut self.notices[topic as usize];
        let cooldown = topic.cooldown_hours() * TICKS_PER_HOUR;
        if *slot != u64::MAX && tick < slot.saturating_add(cooldown) {
            return;
        }
        *slot = tick;
        events.push(SimEvent::Notification(notification));
    }

    /// Records a job in the citizen and the job index; the caller updates `Building::staffed`.
    pub(crate) fn set_job(&mut self, id: CitizenId, job: Option<Employment>) {
        let Some(c) = self.citizens.get_mut(id) else { return };
        let old = std::mem::replace(&mut c.job, job);
        if let Some(old) = old
            && let Some(list) = self.jobs.get_mut(&old.building)
        {
            list.retain(|w| *w != id);
            if list.is_empty() {
                self.jobs.remove(&old.building);
            }
        }
        if let Some(new) = job {
            let list = self.jobs.entry(new.building).or_default();
            if let Err(pos) = list.binary_search(&id) {
                list.insert(pos, id);
            }
        }
    }

    /// Structural consistency (used when validating a loaded state).
    pub(crate) fn validate(&self, state: &GameState, defs: &Defs) -> Result<(), String> {
        self.citizens.check_integrity().map_err(|e| format!("citizen arena: {e}"))?;
        self.households.check_integrity().map_err(|e| format!("household arena: {e}"))?;
        if !self.stats.is_consistent() {
            return Err("population history out of order".into());
        }
        if !self.notices.is_empty() && self.notices.len() != Notice::COUNT {
            return Err("notification cooldowns malformed".into());
        }
        let mut counted = 0usize;
        let mut listed = std::collections::BTreeSet::new();
        for (hid, h) in self.households.iter() {
            if h.members.is_empty() {
                return Err(format!("household {hid:?} has no members"));
            }
            if h.pantry.windows(2).any(|w| w[0].resource >= w[1].resource)
                || h.pantry.iter().any(|p| p.milli == 0 || p.resource.index() >= defs.resources().len())
            {
                return Err(format!("household {hid:?} has a malformed pantry"));
            }
            let p = &h.purse;
            if [p.savings_milli, p.earned_today, p.spent_today, p.earned_yesterday, p.spent_yesterday]
                .iter()
                .any(|v| !(0..=MAX_PURSE_MILLI).contains(v))
            {
                return Err(format!("household {hid:?} has a negative or impossibly large purse"));
            }
            for m in &h.members {
                if !listed.insert(*m) {
                    return Err(format!("citizen {m:?} is listed twice"));
                }
                let c = self.citizens.get(*m).ok_or_else(|| format!("household {hid:?} lists a missing citizen"))?;
                if c.household != hid {
                    return Err(format!("citizen {m:?} does not point back to household {hid:?}"));
                }
            }
            counted += h.members.len();
            if let Some(home) = h.home {
                let listed = self.homes.get(&home).is_some_and(|hm| hm.households.contains(&hid));
                if !listed {
                    return Err(format!("household {hid:?} lives in {home:?}, which does not list it"));
                }
            }
        }
        if counted != self.citizens.len() {
            return Err("citizens and household members differ".into());
        }
        for (bid, home) in &self.homes {
            let b = state.buildings.get(*bid).ok_or_else(|| format!("home {bid:?} does not exist"))?;
            let is_house = defs.buildings().get(b.kind().index()).is_some_and(|d| d.housing.is_some());
            if !is_house || !b.is_complete() {
                return Err(format!("home {bid:?} is not a finished house"));
            }
            for hh in &home.households {
                if self.households.get(*hh).and_then(|h| h.home) != Some(*bid) {
                    return Err(format!("home {bid:?} lists household {hh:?}, which lives elsewhere"));
                }
            }
        }
        // Capacities and life stages derive from balancing data that may change between saving
        // and loading; `rederive` reconciles them, so they are not structural errors here.
        let today = i64::try_from(state.clock.total_days()).unwrap_or(i64::MAX);
        for (cid, c) in self.citizens.iter() {
            if !c.needs.is_valid() || c.health > MAX_VALUE || c.happiness > MAX_VALUE || c.education > 100 {
                return Err(format!("citizen {cid:?} has values out of range"));
            }
            if c.birth_day > today || c.last_birth_day.is_some_and(|d| d < c.birth_day || d > today) {
                return Err(format!("citizen {cid:?} has a birth day out of range"));
            }
            if c.partner == Some(cid) {
                return Err(format!("citizen {cid:?} is partnered with itself"));
            }
            if let Some(p) = c.partner
                && self.citizens.get(p).is_none_or(|pc| pc.partner != Some(cid))
            {
                return Err(format!("citizen {cid:?} has a one-sided partnership"));
            }
            if c.soldier != state.military.soldier(cid).map(|s| s.unit) {
                return Err(format!("citizen {cid:?} and the military disagree about its service"));
            }
            if c.sick_leave && c.sick_since.is_none_or(|t| t > state.clock.tick) {
                return Err(format!("citizen {cid:?} is on sick leave without an illness"));
            }
            if c.sick_since.is_some_and(|t| t > state.clock.tick) {
                return Err(format!("citizen {cid:?} fell ill in the future"));
            }
            if let Some(job) = c.job {
                if !c.can_take_job() {
                    return Err(format!("citizen {cid:?} has a job but cannot work"));
                }
                if state.buildings.get(job.building).is_none() {
                    return Err(format!("citizen {cid:?} works at a missing building"));
                }
                if !self.workers_of(job.building).contains(&cid) {
                    return Err(format!("job index misses citizen {cid:?}"));
                }
            }
        }
        for (bid, workers) in &self.jobs {
            let b = state.buildings.get(*bid).ok_or_else(|| format!("job index names missing building {bid:?}"))?;
            if usize::from(b.staffed()) != workers.len() {
                return Err(format!("building {bid:?} has staffed {} but {} workers", b.staffed(), workers.len()));
            }
            if workers.windows(2).any(|w| w[0] >= w[1])
                || workers.iter().any(|w| self.citizens.get(*w).and_then(|c| c.job).map(|j| j.building) != Some(*bid))
            {
                return Err(format!("job index of building {bid:?} is inconsistent"));
            }
        }
        for (bid, b) in state.buildings.iter() {
            if b.staffed() > 0 && !self.jobs.contains_key(&bid) {
                return Err(format!("building {bid:?} is staffed without workers"));
            }
        }
        friends::validate(state, defs)?;
        movement::validate(state, defs)
    }
}

/// Re-applies balancing-derived population values after loading with possibly rebalanced
/// data: life stages that match neither today's nor yesterday's age (changed age limits) are
/// corrected (citizens who can no longer work leave their job) and a new job allocation and
/// housing check are requested; houses over their
/// (possibly lowered) capacity give up households in the next step. A no-op for unchanged
/// data (a stage lagging by the pending daily check is kept), so loading stays exact.
pub(crate) fn rederive(state: &mut GameState, defs: &Defs) {
    let rules = &defs.rules().population;
    let today = state.clock.total_days();
    let mut changed = false;
    let mut retired = Vec::new();
    for (id, c) in state.population.citizens.iter_mut() {
        let now = LifeStage::for_age(c.age_on(today), rules);
        let before = LifeStage::for_age(c.age_on(today.saturating_sub(1)), rules);
        if c.stage != now && c.stage != before {
            c.stage = now;
            changed = true;
            if !c.can_work() && c.job.is_some() {
                retired.push(id);
            }
        }
    }
    for id in retired {
        demography::release_job(state, id);
    }
    let overfull = state
        .population
        .homes()
        .any(|(id, home)| state.population.home_residents(home) > home_capacity(state, defs, id));
    if changed || overfull {
        state.workforce.mark_dirty();
        state.workforce.refresh_counts(&state.population);
    }
    crate::navigation::rebuild(state, defs);
    movement::rebuild_derived(state);
    state.population.refresh_occupancy();
}

/// Test fixture: families (couples with 0–2 children) until `citizens` people live in the
/// kingdom, moved into free houses and placed inside them.
#[cfg(test)]
pub(crate) fn test_populate(state: &mut GameState, defs: &Defs, citizens: usize) {
    let mut families = Vec::new();
    while state.population.len() < citizens {
        let kids = (state.population.len() % 3) as u16;
        families.push(demography::make_family(state, defs, 2, kids));
    }
    sync_homes(state, defs, &mut Vec::new());
    for f in families {
        movement::place_inside_home(state, defs, f);
    }
    state.workforce.mark_dirty();
}

/// Pipeline step 7: housing, households, citizens, houses, immigration and marriages (see the
/// module documentation for the schedule).
pub(crate) fn run_population(state: &mut GameState, defs: &Defs, events: &mut Vec<SimEvent>) {
    // Work spots of builders and farmers are chosen by reachability (navigation components).
    crate::navigation::prepare(state, defs);
    let clock = state.clock;
    if state.workforce.needs_housing() || clock.is_new_hour() {
        sync_homes(state, defs, events);
    }
    let mut ctx = supply::TickContext::default();
    supply::update_households(state, defs, &mut ctx, events);
    needs::update_citizens(state, defs, events);
    housing::update_homes(state, defs, events);
    if clock.is_new_hour() {
        demography::run_immigration(state, defs, events);
        friends::run_hour(state, defs);
    }
    if clock.is_new_day() {
        demography::run_marriages(state, defs, events);
        gain_experience(state);
    }
    if ctx.stock_changed {
        crate::economy::storage::refresh_stock(state, defs);
    }
    state.workforce.refresh_counts(&state.population);
}
