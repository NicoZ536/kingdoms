//! German number and text formatting for the HUD, and text summaries of game data (costs,
//! recipes, tile information). Pure functions, unit tested.

use glam::IVec2;
use kingdoms_sim::defs::{BuildingDef, Cost, Defs, RecipeDef, ResourceRate};
use kingdoms_sim::economy::{Economy, MILLI_PER_GOLD};
use kingdoms_sim::grid::Direction;
use kingdoms_sim::state::GameState;
use kingdoms_sim::world::{Tile, TreeKind, Vegetation};

/// German names of the 16 compass points, clockwise from north (the simulation's map north,
/// −y).
const COMPASS: [&str; 16] = [
    "Nord", "Nordnordost", "Nordost", "Ostnordost", "Ost", "Ostsüdost", "Südost", "Südsüdost", "Süd", "Südsüdwest", "Südwest",
    "Westsüdwest", "West", "Westnordwest", "Nordwest", "Nordnordwest",
];

/// The wind in words: where it comes from (`towards` is the compass point it blows to, 0–15
/// as `WeatherState::wind_direction`) and how strong it is (0–100), e.g. „aus West, frisch“.
pub fn wind_text(towards: u8, strength: u8) -> String {
    let word = match strength {
        0..15 => return "windstill".to_owned(),
        15..35 => "schwach",
        35..55 => "mäßig",
        55..70 => "frisch",
        70..85 => "stürmisch",
        _ => "Orkan",
    };
    format!("aus {}, {word}", COMPASS[usize::from((towards % 16 + 8) % 16)])
}

/// The typographic minus sign (U+2212) that every negative number of the game uses.
pub const MINUS: char = '−';

/// Integer with German thousands separators ("12.345", "−1.200").
pub fn int(v: i64) -> String {
    let digits = v.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    if v < 0 { format!("{MINUS}{out}") } else { out }
}

/// Decimal number with a German comma and at most `decimals` places, trailing zeros removed
/// ("1,5", "2", "0,25", "−3,5").
pub fn decimal(v: f64, decimals: usize) -> String {
    let mut s = format!("{v:.decimals$}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s == "-0" {
        s = "0".into();
    }
    s.replace('.', ",").replace('-', "−")
}

/// Decimal number with exactly `decimals` places and a German comma („3,50“, „−1,20“): prices
/// in a column line up.
pub fn decimal_fixed(v: f64, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    let s = if s.trim_start_matches(['-', '0', '.']).is_empty() { s.trim_start_matches('-').to_owned() } else { s };
    s.replace('.', ",").replace('-', "−")
}

/// Signed whole number with thousands separators („+12“, „−1.500“, „0“).
pub fn signed(v: i64) -> String {
    if v > 0 { format!("+{}", int(v)) } else { int(v) }
}

/// Signed gold amount from milli-gold with one decimal ("+28,1", "−3", "0").
pub fn signed_gold_milli(milli: i64) -> String {
    let gold = milli as f64 / MILLI_PER_GOLD as f64;
    let text = decimal(gold, 1);
    if text == "0" || gold < 0.0 { text } else { format!("+{text}") }
}

/// Gold amount per unit of a good from milli-gold: two decimals below 10 Gold, so cheap goods
/// do not read as free („0,03“, „3,8“, „12,5“).
pub fn unit_gold_milli(milli: i64) -> String {
    let gold = milli as f64 / MILLI_PER_GOLD as f64;
    decimal(gold, if milli.abs() < 10 * MILLI_PER_GOLD { 2 } else { 1 })
}

/// A game duration from minutes in the one German form the game and the simulation's reports
/// use: „45 min“, „2 h“, „7 h 30 min“.
pub fn duration_minutes(minutes: u64) -> String {
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// A longer game duration from whole hours: „1 h“, „14 h“, from two days on in days („2 Tage“,
/// rounded up).
pub fn duration_hours(hours: u64) -> String {
    match hours {
        0 | 1 => "1 h".to_owned(),
        2..=47 => format!("{hours} h"),
        _ => format!("{} Tage", hours.div_ceil(24)),
    }
}

/// Game hours ("2 h", "1,5 h").
pub fn hours(h: f32) -> String {
    format!("{} h", decimal(f64::from(h), 1))
}

/// Recipe amounts like "2 Holz + 1 Kohle".
pub fn amounts(defs: &Defs, list: &[ResourceRate]) -> String {
    list.iter()
        .map(|r| {
            let name = defs.resources().get(r.resource.index()).map_or("?", |d| d.name.as_str());
            format!("{} {name}", decimal(f64::from(r.amount), 2))
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

/// Recipe summary: "2 Holz → 3 Bretter (2 h)"; a recipe without goods (planting saplings)
/// shows its name: „Setzling pflanzen (2 h)“.
pub fn recipe_summary(defs: &Defs, recipe: &RecipeDef) -> String {
    let outputs = if recipe.outputs.is_empty() { recipe.name.clone() } else { amounts(defs, &recipe.outputs) };
    if recipe.inputs.is_empty() {
        format!("{outputs} ({})", hours(recipe.hours))
    } else {
        format!("{} → {outputs} ({})", amounts(defs, &recipe.inputs), hours(recipe.hours))
    }
}

/// Explanation of recipe and build durations: they are round-the-clock hours, which the
/// workers catch up on during their shift (docs/game_design.md §5.2).
pub fn shift_note(defs: &Defs) -> String {
    let routine = &defs.rules().walking.routine;
    let factor = routine.shift_factor_permille() as f64 / 1000.0;
    format!(
        "Die Stunden gelten für Betrieb rund um die Uhr. Die Arbeiter schaffen in ihrer Schicht von {} h \
         die Arbeit von 24 h ({}-fach schneller); ein Arbeitsgang dauert in Arbeitszeit also nur den \
         {}. Teil, die Tagesmenge bleibt gleich – jede Minute Arbeitsweg kostet davon.",
        routine.shift_hours(),
        decimal(factor, 1),
        decimal(factor, 1),
    )
}

/// One part of a price and whether the kingdom can pay it now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CostItem {
    /// Icon sprite name (`res_<id>` or `ui_gold`).
    pub icon: String,
    /// German name.
    pub name: String,
    pub needed: u32,
    /// Gold in the treasury or freely available stock.
    pub available: i64,
}

impl CostItem {
    pub fn affordable(&self) -> bool {
        self.available >= i64::from(self.needed)
    }
}

/// Gold and materials of `cost` with what is available (gold first, then materials in data
/// order). Uses the same measure as the simulation's affordability check (free stock =
/// stock minus materials bound to construction sites).
pub fn cost_items(defs: &Defs, economy: &Economy, cost: &Cost) -> Vec<CostItem> {
    let mut items = Vec::new();
    if cost.gold > 0 {
        items.push(CostItem { icon: "ui_gold".into(), name: "Gold".into(), needed: cost.gold, available: economy.treasury() });
    }
    for m in &cost.materials {
        let Some(def) = defs.resources().get(m.resource.index()) else { continue };
        items.push(CostItem {
            icon: format!("res_{}", def.key),
            name: def.name.clone(),
            needed: m.amount,
            available: i64::from(economy.available(m.resource)),
        });
    }
    items
}

/// "2 × 3" footprint text.
pub fn size(def: &BuildingDef) -> String {
    format!("{} × {}", def.size.x, def.size.y)
}

/// German name of a compass direction ("Süden").
pub fn direction(dir: Direction) -> &'static str {
    match dir {
        Direction::North => "Norden",
        Direction::East => "Osten",
        Direction::South => "Süden",
        Direction::West => "Westen",
    }
}

/// A count with the singular or plural noun: `count(1, "Tag", "Tage")` → „1 Tag“, otherwise
/// „3 Tage“ (also „0 Tage“).
pub fn count(n: impl Into<u64>, one: &str, many: &str) -> String {
    let n = n.into();
    if n == 1 { format!("1 {one}") } else { format!("{n} {many}") }
}

/// Days with one decimal: „1 Tag“, „0,5 Tage“, „12,3 Tage“.
pub fn days_decimal(days: f64) -> String {
    let text = decimal(days, 1);
    if text == "1" { "1 Tag".to_owned() } else { format!("{text} Tage") }
}

/// Days with one decimal in the dative („nach 1 Tag“, „nach 2,5 Tagen“).
pub fn days_dative(days: f64) -> String {
    let text = decimal(days, 1);
    if text == "1" { "1 Tag".to_owned() } else { format!("{text} Tagen") }
}

/// An age in years: „1 Jahr“, „34 Jahre“.
pub fn years(age: u32) -> String {
    count(age, "Jahr", "Jahre")
}

/// A number of map tiles with the right noun: „1 Feld“, „3 Felder“.
pub fn tiles(n: usize) -> String {
    if n == 1 { "1 Feld".to_owned() } else { format!("{n} Felder") }
}

/// German name of a tree kind.
pub fn tree_name(kind: TreeKind) -> &'static str {
    match kind {
        TreeKind::Deciduous => "Eiche",
        TreeKind::Conifer => "Kiefer",
        TreeKind::Birch => "Birke",
        TreeKind::Willow => "Weide",
    }
}

/// Fertility as the simulation's raw value (0–255, like the thresholds in the design) with the
/// verdict for fields, e.g. „128 · Ackerboden“ or „45 · zu karg für Felder (ab 60)“.
pub fn fertility_text(defs: &Defs, tile: &Tile) -> String {
    let min = defs.rules().agriculture.min_field_fertility;
    let verdict = if kingdoms_sim::agriculture::is_arable(defs, tile) {
        "Ackerboden".to_owned()
    } else if tile.fertility < min {
        format!("zu karg für Felder (ab {min})")
    } else {
        "kein Ackerboden".to_owned()
    };
    format!("{} · {verdict}", tile.fertility)
}

/// Label/value rows describing a map tile (hover tooltip and tile panel).
pub fn tile_rows(state: &GameState, defs: &Defs, p: IVec2) -> Vec<(String, String)> {
    let Some(tile) = state.world().get(p) else { return Vec::new() };
    let mut rows = vec![
        ("Gelände".to_owned(), format!("{} · {}", tile.terrain.display_name(), tile.biome.display_name())),
        ("Höhe".to_owned(), format!("Stufe {}", tile.elevation)),
    ];
    if !tile.is_water() {
        rows.push(("Fruchtbarkeit".to_owned(), fertility_text(defs, tile)));
    }
    match tile.vegetation {
        Vegetation::None => {}
        Vegetation::Tree { kind, growth } => {
            rows.push(("Bewuchs".to_owned(), format!("{} (gewachsen {} %)", tree_name(kind), u32::from(growth) * 100 / 255)));
        }
        Vegetation::Stump { .. } => rows.push(("Bewuchs".to_owned(), "Baumstumpf".to_owned())),
        Vegetation::Bush => rows.push(("Bewuchs".to_owned(), "Gebüsch".to_owned())),
        Vegetation::Boulder => rows.push(("Bewuchs".to_owned(), "Felsbrocken".to_owned())),
    }
    if let Some(d) = tile.deposit.filter(|d| d.amount > 0) {
        let units = if d.amount == 1 { "Einheit" } else { "Einheiten" };
        rows.push(("Vorkommen".to_owned(), format!("{} ({} {units})", d.kind.display_name(), int(i64::from(d.amount)))));
    }
    if tile.river {
        rows.push(("Wasser".to_owned(), "Fluss (Süßwasser)".to_owned()));
    }
    let owner = match state.diplomacy().land().owner(p).and_then(|k| state.diplomacy().kingdom(k)) {
        Some(k) if state.diplomacy().land().has_structure(p) => format!("{} (Bauwerk)", k.name()),
        Some(k) => format!("Königreich {}", k.name()),
        None if state.territory().contains(p) => "Eigenes Gebiet".to_owned(),
        None => "Außerhalb des Gebiets".to_owned(),
    };
    rows.push(("Gebiet".to_owned(), owner));
    if let Some(road) = state.roads().get(p) {
        let name = defs.roads().get(road.road.index()).map_or("Straße", |r| r.name.as_str());
        let text = if road.is_bridge() { "Brücke".to_owned() } else { name.to_owned() };
        rows.push(("Weg".to_owned(), text));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_prices_keep_their_cents() {
        assert_eq!(unit_gold_milli(34), "0,03");
        assert_eq!(unit_gold_milli(3_800), "3,8");
        assert_eq!(unit_gold_milli(1_000), "1");
        assert_eq!(unit_gold_milli(12_480), "12,5");
    }

    #[test]
    fn wind_is_named_by_where_it_comes_from() {
        // Blowing towards east (4) means wind from the west.
        assert_eq!(wind_text(4, 40), "aus West, mäßig");
        assert_eq!(wind_text(0, 90), "aus Süd, Orkan");
        assert_eq!(wind_text(10, 60), "aus Nordost, frisch");
        assert_eq!(wind_text(9, 75), "aus Nordnordost, stürmisch");
        assert_eq!(wind_text(3, 5), "windstill");
        assert_eq!(wind_text(20, 20), "aus West, schwach");
    }

    #[test]
    fn counts_use_the_singular_for_one() {
        assert_eq!(count(1u32, "Tag", "Tage"), "1 Tag");
        assert_eq!(count(0u32, "Tag", "Tage"), "0 Tage");
        assert_eq!(count(3u16, "Stunde", "Stunden"), "3 Stunden");
        assert_eq!(years(1), "1 Jahr");
        assert_eq!(years(34), "34 Jahre");
        assert_eq!(duration_minutes(0), "0 min");
        assert_eq!(duration_minutes(45), "45 min");
        assert_eq!(duration_minutes(120), "2 h");
        assert_eq!(duration_minutes(95), "1 h 35 min");
        // The same form as the simulation's report texts.
        for m in [0u32, 35, 60, 95, 936] {
            assert_eq!(duration_minutes(u64::from(m)), kingdoms_sim::report::minutes_text(m));
        }
        assert_eq!(duration_hours(0), "1 h");
        assert_eq!(duration_hours(14), "14 h");
        assert_eq!(duration_hours(48), "2 Tage");
        assert_eq!(duration_hours(49), "3 Tage");
        assert_eq!(days_decimal(1.0), "1 Tag");
        assert_eq!(days_decimal(1.04), "1 Tag");
        assert_eq!(days_decimal(0.5), "0,5 Tage");
        assert_eq!(days_decimal(12.34), "12,3 Tage");
    }

    #[test]
    fn german_numbers() {
        assert_eq!(int(0), "0");
        assert_eq!(int(999), "999");
        assert_eq!(int(1000), "1.000");
        assert_eq!(int(-1_234_567), "−1.234.567");
        assert_eq!(decimal(-3.5, 1), "−3,5");
        assert_eq!(decimal(-617.0, 0), "−617");
        assert_eq!(decimal_fixed(3.0, 2), "3,00");
        assert_eq!(decimal_fixed(2.09, 2), "2,09");
        assert_eq!(decimal_fixed(-1.2, 2), "−1,20");
        assert_eq!(decimal_fixed(-0.001, 2), "0,00");
        assert_eq!(signed(12), "+12");
        assert_eq!(signed(-1500), "−1.500");
        assert_eq!(signed(0), "0");
        assert_eq!(decimal(1.5, 1), "1,5");
        assert_eq!(decimal(2.0, 2), "2");
        assert_eq!(decimal(0.25, 2), "0,25");
        assert_eq!(decimal(-0.01, 1), "0");
        assert_eq!(signed_gold_milli(28_100), "+28,1");
        assert_eq!(signed_gold_milli(-3_000), "−3");
        assert_eq!(signed_gold_milli(0), "0");
        assert_eq!(hours(2.0), "2 h");
        assert_eq!(hours(1.5), "1,5 h");
        assert_eq!(tiles(1), "1 Feld");
        assert_eq!(days_dative(1.0), "1 Tag");
        assert_eq!(days_dative(2.5), "2,5 Tagen");
        assert_eq!(count(1u32, "Platz", "Plätze"), "1 Platz");
        assert_eq!(tiles(0), "0 Felder");
        assert_eq!(tiles(12), "12 Felder");
    }

    fn defs() -> Defs {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        match Defs::load(&dir) {
            Ok(defs) => defs,
            Err(e) => panic!("shipped game data must load: {e}"),
        }
    }

    #[test]
    fn recipes_costs_and_tiles() {
        let defs = defs();
        let sawmill = defs.building_by_key("sawmill").expect("sawmill");
        let recipe = &sawmill.production.as_ref().expect("production").recipes[0];
        assert_eq!(recipe_summary(&defs, recipe), "2 Holz → 3 Bretter (2 h)");
        assert_eq!(size(sawmill), "2 × 3");
        // The forester's recipe yields no goods: its name stands in for the outputs.
        let forester = defs.building_by_key("forester").expect("forester");
        let plant = &forester.production.as_ref().expect("production").recipes[0];
        assert_eq!(recipe_summary(&defs, plant), "Setzling pflanzen (2 h)");

        let state = GameState::new(&defs, kingdoms_sim::state::GameSettings::new(&defs, 7, kingdoms_sim::world::WorldSize::Small));
        let items = cost_items(&defs, state.economy(), &sawmill.cost);
        assert_eq!(items.first().map(|i| i.icon.as_str()), Some("ui_gold"));
        assert!(items.iter().all(|i| i.needed > 0));
        assert!(items.iter().any(|i| i.icon.starts_with("res_")));
        let gold = &items[0];
        assert_eq!(gold.available, state.economy().treasury());
        assert_eq!(gold.affordable(), state.economy().treasury() >= i64::from(gold.needed));

        let rows = tile_rows(&state, &defs, state.world().start);
        assert!(rows.iter().any(|(l, _)| l == "Gelände"));
        assert!(rows.iter().any(|(l, v)| l == "Gebiet" && v == "Eigenes Gebiet"));
        assert!(tile_rows(&state, &defs, IVec2::new(-1, 0)).is_empty());
    }

    #[test]
    fn fertility_uses_the_raw_value_of_the_rules() {
        use kingdoms_sim::world::{Biome, Terrain};
        let defs = defs();
        let min = defs.rules().agriculture.min_field_fertility;
        let tile = |terrain, fertility| Tile {
            terrain,
            biome: Biome::Grassland,
            elevation: 2,
            fertility,
            moisture: 128,
            temperature: 128,
            vegetation: Vegetation::None,
            deposit: None,
            river: false,
        };
        // Exactly at the threshold of the design (≥ 60) the soil is arable.
        assert_eq!(fertility_text(&defs, &tile(Terrain::Grass, min)), format!("{min} · Ackerboden"));
        assert_eq!(fertility_text(&defs, &tile(Terrain::Grass, min - 1)), format!("{} · zu karg für Felder (ab {min})", min - 1));
        assert_eq!(fertility_text(&defs, &tile(Terrain::Sand, 200)), "200 · kein Ackerboden");
    }
}
