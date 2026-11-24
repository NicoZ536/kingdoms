//! Population balancing (`rules.ron` → `population`) and name lists (`names.ron`),
//! docs/game_design.md §6.

use serde::{Deserialize, Serialize};

use super::common::NeedKind;
use super::error::{Loc, Problems};

/// Weight of every need in the happiness mean (docs/game_design.md §6: "gewichteter Mittelwert").
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NeedWeights {
    /// Nahrung.
    pub hunger: f32,
    /// Wasser.
    pub thirst: f32,
    /// Unterkunft.
    pub shelter: f32,
    /// Wärme.
    pub warmth: f32,
    /// Sicherheit.
    pub safety: f32,
    /// Gesundheit.
    pub health: f32,
    /// Gemeinschaft.
    pub social: f32,
    /// Unterhaltung.
    pub entertainment: f32,
    /// Glaube.
    pub faith: f32,
}

impl NeedWeights {
    /// Weight of `need`.
    pub fn get(&self, need: NeedKind) -> f32 {
        match need {
            NeedKind::Hunger => self.hunger,
            NeedKind::Thirst => self.thirst,
            NeedKind::Shelter => self.shelter,
            NeedKind::Warmth => self.warmth,
            NeedKind::Safety => self.safety,
            NeedKind::Health => self.health,
            NeedKind::Social => self.social,
            NeedKind::Entertainment => self.entertainment,
            NeedKind::Faith => self.faith,
        }
    }

    /// Sum of all weights.
    pub fn total(&self) -> f32 {
        NeedKind::ALL.iter().map(|n| self.get(*n)).sum()
    }
}

/// Old-age mortality from an age on.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MortalityStep {
    /// First age (in years) the rate applies to.
    pub from_age: u8,
    /// Chance to die within one year of life (= one season), in percent.
    pub percent_per_year: f32,
}

/// Balancing of citizens, households, needs, happiness and demography.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PopulationRules {
    /// Age from which a citizen is an adult (works, can marry later); younger ones are children.
    pub adult_age: u8,
    /// Age from which a citizen is an elder (no work).
    pub elder_age: u8,
    /// Old-age mortality steps, ascending by age.
    pub mortality: Vec<MortalityStep>,
    /// Saturation an adult eats per day.
    pub food_per_day: f32,
    /// Food of a child in percent of an adult's.
    pub child_food_percent: u8,
    /// Days of consumption a household keeps in its pantry (food, fuel, goods).
    pub pantry_days: f32,
    /// Heating value a resident burns per winter day.
    pub fuel_per_resident_day: f32,
    /// Heating comes first (docs/game_design.md §6.1): on days that need heating workshops get
    /// fuel (wood, coal) only beyond this many days of the households' heating demand.
    pub fuel_reserve_days: f32,
    /// Fuel lasting fewer days than this at today's heating demand is reported once per spell
    /// („Brennholz wird knapp“), on cold days and on the last day of autumn.
    pub fuel_warning_days: f32,
    /// Speed at which needs follow their supply level, in points per game hour.
    pub need_change_per_hour: f32,
    /// Hunger lost per game hour without food.
    pub hunger_decay_per_hour: f32,
    /// Average need value of the residents from which a housing requirement counts as met.
    pub need_met_threshold: u8,
    /// Thirst level without a well or fresh water nearby (water fetched from afar).
    pub no_water_thirst: u8,
    /// Safety level without a watchtower, barracks or castle in reach (no threats yet).
    pub base_safety: u8,
    /// Health need level without hospital and with a single kind of food.
    pub base_health_need: u8,
    /// Health need bonus per additional kind of food in the pantry.
    pub variety_bonus_per_food: u8,
    /// Highest food variety bonus.
    pub max_variety_bonus: u8,
    /// Health need bonus of a hospital in reach.
    pub hospital_health_bonus: u8,
    /// Social level of a single without market or tavern.
    pub base_social: u8,
    /// Social bonus of living with family (household of two or more).
    pub family_social_bonus: u8,
    /// Social bonus of a market in reach.
    pub market_social_bonus: u8,
    /// Social bonus of a tavern in reach.
    pub tavern_social_bonus: u8,
    /// Entertainment level without a tavern in reach.
    pub base_entertainment: u8,
    /// Faith level without a church in reach.
    pub base_faith: u8,
    /// Warmth in winter without fuel.
    pub winter_warmth_without_fuel: u8,
    /// Warmth bonus of clothes in winter.
    pub clothes_warmth_bonus: u8,
    /// Weights of the needs in the happiness mean.
    pub need_weights: NeedWeights,
    /// Speed at which happiness follows its target, in points per game hour.
    pub happiness_change_per_hour: f32,
    /// Tax rate without happiness effect.
    pub neutral_tax_rate_percent: u8,
    /// Happiness change per percent of tax below (+) or above (−) the neutral rate.
    pub tax_happiness_per_percent: f32,
    /// Happiness modifier of an employed adult (wages).
    pub employed_happiness: i8,
    /// Happiness modifier of an unemployed adult.
    pub unemployed_happiness: i8,
    /// Happiness modifier per goods requirement of the own housing tier that is not supplied.
    pub missing_goods_happiness: i8,
    /// Speed at which health follows its target, in points per game hour.
    pub health_change_per_hour: f32,
    /// Health lost per game hour while starving (hunger 0).
    pub starvation_damage_per_hour: f32,
    /// Health lost per game hour while freezing (winter, warmth below `freezing_warmth`).
    pub freezing_damage_per_hour: f32,
    /// Warmth below which a citizen freezes in winter.
    pub freezing_warmth: u8,
    /// Highest health of an elder drops by this per year beyond `elder_age`.
    pub elder_health_loss_per_year: f32,
    /// Daily chance to die of disease at health 50, in permille (scales with the square of
    /// the missing health; 0 at full health).
    pub disease_death_per_mille: f32,
    /// Disease risk with a hospital in reach, in percent.
    pub hospital_disease_percent: u8,
    /// Household happiness below which a household considers leaving.
    pub emigration_happiness: u8,
    /// Consecutive unhappy days after which a household emigrates.
    pub emigration_days: u8,
    /// Average happiness above which new settlers come.
    pub immigration_happiness: u8,
    /// Hourly immigration chance in percent per happiness point above the threshold.
    pub immigration_chance_per_point: f32,
    /// Highest hourly immigration chance in percent.
    pub immigration_max_chance_percent: u8,
    /// Settlers that may arrive per day at least.
    pub immigration_daily_base: u16,
    /// Settlers that may arrive per day in percent of the population (if more than the base).
    pub immigration_daily_percent: u8,
    /// Days of food for the whole population the kingdom stock must hold to attract settlers.
    pub immigration_food_days: f32,
    /// Places immigrants leave free in every house, for births and marriages.
    pub immigration_reserved_places: u8,
    /// Chance that an adult immigrant is educated, in percent.
    pub immigrant_educated_percent: u8,
    /// Daily birth chance of an eligible couple, in percent.
    pub birth_chance_percent: f32,
    /// Youngest age of a mother.
    pub fertile_min_age: u8,
    /// Oldest age of a mother.
    pub fertile_max_age: u8,
    /// Household happiness needed for births.
    pub birth_min_happiness: u8,
    /// Days between two births of the same mother.
    pub birth_interval_days: u16,
    /// Youngest age to marry.
    pub marriage_min_age: u8,
    /// Largest age difference of a couple.
    pub marriage_max_age_gap: u8,
    /// Daily chance that an eligible single woman marries, in percent.
    pub marriage_chance_percent: u8,
    /// Age from which children go to school.
    pub school_age: u8,
    /// Education points a pupil gains per day with a school in reach (0–100 scale).
    pub education_per_day: u8,
    /// Education from which a citizen counts as educated (needed for educated jobs).
    pub educated_threshold: u8,
    /// Hours a house may miss the requirements of its tier before it is downgraded.
    pub downgrade_after_hours: u16,
    /// Health from which a worker reaches full productivity (below: down to 50 %).
    pub full_productivity_health: u8,
    /// Happiness from which a worker reaches full productivity (below: down to 50 %).
    pub full_productivity_happiness: u8,
    /// Entertainment level of a tavern in reach whose goods (beer) are sold out and none in
    /// the pantry; its social bonus is halved then (docs/game_design.md §5.4, §6).
    pub tavern_without_goods_entertainment: u8,
    /// Friends a citizen keeps at most.
    pub max_friends: u8,
    /// Social bonus per friend.
    pub friend_social_bonus: u8,
    /// Chance per game hour, in permille, that two citizens at the same place (work, school,
    /// tavern or church, the same house) become friends.
    pub friendship_chance_permille: u16,
    /// Health below which a citizen is sick: stays at home (or in a hospital in reach) instead
    /// of working, going to school or out.
    pub sick_health: u8,
    /// Health recovery in a hospital in percent of the normal speed.
    pub hospital_recovery_percent: u16,
    /// Hours a sick worker keeps the job; then he goes on sick leave: gives the job up and takes
    /// no new one until healthy again, so somebody else can fill the place (docs §6).
    pub sick_leave_after_hours: u16,
    /// Training in skilled and educated jobs (docs/game_design.md §6 Qualifikation).
    pub training: TrainingRules,
    /// Household money (docs/game_design.md §6 Wohlstand).
    pub wealth: WealthRules,
}

/// Training in skilled and educated jobs (docs/game_design.md §6): a worker new to the job
/// category starts at a share of the full productivity and reaches it after some days of work;
/// educated workers start higher. Unskilled jobs need no training.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainingRules {
    /// Productivity of an untrained worker in percent.
    pub start_percent: u8,
    /// Productivity of an untrained educated worker in percent.
    pub educated_start_percent: u8,
    /// Days of work in the job category to full productivity.
    pub full_after_days: u8,
}

impl TrainingRules {
    fn validate(&self, p: &mut Problems, loc: &Loc<'_>) {
        p.check(
            self.start_percent <= 100 && self.educated_start_percent <= 100 && self.start_percent <= self.educated_start_percent,
            loc,
            || "population.training: start percentages must be <= 100, the educated one at least the plain one".into(),
        );
        p.check(self.full_after_days > 0, loc, || "population.training: full_after_days must be positive".into());
    }
}

/// Household savings (docs/game_design.md §6): income, poverty and wealth.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WealthRules {
    /// Savings a new household (settlers, immigrants) brings per member, in gold.
    pub start_savings_per_member: f32,
    /// Earnings of an elder in percent of a worker's.
    pub elder_income_percent: u8,
    /// Earnings of an adult without a job in percent of a worker's.
    pub unemployed_income_percent: u8,
    /// Earnings per worker and day of a homeless household (no house tier), in gold.
    pub homeless_income_per_worker: f32,
    /// Savings per member below which a household is poor, in gold.
    pub poor_savings_per_member: f32,
    /// Savings per member from which a household is wealthy, in gold.
    pub wealthy_savings_per_member: f32,
    /// Happiness modifier of a poor household's members.
    pub poverty_happiness: i8,
    /// Happiness modifier of a wealthy household's members.
    pub wealth_happiness: i8,
}

impl WealthRules {
    fn validate(&self, p: &mut Problems, loc: &Loc<'_>) {
        let ok = |v: f32| v.is_finite() && v >= 0.0;
        p.check(
            ok(self.start_savings_per_member)
                && ok(self.homeless_income_per_worker)
                && ok(self.poor_savings_per_member)
                && self.wealthy_savings_per_member.is_finite()
                && self.wealthy_savings_per_member > self.poor_savings_per_member,
            loc,
            || "population.wealth: amounts must be >= 0 and wealthy above poor".into(),
        );
        p.check(self.elder_income_percent <= 100 && self.unemployed_income_percent <= 100, loc, || {
            "population.wealth: income percentages must be <= 100".into()
        });
        p.check(self.poverty_happiness <= 0 && self.wealth_happiness >= 0, loc, || {
            "population.wealth: poverty_happiness must be <= 0 and wealth_happiness >= 0".into()
        });
    }
}

impl PopulationRules {
    /// Old-age death chance within one year of life at `age`, in percent.
    pub fn mortality_percent(&self, age: u32) -> f32 {
        self.mortality.iter().rev().find(|m| u32::from(m.from_age) <= age).map_or(0.0, |m| m.percent_per_year)
    }

    /// Checks value ranges.
    pub(crate) fn validate(&self, p: &mut Problems, loc: &Loc<'_>) {
        let finite_nonneg = |v: f32| v.is_finite() && v >= 0.0;
        let percent = |v: u8| v <= 100;
        p.check(self.adult_age > 0 && self.adult_age < self.elder_age, loc, || {
            "population: adult_age must be positive and below elder_age".into()
        });
        p.check(!self.mortality.is_empty(), loc, || "population: mortality needs at least one step".into());
        p.check(self.mortality.windows(2).all(|w| w[0].from_age < w[1].from_age), loc, || {
            "population: mortality steps must be ascending by age".into()
        });
        p.check(
            self.mortality.iter().all(|m| m.percent_per_year.is_finite() && (0.0..=100.0).contains(&m.percent_per_year)),
            loc,
            || "population: mortality percentages must be in 0..=100".into(),
        );
        p.check(self.mortality.last().is_some_and(|m| m.percent_per_year >= 100.0), loc, || {
            "population: the last mortality step must be 100 % (finite life span)".into()
        });
        for (name, v) in [
            ("food_per_day", self.food_per_day),
            ("pantry_days", self.pantry_days),
            ("need_change_per_hour", self.need_change_per_hour),
            ("hunger_decay_per_hour", self.hunger_decay_per_hour),
            ("happiness_change_per_hour", self.happiness_change_per_hour),
            ("health_change_per_hour", self.health_change_per_hour),
        ] {
            p.check(v.is_finite() && v > 0.0, loc, || format!("population: {name} must be positive"));
        }
        for (name, v) in [
            ("fuel_per_resident_day", self.fuel_per_resident_day),
            ("fuel_reserve_days", self.fuel_reserve_days),
            ("fuel_warning_days", self.fuel_warning_days),
            ("tax_happiness_per_percent", self.tax_happiness_per_percent),
            ("starvation_damage_per_hour", self.starvation_damage_per_hour),
            ("freezing_damage_per_hour", self.freezing_damage_per_hour),
            ("elder_health_loss_per_year", self.elder_health_loss_per_year),
            ("disease_death_per_mille", self.disease_death_per_mille),
            ("immigration_chance_per_point", self.immigration_chance_per_point),
            ("immigration_food_days", self.immigration_food_days),
            ("birth_chance_percent", self.birth_chance_percent),
        ] {
            p.check(finite_nonneg(v), loc, || format!("population: {name} must be >= 0"));
        }
        p.check(self.birth_chance_percent <= 100.0, loc, || "population: birth_chance_percent must be <= 100".into());
        for (name, v) in [
            ("child_food_percent", self.child_food_percent),
            ("need_met_threshold", self.need_met_threshold),
            ("no_water_thirst", self.no_water_thirst),
            ("base_safety", self.base_safety),
            ("base_health_need", self.base_health_need),
            ("base_social", self.base_social),
            ("base_entertainment", self.base_entertainment),
            ("base_faith", self.base_faith),
            ("winter_warmth_without_fuel", self.winter_warmth_without_fuel),
            ("freezing_warmth", self.freezing_warmth),
            ("hospital_disease_percent", self.hospital_disease_percent),
            ("emigration_happiness", self.emigration_happiness),
            ("immigration_happiness", self.immigration_happiness),
            ("immigration_max_chance_percent", self.immigration_max_chance_percent),
            ("immigration_daily_percent", self.immigration_daily_percent),
            ("immigrant_educated_percent", self.immigrant_educated_percent),
            ("birth_min_happiness", self.birth_min_happiness),
            ("marriage_chance_percent", self.marriage_chance_percent),
            ("educated_threshold", self.educated_threshold),
            ("full_productivity_health", self.full_productivity_health),
            ("full_productivity_happiness", self.full_productivity_happiness),
            ("tavern_without_goods_entertainment", self.tavern_without_goods_entertainment),
            ("sick_health", self.sick_health),
        ] {
            p.check(percent(v), loc, || format!("population: {name} must be <= 100"));
        }
        p.check(self.emigration_happiness < self.immigration_happiness, loc, || {
            "population: emigration_happiness must be below immigration_happiness".into()
        });
        p.check(self.emigration_days > 0, loc, || "population: emigration_days must be positive".into());
        p.check(self.fertile_min_age >= self.adult_age && self.fertile_min_age <= self.fertile_max_age, loc, || {
            "population: fertile ages must be adult and ascending".into()
        });
        p.check(self.marriage_min_age >= self.adult_age, loc, || "population: marriage_min_age must be adult".into());
        p.check(self.school_age < self.adult_age, loc, || "population: school_age must be below adult_age".into());
        p.check(self.downgrade_after_hours > 0, loc, || "population: downgrade_after_hours must be positive".into());
        p.check(self.max_friends <= 8, loc, || "population: max_friends must be <= 8".into());
        p.check(self.friendship_chance_permille <= 1000, loc, || "population: friendship_chance_permille must be <= 1000".into());
        p.check(self.hospital_recovery_percent >= 100, loc, || "population: hospital_recovery_percent must be >= 100".into());
        p.check(self.sick_leave_after_hours > 0, loc, || "population: sick_leave_after_hours must be positive".into());
        self.training.validate(p, loc);
        self.wealth.validate(p, loc);
        let w = self.need_weights;
        p.check(NeedKind::ALL.iter().all(|n| finite_nonneg(w.get(*n))) && w.total() > 0.0, loc, || {
            "population: need weights must be >= 0 with a positive sum".into()
        });
    }
}

/// German names for citizens (`names.ron`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NameLists {
    /// First names of men.
    pub male: Vec<String>,
    /// First names of women.
    pub female: Vec<String>,
    /// Family names (inherited by children).
    pub family: Vec<String>,
}

impl NameLists {
    /// Checks that every list has names and no name is blank.
    pub(crate) fn validate(&self, p: &mut Problems, file: &str) {
        let loc = Loc { file, kind: "names", key: "" };
        for (list, names) in [("male", &self.male), ("female", &self.female), ("family", &self.family)] {
            p.check(!names.is_empty(), &loc, || format!("the list '{list}' must not be empty"));
            p.check(names.iter().all(|n| !n.trim().is_empty()), &loc, || format!("the list '{list}' contains a blank name"));
        }
    }
}
