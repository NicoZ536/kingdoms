//! Audio glue: sounds for simulation events (placed with stereo pan and distance from the
//! camera), the ambience layers around the camera (day or night by the light of the map view,
//! rain and storm by the weather, a crowd near staffed markets), rolling carts of caravans near
//! the camera, the advisor's warnings about food running out and unhappy citizens
//! ([`Advisor`]), and the music mood (war while the kingdom is at war or enemies are on the map,
//! tension while enemies approach or a demand waits, else night, winter, peaceful). Sound ids
//! are the ones of `assets/audio/manifest.ron` (every id there is used, see the test
//! `every_manifest_id_is_used`); without audio files or device everything stays silent.

use glam::IVec2;
use kingdoms_engine::audio::{Audio, MusicMood, Placement};
use kingdoms_engine::rendering::camera::Camera2D;
use kingdoms_engine::rendering::iso;
use kingdoms_sim::defs::{DamageCause, Defs, ServiceKind, Weather};
use kingdoms_sim::diplomacy::{Party, ProposalKind};
use kingdoms_sim::event::{Severity, SimEvent};
use kingdoms_sim::military::{BattleOutcome, WarbandStage};
use kingdoms_sim::population::CitizenRole;
use kingdoms_sim::report;
use kingdoms_sim::state::GameState;
use kingdoms_sim::time::{Season, TICKS_PER_DAY, TICKS_PER_HOUR};
use kingdoms_sim::world::Vegetation;

use crate::session::{EventCursor, Session};

/// Seconds between ambience updates.
const AMBIENCE_INTERVAL: f32 = 0.5;
/// Radius in tiles sampled around the camera for the ambience.
const AMBIENCE_RADIUS: i32 = 14;
/// Event sounds played per frame at most (fast game speeds produce bursts).
const MAX_CUES_PER_FRAME: usize = 6;
/// Night value (0 day – 1 night) above which the night music plays, and below which it ends.
const NIGHT_MUSIC_ON: f32 = 0.6;
const NIGHT_MUSIC_OFF: f32 = 0.4;

/// Seconds between two rumbles of the nearest caravan cart.
const CART_INTERVAL: f32 = 2.5;
/// Tiles from the camera centre within which carts are heard.
const CART_RADIUS: f32 = 12.0;
/// Days of food below which the advisor warns (as the overview's threat line).
pub const FOOD_WARNING_DAYS: f32 = 3.0;
/// Average happiness below which the advisor warns when citizens are unhappy (as the
/// overview's threat line).
pub const UNHAPPY_AVERAGE: u8 = 50;
/// The advisor repeats a warning that stays true after this many ticks (one game day).
const ADVISOR_REPEAT_TICKS: u64 = TICKS_PER_DAY;

/// Every ambience layer the game uses.
pub const AMBIENCE_LAYERS: [&str; 11] = [
    "amb_birds_day",
    "amb_night",
    "amb_wind",
    "amb_winter_wind",
    "amb_river",
    "amb_sea",
    "amb_forest",
    "amb_town",
    "amb_rain",
    "amb_storm",
    "amb_market",
];

/// A sound to play for an event, at a tile or centred.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cue {
    pub sound: &'static str,
    pub at: Option<IVec2>,
}

impl Cue {
    fn at(sound: &'static str, at: Option<IVec2>) -> Self {
        Self { sound, at }
    }

    fn centred(sound: &'static str) -> Self {
        Self { sound, at: None }
    }
}

/// Sound of a finished work cycle by building type (the forester's planting sounds through
/// `SimEvent::TreePlanted`).
pub fn production_sound(key: &str) -> Option<&'static str> {
    Some(match key {
        "woodcutter" | "lumber_camp" => "sfx_wood_chop",
        "sawmill" | "large_sawmill" | "carpenter" | "large_carpenter" => "sfx_saw",
        "quarry" | "large_quarry" | "clay_pit" | "sand_pit" => "sfx_stone_pick",
        "iron_mine" | "coal_mine" | "gold_mine" => "sfx_mine",
        "smithy" | "master_smithy" | "weaponsmith" | "smelter" | "blast_furnace" | "jeweler" => "sfx_anvil",
        "mill" | "large_mill" => "sfx_mill",
        "bakery" | "large_bakery" => "sfx_bakery",
        "brewery" | "large_brewery" | "winery" | "cheese_dairy" => "sfx_brewery",
        "weaver" | "large_weaver" | "tailor" | "large_tailor" => "sfx_loom",
        "charcoal_kiln" | "brickworks" | "large_brickworks" | "pottery" | "large_pottery" => "sfx_kiln",
        "tannery" | "large_tannery" | "artisan" => "sfx_workshop",
        "apiary" | "large_apiary" => "sfx_bees",
        "cattle_farm" => "sfx_cow",
        "sheep_farm" => "sfx_sheep",
        "chicken_farm" => "sfx_chicken",
        "fishery" | "large_fishery" => "sfx_fishing",
        "wheat_farm" | "vegetable_farm" | "orchard" => "sfx_harvest",
        _ => return None,
    })
}

/// Effect sound (played at the event's place) and advisor voice of an event by its data id.
pub fn event_sounds(key: &str) -> (Option<&'static str>, &'static str) {
    match key {
        "fire" => (Some("sfx_fire"), "vo_fire"),
        "plague" => (Some("sfx_plague"), "vo_plague"),
        "bandit_raid" => (Some("sfx_raid"), "vo_raiders"),
        "flood" => (Some("sfx_flood"), "vo_flood"),
        "storm_damage" => (Some("sfx_storm_damage"), "vo_storm_damage"),
        "drought" => (None, "vo_drought"),
        "bad_harvest" => (None, "vo_harvest_bad"),
        "bountiful_harvest" => (Some("sfx_harvest"), "vo_harvest_good"),
        "merchant_visit" => (Some("sfx_coins"), "vo_trade_offer"),
        "refugees" => (None, "vo_refugees"),
        "religious_festival" => (Some("sfx_church_bell"), "vo_festival"),
        "political_unrest" => (None, "vo_unrest"),
        "new_deposit" => (None, "vo_deposit_found"),
        _ => (None, "vo_event_decision"),
    }
}

/// Sound of a building damaged by an event.
pub const fn damage_sound(cause: DamageCause) -> &'static str {
    match cause {
        DamageCause::Fire => "sfx_fire",
        DamageCause::Flood => "sfx_flood",
        DamageCause::Storm => "sfx_storm_damage",
        DamageCause::Attack => "sfx_battle",
    }
}

/// Sound and advisor voice at the end of a battle.
pub const fn battle_sounds(outcome: BattleOutcome) -> (&'static str, &'static str) {
    match outcome {
        BattleOutcome::Victory => ("sfx_victory", "vo_victory"),
        BattleOutcome::Defeat => ("sfx_defeat", "vo_battle_lost"),
        BattleOutcome::Draw => ("sfx_battle", "vo_battle_over"),
    }
}

/// Where the merchant of caravan `id` walks.
fn caravan_tile(state: &GameState, id: u32) -> Option<IVec2> {
    let c = state.diplomacy().caravan(id)?;
    let t = state.population().walking().traveler(*c.walkers.first()?)?;
    Some(t.motion().pos().tile())
}

fn building_tile(state: &GameState, id: kingdoms_sim::buildings::BuildingId) -> Option<IVec2> {
    state.building(id).map(|b| b.rect().origin + b.rect().size / 2)
}

fn middle(tiles: &[IVec2]) -> Option<IVec2> {
    tiles.get(tiles.len() / 2).copied()
}

/// The sounds of one simulation event.
pub fn event_cues(event: &SimEvent, state: &GameState, defs: &Defs) -> Vec<Cue> {
    match event {
        SimEvent::BuildingPlaced { origin, kind, .. } => {
            let size = defs.buildings().get(kind.index()).map_or(IVec2::ONE, |d| d.size);
            vec![Cue::at("sfx_place_building", Some(*origin + size / 2))]
        }
        SimEvent::ConstructionStarted { building, .. } | SimEvent::UpgradeStarted { building, .. } => {
            vec![Cue::at("sfx_construction", building_tile(state, *building))]
        }
        SimEvent::BuildingCompleted { building, .. } => vec![
            Cue::at("sfx_construction_complete", building_tile(state, *building)),
            Cue::centred("vo_building_complete"),
        ],
        SimEvent::BuildingMoved { building, .. } => vec![Cue::at("sfx_construction_complete", building_tile(state, *building))],
        SimEvent::ConstructionCancelled { .. } => vec![Cue::centred("sfx_demolish")],
        SimEvent::BuildingDemolished { origin, .. } => vec![Cue::at("sfx_demolish", Some(*origin))],
        SimEvent::UpgradeCancelled { building, .. } => vec![Cue::at("sfx_demolish", building_tile(state, *building))],
        SimEvent::BuildingUpgraded { building, .. } => vec![Cue::at("sfx_upgrade", building_tile(state, *building))],
        SimEvent::HouseDowngraded { building, .. } => vec![Cue::at("sfx_downgrade", building_tile(state, *building))],
        SimEvent::MoveStarted { to, .. } => vec![Cue::at("sfx_place_building", Some(*to))],
        SimEvent::RoadsBuilt { tiles, .. } => vec![Cue::at("sfx_road_place", middle(tiles))],
        SimEvent::RoadsRemoved { tiles } => vec![Cue::at("sfx_road_remove", middle(tiles))],
        SimEvent::TreeFelled { position } => vec![Cue::at("sfx_tree_fall", Some(*position))],
        SimEvent::TreePlanted { position } => vec![Cue::at("sfx_tree_plant", Some(*position))],
        SimEvent::ProductionCompleted { building, .. } => state
            .building(*building)
            .and_then(|b| defs.buildings().get(b.kind().index()))
            .and_then(|d| production_sound(&d.key))
            .map(|sound| vec![Cue::at(sound, building_tile(state, *building))])
            .unwrap_or_default(),
        SimEvent::DepositDepleted { position, .. } => {
            vec![Cue::at("sfx_event_bad", Some(*position)), Cue::centred("vo_deposit_depleted")]
        }
        SimEvent::ResourcesLost { .. } => vec![Cue::centred("vo_storage_full")],
        SimEvent::TradeExecuted { .. } => vec![Cue::centred("sfx_trade")],
        SimEvent::TreasuryLow { .. } => vec![Cue::centred("vo_treasury_low")],
        SimEvent::BankruptcyStarted { .. } => vec![Cue::centred("sfx_alarm_bell"), Cue::centred("vo_bankrupt")],
        SimEvent::BankruptcyEnded { .. } => vec![Cue::centred("sfx_event_good")],
        SimEvent::SeasonChanged { season, .. } => vec![Cue::centred("sfx_season_change"), Cue::centred(season_voice(*season))],
        SimEvent::CitizenBorn { .. } => vec![Cue::centred("sfx_birth")],
        SimEvent::CitizenDied { .. } => vec![Cue::centred("sfx_death_bell")],
        SimEvent::CitizensMarried { .. } => vec![Cue::centred("sfx_wedding")],
        SimEvent::HouseholdArrived { position, .. } => vec![Cue::at("sfx_event_good", Some(*position)), Cue::centred("vo_immigrants")],
        SimEvent::HouseholdEmigrated { .. } => vec![Cue::centred("vo_emigrants")],
        SimEvent::ResearchCompleted { .. } => vec![Cue::centred("sfx_research_complete"), Cue::centred("vo_research_complete")],
        SimEvent::EventStarted { event, position, .. } => {
            let key = defs.events().get(event.index()).map_or("", |d| d.key.as_str());
            let (sfx, voice) = event_sounds(key);
            let mut cues = vec![Cue::centred("sfx_event_pending")];
            if let Some(sfx) = sfx {
                cues.push(Cue::at(sfx, *position));
            }
            cues.push(Cue::centred(voice));
            cues
        }
        SimEvent::EventResolved { .. } => vec![Cue::centred("sfx_event_resolved")],
        SimEvent::BuildingDamaged { building, cause, .. } => vec![Cue::at(damage_sound(*cause), building_tile(state, *building))],
        SimEvent::BuildingRepaired { building, .. } => {
            vec![Cue::at("sfx_construction_complete", building_tile(state, *building)), Cue::centred("vo_rebuilt")]
        }
        SimEvent::CitizensFellIll { .. } => vec![Cue::centred("sfx_plague")],
        SimEvent::FieldsDamaged { .. } => vec![Cue::centred("sfx_event_bad")],
        SimEvent::DepositDiscovered { position, .. } => vec![Cue::at("sfx_deposit_found", Some(*position))],
        // War and diplomacy.
        SimEvent::ThreatApproaching { kingdom, .. } => {
            vec![Cue::centred("sfx_war_horn"), Cue::centred(if kingdom.is_some() { "vo_army_approaching" } else { "vo_raiders" })]
        }
        SimEvent::ThreatArrived { .. } => vec![Cue::centred("sfx_alarm_bell"), Cue::centred("vo_enemy_attack")],
        SimEvent::BattleEnded { outcome, .. } => {
            let (sfx, voice) = battle_sounds(*outcome);
            vec![Cue::centred(sfx), Cue::centred(voice)]
        }
        SimEvent::StructureDestroyed { origin, .. } => vec![Cue::at("sfx_wall_break", Some(*origin))],
        SimEvent::WarDeclared { attacker, .. } => {
            let voice = if matches!(attacker, Party::Player) { "vo_war_we_declared" } else { "vo_war_declared" };
            vec![Cue::centred("sfx_war_drums"), Cue::centred(voice)]
        }
        SimEvent::PeaceMade { .. } => vec![Cue::centred("sfx_church_bell"), Cue::centred("vo_peace")],
        SimEvent::TreatySigned { .. } => vec![Cue::centred("sfx_treaty"), Cue::centred("vo_treaty")],
        SimEvent::TreatyEnded { .. } | SimEvent::BorderIncident { .. } => vec![Cue::centred("sfx_event_bad")],
        SimEvent::ProposalReceived { kingdom, serial, .. } => {
            let demand = state
                .diplomacy()
                .proposals()
                .iter()
                .any(|p| p.serial == *serial && p.kingdom == *kingdom && matches!(p.kind, ProposalKind::Tribute { .. }));
            vec![Cue::centred("sfx_herald"), Cue::centred(if demand { "vo_tribute_demand" } else { "vo_envoy" })]
        }
        SimEvent::GiftSent { .. } | SimEvent::GiftReceived { .. } => vec![Cue::centred("sfx_coins")],
        SimEvent::RaidHappened { .. } => vec![Cue::centred("sfx_raid")],
        SimEvent::CaravanArrived { caravan, .. } => {
            vec![Cue::at("sfx_caravan_bell", caravan_tile(state, *caravan)), Cue::centred("vo_caravan_arrived")]
        }
        SimEvent::CaravanTraded { caravan, .. } => vec![Cue::at("sfx_trade", caravan_tile(state, *caravan))],
        SimEvent::CaravanReturned { caravan, .. } => {
            vec![Cue::at("sfx_caravan_bell", caravan_tile(state, *caravan)), Cue::centred("vo_caravan_back")]
        }
        SimEvent::CaravanRobbed { .. } => vec![Cue::centred("sfx_event_bad"), Cue::centred("vo_caravan_robbed")],
        SimEvent::SoldiersRecruited { building, .. } => vec![Cue::at("sfx_drill", building_tile(state, *building))],
        SimEvent::RankAchieved { .. } => vec![Cue::centred("sfx_rank_up"), Cue::centred("vo_rank_up")],
        SimEvent::FestivalHeld { .. } => vec![Cue::centred("sfx_church_bell"), Cue::centred("sfx_event_good")],
        SimEvent::GameOver { .. } => vec![Cue::centred("sfx_defeat"), Cue::centred("vo_game_over")],
        SimEvent::Notification(n) => match n.severity {
            Severity::Danger => vec![Cue::centred("sfx_event_bad")],
            Severity::Warning => vec![Cue::centred("ui_notification")],
            Severity::Info | Severity::Good => Vec::new(),
        },
        SimEvent::CommandRejected { .. } => vec![Cue::centred("ui_error")],
        // A workshop that lacks a food (bakery without flour, tavern without beer …).
        SimEvent::Shortage { resource, position, .. } if defs.resources().get(resource.index()).is_some_and(|r| r.is_edible()) => {
            vec![Cue::at("sfx_event_bad", Some(*position)), Cue::centred("vo_food_shortage")]
        }
        _ => Vec::new(),
    }
}

fn season_voice(season: Season) -> &'static str {
    match season {
        Season::Spring => "vo_spring",
        Season::Summer => "vo_summer",
        Season::Autumn => "vo_autumn",
        Season::Winter => "vo_winter",
    }
}

/// Target volumes of the ambience layers for a camera centred on `centre` in `season` with
/// `night` 0 (day) to 1 (night) from the light of the map, under `weather` at `intensity`
/// percent (in the order of [`AMBIENCE_LAYERS`]). Birds by day, crickets and owls by night,
/// quieter woods and town at night, a market crowd by day near markets with traders at work.
pub fn ambience_targets(
    state: &GameState,
    defs: &Defs,
    centre: IVec2,
    night: f32,
    season: Season,
    weather: (Weather, u8),
    zoom: f32,
) -> [f32; 11] {
    let (mut total, mut deep, mut fresh, mut trees, mut built, mut market) = (0u32, 0u32, 0u32, 0u32, 0u32, 0u32);
    let world = state.world();
    for dy in (-AMBIENCE_RADIUS..=AMBIENCE_RADIUS).step_by(2) {
        for dx in (-AMBIENCE_RADIUS..=AMBIENCE_RADIUS).step_by(2) {
            let p = centre + IVec2::new(dx, dy);
            let Some(tile) = world.get(p) else { continue };
            total += 1;
            match tile.terrain {
                kingdoms_sim::world::Terrain::DeepWater => deep += 1,
                kingdoms_sim::world::Terrain::ShallowWater => fresh += 1,
                _ if tile.river => fresh += 1,
                _ => {}
            }
            if matches!(tile.vegetation, Vegetation::Tree { .. }) {
                trees += 1;
            }
            if let Some((_, b)) = state.buildings().at(p) {
                built += 1;
                let is_market = defs.buildings().get(b.kind().index()).is_some_and(|d| d.services.iter().any(|s| s.kind == ServiceKind::Market));
                if is_market && b.is_operational() && b.staffed() > 0 {
                    market += 1;
                }
            }
        }
    }
    let share = |n: u32| if total == 0 { 0.0 } else { n as f32 / total as f32 };
    let night = night.clamp(0.0, 1.0);
    let day = 1.0 - night;
    let winter = season == Season::Winter;
    let land = 1.0 - share(deep) - share(fresh);
    let (weather, intensity) = weather;
    let wet = matches!(weather, Weather::Rain | Weather::Storm);
    let strength = f32::from(intensity.min(100)) / 100.0;
    let birds = if winter || wet { 0.0 } else { (0.3 + 0.6 * land).min(0.9) * day };
    let crickets = if winter { 0.25 } else { 0.6 } * night;
    let wind = if zoom < 0.5 { 0.5 } else { 0.25 };
    let winter_wind = if winter { 0.6 } else { 0.0 };
    let river = (share(fresh) * 5.0).min(1.0);
    let sea = (share(deep) * 3.0).min(1.0);
    let forest = (share(trees) * 2.0).min(1.0) * (1.0 - 0.5 * night);
    let town = (share(built) * 6.0).min(1.0) * (1.0 - 0.6 * night);
    let rain = if weather == Weather::Rain { 0.4 + 0.5 * strength } else { 0.0 };
    let storm = if weather == Weather::Storm { 0.5 + 0.5 * strength } else { 0.0 };
    let crowd = if wet { 0.5 } else { 1.0 };
    let market = (share(market) * 12.0).min(0.9) * day * crowd;
    [birds, crickets, wind, winter_wind, river, sea, forest, town, rain, storm, market]
}

/// The moving caravan cart nearest to tile-space point `centre` within [`CART_RADIUS`].
pub fn nearest_cart(state: &GameState, defs: &Defs, centre: glam::Vec2) -> Option<IVec2> {
    let r = CART_RADIUS.ceil() as i32;
    let c = centre.floor().as_ivec2();
    let rect = kingdoms_sim::grid::TileRect::new(c - IVec2::splat(r), IVec2::splat(2 * r + 1));
    state
        .citizens_in(defs, rect)
        .filter(|v| v.role == CitizenRole::Cart && v.moving)
        .map(|v| {
            let (x, y) = v.pos.to_tiles();
            (glam::Vec2::new(x, y).distance(centre), v.pos.tile())
        })
        .filter(|(d, _)| *d <= CART_RADIUS)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, tile)| tile)
}

/// One warning of the advisor: whether it holds now, since when it was last spoken.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Warning {
    active: bool,
    spoken: Option<u64>,
}

impl Warning {
    /// Updates the warning with the current condition at `tick`; true if the advisor should
    /// speak now: when it becomes true, and again after [`ADVISOR_REPEAT_TICKS`] while it
    /// stays true.
    pub fn update(&mut self, active: bool, tick: u64) -> bool {
        let speak = active && (!self.active || self.spoken.is_none_or(|t| tick.saturating_sub(t) >= ADVISOR_REPEAT_TICKS));
        self.active = active;
        if speak {
            self.spoken = Some(tick);
        }
        speak
    }
}

/// The advisor's recurring warnings, checked once per game hour: food for less than
/// [`FOOD_WARNING_DAYS`] days, and unhappy citizens with an average below
/// [`UNHAPPY_AVERAGE`] (the overview's threat lines).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Advisor {
    hour: Option<u64>,
    food: Warning,
    unhappy: Warning,
}

impl Advisor {
    /// The voice lines due at the current state (at most once per game hour).
    pub fn check(&mut self, state: &GameState, defs: &Defs) -> Vec<&'static str> {
        let tick = state.clock().tick;
        let hour = tick / TICKS_PER_HOUR;
        if self.hour == Some(hour) {
            return Vec::new();
        }
        self.hour = Some(hour);
        let food = report::food_forecast(state, defs).days_of_stock.is_some_and(|d| d < FOOD_WARNING_DAYS);
        let happiness = report::happiness_report(state, defs);
        let unhappy = happiness.unhappy > 0 && happiness.average < UNHAPPY_AVERAGE;
        let mut lines = Vec::new();
        if self.food.update(food, tick) {
            lines.push("vo_food_shortage");
        }
        if self.unhappy.update(unhappy, tick) {
            lines.push("vo_low_happiness");
        }
        lines
    }
}

/// How threatened the kingdom is, for the music.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Danger {
    Calm,
    /// Enemies approach, a tribute demand waits or the soldiers go unpaid.
    Tense,
    /// At war or enemies on the map.
    War,
}

/// The kingdom's danger from the public state.
pub fn danger(state: &GameState) -> Danger {
    let m = state.military();
    let at_war = state.diplomacy().kingdoms().any(|(_, k)| k.at_war());
    if at_war || m.enemies_on_map() {
        return Danger::War;
    }
    let approaching = m.warbands().iter().any(|w| matches!(w.stage(), WarbandStage::Approaching { .. }));
    let demand = state.diplomacy().proposals().iter().any(|p| matches!(p.kind, ProposalKind::Tribute { .. }));
    if approaching || demand || m.is_unpaid() { Danger::Tense } else { Danger::Calm }
}

/// Inhabitants from which a happy kingdom gets the prosperous music.
const PROSPEROUS_POPULATION: usize = 50;
/// Average happiness from which a kingdom of [`PROSPEROUS_POPULATION`] is prosperous.
const PROSPEROUS_HAPPINESS: f32 = 70.0;

/// The kingdom thrives (for the music): a festival day, or a solvent town of at least
/// [`PROSPEROUS_POPULATION`] inhabitants with an average happiness of
/// [`PROSPEROUS_HAPPINESS`].
pub fn prosperous(state: &GameState) -> bool {
    if state.events().festival_active(state.clock().tick) {
        return true;
    }
    let pop = state.population();
    !state.economy().is_bankrupt()
        && pop.len() >= PROSPEROUS_POPULATION
        && pop.average_happiness().is_some_and(|h| h >= PROSPEROUS_HAPPINESS)
}

/// Music mood: war or tension first (`danger`), then night music once `night` rises above
/// [`NIGHT_MUSIC_ON`] until it falls below [`NIGHT_MUSIC_OFF`] (`was_night`: the previous
/// choice), else winter music in winter, else prosperous music while the kingdom thrives
/// (`prosperous`), else peaceful music.
pub fn music_mood(night: f32, season: Season, was_night: bool, danger: Danger, prosperous: bool) -> MusicMood {
    match danger {
        Danger::War => return MusicMood::War,
        Danger::Tense => return MusicMood::Tense,
        Danger::Calm => {}
    }
    let threshold = if was_night { NIGHT_MUSIC_OFF } else { NIGHT_MUSIC_ON };
    if night > threshold {
        MusicMood::Night
    } else if season == Season::Winter {
        MusicMood::Winter
    } else if prosperous {
        MusicMood::Prosperous
    } else {
        MusicMood::Peaceful
    }
}

/// Sound state of the running game.
pub struct GameAudio {
    cursor: EventCursor,
    ambience_timer: f32,
    cart_timer: f32,
    /// The night music was chosen last.
    night_music: bool,
    advisor: Advisor,
}

impl GameAudio {
    pub fn new(session: &Session) -> Self {
        Self {
            cursor: session.events().cursor_at_end(),
            ambience_timer: 0.0,
            cart_timer: 0.0,
            night_music: false,
            advisor: Advisor::default(),
        }
    }

    /// Starts over for another session (after loading).
    pub fn reset(&mut self, session: &Session) {
        self.cursor = session.events().cursor_at_end();
        self.ambience_timer = 0.0;
        self.cart_timer = 0.0;
        self.advisor = Advisor::default();
    }

    /// Plays the sounds of new events, updates the ambience and the music mood (`night`: 0 by
    /// day – 1 at night, from the light of the map view).
    pub fn update(&mut self, audio: &mut Audio, session: &Session, camera: &Camera2D, night: f32, dt: f32) {
        let state = session.state();
        let defs = session.defs();
        let place = |tile: IVec2| {
            let level = state.world().get(tile).map_or(0.0, |t| f32::from(t.elevation));
            let screen = camera.world_to_screen(iso::tile_center_world(tile, level));
            Placement::from_screen(screen, camera.viewport(), camera.zoom)
        };
        let read = session.events().read(&mut self.cursor);
        if !read.missed {
            let mut played = 0;
            for logged in read.events {
                for cue in event_cues(&logged.event, state, defs) {
                    if played >= MAX_CUES_PER_FRAME {
                        break;
                    }
                    match cue.at {
                        Some(tile) => audio.play_placed(cue.sound, place(tile)),
                        None => audio.play(cue.sound),
                    }
                    played += 1;
                }
            }
        }
        for line in self.advisor.check(state, defs) {
            audio.play(line);
        }
        self.cart_timer -= dt;
        if self.cart_timer <= 0.0 {
            self.cart_timer = CART_INTERVAL;
            if let Some(tile) = nearest_cart(state, defs, iso::world_to_tile(camera.position)) {
                audio.play_placed("sfx_cart", place(tile));
            }
        }
        self.ambience_timer -= dt;
        if self.ambience_timer > 0.0 {
            return;
        }
        self.ambience_timer = AMBIENCE_INTERVAL;
        let season = state.clock().season();
        let centre = iso::world_to_tile(camera.position).floor().as_ivec2();
        let weather = (state.weather().current(), state.weather().intensity());
        let targets = ambience_targets(state, defs, centre, night, season, weather, camera.zoom);
        for (id, volume) in AMBIENCE_LAYERS.iter().zip(targets) {
            audio.set_ambience(id, volume);
        }
        let mood = music_mood(night, season, self.night_music, danger(state), prosperous(state));
        self.night_music = mood == MusicMood::Night;
        audio.set_music_mood(mood);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use kingdoms_sim::state::GameSettings;
    use kingdoms_sim::world::WorldSize;

    use super::*;

    fn defs() -> Arc<Defs> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        match Defs::load(&dir) {
            Ok(defs) => Arc::new(defs),
            Err(e) => panic!("shipped game data must load: {e}"),
        }
    }

    /// The shipped manifest: every voice line carries the exact German words to speak, every
    /// file lies in one of the channel folders and is named `.ogg` (other formats are found by
    /// the file's stem).
    #[test]
    fn shipped_manifest_is_ready_for_delivery() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/audio/manifest.ron");
        let text = std::fs::read_to_string(&path).expect("manifest");
        let m: kingdoms_engine::audio::AudioManifest = ron::from_str(&text).expect("valid manifest");
        assert!(m.voice_text_problems().is_empty(), "{:?}", m.voice_text_problems());
        use kingdoms_engine::audio::AudioChannel as C;
        let folder_of = |channel: C| match channel {
            C::Music => "music",
            C::Ambience => "ambience",
            C::Effects => "sfx",
            C::Ui => "ui",
            C::Voice => "voice",
        };
        let files = m.music.iter().map(|t| (C::Music, &t.file)).chain(m.sounds.values().flat_map(|s| s.files.iter().map(move |f| (s.channel, f))));
        for (channel, file) in files {
            assert_eq!(file.split('/').next(), Some(folder_of(channel)), "{file} belongs in the folder of its channel");
            assert!(file.ends_with(".ogg") && file.matches('/').count() == 1, "{file}");
        }
        for s in m.sounds.values().filter(|s| s.channel == kingdoms_engine::audio::AudioChannel::Voice) {
            for line in &s.text {
                assert!(line.ends_with(['.', '!', '?']), "a spoken line ends like a sentence: {line}");
            }
        }
    }

    /// An upgraded workshop sounds like the type it was upgraded from.
    #[test]
    fn upgraded_types_keep_their_work_sound() {
        let defs = defs();
        for def in defs.buildings().iter() {
            let Some(up) = def.upgrade.as_ref() else { continue };
            let Some(target) = defs.buildings().get(up.to.index()) else { continue };
            if let Some(sound) = production_sound(&def.key) {
                assert_eq!(production_sound(&target.key), Some(sound), "{} → {}", def.key, target.key);
            }
        }
    }

    /// Every building that produces something sounds when a work cycle ends: by its own work
    /// sound, the forester by planting.
    #[test]
    fn every_producer_has_a_work_sound() {
        let defs = defs();
        let silent: Vec<&str> = defs
            .buildings()
            .iter()
            .filter(|d| d.production.as_ref().is_some_and(|p| !p.recipes.is_empty() || p.gathering.is_some()))
            .filter(|d| production_sound(&d.key).is_none() && d.key != "forester")
            .map(|d| d.key.as_str())
            .collect();
        assert!(silent.is_empty(), "no work sound: {silent:?}");
    }

    /// Every sound id the game can request exists in the shipped manifest.
    #[test]
    fn all_ids_are_in_the_manifest() {
        let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/audio/manifest.ron"))
            .expect("manifest");
        let defs = defs();
        let mut ids: BTreeSet<&str> = AMBIENCE_LAYERS.into_iter().collect();
        ids.extend(defs.buildings().iter().filter_map(|d| production_sound(&d.key)));
        ids.extend(Season::ALL.map(season_voice));
        ids.extend([
            "sfx_place_building", "sfx_construction", "sfx_construction_complete", "sfx_demolish", "sfx_upgrade",
            "sfx_downgrade", "sfx_road_place", "sfx_road_remove", "sfx_tree_fall", "sfx_tree_plant", "sfx_event_bad",
            "sfx_event_good", "sfx_trade", "sfx_alarm_bell", "sfx_season_change", "sfx_birth", "sfx_death_bell",
            "sfx_wedding", "vo_building_complete", "vo_deposit_depleted", "vo_storage_full", "vo_treasury_low",
            "vo_bankrupt", "vo_immigrants", "vo_emigrants", "vo_welcome", "vo_game_saved", "ui_notification",
            "ui_error", "ui_click", "ui_open", "ui_close", "ui_select", "ui_speed", "ui_pause", "ui_rotate", "ui_save",
            "sfx_research_complete", "vo_research_complete", "sfx_event_pending", "sfx_event_resolved", "sfx_plague",
            "sfx_deposit_found", "vo_rebuilt", "vo_event_decision",
            // War, diplomacy, caravans, ranks.
            "sfx_war_horn", "vo_army_approaching", "vo_raiders", "vo_enemy_attack", "sfx_wall_break", "sfx_war_drums",
            "vo_war_declared", "vo_war_we_declared", "sfx_church_bell", "vo_peace", "sfx_treaty", "vo_treaty", "sfx_herald",
            "vo_tribute_demand", "vo_envoy", "sfx_coins", "sfx_raid", "sfx_caravan_bell", "vo_caravan_arrived",
            "vo_caravan_back", "vo_caravan_robbed", "sfx_drill", "sfx_rank_up", "vo_rank_up", "sfx_defeat", "vo_game_over",
            // Advisor, carts, hover and the picked citizen.
            "vo_food_shortage", "vo_low_happiness", "sfx_cart", "ui_hover", "vo_citizen_yes",
        ]);
        for outcome in [BattleOutcome::Victory, BattleOutcome::Defeat, BattleOutcome::Draw] {
            let (sfx, voice) = battle_sounds(outcome);
            ids.extend([sfx, voice]);
        }
        // Every event of the data, unknown ones included, and every kind of damage.
        for key in defs.events().iter().map(|e| e.key.as_str()).chain(["unknown_event"]) {
            let (sfx, voice) = event_sounds(key);
            ids.extend(sfx);
            ids.insert(voice);
        }
        ids.extend([DamageCause::Fire, DamageCause::Flood, DamageCause::Storm, DamageCause::Attack].map(damage_sound));
        let manifest: kingdoms_engine::audio::AudioManifest = ron::from_str(&text).expect("manifest.ron parses");
        for id in ids {
            assert!(manifest.sounds.contains_key(id), "sound id {id} missing in manifest.ron");
        }
    }

    /// Every sound id of the shipped manifest is requested somewhere in the game or the engine
    /// (as a string literal outside the test modules), so no manifest entry is dead.
    #[test]
    fn every_manifest_id_is_used() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let text = std::fs::read_to_string(root.join("assets/audio/manifest.ron")).expect("manifest");
        let manifest: kingdoms_engine::audio::AudioManifest = ron::from_str(&text).expect("manifest.ron parses");
        fn sources(dir: &std::path::Path, out: &mut String) {
            let Ok(entries) = std::fs::read_dir(dir) else { return };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    sources(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let text = std::fs::read_to_string(&path).unwrap_or_default();
                    // Only the code that runs: the test module at the end of a file is cut off.
                    let code = text.find("#[cfg(test)]\nmod tests").map_or(text.as_str(), |end| &text[..end]);
                    out.push_str(code);
                }
            }
        }
        let mut code = String::new();
        sources(&root.join("src"), &mut code);
        sources(&root.join("crates/engine/src"), &mut code);
        let unused: Vec<&String> = manifest.sounds.keys().filter(|id| !code.contains(&format!("\"{id}\""))).collect();
        assert!(unused.is_empty(), "manifest ids never requested: {unused:?}");
    }

    /// The generated list for the audio supplier (`assets/audio/delivery.json`) names every id of
    /// the manifest exactly once, with all of its files and, for voices, the exact spoken text.
    #[test]
    fn the_delivery_list_matches_the_manifest() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/audio");
        let text = std::fs::read_to_string(root.join("manifest.ron")).expect("manifest");
        let manifest: kingdoms_engine::audio::AudioManifest = ron::from_str(&text).expect("manifest.ron parses");
        let list = std::fs::read_to_string(root.join("delivery.json")).expect("delivery.json");
        let json = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
        let ids = manifest.music.len() + manifest.sounds.len();
        assert_eq!(list.matches("\"id\": ").count(), ids, "one entry per manifest id");
        let no_text: &[String] = &[];
        let entries = manifest.music.iter().map(|t| (t.id.as_str(), std::slice::from_ref(&t.file), no_text))
            .chain(manifest.sounds.iter().map(|(id, s)| (id.as_str(), s.files.as_slice(), s.text.as_slice())));
        for (id, files, texts) in entries {
            assert_eq!(list.matches(&format!("\"id\": {}", json(id))).count(), 1, "{id} is listed once");
            for item in files.iter().chain(texts) {
                assert!(list.contains(&json(item)), "{id}: {item} is missing in delivery.json");
            }
        }
    }

    #[test]
    fn the_advisor_warns_once_and_repeats_daily() {
        let mut w = Warning::default();
        assert!(!w.update(false, 10));
        assert!(w.update(true, 20), "speaks when the warning starts");
        assert!(!w.update(true, 20 + TICKS_PER_HOUR), "not every hour");
        assert!(w.update(true, 20 + ADVISOR_REPEAT_TICKS), "again a day later");
        assert!(!w.update(false, 30 + ADVISOR_REPEAT_TICKS));
        assert!(w.update(true, 40 + ADVISOR_REPEAT_TICKS), "a new shortage is announced at once");
        // A fresh game: no hunger, nobody unhappy, nothing to say; the check runs once an hour.
        let defs = defs();
        let state = GameState::new(&defs, GameSettings::new(&defs, 7, WorldSize::Small));
        let mut advisor = Advisor::default();
        assert!(advisor.check(&state, &defs).is_empty());
        assert_eq!(advisor.hour, Some(state.clock().tick / TICKS_PER_HOUR));
    }

    #[test]
    fn food_shortages_bring_the_advisor() {
        let defs = defs();
        let state = GameState::new(&defs, GameSettings::new(&defs, 7, WorldSize::Small));
        let Some((building, _)) = state.buildings().iter().next() else { panic!("a start building") };
        let p = IVec2::new(4, 5);
        let food = defs.resources().iter().find(|r| r.is_edible()).map(|r| r.id);
        let wood = defs.resources().iter().find(|r| !r.is_edible()).map(|r| r.id);
        let (Some(food), Some(wood)) = (food, wood) else { panic!("food and other goods") };
        let hungry = event_cues(&SimEvent::Shortage { building, resource: food, position: p }, &state, &defs);
        assert!(hungry.contains(&Cue::centred("vo_food_shortage")));
        assert!(event_cues(&SimEvent::Shortage { building, resource: wood, position: p }, &state, &defs).is_empty());
    }

    #[test]
    fn events_map_to_placed_cues() {
        let defs = defs();
        let state = GameState::new(&defs, GameSettings::new(&defs, 7, WorldSize::Small));
        let p = IVec2::new(10, 12);
        assert_eq!(event_cues(&SimEvent::TreeFelled { position: p }, &state, &defs), vec![Cue::at("sfx_tree_fall", Some(p))]);
        let tiles = vec![IVec2::new(1, 1), IVec2::new(2, 1), IVec2::new(3, 1)];
        assert_eq!(
            event_cues(&SimEvent::RoadsRemoved { tiles }, &state, &defs),
            vec![Cue::at("sfx_road_remove", Some(IVec2::new(2, 1)))]
        );
        let season = event_cues(&SimEvent::SeasonChanged { season: Season::Winter, year: 1 }, &state, &defs);
        assert!(season.contains(&Cue::centred("vo_winter")));
        assert!(event_cues(&SimEvent::YearChanged { year: 2 }, &state, &defs).is_empty());
        // An event sounds at its place and the advisor names it.
        let Some(fire) = defs.event_id("fire") else { panic!("the data has the fire event") };
        let started = SimEvent::EventStarted { serial: 1, event: fire, position: Some(p), expires_tick: 2400 };
        assert_eq!(
            event_cues(&started, &state, &defs),
            vec![Cue::centred("sfx_event_pending"), Cue::at("sfx_fire", Some(p)), Cue::centred("vo_fire")]
        );
    }

    #[test]
    fn ambience_follows_time_and_place() {
        let defs = defs();
        let state = GameState::new(&defs, GameSettings::new(&defs, 7, WorldSize::Small));
        let start = state.world().start;
        let clear = (Weather::Clear, 50);
        let day = ambience_targets(&state, &defs, start, 0.0, Season::Summer, clear, 1.0);
        let night = ambience_targets(&state, &defs, start, 1.0, Season::Summer, clear, 1.0);
        let winter = ambience_targets(&state, &defs, start, 0.0, Season::Winter, clear, 1.0);
        assert!(day[0] > 0.0 && day[1] == 0.0, "birds by day: {day:?}");
        assert!(night[0] == 0.0 && night[1] > 0.0, "night layer: {night:?}");
        assert!(winter[0] == 0.0 && winter[3] > 0.0, "winter wind: {winter:?}");
        // The start settlement has buildings around it; the town is quieter at night.
        assert!(day[7] > 0.0 && night[7] < day[7], "town ambience: {day:?} {night:?}");
        assert!(day.iter().all(|v| (0.0..=1.0).contains(v)));
        // Dusk blends both.
        let dusk = ambience_targets(&state, &defs, start, 0.5, Season::Summer, clear, 1.0);
        assert!(dusk[0] > 0.0 && dusk[1] > 0.0);
        // Rain and storm layers follow the weather.
        let rain = ambience_targets(&state, &defs, start, 0.0, Season::Summer, (Weather::Rain, 80), 1.0);
        let storm = ambience_targets(&state, &defs, start, 0.0, Season::Summer, (Weather::Storm, 80), 1.0);
        assert!(rain[8] > 0.0 && rain[9] == 0.0 && rain[0] == 0.0);
        assert!(storm[9] > 0.0 && storm[8] == 0.0);
        assert!(day[8] == 0.0 && day[9] == 0.0);
        // The market crowd is a daytime sound.
        assert_eq!(night[10], 0.0);
    }

    #[test]
    fn music_follows_night_and_winter_with_hysteresis() {
        let calm = Danger::Calm;
        assert_eq!(music_mood(0.0, Season::Summer, false, calm, false), MusicMood::Peaceful);
        assert_eq!(music_mood(0.0, Season::Winter, false, calm, false), MusicMood::Winter);
        assert_eq!(music_mood(0.9, Season::Winter, false, calm, false), MusicMood::Night);
        // At dusk the mood does not flicker: on above 0.6, off below 0.4.
        assert_eq!(music_mood(0.5, Season::Summer, false, calm, false), MusicMood::Peaceful);
        assert_eq!(music_mood(0.5, Season::Summer, true, calm, false), MusicMood::Night);
        assert_eq!(music_mood(0.3, Season::Summer, true, calm, false), MusicMood::Peaceful);
        // Danger comes first, day or night.
        assert_eq!(music_mood(0.9, Season::Winter, true, Danger::Tense, false), MusicMood::Tense);
        assert_eq!(music_mood(0.0, Season::Summer, false, Danger::War, false), MusicMood::War);
        // A thriving kingdom by day outside the winter.
        assert_eq!(music_mood(0.0, Season::Summer, false, calm, true), MusicMood::Prosperous);
        assert_eq!(music_mood(0.0, Season::Winter, false, calm, true), MusicMood::Winter);
        assert_eq!(music_mood(0.9, Season::Summer, false, calm, true), MusicMood::Night);
        assert_eq!(music_mood(0.0, Season::Summer, false, Danger::Tense, true), MusicMood::Tense);
    }

    #[test]
    fn threats_and_wars_set_the_danger() {
        let defs = defs();
        let mut state = GameState::new(&defs, GameSettings::new(&defs, 7, WorldSize::Small));
        assert_eq!(danger(&state), Danger::Calm);
        state.tick(&defs, vec![kingdoms_sim::command::Command::DebugSpawnThreat { kingdom: None, size: 3 }]);
        assert_eq!(danger(&state), Danger::Tense, "bandits gather at the edge");
        let Some((id, _)) = state.diplomacy().kingdoms().next() else { panic!("a small map has kingdoms") };
        let war = kingdoms_sim::command::Command::Diplomacy { kingdom: id, action: kingdoms_sim::diplomacy::DiplomaticAction::DeclareWar };
        state.tick(&defs, vec![war]);
        assert_eq!(danger(&state), Danger::War);
        // The war sounds like drums and the advisor names who declared it.
        let declared = SimEvent::WarDeclared { attacker: Party::Player, defender: Party::Kingdom(id) };
        assert_eq!(event_cues(&declared, &state, &defs), vec![Cue::centred("sfx_war_drums"), Cue::centred("vo_war_we_declared")]);
    }
}
