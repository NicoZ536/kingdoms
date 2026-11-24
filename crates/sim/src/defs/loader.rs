//! Reading, parsing, resolving and cross-validating the data files.
//!
//! Loading runs in phases and collects *all* problems of a phase before giving up:
//! 1. read + parse every file (`ron`, unknown fields are rejected),
//! 2. index the canonical string ids of every kind (format, duplicates),
//! 3. resolve each definition (string references → typed ids, value ranges),
//! 4. cross-validate (unlock derivation, upgrade chains, tech cycles, producers, scenarios).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::de::DeserializeOwned;

use super::building::{BuildingCategory, BuildingDef, BuildingRole, Gathering, RawBuilding};
use super::crop::{CropDef, RawCrop};
use super::diplomacy::RawKingdomData;
use super::error::{DefsError, Loc, Problems};
use super::events::{EventDef, RawEvent};
use super::ids::{BuildingTypeId, CropId, EventDefId, ResourceId, RoadTypeId, ScenarioId, TechId, UnitTypeId};
use super::resource::{RawResource, ResourceDef};
use super::road::{RawRoad, RoadDef};
use super::population::NameLists;
use super::rules::GameRules;
use super::scenario::{RawScenario, ScenarioDef};
use super::tech::{RawTech, TechDef, TechEffect};
use super::unit::{RawUnit, UnitDef};
use super::{DEFAULT_SCENARIO, DataFile, Defs};

/// Sub-directories holding lists of definitions (any number of `.ron` files each).
const LIST_DIRS: [&str; 5] = ["resources", "buildings", "technologies", "events", "scenarios"];
/// Single files directly in the data directory.
const SINGLE_FILES: [&str; 6] = ["roads.ron", "crops.ron", "units.ron", "rules.ron", "names.ron", "kingdoms.ron"];

/// A parsed definition together with the file it came from.
#[derive(Clone, Debug)]
pub(crate) struct Entry<T> {
    pub file: String,
    pub def: T,
}

/// All parsed (unresolved) data.
#[derive(Clone, Debug, Default)]
pub(crate) struct RawData {
    pub resources: Vec<Entry<RawResource>>,
    pub buildings: Vec<Entry<RawBuilding>>,
    pub roads: Vec<Entry<RawRoad>>,
    pub crops: Vec<Entry<RawCrop>>,
    pub techs: Vec<Entry<RawTech>>,
    pub units: Vec<Entry<RawUnit>>,
    pub events: Vec<Entry<RawEvent>>,
    pub scenarios: Vec<Entry<RawScenario>>,
    pub rules: Option<Entry<GameRules>>,
    pub names: Option<Entry<NameLists>>,
    pub kingdoms: Option<Entry<RawKingdomData>>,
}

/// Reads every data file below `dir` (sorted by path, so the result is deterministic).
pub(crate) fn read_dir(dir: &Path) -> Result<Vec<DataFile>, DefsError> {
    if !dir.is_dir() {
        return Err(DefsError::MissingDirectory(dir.to_path_buf()));
    }
    let mut problems = Problems::default();
    let mut files = Vec::new();
    for sub in LIST_DIRS {
        let sub_dir = dir.join(sub);
        let entries = match std::fs::read_dir(&sub_dir) {
            Ok(entries) => entries,
            Err(e) => {
                problems.file(sub, format!("cannot read directory: {e}"));
                continue;
            }
        };
        let mut names: Vec<String> = Vec::new();
        for entry in entries {
            match entry {
                Ok(entry) => {
                    let path = entry.path();
                    if path.is_file() && path.extension().is_some_and(|e| e == "ron") {
                        match path.file_name().and_then(|n| n.to_str()) {
                            Some(name) => names.push(name.to_string()),
                            None => problems.file(sub, format!("file name is not valid UTF-8: {}", path.display())),
                        }
                    }
                }
                Err(e) => problems.file(sub, format!("cannot list directory: {e}")),
            }
        }
        names.sort();
        for name in names {
            let rel = format!("{sub}/{name}");
            match std::fs::read_to_string(sub_dir.join(&name)) {
                Ok(text) => files.push(DataFile { path: rel, text }),
                Err(e) => problems.file(&rel, format!("cannot read file: {e}")),
            }
        }
    }
    for name in SINGLE_FILES {
        match std::fs::read_to_string(dir.join(name)) {
            Ok(text) => files.push(DataFile { path: name.to_string(), text }),
            Err(e) => problems.file(name, format!("cannot read file: {e}")),
        }
    }
    problems.into_result(files)
}

fn parse<T: DeserializeOwned>(file: &DataFile, problems: &mut Problems) -> Option<T> {
    match ron::Options::default().from_str::<T>(&file.text) {
        Ok(value) => Some(value),
        Err(e) => {
            problems.file(&file.path, format!("parse error at {e}"));
            None
        }
    }
}

fn push_all<T>(target: &mut Vec<Entry<T>>, file: &DataFile, list: Option<Vec<T>>) {
    if let Some(list) = list {
        target.extend(list.into_iter().map(|def| Entry { file: file.path.clone(), def }));
    }
}

impl RawData {
    /// Parses in-memory data files; paths are relative to the data directory with `/` separators.
    pub(crate) fn parse(files: &[DataFile]) -> Result<RawData, DefsError> {
        let mut problems = Problems::default();
        let mut raw = RawData::default();
        let mut sorted: Vec<&DataFile> = files.iter().collect();
        sorted.sort_by(|a, b| a.path.cmp(&b.path));
        for file in sorted {
            let path = file.path.as_str();
            let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
            if !name.ends_with(".ron") {
                problems.file(path, "data files must have the extension .ron");
                continue;
            }
            match (dir, name) {
                ("resources", _) => push_all(&mut raw.resources, file, parse(file, &mut problems)),
                ("buildings", _) => push_all(&mut raw.buildings, file, parse(file, &mut problems)),
                ("technologies", _) => push_all(&mut raw.techs, file, parse(file, &mut problems)),
                ("events", _) => push_all(&mut raw.events, file, parse(file, &mut problems)),
                ("scenarios", _) => {
                    if let Some(def) = parse::<RawScenario>(file, &mut problems) {
                        let stem = name.trim_end_matches(".ron");
                        if def.id != stem {
                            problems.file(path, format!("scenario id '{}' must match the file name '{stem}'", def.id));
                        }
                        raw.scenarios.push(Entry { file: file.path.clone(), def });
                    }
                }
                ("", "roads.ron") => push_all(&mut raw.roads, file, parse(file, &mut problems)),
                ("", "crops.ron") => push_all(&mut raw.crops, file, parse(file, &mut problems)),
                ("", "units.ron") => push_all(&mut raw.units, file, parse(file, &mut problems)),
                ("", "rules.ron") => {
                    if let Some(def) = parse::<GameRules>(file, &mut problems) {
                        raw.rules = Some(Entry { file: file.path.clone(), def });
                    }
                }
                ("", "names.ron") => {
                    if let Some(def) = parse::<NameLists>(file, &mut problems) {
                        raw.names = Some(Entry { file: file.path.clone(), def });
                    }
                }
                ("", "kingdoms.ron") => {
                    if let Some(def) = parse::<RawKingdomData>(file, &mut problems) {
                        raw.kingdoms = Some(Entry { file: file.path.clone(), def });
                    }
                }
                _ => problems.file(path, "unrecognized data file location"),
            }
        }
        if problems.is_empty() {
            let present = |n: usize, what: &str, problems: &mut Problems| {
                if n == 0 {
                    problems.file(what, "no definitions found");
                }
            };
            present(raw.resources.len(), "resources/", &mut problems);
            present(raw.buildings.len(), "buildings/", &mut problems);
            present(raw.techs.len(), "technologies/", &mut problems);
            present(raw.scenarios.len(), "scenarios/", &mut problems);
            present(raw.roads.len(), "roads.ron", &mut problems);
            present(raw.crops.len(), "crops.ron", &mut problems);
            present(raw.units.len(), "units.ron", &mut problems);
            if raw.rules.is_none() {
                problems.file("rules.ron", "missing");
            }
            if raw.names.is_none() {
                problems.file("names.ron", "missing");
            }
            if raw.kingdoms.is_none() {
                problems.file("kingdoms.ron", "missing");
            }
        }
        problems.into_result(raw)
    }
}

/// Resolution context: string-id indices of every kind plus the problem collector.
pub(crate) struct Ctx {
    pub problems: Problems,
    resources: BTreeMap<String, ResourceId>,
    buildings: BTreeMap<String, BuildingTypeId>,
    roads: BTreeMap<String, RoadTypeId>,
    crops: BTreeMap<String, CropId>,
    techs: BTreeMap<String, TechId>,
    units: BTreeMap<String, UnitTypeId>,
    events: BTreeMap<String, EventDefId>,
}

fn lookup<I: Copy>(
    map: &BTreeMap<String, I>,
    problems: &mut Problems,
    loc: &Loc<'_>,
    kind: &str,
    key: &str,
    what: &str,
) -> Option<I> {
    let found = map.get(key).copied();
    if found.is_none() {
        problems.at(loc, format!("unknown {kind} '{key}' in {what}"));
    }
    found
}

impl Ctx {
    /// Resolves a resource reference, recording a problem if it is unknown.
    pub fn resource(&mut self, loc: &Loc<'_>, key: &str, what: &str) -> Option<ResourceId> {
        lookup(&self.resources, &mut self.problems, loc, "resource", key, what)
    }
    /// Resolves a building reference.
    pub fn building(&mut self, loc: &Loc<'_>, key: &str, what: &str) -> Option<BuildingTypeId> {
        lookup(&self.buildings, &mut self.problems, loc, "building", key, what)
    }
    /// Resolves an event reference.
    pub fn event(&mut self, loc: &Loc<'_>, key: &str, what: &str) -> Option<EventDefId> {
        lookup(&self.events, &mut self.problems, loc, "event", key, what)
    }
    /// Resolves a road reference.
    pub fn road(&mut self, loc: &Loc<'_>, key: &str, what: &str) -> Option<RoadTypeId> {
        lookup(&self.roads, &mut self.problems, loc, "road", key, what)
    }
    /// Resolves a crop reference.
    pub fn crop(&mut self, loc: &Loc<'_>, key: &str, what: &str) -> Option<CropId> {
        lookup(&self.crops, &mut self.problems, loc, "crop", key, what)
    }
    /// Resolves a technology reference.
    pub fn tech(&mut self, loc: &Loc<'_>, key: &str, what: &str) -> Option<TechId> {
        lookup(&self.techs, &mut self.problems, loc, "technology", key, what)
    }
    /// Resolves a unit reference.
    pub fn unit(&mut self, loc: &Loc<'_>, key: &str, what: &str) -> Option<UnitTypeId> {
        lookup(&self.units, &mut self.problems, loc, "unit", key, what)
    }
}

fn valid_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Builds the string → typed-id index of one kind, reporting malformed and duplicate ids.
fn index<T, I: Copy>(
    entries: &[Entry<T>],
    kind: &'static str,
    key_of: impl Fn(&T) -> &str,
    make: impl Fn(usize) -> Option<I>,
    problems: &mut Problems,
) -> BTreeMap<String, I> {
    let mut map = BTreeMap::new();
    let mut first_file: BTreeMap<&str, &str> = BTreeMap::new();
    for (i, entry) in entries.iter().enumerate() {
        let key = key_of(&entry.def);
        let loc = Loc { file: &entry.file, kind, key };
        if !valid_key(key) {
            problems.at(&loc, "ids must be lower_snake_case ASCII starting with a letter");
        }
        if let Some(prev) = first_file.get(key) {
            problems.at(&loc, format!("duplicate id (first defined in {prev})"));
            continue;
        }
        first_file.insert(key, &entry.file);
        match make(i) {
            Some(id) => {
                map.insert(key.to_string(), id);
            }
            None => problems.at(&loc, format!("too many {kind} definitions for the id type")),
        }
    }
    map
}

fn loc<'a, T>(entry: &'a Entry<T>, kind: &'static str, key: &'a str) -> Loc<'a> {
    Loc { file: &entry.file, kind, key }
}

/// Collects `Option`s that are `None` only if a problem was recorded.
fn complete<T>(items: Vec<Option<T>>, problems: &mut Problems, kind: &str) -> Vec<T> {
    let n = items.len();
    let out: Vec<T> = items.into_iter().flatten().collect();
    if out.len() != n && problems.is_empty() {
        problems.file(kind, "internal error: unresolved definition without a recorded problem");
    }
    out
}

/// Resolves and validates parsed data into [`Defs`].
pub(crate) fn resolve(raw: &RawData) -> Result<Defs, DefsError> {
    let mut problems = Problems::default();
    let resources = index(&raw.resources, "resource", |d| &d.id, ResourceId::from_index, &mut problems);
    let buildings = index(&raw.buildings, "building", |d| &d.id, BuildingTypeId::from_index, &mut problems);
    let roads = index(&raw.roads, "road", |d| &d.id, RoadTypeId::from_index, &mut problems);
    let crops = index(&raw.crops, "crop", |d| &d.id, CropId::from_index, &mut problems);
    let techs = index(&raw.techs, "technology", |d| &d.id, TechId::from_index, &mut problems);
    let units = index(&raw.units, "unit", |d| &d.id, UnitTypeId::from_index, &mut problems);
    // Events refer to each other (`NoEventThisSeason`); scenario ids are only referenced by key
    // from code, so they are indexed for the checks.
    let events = index(&raw.events, "event", |d| &d.id, EventDefId::from_index, &mut problems);
    index(&raw.scenarios, "scenario", |d| &d.id, ScenarioId::from_index, &mut problems);
    if !problems.is_empty() {
        // Ids are indices; with duplicates/overflows they would be misaligned.
        return Err(problems.into_error());
    }

    let rules = match &raw.rules {
        Some(entry) => {
            entry.def.validate(&mut problems, &entry.file);
            entry.def.clone()
        }
        None => {
            problems.file("rules.ron", "missing");
            return Err(problems.into_error());
        }
    };
    let names = match &raw.names {
        Some(entry) => {
            entry.def.validate(&mut problems, &entry.file);
            entry.def.clone()
        }
        None => {
            problems.file("names.ron", "missing");
            return Err(problems.into_error());
        }
    };

    let mut ctx = Ctx { problems, resources, buildings, roads, crops, techs, units, events };
    let rules_file = raw.rules.as_ref().map_or("rules.ron", |e| e.file.as_str());
    let (ranks, army_mix) = super::military::resolve_military(&mut ctx, &rules, rules_file);

    let resource_defs: Vec<ResourceDef> = raw
        .resources
        .iter()
        .enumerate()
        .map(|(i, e)| e.def.resolve(&mut ctx, &loc(e, "resource", &e.def.id), id_at(i)))
        .collect();
    let crop_defs: Vec<Option<CropDef>> = raw
        .crops
        .iter()
        .enumerate()
        .map(|(i, e)| e.def.resolve(&mut ctx, &loc(e, "crop", &e.def.id), id_at(i)))
        .collect();
    let mut building_defs: Vec<BuildingDef> = raw
        .buildings
        .iter()
        .enumerate()
        .map(|(i, e)| e.def.resolve(&mut ctx, &loc(e, "building", &e.def.id), id_at(i), rules.default_buffer_capacity))
        .collect();
    let mut road_defs: Vec<RoadDef> = raw
        .roads
        .iter()
        .enumerate()
        .map(|(i, e)| e.def.resolve(&mut ctx, &loc(e, "road", &e.def.id), id_at(i)))
        .collect();
    let unit_defs: Vec<Option<UnitDef>> = raw
        .units
        .iter()
        .enumerate()
        .map(|(i, e)| e.def.resolve(&mut ctx, &loc(e, "unit", &e.def.id), id_at(i)))
        .collect();
    let tech_defs: Vec<TechDef> = raw
        .techs
        .iter()
        .enumerate()
        .map(|(i, e)| e.def.resolve(&mut ctx, &loc(e, "technology", &e.def.id), id_at(i)))
        .collect();
    let event_defs: Vec<EventDef> = raw
        .events
        .iter()
        .enumerate()
        .map(|(i, e)| e.def.resolve(&mut ctx, &loc(e, "event", &e.def.id), id_at(i)))
        .collect();
    let scenario_defs: Vec<ScenarioDef> = raw
        .scenarios
        .iter()
        .enumerate()
        .map(|(i, e)| e.def.resolve(&mut ctx, &loc(e, "scenario", &e.def.id), id_at(i), rules.max_tax_rate_percent))
        .collect();
    let most_kingdoms = crate::world::WorldSize::ALL.iter().map(|s| rules.diplomacy.kingdoms.get(*s)).max().unwrap_or(0);
    let kingdoms = match &raw.kingdoms {
        Some(entry) => entry.def.resolve(&mut ctx, &entry.file, most_kingdoms),
        None => {
            ctx.problems.file("kingdoms.ron", "missing");
            return Err(ctx.problems.into_error());
        }
    };

    let mut problems = ctx.problems;
    let crop_defs = complete(crop_defs, &mut problems, "crops.ron");
    let mut unit_defs = complete(unit_defs, &mut problems, "units.ron");
    if !problems.is_empty() {
        return Err(problems.into_error());
    }

    let checker = CrossCheck { raw, problems: &mut problems };
    checker.run(
        &resource_defs,
        &mut building_defs,
        &mut road_defs,
        &crop_defs,
        &mut unit_defs,
        &tech_defs,
        &scenario_defs,
    );

    let default_scenario = match scenario_defs.iter().find(|s| s.key == DEFAULT_SCENARIO) {
        Some(s) => s.id,
        None => {
            problems.file("scenarios/", format!("the scenario '{DEFAULT_SCENARIO}' is required"));
            return Err(problems.into_error());
        }
    };

    let defs = Defs::from_parts(super::DefsParts {
        resources: resource_defs,
        buildings: building_defs,
        roads: road_defs,
        crops: crop_defs,
        techs: tech_defs,
        units: unit_defs,
        events: event_defs,
        scenarios: scenario_defs,
        rules,
        names,
        kingdoms,
        ranks,
        army_mix,
        default_scenario,
    });
    problems.into_result(defs)
}

/// Typed id for list index `i`; indices were checked to fit in [`index`].
fn id_at<I: FromIndex>(i: usize) -> I {
    I::from_index_checked(i)
}

/// Conversion used by [`id_at`]; panics only if `index` skipped its overflow check.
trait FromIndex: Sized {
    fn from_index_checked(i: usize) -> Self;
}

macro_rules! impl_from_index {
    ($($t:ty),*) => {$(
        impl FromIndex for $t {
            fn from_index_checked(i: usize) -> Self {
                <$t>::from_index(i).expect("definition count was checked against the id range")
            }
        }
    )*};
}
impl_from_index!(ResourceId, BuildingTypeId, RoadTypeId, CropId, TechId, UnitTypeId, EventDefId, ScenarioId);

/// Validation across kinds of definitions.
struct CrossCheck<'a> {
    raw: &'a RawData,
    problems: &'a mut Problems,
}

fn building_loc<'a>(raw: &'a RawData, b: &BuildingDef) -> Loc<'a> {
    let entry = &raw.buildings[b.id.index()];
    Loc { file: &entry.file, kind: "building", key: &entry.def.id }
}

impl CrossCheck<'_> {
    #[allow(clippy::too_many_arguments)]
    fn run(
        mut self,
        resources: &[ResourceDef],
        buildings: &mut [BuildingDef],
        roads: &mut [RoadDef],
        crops: &[CropDef],
        units: &mut [UnitDef],
        techs: &[TechDef],
        scenarios: &[ScenarioDef],
    ) {
        self.derive_unlocks(buildings, roads, units, techs);
        self.check_units(buildings, units);
        self.check_upgrades(buildings);
        self.check_housing(resources, buildings);
        self.check_producers(resources, buildings, crops);
        self.check_tech_cycles(techs);
        self.check_scenarios(buildings, techs, scenarios);
    }

    /// `unlocked_by` of buildings, roads and units comes from tech effects; at most one each.
    fn derive_unlocks(
        &mut self,
        buildings: &mut [BuildingDef],
        roads: &mut [RoadDef],
        units: &mut [UnitDef],
        techs: &[TechDef],
    ) {
        for tech in techs {
            let entry = &self.raw.techs[tech.id.index()];
            let loc = Loc { file: &entry.file, kind: "technology", key: &entry.def.id };
            for effect in &tech.effects {
                let (slot, name) = match *effect {
                    TechEffect::UnlockBuilding(b) => {
                        let d = &mut buildings[b.index()];
                        (&mut d.unlocked_by, d.key.as_str())
                    }
                    TechEffect::UnlockRoad(r) => {
                        let d = &mut roads[r.index()];
                        (&mut d.unlocked_by, d.key.as_str())
                    }
                    TechEffect::UnlockUnit(u) => {
                        let d = &mut units[u.index()];
                        (&mut d.unlocked_by, d.key.as_str())
                    }
                    _ => continue,
                };
                match *slot {
                    Some(previous) => self.problems.at(
                        &loc,
                        format!(
                            "'{name}' is already unlocked by technology '{}'",
                            self.raw.techs[previous.index()].def.id
                        ),
                    ),
                    None => *slot = Some(tech.id),
                }
            }
        }
    }

    fn check_units(&mut self, buildings: &mut [BuildingDef], units: &[UnitDef]) {
        for unit in units {
            let entry = &self.raw.units[unit.id.index()];
            let loc = Loc { file: &entry.file, kind: "unit", key: &entry.def.id };
            let trainer = &mut buildings[unit.trained_at.index()];
            if trainer.category != BuildingCategory::Military {
                self.problems.at(&loc, format!("trained_at '{}' is not a military building", trainer.key));
            }
            trainer.trains.push(unit.id);
        }
    }

    /// Fills in the upgrade defaults (the target's construction cost and build hours) and checks
    /// that an upgrade keeps the building's state valid (docs/game_design.md §5): same role,
    /// houses only to the next tier, production only to production with the same gathering and
    /// the same recipes first (their indices stay valid), storage stays storage, no fewer
    /// workers, the same trained units, a footprint at least as large, no farms, walls or gates.
    fn check_upgrades(&mut self, buildings: &mut [BuildingDef]) {
        for i in 0..buildings.len() {
            let Some(target) = buildings[i].upgrade.as_ref().map(|u| u.to) else { continue };
            let raw = self.raw.buildings[i].def.upgrade.as_ref();
            let (default_cost, default_hours) =
                (raw.is_some_and(|u| u.cost.is_none()), raw.is_some_and(|u| u.hours.is_none()));
            let (cost, hours) = (buildings[target.index()].cost.clone(), buildings[target.index()].build_hours);
            if let Some(u) = buildings[i].upgrade.as_mut() {
                if default_cost {
                    u.cost = cost;
                }
                if default_hours {
                    u.hours = hours;
                }
            }
        }
        // Lineage (nearest ancestor first) and the units an upgrade keeps training. Chains are
        // short; the walk stops at the number of types (cycles are reported below).
        for i in 0..buildings.len() {
            let mut lineage = Vec::new();
            let mut current = buildings[i].id;
            while let Some(parent) = buildings.iter().find(|o| o.upgrade.as_ref().is_some_and(|u| u.to == current)).map(|o| o.id) {
                if parent == buildings[i].id || lineage.contains(&parent) || lineage.len() > buildings.len() {
                    break;
                }
                lineage.push(parent);
                current = parent;
            }
            let inherited: Vec<UnitTypeId> = lineage.iter().flat_map(|a| buildings[a.index()].trains.clone()).collect();
            let b = &mut buildings[i];
            for u in inherited {
                if !b.trains.contains(&u) {
                    b.trains.push(u);
                }
            }
            b.trains.sort();
            b.lineage = lineage;
        }
        let targets: BTreeSet<BuildingTypeId> = buildings.iter().filter_map(|b| b.upgrade.as_ref().map(|u| u.to)).collect();
        for b in buildings.iter() {
            let loc = building_loc(self.raw, b);
            if !b.buildable && !targets.contains(&b.id) {
                self.problems.at(&loc, "not buildable and not reachable by any upgrade");
            }
            let Some(up) = &b.upgrade else { continue };
            let t = &buildings[up.to.index()];
            let fail = |problems: &mut Problems, what: &str| {
                problems.at(&loc, format!("upgrade to '{}': {what}", t.key));
            };
            // Chains are short; a walk longer than the number of types is a cycle.
            let mut next = Some(up.to);
            let mut steps = 0;
            while let Some(n) = next {
                if n == b.id || steps > buildings.len() {
                    fail(self.problems, "the upgrade chain forms a cycle");
                    break;
                }
                next = buildings[n.index()].upgrade.as_ref().map(|u| u.to);
                steps += 1;
            }
            if t.role != b.role || matches!(b.role, BuildingRole::WallSegment | BuildingRole::Gate) {
                fail(self.problems, "needs the same role (walls and gates cannot be upgraded)");
            }
            if t.access != b.access {
                fail(self.problems, "needs the same access (a tower on a wall line stays reachable)");
            }
            if t.size.x < b.size.x || t.size.y < b.size.y {
                fail(self.problems, "the footprint must not shrink");
            }
            if t.workers < b.workers {
                fail(self.problems, "the target must not have fewer workers");
            }
            if !b.trains.iter().all(|u| t.trains.contains(u)) {
                fail(self.problems, "the target must train the same units");
            }
            if b.storage.is_some() && t.storage.is_none() {
                fail(self.problems, "a storage must stay a storage");
            }
            match (&b.production, &t.production) {
                (None, None) => {}
                (Some(p), Some(tp)) => {
                    let gathering = |g: &Option<Gathering>| g.as_ref().map(std::mem::discriminant);
                    if matches!(p.gathering, Some(Gathering::Fields { .. } | Gathering::Pasture { .. })) {
                        fail(self.problems, "farms with fields or pastures cannot be upgraded");
                    } else if gathering(&p.gathering) != gathering(&tp.gathering) {
                        fail(self.problems, "needs the same kind of gathering");
                    }
                    let same_first = p.recipes.len() <= tp.recipes.len()
                        && p.recipes.iter().zip(&tp.recipes).all(|(a, c)| a.key == c.key);
                    if !same_first {
                        fail(self.problems, "must start with the same recipes in the same order");
                    }
                }
                _ => fail(self.problems, "production must stay production"),
            }
            match (&b.housing, &t.housing) {
                (None, None) => {}
                (Some(h), Some(th)) => {
                    if th.tier != h.tier + 1 {
                        fail(self.problems, &format!("must have tier {}", h.tier + 1));
                    }
                    if !th.requirements.includes(&h.requirements) {
                        fail(self.problems, "its requirements must include those of this tier");
                    }
                }
                (Some(_), None) => fail(self.problems, "is not a house"),
                (None, Some(_)) => fail(self.problems, "only houses can become houses"),
            }
        }
    }

    fn check_housing(&mut self, resources: &[ResourceDef], buildings: &[BuildingDef]) {
        for b in buildings {
            let loc = building_loc(self.raw, b);
            let Some(h) = &b.housing else { continue };
            // Households consume the goods their tier asks for, so each needs a usage rate.
            for g in &h.requirements.goods {
                for r in &g.any_of {
                    if resources.get(r.index()).is_none_or(|d| d.household_use <= 0.0) {
                        let key = resources.get(r.index()).map_or("?", |d| d.key.as_str());
                        self.problems.at(&loc, format!("required good '{key}' needs a household_use > 0"));
                    }
                }
            }
            let predecessors =
                buildings.iter().filter(|o| o.upgrade.as_ref().map(|u| u.to) == Some(b.id)).count();
            if predecessors > 1 {
                self.problems.at(&loc, "at most one housing tier may upgrade to this house (downgrades go back)");
            }
        }
    }

    /// Every resource must be producible (recipe or crop) unless it is import-only, and
    /// import-only resources must not be produced.
    fn check_producers(&mut self, resources: &[ResourceDef], buildings: &[BuildingDef], crops: &[CropDef]) {
        let mut produced: BTreeSet<ResourceId> = crops.iter().map(|c| c.output).collect();
        for b in buildings {
            let Some(p) = &b.production else { continue };
            for r in &p.recipes {
                produced.extend(r.outputs.iter().map(|o| o.resource));
            }
        }
        for res in resources {
            let entry = &self.raw.resources[res.id.index()];
            let loc = Loc { file: &entry.file, kind: "resource", key: &entry.def.id };
            if res.import_only && produced.contains(&res.id) {
                self.problems.at(&loc, "import-only resource is produced by a recipe or crop");
            }
            if !res.import_only && !produced.contains(&res.id) {
                self.problems.at(&loc, "no building or crop produces this resource");
            }
        }
        // Field farms must reference crops; a crop should be grown by at least one farm.
        let mut grown: BTreeSet<CropId> = BTreeSet::new();
        for b in buildings {
            if let Some(Gathering::Fields { crops: list, .. }) =
                b.production.as_ref().and_then(|p| p.gathering.as_ref())
            {
                grown.extend(list.iter().copied());
            }
        }
        for crop in crops {
            if !grown.contains(&crop.id) {
                let entry = &self.raw.crops[crop.id.index()];
                let loc = Loc { file: &entry.file, kind: "crop", key: &entry.def.id };
                self.problems.at(&loc, "no farm grows this crop");
            }
        }
    }

    /// Detects prerequisite cycles (iterative DFS in index order).
    fn check_tech_cycles(&mut self, techs: &[TechDef]) {
        #[derive(Clone, Copy, PartialEq)]
        enum Mark {
            New,
            Active,
            Done,
        }
        let mut marks = vec![Mark::New; techs.len()];
        for start in 0..techs.len() {
            if marks[start] != Mark::New {
                continue;
            }
            let mut stack: Vec<(usize, usize)> = vec![(start, 0)];
            marks[start] = Mark::Active;
            while let Some(&mut (node, ref mut next)) = stack.last_mut() {
                if let Some(pre) = techs[node].prerequisites.get(*next) {
                    *next += 1;
                    let p = pre.index();
                    match marks[p] {
                        Mark::New => {
                            marks[p] = Mark::Active;
                            stack.push((p, 0));
                        }
                        Mark::Active => {
                            let entry = &self.raw.techs[p];
                            let loc = Loc { file: &entry.file, kind: "technology", key: &entry.def.id };
                            self.problems.at(&loc, "prerequisites form a cycle");
                        }
                        Mark::Done => {}
                    }
                } else {
                    marks[node] = Mark::Done;
                    stack.pop();
                }
            }
        }
    }

    fn check_scenarios(&mut self, buildings: &[BuildingDef], techs: &[TechDef], scenarios: &[ScenarioDef]) {
        for s in scenarios {
            let entry = &self.raw.scenarios[s.id.index()];
            let loc = Loc { file: &entry.file, kind: "scenario", key: &entry.def.id };
            match s.start_buildings.first() {
                Some(first) if buildings[first.building.index()].role == BuildingRole::Headquarters => {}
                _ => self.problems.at(&loc, "the first start building must be a headquarters"),
            }
            let hq = s
                .start_buildings
                .iter()
                .filter(|b| buildings[b.building.index()].role == BuildingRole::Headquarters)
                .count();
            if hq > 1 {
                self.problems.at(&loc, "only one headquarters may be placed at the start");
            }
            for t in &s.researched {
                for pre in &techs[t.index()].prerequisites {
                    if !s.researched.contains(pre) {
                        self.problems.at(
                            &loc,
                            format!("researched '{}' requires '{}'", techs[t.index()].key, techs[pre.index()].key),
                        );
                    }
                }
            }
        }
    }
}
