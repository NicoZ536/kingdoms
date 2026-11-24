//! Start parameters of a game read from the environment (developer and testing aids; players
//! start games from the main menu).

use glam::IVec2;
use kingdoms_engine::core::time::GameSpeed;
use kingdoms_sim::defs::Weather;
use kingdoms_sim::world::WorldSize;

use super::{DevCase, DevPlan};

/// Variables that start a game directly instead of the main menu (developer and test runs).
pub const DIRECT_START_VARIABLES: [&str; 26] = [
    "KINGDOMS_SEED",
    "KINGDOMS_WORLD_SIZE",
    "KINGDOMS_WARP_TICKS",
    "KINGDOMS_DEV_SETTLEMENT",
    "KINGDOMS_DEV_UNLOCK",
    "KINGDOMS_DEV_WEATHER",
    "KINGDOMS_DEV_CROWD",
    "KINGDOMS_DEV_SHOWCASE",
    "KINGDOMS_DEV_EVENT",
    "KINGDOMS_DEV_RESEARCH",
    "KINGDOMS_DEV_ARMY",
    "KINGDOMS_DEV_WALLS",
    "KINGDOMS_DEV_THREAT",
    "KINGDOMS_DEV_TRADE",
    "KINGDOMS_DEV_UPGRADE",
    "KINGDOMS_DEV_CASE",
    "KINGDOMS_FOCUS",
    "KINGDOMS_ZOOM",
    "KINGDOMS_SELECT",
    "KINGDOMS_TOOL",
    "KINGDOMS_CURSOR",
    "KINGDOMS_MENU",
    "KINGDOMS_SPEED",
    "KINGDOMS_WINDOW",
    "KINGDOMS_PANEL",
    "KINGDOMS_QUICK",
];

/// What the game shows after loading its graphics.
#[derive(Clone, Debug, PartialEq)]
pub enum StartMode {
    /// The main menu (the normal start).
    Menu,
    /// A game started right away with these options.
    Game(Box<LaunchOptions>),
}

impl StartMode {
    /// `KINGDOMS_START=menu|game` decides; otherwise any of [`DIRECT_START_VARIABLES`] starts a
    /// game directly, so scripted runs keep working without the menu.
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let direct = match lookup("KINGDOMS_START").map(|v| v.trim().to_ascii_lowercase()) {
            Some(v) if v == "menu" => false,
            Some(v) if v == "game" => true,
            Some(other) => {
                log::warn!("KINGDOMS_START={other:?} is invalid (menu or game)");
                DIRECT_START_VARIABLES.iter().any(|k| lookup(k).is_some())
            }
            None => DIRECT_START_VARIABLES.iter().any(|k| lookup(k).is_some()),
        };
        if direct { StartMode::Game(Box::new(LaunchOptions::from_lookup(lookup))) } else { StartMode::Menu }
    }
}

/// World seed used when `KINGDOMS_SEED` is not set.
pub const DEFAULT_SEED: u64 = 7;

/// How the game session is started.
#[derive(Clone, Debug, PartialEq)]
pub struct LaunchOptions {
    /// World and simulation seed (`KINGDOMS_SEED`).
    pub seed: u64,
    /// Map size (`KINGDOMS_WORLD_SIZE` = small | medium | large | huge).
    pub world_size: WorldSize,
    /// Simulation ticks run right after the game was created (`KINGDOMS_WARP_TICKS`).
    pub warp_ticks: u64,
    /// Lay out a test settlement through regular commands (`KINGDOMS_DEV_SETTLEMENT=1`
    /// showcase, `=2` growing town).
    pub dev_settlement: Option<DevPlan>,
    /// Unlock all technologies at the start with `DebugUnlockAllResearch`
    /// (`KINGDOMS_DEV_UNLOCK=1`).
    pub dev_unlock: bool,
    /// Weather forced after the warp ticks with `DebugSetWeather`
    /// (`KINGDOMS_DEV_WEATHER=<weather>[:<intensity>[:<days>]]`).
    pub dev_weather: Option<DevWeather>,
    /// Start the benchmark town with this many settlers instead of the default scenario
    /// (`KINGDOMS_DEV_CROWD=<settlers>`, see `dev_crowd`).
    pub dev_crowd: Option<u16>,
    /// Start the showcase of every building instead of the default scenario
    /// (`KINGDOMS_DEV_SHOWCASE=1`, see `dev_showcase`; `KINGDOMS_DEV_CROWD` wins if both are
    /// set).
    pub dev_showcase: bool,
    /// Event started after the warp ticks with `DebugTriggerEvent`, optionally decided right
    /// away with a choice (`KINGDOMS_DEV_EVENT=<event>[:<choice>]`).
    pub dev_event: Option<DevEvent>,
    /// Technologies queued at the start with `QueueResearch`, in order
    /// (`KINGDOMS_DEV_RESEARCH=<tech>,<tech>,…`).
    pub dev_research: Vec<String>,
    /// Raise an army of this many soldiers during the warp (`KINGDOMS_DEV_ARMY=<soldiers>`,
    /// see `dev_army`).
    pub dev_army: Option<u32>,
    /// Ring the headquarters with a town wall and gates this many tiles out during the warp
    /// (`KINGDOMS_DEV_WALLS=<radius>`, see `dev_walls`).
    pub dev_walls: Option<u32>,
    /// Enemies sent after the warp (`KINGDOMS_DEV_THREAT=banditen|<kingdom 1–7>[:<size>]`; an
    /// army declares war first).
    pub dev_threat: Option<DevThreat>,
    /// Build a trading post, set matching orders and send a caravan during the warp
    /// (`KINGDOMS_DEV_TRADE=1`, see `dev_trade`).
    pub dev_trade: bool,
    /// After the warp, upgrade every finished building that has a next level (houses excepted)
    /// with granted costs, then run this many game hours (`KINGDOMS_DEV_UPGRADE=<hours>`, 1–96).
    pub dev_upgrade: Option<u32>,
    /// After the warp, set up a state rarely seen in play and look at it
    /// (`KINGDOMS_DEV_CASE=gasthaus|einarbeitung|lagerferne`, see `dev_case`).
    pub dev_case: Option<DevCase>,
    /// Tile the camera starts on instead of the headquarters (`KINGDOMS_FOCUS=x,y`).
    pub focus: Option<IVec2>,
    /// The camera follows the first enemy group (`KINGDOMS_FOCUS=feind`) or the first caravan
    /// (`KINGDOMS_FOCUS=karawane`) on the map.
    pub follow: Option<FollowTarget>,
    /// Initial camera zoom (`KINGDOMS_ZOOM`).
    pub zoom: Option<f32>,
    /// Tile whose building (or the tile itself) starts selected (`KINGDOMS_SELECT=x,y`).
    pub select: Option<IVec2>,
    /// Tool active at the start (`KINGDOMS_TOOL`, screenshots): `build:<building>[:<rotation>]`,
    /// `road:<road>[:<x>,<y>]` (with drag start), `demolish`, `move:<x>,<y>`.
    pub tool: Option<ToolSpec>,
    /// Tile the cursor is treated as resting on, overriding the mouse (`KINGDOMS_CURSOR=x,y`).
    pub cursor: Option<IVec2>,
    /// Open the pause menu at the start (`KINGDOMS_MENU=1`).
    pub menu: bool,
    /// Game speed at the start (`KINGDOMS_SPEED=pause|1|2|4`; default normal speed).
    pub speed: Option<GameSpeed>,
    /// Management window tab open at the start (`KINGDOMS_WINDOW=wirtschaft|lager|produktion|
    /// finanzen|handel|arbeit|bevoelkerung|landwirtschaft|forschung|statistik`; resolved by the
    /// world scene).
    pub window: Option<String>,
    /// HUD panel open at the start (`KINGDOMS_PANEL=bauen|strassen|landwirtschaft|produktion|
    /// militaer|ereignisse|meldungen|uebersicht`; resolved by the world scene).
    pub panel: Option<String>,
    /// Quicksave right after the start or continue from the quicksave (`KINGDOMS_QUICK=save|load`).
    pub quick: Option<QuickStart>,
}

/// Event started by `KINGDOMS_DEV_EVENT`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DevEvent {
    /// Event id of the data (`events.ron`).
    pub event: String,
    /// Choice taken right away (index, 0 = the default choice).
    pub choice: Option<u8>,
}

impl DevEvent {
    /// Parses `<event>[:<choice>]`.
    pub fn parse(value: &str) -> Option<Self> {
        let mut parts = value.split(':');
        let event = parts.next()?.trim().to_owned();
        if event.is_empty() {
            return None;
        }
        let choice = match parts.next() {
            Some(c) => Some(c.trim().parse().ok()?),
            None => None,
        };
        parts.next().is_none().then_some(Self { event, choice })
    }
}

/// What the camera follows (`KINGDOMS_FOCUS=feind|karawane`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FollowTarget {
    /// The first enemy group on the map.
    Enemy,
    /// The merchant of the first caravan on the map.
    Caravan,
}

/// Enemies sent by `KINGDOMS_DEV_THREAT`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DevThreat {
    /// Kingdom number (1-based, in id order); `None`: bandits.
    pub kingdom: Option<u8>,
    /// Fighters (1–120, default 12).
    pub size: u16,
}

impl DevThreat {
    /// Parses `banditen|bandits|<kingdom 1–7>[:<size>]`.
    pub fn parse(value: &str) -> Option<Self> {
        let mut parts = value.split(':').map(str::trim);
        let kingdom = match parts.next()?.to_lowercase().as_str() {
            "banditen" | "bandits" | "raeuber" | "räuber" => None,
            n => Some(n.parse::<u8>().ok().filter(|k| (1..=7).contains(k))?),
        };
        let size = match parts.next() {
            Some(v) => v.parse::<u16>().ok().filter(|s| (1..=120).contains(s))?,
            None => 12,
        };
        parts.next().is_none().then_some(Self { kingdom, size })
    }
}

/// Weather forced by `KINGDOMS_DEV_WEATHER`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DevWeather {
    pub weather: Weather,
    /// Intensity in percent (1–100, default 80).
    pub intensity: u8,
    /// Days it lasts (default 1).
    pub days: u16,
}

impl DevWeather {
    /// Parses `<weather>[:<intensity>[:<days>]]`; the weather in English or German (`clear`/
    /// `klar`, `cloudy`/`bewoelkt`, `rain`/`regen`, `storm`/`sturm`, `snow`/`schnee`,
    /// `drought`/`duerre`).
    pub fn parse(value: &str) -> Option<Self> {
        let mut parts = value.split(':').map(str::trim);
        let weather = match parts.next()?.to_lowercase().as_str() {
            "clear" | "klar" => Weather::Clear,
            "cloudy" | "bewoelkt" | "bewölkt" => Weather::Cloudy,
            "rain" | "regen" => Weather::Rain,
            "storm" | "sturm" => Weather::Storm,
            "snow" | "schnee" => Weather::Snow,
            "drought" | "duerre" | "dürre" => Weather::Drought,
            _ => return None,
        };
        let intensity = match parts.next() {
            Some(v) => v.parse::<u8>().ok().filter(|i| (1..=100).contains(i))?,
            None => 80,
        };
        let days = match parts.next() {
            Some(v) => v.parse::<u16>().ok().filter(|d| *d > 0)?,
            None => 1,
        };
        parts.next().is_none().then_some(Self { weather, intensity, days })
    }
}

/// Save action at the start (`KINGDOMS_QUICK`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuickStart {
    /// Write the quicksave once the game is set up (after the warp ticks).
    Save,
    /// Continue from the quicksave (errors are shown, the new game continues).
    Load,
}

/// A tool requested by `KINGDOMS_TOOL` (keys are resolved against the game data later).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolSpec {
    Build { building: String, rotation: u8 },
    Road { road: String, drag_from: Option<IVec2> },
    Demolish,
    Move { tile: IVec2 },
}

impl ToolSpec {
    /// Parses `build:<building>[:<rotation>]`, `road:<road>[:<x>,<y>]`, `demolish` or
    /// `move:<x>,<y>`.
    pub fn parse(value: &str) -> Option<Self> {
        let mut parts = value.split(':');
        match parts.next()?.trim() {
            "build" => {
                let building = parts.next()?.trim().to_owned();
                let rotation = match parts.next() {
                    Some(r) => r.trim().parse().ok().filter(|r| *r < 4)?,
                    None => 0,
                };
                (!building.is_empty()).then_some(ToolSpec::Build { building, rotation })
            }
            "road" => {
                let road = parts.next()?.trim().to_owned();
                let drag_from = match parts.next() {
                    Some(tile) => Some(parse_tile(tile)?),
                    None => None,
                };
                (!road.is_empty()).then_some(ToolSpec::Road { road, drag_from })
            }
            "demolish" => Some(ToolSpec::Demolish),
            "move" => Some(ToolSpec::Move { tile: parse_tile(parts.next()?)? }),
            _ => None,
        }
    }
}

impl Default for LaunchOptions {
    fn default() -> Self {
        Self {
            seed: DEFAULT_SEED,
            world_size: WorldSize::Medium,
            warp_ticks: 0,
            dev_settlement: None,
            dev_unlock: false,
            dev_weather: None,
            dev_crowd: None,
            dev_showcase: false,
            dev_event: None,
            dev_research: Vec::new(),
            dev_army: None,
            dev_walls: None,
            dev_threat: None,
            dev_trade: false,
            dev_upgrade: None,
            dev_case: None,
            focus: None,
            follow: None,
            zoom: None,
            select: None,
            tool: None,
            cursor: None,
            menu: false,
            speed: None,
            window: None,
            panel: None,
            quick: None,
        }
    }
}

impl LaunchOptions {
    /// Reads the options through `lookup` (environment variable name → value, e.g. the
    /// process environment); invalid values are reported in the log and replaced by the
    /// defaults.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let defaults = Self::default();
        let seed = parse_or("KINGDOMS_SEED", lookup("KINGDOMS_SEED"), defaults.seed, |v| v.parse().ok());
        let world_size =
            parse_or("KINGDOMS_WORLD_SIZE", lookup("KINGDOMS_WORLD_SIZE"), defaults.world_size, parse_world_size);
        let warp_ticks =
            parse_or("KINGDOMS_WARP_TICKS", lookup("KINGDOMS_WARP_TICKS"), defaults.warp_ticks, |v| v.parse().ok());
        let dev_settlement = lookup("KINGDOMS_DEV_SETTLEMENT").and_then(|v| DevPlan::parse(&v));
        let dev_unlock = lookup("KINGDOMS_DEV_UNLOCK").is_some_and(|v| is_truthy(&v));
        let dev_weather =
            parse_or("KINGDOMS_DEV_WEATHER", lookup("KINGDOMS_DEV_WEATHER"), None, |v| DevWeather::parse(v).map(Some));
        let dev_crowd = parse_or("KINGDOMS_DEV_CROWD", lookup("KINGDOMS_DEV_CROWD"), None, |v| {
            v.parse::<u16>().ok().filter(|n| super::dev_crowd::SETTLERS.contains(n)).map(Some)
        });
        let dev_showcase = lookup("KINGDOMS_DEV_SHOWCASE").is_some_and(|v| is_truthy(&v));
        let dev_event = parse_or("KINGDOMS_DEV_EVENT", lookup("KINGDOMS_DEV_EVENT"), None, |v| DevEvent::parse(v).map(Some));
        let dev_research = lookup("KINGDOMS_DEV_RESEARCH")
            .map(|v| v.split(',').map(|t| t.trim().to_owned()).filter(|t| !t.is_empty()).collect())
            .unwrap_or_default();
        let dev_army = parse_or("KINGDOMS_DEV_ARMY", lookup("KINGDOMS_DEV_ARMY"), None, |v| {
            v.parse::<u32>().ok().filter(|n| (1..=60).contains(n)).map(Some)
        });
        let dev_walls = parse_or("KINGDOMS_DEV_WALLS", lookup("KINGDOMS_DEV_WALLS"), None, |v| {
            v.parse::<u32>().ok().filter(|r| super::dev_walls::RADIUS_RANGE.contains(r)).map(Some)
        });
        let dev_trade = lookup("KINGDOMS_DEV_TRADE").is_some_and(|v| is_truthy(&v));
        let dev_upgrade = parse_or("KINGDOMS_DEV_UPGRADE", lookup("KINGDOMS_DEV_UPGRADE"), None, |v| {
            v.parse::<u32>().ok().filter(|h| (1..=96).contains(h)).map(Some)
        });
        let dev_threat = parse_or("KINGDOMS_DEV_THREAT", lookup("KINGDOMS_DEV_THREAT"), None, |v| DevThreat::parse(v).map(Some));
        let dev_case = parse_or("KINGDOMS_DEV_CASE", lookup("KINGDOMS_DEV_CASE"), None, |v| DevCase::parse(v).map(Some));
        let follow = lookup("KINGDOMS_FOCUS").and_then(|v| match v.trim().to_lowercase().as_str() {
            "feind" | "enemy" => Some(FollowTarget::Enemy),
            "karawane" | "caravan" => Some(FollowTarget::Caravan),
            _ => None,
        });
        let focus = if follow.is_some() { None } else { parse_or("KINGDOMS_FOCUS", lookup("KINGDOMS_FOCUS"), None, |v| parse_tile(v).map(Some)) };
        let zoom = parse_or("KINGDOMS_ZOOM", lookup("KINGDOMS_ZOOM"), None, |v| {
            v.parse::<f32>().ok().filter(|z| z.is_finite() && *z > 0.0).map(Some)
        });
        let select = parse_or("KINGDOMS_SELECT", lookup("KINGDOMS_SELECT"), None, |v| parse_tile(v).map(Some));
        let tool = parse_or("KINGDOMS_TOOL", lookup("KINGDOMS_TOOL"), None, |v| ToolSpec::parse(v).map(Some));
        let cursor = parse_or("KINGDOMS_CURSOR", lookup("KINGDOMS_CURSOR"), None, |v| parse_tile(v).map(Some));
        let menu = lookup("KINGDOMS_MENU").is_some_and(|v| is_truthy(&v));
        let speed = parse_or("KINGDOMS_SPEED", lookup("KINGDOMS_SPEED"), None, |v| parse_speed(v).map(Some));
        let window = lookup("KINGDOMS_WINDOW").map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
        let panel = lookup("KINGDOMS_PANEL").map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
        let quick = parse_or("KINGDOMS_QUICK", lookup("KINGDOMS_QUICK"), None, |v| match v.to_ascii_lowercase().as_str() {
            "save" => Some(Some(QuickStart::Save)),
            "load" => Some(Some(QuickStart::Load)),
            _ => None,
        });
        Self {
            seed,
            world_size,
            warp_ticks,
            dev_settlement,
            dev_unlock,
            dev_weather,
            dev_crowd,
            dev_showcase,
            dev_event,
            dev_research,
            dev_army,
            dev_walls,
            dev_threat,
            dev_trade,
            dev_upgrade,
            dev_case,
            focus,
            follow,
            zoom,
            select,
            tool,
            cursor,
            menu,
            speed,
            window,
            panel,
            quick,
        }
    }
}

fn parse_or<T: std::fmt::Debug>(name: &str, value: Option<String>, default: T, parse: impl Fn(&str) -> Option<T>) -> T {
    let Some(raw) = value else { return default };
    match parse(raw.trim()) {
        Some(v) => v,
        None => {
            log::warn!("{name}={raw:?} is invalid; using {default:?}");
            default
        }
    }
}

/// Parses `small`, `medium`, `large` or `huge` (case-insensitive).
pub fn parse_world_size(value: &str) -> Option<WorldSize> {
    match value.to_ascii_lowercase().as_str() {
        "small" => Some(WorldSize::Small),
        "medium" => Some(WorldSize::Medium),
        "large" => Some(WorldSize::Large),
        "huge" => Some(WorldSize::Huge),
        _ => None,
    }
}

/// Parses a game speed: `pause` (or `0`), `1`, `2` or `4`.
pub fn parse_speed(value: &str) -> Option<GameSpeed> {
    match value.to_ascii_lowercase().as_str() {
        "pause" | "0" => Some(GameSpeed::Paused),
        "1" => Some(GameSpeed::Normal),
        "2" => Some(GameSpeed::Double),
        "4" => Some(GameSpeed::Quadruple),
        _ => None,
    }
}

/// Parses a tile position `x,y`.
fn parse_tile(value: &str) -> Option<IVec2> {
    let (x, y) = value.split_once(',')?;
    Some(IVec2::new(x.trim().parse().ok()?, y.trim().parse().ok()?))
}

fn is_truthy(value: &str) -> bool {
    !matches!(value.trim().to_ascii_lowercase().as_str(), "" | "0" | "false" | "no" | "off")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(vars: &[(&str, &str)]) -> LaunchOptions {
        LaunchOptions::from_lookup(|key| vars.iter().find(|(k, _)| *k == key).map(|(_, v)| (*v).to_owned()))
    }

    #[test]
    fn defaults_without_variables() {
        assert_eq!(options(&[]), LaunchOptions::default());
        assert_eq!(LaunchOptions::default().seed, 7);
        assert_eq!(LaunchOptions::default().world_size, WorldSize::Medium);
    }

    #[test]
    fn parses_all_variables() {
        let o = options(&[
            ("KINGDOMS_SEED", "42"),
            ("KINGDOMS_WORLD_SIZE", "Huge"),
            ("KINGDOMS_WARP_TICKS", " 4800 "),
            ("KINGDOMS_DEV_SETTLEMENT", "1"),
            ("KINGDOMS_DEV_UNLOCK", "1"),
            ("KINGDOMS_DEV_WEATHER", "Sturm:90:2"),
            ("KINGDOMS_DEV_CROWD", "2000"),
            ("KINGDOMS_DEV_SHOWCASE", "1"),
            ("KINGDOMS_DEV_EVENT", "fire:1"),
            ("KINGDOMS_DEV_RESEARCH", "grain_farming, improved_plows,"),
            ("KINGDOMS_DEV_ARMY", "12"),
            ("KINGDOMS_DEV_WALLS", "9"),
            ("KINGDOMS_DEV_THREAT", "2:30"),
            ("KINGDOMS_DEV_TRADE", "1"),
            ("KINGDOMS_DEV_UPGRADE", "12"),
            ("KINGDOMS_DEV_CASE", "Gasthaus"),
            ("KINGDOMS_FOCUS", "30, 41"),
            ("KINGDOMS_ZOOM", "2.5"),
            ("KINGDOMS_SELECT", "12,7"),
            ("KINGDOMS_TOOL", "road:gravel:3,4"),
            ("KINGDOMS_CURSOR", "9,4"),
            ("KINGDOMS_MENU", "1"),
            ("KINGDOMS_SPEED", "Pause"),
            ("KINGDOMS_WINDOW", " finanzen "),
            ("KINGDOMS_PANEL", "uebersicht"),
            ("KINGDOMS_QUICK", "Load"),
        ]);
        let expected = LaunchOptions {
            seed: 42,
            world_size: WorldSize::Huge,
            warp_ticks: 4800,
            dev_settlement: Some(DevPlan::Showcase),
            dev_unlock: true,
            dev_weather: Some(DevWeather { weather: Weather::Storm, intensity: 90, days: 2 }),
            dev_crowd: Some(2000),
            dev_showcase: true,
            dev_event: Some(DevEvent { event: "fire".into(), choice: Some(1) }),
            dev_research: vec!["grain_farming".into(), "improved_plows".into()],
            dev_army: Some(12),
            dev_walls: Some(9),
            dev_threat: Some(DevThreat { kingdom: Some(2), size: 30 }),
            dev_trade: true,
            dev_upgrade: Some(12),
            dev_case: Some(DevCase::Tavern),
            focus: Some(IVec2::new(30, 41)),
            follow: None,
            zoom: Some(2.5),
            select: Some(IVec2::new(12, 7)),
            tool: Some(ToolSpec::Road { road: "gravel".into(), drag_from: Some(IVec2::new(3, 4)) }),
            cursor: Some(IVec2::new(9, 4)),
            menu: true,
            speed: Some(GameSpeed::Paused),
            window: Some("finanzen".into()),
            panel: Some("uebersicht".into()),
            quick: Some(QuickStart::Load),
        };
        assert_eq!(o, expected);
    }

    #[test]
    fn invalid_values_fall_back_to_defaults() {
        let o = options(&[
            ("KINGDOMS_SEED", "seven"),
            ("KINGDOMS_WORLD_SIZE", "gigantic"),
            ("KINGDOMS_WARP_TICKS", "-5"),
            ("KINGDOMS_DEV_SETTLEMENT", "0"),
            ("KINGDOMS_FOCUS", "3"),
            ("KINGDOMS_ZOOM", "-1"),
            ("KINGDOMS_DEV_UNLOCK", "0"),
            ("KINGDOMS_DEV_WEATHER", "hail"),
            ("KINGDOMS_DEV_CROWD", "9"),
            ("KINGDOMS_DEV_SHOWCASE", "0"),
            ("KINGDOMS_DEV_EVENT", "fire:x"),
            ("KINGDOMS_SPEED", "3"),
            ("KINGDOMS_DEV_ARMY", "0"),
            ("KINGDOMS_DEV_WALLS", "2"),
            ("KINGDOMS_DEV_THREAT", "9"),
            ("KINGDOMS_DEV_UPGRADE", "0"),
            ("KINGDOMS_DEV_CASE", "taverne"),
        ]);
        assert_eq!(o, LaunchOptions::default());
        assert_eq!(parse_world_size("small"), Some(WorldSize::Small));
        assert_eq!(parse_world_size("LARGE"), Some(WorldSize::Large));
        assert!(!is_truthy("off") && is_truthy("yes"));
    }

    #[test]
    fn dev_event_values() {
        assert_eq!(DevEvent::parse("merchant_visit"), Some(DevEvent { event: "merchant_visit".into(), choice: None }));
        assert_eq!(DevEvent::parse("fire:0"), Some(DevEvent { event: "fire".into(), choice: Some(0) }));
        assert_eq!(DevEvent::parse(""), None);
        assert_eq!(DevEvent::parse("fire:-1"), None);
        assert_eq!(DevEvent::parse("fire:1:2"), None);
    }

    #[test]
    fn dev_threat_values() {
        assert_eq!(DevThreat::parse("banditen"), Some(DevThreat { kingdom: None, size: 12 }));
        assert_eq!(DevThreat::parse("Räuber:5"), Some(DevThreat { kingdom: None, size: 5 }));
        assert_eq!(DevThreat::parse("1:120"), Some(DevThreat { kingdom: Some(1), size: 120 }));
        assert_eq!(DevThreat::parse("0"), None);
        assert_eq!(DevThreat::parse("bandits:0"), None);
        assert_eq!(DevThreat::parse("bandits:5:1"), None);
        let o = options(&[("KINGDOMS_FOCUS", "Feind")]);
        assert!(o.follow == Some(FollowTarget::Enemy) && o.focus.is_none());
        assert_eq!(options(&[("KINGDOMS_FOCUS", "karawane")]).follow, Some(FollowTarget::Caravan));
    }

    #[test]
    fn dev_weather_values() {
        assert_eq!(DevWeather::parse("snow"), Some(DevWeather { weather: Weather::Snow, intensity: 80, days: 1 }));
        assert_eq!(DevWeather::parse("Regen:35"), Some(DevWeather { weather: Weather::Rain, intensity: 35, days: 1 }));
        assert_eq!(DevWeather::parse("dürre:100:3").map(|w| (w.weather, w.days)), Some((Weather::Drought, 3)));
        assert_eq!(DevWeather::parse("klar").map(|w| w.weather), Some(Weather::Clear));
        assert_eq!(DevWeather::parse("bewölkt").map(|w| w.weather), Some(Weather::Cloudy));
        assert_eq!(DevWeather::parse("storm:0"), None);
        assert_eq!(DevWeather::parse("storm:101"), None);
        assert_eq!(DevWeather::parse("storm:50:0"), None);
        assert_eq!(DevWeather::parse("storm:50:1:9"), None);
        assert_eq!(DevWeather::parse("fog"), None);
    }

    #[test]
    fn start_mode_from_variables() {
        let mode = |vars: &[(&str, &str)]| {
            StartMode::from_lookup(|key| vars.iter().find(|(k, _)| *k == key).map(|(_, v)| (*v).to_owned()))
        };
        assert_eq!(mode(&[]), StartMode::Menu);
        assert_eq!(mode(&[("KINGDOMS_AUTOSHOT", "/tmp/x.png")]), StartMode::Menu);
        let seeded = mode(&[("KINGDOMS_SEED", "7")]);
        assert!(matches!(&seeded, StartMode::Game(o) if o.seed == 7));
        assert!(matches!(mode(&[("KINGDOMS_DEV_SETTLEMENT", "2")]), StartMode::Game(_)));
        assert_eq!(mode(&[("KINGDOMS_SEED", "7"), ("KINGDOMS_START", "menu")]), StartMode::Menu);
        assert_eq!(mode(&[("KINGDOMS_START", "game")]), StartMode::Game(Box::default()));
        assert_eq!(mode(&[("KINGDOMS_START", "sometimes")]), StartMode::Menu);
    }

    #[test]
    fn tool_specs() {
        assert_eq!(ToolSpec::parse("build:sawmill"), Some(ToolSpec::Build { building: "sawmill".into(), rotation: 0 }));
        assert_eq!(ToolSpec::parse("build:sawmill:3"), Some(ToolSpec::Build { building: "sawmill".into(), rotation: 3 }));
        assert_eq!(ToolSpec::parse("build:sawmill:4"), None);
        assert_eq!(ToolSpec::parse("road:path"), Some(ToolSpec::Road { road: "path".into(), drag_from: None }));
        assert_eq!(ToolSpec::parse("demolish"), Some(ToolSpec::Demolish));
        assert_eq!(ToolSpec::parse("move:4,5"), Some(ToolSpec::Move { tile: IVec2::new(4, 5) }));
        assert_eq!(ToolSpec::parse("move"), None);
        assert_eq!(ToolSpec::parse("fly"), None);
        assert_eq!(options(&[("KINGDOMS_TOOL", "nonsense")]).tool, None);
    }
}
