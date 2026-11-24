//! Population reports for the UI: overview, needs, happiness, houses and citizens
//! (docs/game_design.md §6). Labels are German.

use serde::{Deserialize, Serialize};

use crate::buildings::BuildingId;
use crate::defs::{BuildingTypeId, Defs, JobCategory, NeedKind};
use crate::economy::storage;
use crate::population::{
    Activity, Citizen, CitizenId, DayCounts, Destination, HappinessFactor, HomeRequirements, HouseholdId, LifeStage,
    PopulationDay, Sex, SubPos, WealthClass, activity_text, citizen_happiness_parts, current_activity, home_requirements,
    wealth_class, work_efficiency_permille,
};
use crate::state::GameState;
use crate::workforce;

/// Citizens of one life stage.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageRow {
    /// The stage.
    pub stage: LifeStage,
    /// German label (plural).
    pub label: String,
    /// Men.
    pub male: u32,
    /// Women.
    pub female: u32,
}

impl StageRow {
    /// Men and women.
    pub fn total(&self) -> u32 {
        self.male + self.female
    }
}

/// Population window: counts, housing, jobs, today's changes and the daily history.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PopulationSummary {
    /// Citizens.
    pub population: u32,
    /// Per life stage and sex (children, adults, elders).
    pub stages: Vec<StageRow>,
    /// Households.
    pub households: u32,
    /// Places in all finished houses.
    pub housing_capacity: u32,
    /// Citizens with a home.
    pub housed: u32,
    /// Citizens without a home.
    pub homeless: u32,
    /// Free places open to immigrants (every house keeps `immigration_reserved_places` for
    /// births and marriages).
    pub immigrant_places: u32,
    /// Adults of working age.
    pub working_age: u32,
    /// Citizens with a job or construction assignment.
    pub employed: u32,
    /// Adults of working age without a job.
    pub unemployed: u32,
    /// Educated citizens.
    pub educated: u32,
    /// Average happiness in points.
    pub average_happiness: u8,
    /// Average health in points.
    pub average_health: u8,
    /// Births, deaths, arrivals and departures so far today.
    pub today: DayCounts,
    /// Closed days, oldest first.
    pub history: Vec<PopulationDay>,
    /// Household savings (Wohlstand).
    pub wealth: WealthSummary,
    /// Taverns (finished, staffed) and how many of them have their goods (beer) in stock.
    pub taverns: TavernSummary,
}

/// Savings of all households (docs/game_design.md §6 Wohlstand), in whole gold.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WealthSummary {
    /// Savings of all households together.
    pub total_savings: i64,
    /// Average savings per household.
    pub average_savings: i64,
    /// Median savings of a household.
    pub median_savings: i64,
    /// Households per wealth class (poor, modest, wealthy).
    pub classes: Vec<WealthClassRow>,
    /// Earned by all households on the previous day.
    pub earned_yesterday: i64,
    /// Spent by all households on the previous day (taxes, fees, purchases).
    pub spent_yesterday: i64,
}

/// Households of one wealth class.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WealthClassRow {
    /// The class.
    pub class: WealthClass,
    /// German label.
    pub label: String,
    /// Households.
    pub households: u32,
}

/// Taverns and their stock.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TavernSummary {
    /// Operational, staffed taverns.
    pub open: u32,
    /// Of those, the ones with everything they sell in stock.
    pub stocked: u32,
}

fn wealth_summary(state: &GameState, defs: &Defs) -> WealthSummary {
    let pop = state.population();
    let gold = |m: i64| m / crate::economy::MILLI_PER_GOLD;
    let mut savings: Vec<i64> = Vec::new();
    let mut counts = [0u32; 3];
    let (mut earned, mut spent) = (0i64, 0i64);
    for (id, h) in pop.households() {
        savings.push(h.purse().savings_milli());
        earned = earned.saturating_add(h.purse().earned_yesterday_milli());
        spent = spent.saturating_add(h.purse().spent_yesterday_milli());
        match wealth_class(state, defs, id) {
            Some(WealthClass::Poor) => counts[0] += 1,
            Some(WealthClass::Modest) => counts[1] += 1,
            Some(WealthClass::Wealthy) => counts[2] += 1,
            None => {}
        }
    }
    savings.sort_unstable();
    let total = savings.iter().fold(0i64, |sum, s| sum.saturating_add(*s));
    let n = i64::try_from(savings.len()).unwrap_or(i64::MAX).max(1);
    let classes = [WealthClass::Poor, WealthClass::Modest, WealthClass::Wealthy]
        .iter()
        .zip(counts)
        .map(|(c, households)| WealthClassRow { class: *c, label: c.display_name().to_string(), households })
        .collect();
    WealthSummary {
        total_savings: gold(total),
        average_savings: gold(total / n),
        median_savings: savings.get(savings.len() / 2).copied().map_or(0, gold),
        classes,
        earned_yesterday: gold(earned),
        spent_yesterday: gold(spent),
    }
}

fn tavern_summary(state: &GameState, defs: &Defs) -> TavernSummary {
    let mut out = TavernSummary::default();
    for (_, b) in state.buildings().iter() {
        let Some(def) = defs.buildings().get(b.kind().index()) else { continue };
        if def.sells.is_empty() || !b.is_operational() || (def.workers > 0 && b.trade_staff() == 0) {
            continue;
        }
        out.open += 1;
        if def.sells.iter().all(|r| b.inventory().get(*r) > 0) {
            out.stocked += 1;
        }
    }
    out
}

/// Counts and history of the whole population.
pub fn population_summary(state: &GameState, defs: &Defs) -> PopulationSummary {
    let pop = state.population();
    let mut stages: Vec<StageRow> = LifeStage::ALL
        .iter()
        .map(|s| StageRow { stage: *s, label: s.plural_name().to_string(), male: 0, female: 0 })
        .collect();
    let (mut educated, mut health) = (0u32, 0u64);
    for (_, c) in pop.citizens() {
        if let Some(row) = stages.iter_mut().find(|r| r.stage == c.stage()) {
            match c.sex() {
                Sex::Male => row.male += 1,
                Sex::Female => row.female += 1,
            }
        }
        if c.is_educated() {
            educated += 1;
        }
        health += u64::from(c.health());
    }
    let jobs = workforce::summary(state, defs);
    let n = pop.len().max(1) as u64;
    PopulationSummary {
        population: jobs.population,
        stages,
        households: u32::try_from(pop.households().count()).unwrap_or(u32::MAX),
        housing_capacity: jobs.housing_capacity,
        housed: jobs.housed,
        homeless: jobs.homeless,
        immigrant_places: crate::population::immigrant_places(state, defs),
        working_age: jobs.working_age,
        employed: jobs.employed,
        unemployed: jobs.unemployed,
        educated,
        average_happiness: pop.average_happiness().map_or(0, |h| h.round().clamp(0.0, 100.0) as u8),
        average_health: if pop.is_empty() { 0 } else { (health / n) as u8 },
        today: *pop.stats().today(),
        history: pop.stats().history().copied().collect(),
        wealth: wealth_summary(state, defs),
        taverns: tavern_summary(state, defs),
    }
}

/// One need: average and share of citizens at or above the satisfaction threshold.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeedRow {
    /// The need.
    pub need: NeedKind,
    /// German label.
    pub label: String,
    /// Average in points.
    pub average: u8,
    /// Citizens with the need at least at `need_met_threshold`, in percent.
    pub satisfied_percent: u8,
}

fn need_rows(defs: &Defs, citizens: &[&Citizen]) -> Vec<NeedRow> {
    let threshold = defs.rules().population.need_met_threshold;
    let n = citizens.len();
    NeedKind::ALL
        .iter()
        .map(|need| {
            let (sum, ok) = citizens.iter().fold((0u64, 0u64), |(sum, ok), c| {
                let v = c.needs().get(*need);
                (sum + u64::from(v), ok + u64::from(v >= threshold))
            });
            let (average, satisfied_percent) =
                if n == 0 { (0, 0) } else { ((sum / n as u64) as u8, (ok * 100 / n as u64) as u8) };
            NeedRow { need: *need, label: need.display_name().to_string(), average, satisfied_percent }
        })
        .collect()
}

/// Need satisfaction of the whole kingdom.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeedsSummary {
    /// One row per need in canonical order.
    pub needs: Vec<NeedRow>,
}

/// Kingdom-wide need satisfaction.
pub fn needs_summary(state: &GameState, defs: &Defs) -> NeedsSummary {
    let citizens: Vec<&Citizen> = state.population().citizens().map(|(_, c)| c).collect();
    NeedsSummary { needs: need_rows(defs, &citizens) }
}

/// One happiness factor with its average contribution.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HappinessRow {
    /// The factor.
    pub factor: HappinessFactor,
    /// German label.
    pub label: String,
    /// Contribution in points (needs: weighted share of the mean; modifiers: signed).
    pub points: f32,
}

/// Happiness window: average, factor breakdown and the citizens close to the thresholds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HappinessReport {
    /// Average happiness in points.
    pub average: u8,
    /// Average contribution of every factor (needs first, then modifiers present anywhere).
    pub factors: Vec<HappinessRow>,
    /// Citizens below the emigration threshold.
    pub unhappy: u32,
    /// Citizens above the immigration threshold.
    pub happy: u32,
}

fn rows_from(sums: Vec<(HappinessFactor, f32)>, n: f32) -> Vec<HappinessRow> {
    sums.into_iter()
        .map(|(factor, total)| HappinessRow { factor, label: factor.label().to_string(), points: total / n })
        .collect()
}

/// Kingdom-wide happiness with its factors.
pub fn happiness_report(state: &GameState, defs: &Defs) -> HappinessReport {
    let rules = &defs.rules().population;
    let pop = state.population();
    let mut sums: Vec<(HappinessFactor, f32)> = Vec::new();
    let (mut unhappy, mut happy) = (0, 0);
    for (id, c) in pop.citizens() {
        for part in citizen_happiness_parts(state, defs, id).unwrap_or_default() {
            match sums.iter_mut().find(|(f, _)| *f == part.factor) {
                Some((_, total)) => *total += part.points,
                None => sums.push((part.factor, part.points)),
            }
        }
        if c.happiness() < rules.emigration_happiness {
            unhappy += 1;
        }
        if c.happiness() > rules.immigration_happiness {
            happy += 1;
        }
    }
    let n = pop.len().max(1) as f32;
    HappinessReport {
        average: pop.average_happiness().map_or(0, |h| h.round().clamp(0.0, 100.0) as u8),
        factors: rows_from(sums, n),
        unhappy,
        happy,
    }
}

/// A citizen's job.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobInfo {
    /// Workplace or construction site.
    pub building: BuildingId,
    /// German name of the building type.
    pub building_name: String,
    /// German job title ("Bauarbeiter", "Träger" or the workplace's name).
    pub title: String,
    /// Job category.
    pub category: JobCategory,
}

fn job_info(state: &GameState, defs: &Defs, c: &Citizen) -> Option<JobInfo> {
    let job = c.job()?;
    let b = state.building(job.building)?;
    let def = defs.buildings().get(b.kind().index())?;
    let title = if job.category == JobCategory::Construction {
        String::from("Bauarbeiter")
    } else if storage::is_operational_storage(defs, b) || def.storage.is_some() {
        String::from("Träger")
    } else {
        def.name.clone()
    };
    Some(JobInfo { building: job.building, building_name: def.name.clone(), title, category: job.category })
}

/// One resident in a list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResidentRow {
    /// The citizen.
    pub citizen: CitizenId,
    /// Full name.
    pub name: String,
    /// Age in years.
    pub age: u32,
    /// Sex.
    pub sex: Sex,
    /// Life stage.
    pub stage: LifeStage,
    /// Job, if any.
    pub job: Option<JobInfo>,
    /// Happiness in points.
    pub happiness: u8,
    /// Health in points.
    pub health: u8,
}

fn resident_row(state: &GameState, defs: &Defs, id: CitizenId) -> Option<ResidentRow> {
    let pop = state.population();
    let c = pop.citizen(id)?;
    Some(ResidentRow {
        citizen: id,
        name: pop.full_name(id)?,
        age: c.age_on(state.clock().total_days()),
        sex: c.sex(),
        stage: c.stage(),
        job: job_info(state, defs, c),
        happiness: c.happiness(),
        health: c.health(),
    })
}

/// Residents of a house (empty for other buildings).
pub fn residents(state: &GameState, defs: &Defs, building: BuildingId) -> Vec<ResidentRow> {
    state.population().residents_of(building).filter_map(|id| resident_row(state, defs, id)).collect()
}

/// A household of a house.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HouseholdRow {
    /// The household.
    pub household: HouseholdId,
    /// Family name.
    pub family_name: String,
    /// Members.
    pub members: u16,
    /// Consecutive unhappy days (emigration after `emigration_days`).
    pub unhappy_days: u8,
    /// Savings in gold.
    pub savings: i64,
    /// Wealth class.
    pub wealth: WealthClass,
    /// German label of the wealth class.
    pub wealth_label: String,
    /// Earned on the previous day, in gold.
    pub earned_yesterday: i64,
    /// Spent on the previous day, in gold.
    pub spent_yesterday: i64,
}

/// House window.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HouseReport {
    /// The house.
    pub building: BuildingId,
    /// Its type.
    pub kind: BuildingTypeId,
    /// Housing tier.
    pub tier: u8,
    /// Capacity including research.
    pub capacity: u16,
    /// Households living here.
    pub households: Vec<HouseholdRow>,
    /// Residents.
    pub residents: Vec<ResidentRow>,
    /// Need averages of the residents.
    pub needs: Vec<NeedRow>,
    /// Average happiness of the residents in points.
    pub happiness: u8,
    /// How the residents meet the requirements to keep the current tier.
    pub keep: Option<HomeRequirements>,
    /// How the residents meet the requirements of the next tier.
    pub next: Option<HomeRequirements>,
    /// Consecutive hours the current tier's requirements were unmet.
    pub unmet_hours: u16,
    /// Hours until the house falls back a tier if nothing changes (`None`: not threatened).
    pub downgrade_in_hours: Option<u16>,
    /// German note if residents find no path home ("Kein Weg: …").
    pub path_problem: Option<String>,
}

/// Residents, supply and tier requirements of a finished house; `None` for other buildings.
pub fn house_report(state: &GameState, defs: &Defs, building: BuildingId) -> Option<HouseReport> {
    let b = state.building(building)?;
    let def = defs.buildings().get(b.kind().index())?;
    let housing = def.housing.as_ref()?;
    let home = state.population().home(building)?;
    let pop = state.population();
    let households = home
        .households()
        .iter()
        .filter_map(|id| {
            pop.household(*id).map(|h| {
                let wealth = wealth_class(state, defs, *id).unwrap_or(WealthClass::Modest);
                HouseholdRow {
                    household: *id,
                    family_name: h.family_name().to_string(),
                    members: h.size(),
                    unhappy_days: h.unhappy_days(),
                    savings: h.purse().savings(),
                    wealth,
                    wealth_label: wealth.display_name().to_string(),
                    earned_yesterday: h.purse().earned_yesterday_milli() / crate::economy::MILLI_PER_GOLD,
                    spent_yesterday: h.purse().spent_yesterday_milli() / crate::economy::MILLI_PER_GOLD,
                }
            })
        })
        .collect();
    let citizens: Vec<&Citizen> = pop.residents_of(building).filter_map(|c| pop.citizen(c)).collect();
    let happiness = if citizens.is_empty() {
        0
    } else {
        (citizens.iter().map(|c| u32::from(c.happiness())).sum::<u32>() / citizens.len() as u32) as u8
    };
    let limit = defs.rules().population.downgrade_after_hours;
    let keep = (housing.tier > 1).then(|| home_requirements(state, defs, building, b.kind())).flatten();
    let threatened = home.unmet_hours() > 0;
    Some(HouseReport {
        building,
        kind: b.kind(),
        tier: housing.tier,
        capacity: crate::workforce::housing_capacity(state, defs, housing.capacity),
        households,
        residents: residents(state, defs, building),
        needs: need_rows(defs, &citizens),
        happiness,
        keep,
        next: def.upgrade.as_ref().and_then(|u| home_requirements(state, defs, building, u.to)),
        unmet_hours: home.unmet_hours(),
        downgrade_in_hours: threatened.then(|| limit.saturating_sub(home.unmet_hours())),
        path_problem: super::path_problem(state, building),
    })
}

/// A related citizen with name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CitizenLink {
    /// The citizen.
    pub citizen: CitizenId,
    /// Full name.
    pub name: String,
}

/// Citizen window.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CitizenDetail {
    /// The citizen.
    pub citizen: CitizenId,
    /// Full name.
    pub name: String,
    /// Age in years.
    pub age: u32,
    /// Sex.
    pub sex: Sex,
    /// German label of the sex.
    pub sex_label: String,
    /// Life stage.
    pub stage: LifeStage,
    /// German label of the stage.
    pub stage_label: String,
    /// Household.
    pub household: HouseholdId,
    /// Family name of the household.
    pub family_name: String,
    /// House (`None`: homeless).
    pub home: Option<BuildingId>,
    /// German name of the house type (`None`: homeless).
    pub home_name: Option<String>,
    /// Job, if any.
    pub job: Option<JobInfo>,
    /// Current activity.
    pub activity: Activity,
    /// German label of the activity.
    pub activity_label: String,
    /// German description incl. the building ("Auf dem Weg zur Arbeit (Sägewerk)").
    pub activity_text: String,
    /// Map position in 1/256 tile (the door while inside a building).
    pub position: SubPos,
    /// Building the citizen is inside, if any.
    pub inside: Option<BuildingId>,
    /// Where the citizen is heading (or stays).
    pub destination: Option<Destination>,
    /// German name of the destination building.
    pub destination_name: Option<String>,
    /// Game minutes of the last walk to work.
    pub last_commute_minutes: Option<u32>,
    /// Goods the citizen carries right now (carriers, gatherers bringing their yield back).
    pub carried: Option<crate::defs::ResourceAmount>,
    /// Needs (single citizen: `satisfied_percent` is 0 or 100).
    pub needs: Vec<NeedRow>,
    /// Happiness in points.
    pub happiness: u8,
    /// Factors of the happiness target.
    pub happiness_factors: Vec<HappinessRow>,
    /// Health in points.
    pub health: u8,
    /// Work efficiency in percent (health and happiness).
    pub work_efficiency_percent: u8,
    /// School progress 0–100.
    pub education: u8,
    /// May take educated jobs.
    pub educated: bool,
    /// Spouse.
    pub partner: Option<CitizenLink>,
    /// Mother (if alive).
    pub mother: Option<CitizenLink>,
    /// Father (if alive).
    pub father: Option<CitizenLink>,
    /// Living children.
    pub children: Vec<CitizenLink>,
    /// Friends.
    pub friends: Vec<CitizenLink>,
    /// Savings of the household in gold (Wohlstand).
    pub household_savings: i64,
    /// Wealth class of the household.
    pub wealth: WealthClass,
    /// German label of the wealth class.
    pub wealth_label: String,
    /// Sick (health below `sick_health`): stays at home or in a hospital.
    pub sick: bool,
    /// On sick leave: ill for a day, gave the job up and takes a new one when healthy.
    pub sick_leave: bool,
    /// Training in the current job (docs §6 Qualifikation; `None` without a job).
    pub training: Option<TrainingInfo>,
}

/// Training of a worker in his job (docs/game_design.md §6 Qualifikation).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrainingInfo {
    /// Days worked in the job's category (at most 255).
    pub days: u8,
    /// Productivity from training in percent (100 = fully trained; unskilled jobs: 100).
    pub percent: u8,
    /// German text („Eingearbeitet“, „In Einarbeitung: 2 von 4 Tagen, 85 %“, „Hilfsarbeit –
    /// keine Einarbeitung nötig“).
    pub text: String,
}

/// Training of `c` in his current job, if employed.
fn training_info(state: &GameState, defs: &Defs, c: &Citizen) -> Option<TrainingInfo> {
    let job = c.job()?;
    let tier = state
        .building(job.building)
        .filter(|b| b.construction_site().is_none())
        .and_then(|b| defs.buildings().get(b.kind().index()))
        .and_then(|d| d.job)
        .map_or(crate::defs::JobTier::Unskilled, |j| j.tier);
    let rules = &defs.rules().population;
    let days = c.experience_days(job.category);
    let percent = (crate::population::training_permille(rules, c, job.category, tier) / 10) as u8;
    let full = rules.training.full_after_days;
    let text = if tier == crate::defs::JobTier::Unskilled {
        "Hilfsarbeit – keine Einarbeitung nötig".to_owned()
    } else if days >= full {
        "Eingearbeitet".to_owned()
    } else {
        format!("In Einarbeitung: {days} von {full} Tagen, {percent} %")
    };
    Some(TrainingInfo { days, percent, text })
}

/// Everything about one citizen; `None` if the citizen does not exist (any more).
pub fn citizen_detail(state: &GameState, defs: &Defs, citizen: CitizenId) -> Option<CitizenDetail> {
    let pop = state.population();
    let c = pop.citizen(citizen)?;
    let household = pop.household(c.household())?;
    let link = |id: Option<CitizenId>| {
        id.and_then(|id| pop.full_name(id).map(|name| CitizenLink { citizen: id, name }))
    };
    let home = household.home();
    let wealth = wealth_class(state, defs, c.household()).unwrap_or(WealthClass::Modest);
    let factors = citizen_happiness_parts(state, defs, citizen)
        .unwrap_or_default()
        .into_iter()
        .map(|p| HappinessRow { factor: p.factor, label: p.factor.label().to_string(), points: p.points })
        .collect();
    Some(CitizenDetail {
        citizen,
        name: pop.full_name(citizen)?,
        age: c.age_on(state.clock().total_days()),
        sex: c.sex(),
        sex_label: c.sex().display_name().to_string(),
        stage: c.stage(),
        stage_label: c.stage().display_name().to_string(),
        household: c.household(),
        family_name: household.family_name().to_string(),
        home,
        home_name: home.and_then(|b| state.building(b)).and_then(|b| defs.buildings().get(b.kind().index())).map(|d| d.name.clone()),
        job: job_info(state, defs, c),
        activity: current_activity(state, defs, c),
        activity_label: current_activity(state, defs, c).display_name().to_string(),
        activity_text: activity_text(state, defs, citizen).unwrap_or_default(),
        position: c.motion().pos(),
        inside: c.motion().inside(),
        destination: c.motion().destination(),
        destination_name: c
            .motion()
            .destination()
            .and_then(Destination::building)
            .and_then(|b| state.building(b))
            .and_then(|b| defs.buildings().get(b.kind().index()))
            .map(|d| d.name.clone()),
        last_commute_minutes: c.motion().last_commute_ticks().map(|t| super::ticks_to_minutes(u64::from(t))),
        carried: c.carried(),
        needs: need_rows(defs, &[c]),
        happiness: c.happiness(),
        happiness_factors: factors,
        health: c.health(),
        work_efficiency_percent: (work_efficiency_permille(&defs.rules().population, c) / 10) as u8,
        education: c.education(),
        educated: c.is_educated(),
        partner: link(c.partner()),
        mother: link(c.mother()),
        father: link(c.father()),
        children: pop.children_of(citizen).into_iter().filter_map(|id| link(Some(id))).collect(),
        friends: c.friends().iter().filter_map(|id| link(Some(*id))).collect(),
        household_savings: household.purse().savings(),
        wealth,
        wealth_label: wealth.display_name().to_string(),
        sick: crate::population::is_sick(defs, c),
        sick_leave: c.is_on_sick_leave(),
        training: training_info(state, defs, c),
    })
}
