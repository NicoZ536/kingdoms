//! "Neues Spiel": seed (number or any text; dice for a random one), map size, difficulty,
//! random events, scenario (a choice only when the data has more than one) and a preview of the
//! generated landscape. The preview runs the simulation's world generator on a worker thread
//! with exactly the parameters the new game will use, shortly after the last change.

use std::hash::{BuildHasher, Hasher};
use std::sync::Arc;
use std::sync::mpsc;

use glam::IVec2;
use kingdoms_engine::egui::{self, Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};
use kingdoms_sim::defs::{Defs, ScenarioId};
use kingdoms_sim::state::{Difficulty, GameSettings};
use kingdoms_sim::world::{DepositKind, GeneratorSettings, Terrain, Vegetation, WorldMap, WorldSize, generate};

use super::{field_label, note, page_button, page_title, spinner};
use crate::ui::hud::natural_rgb;
use crate::ui::theme::{Palette, heading_font};
use crate::ui::widgets::section_header;

/// Seconds after the last change before the preview is generated.
const PREVIEW_DELAY: f64 = 0.35;
/// Radius (tiles) around the start point the preview counts deposits in.
const START_RADIUS: i32 = 24;
/// Longest seed text.
const MAX_SEED_CHARS: usize = 32;

/// What the player chose on the page.
pub enum NewGameAction {
    None,
    Back,
    Start(GameSettings),
}

/// Numbers about a generated map.
#[derive(Clone, Debug, Default, PartialEq)]
struct MapStats {
    water: f32,
    forest: f32,
    mountains: f32,
    /// Deposits near the start: kind and number of tiles.
    deposits: Vec<(DepositKind, u32)>,
    fresh_water_near_start: bool,
}

/// Parameters a preview was made for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PreviewKey {
    seed: u64,
    size: WorldSize,
    rival_sites: u32,
}

struct PreviewImage {
    key: PreviewKey,
    texture: egui::TextureHandle,
    size: IVec2,
    start: IVec2,
    stats: MapStats,
}

struct PreviewResult {
    key: PreviewKey,
    pixels: Vec<Color32>,
    size: IVec2,
    start: IVec2,
    stats: MapStats,
}

/// The landscape preview: generated in the background, shown as a diamond like the minimap.
#[derive(Default)]
struct Preview {
    wanted: Option<(PreviewKey, f64)>,
    running: Option<(PreviewKey, mpsc::Receiver<PreviewResult>)>,
    image: Option<PreviewImage>,
}

impl Preview {
    fn request(&mut self, key: PreviewKey, now: f64) {
        if self.wanted.map(|(k, _)| k) != Some(key) {
            self.wanted = Some((key, now));
        }
    }

    /// Starts a generation when due, takes finished ones. Returns true while one is running.
    fn update(&mut self, ctx: &egui::Context, now: f64) -> bool {
        if let Some((key, receiver)) = &self.running {
            match receiver.try_recv() {
                Ok(result) => {
                    let key = *key;
                    self.running = None;
                    if Some(result.key) == self.wanted.map(|(k, _)| k) || self.image.is_none() {
                        let [w, h] = [result.size.x as usize, result.size.y as usize];
                        let image = egui::ColorImage::new([w, h], result.pixels);
                        let texture = ctx.load_texture("new_game_preview", image, egui::TextureOptions::LINEAR);
                        self.image = Some(PreviewImage { key, texture, size: result.size, start: result.start, stats: result.stats });
                    }
                }
                Err(mpsc::TryRecvError::Empty) => return true,
                Err(mpsc::TryRecvError::Disconnected) => {
                    log::error!("the map preview thread ended without a result");
                    self.running = None;
                }
            }
        }
        let Some((key, since)) = self.wanted else { return false };
        let current = self.image.as_ref().is_some_and(|i| i.key == key);
        if current || now - since < PREVIEW_DELAY {
            return !current;
        }
        let (sender, receiver) = mpsc::channel();
        let spawned = std::thread::Builder::new().name("map-preview".into()).spawn(move || {
            let _ = sender.send(make_preview(key));
        });
        match spawned {
            Ok(_) => self.running = Some((key, receiver)),
            Err(err) => {
                log::error!("cannot start the map preview thread: {err}");
                self.wanted = None;
            }
        }
        true
    }
}

/// Generator settings of a preview (the game's, see `NewGameForm::preview_key`).
fn preview_generator(key: PreviewKey) -> GeneratorSettings {
    GeneratorSettings { rival_sites: key.rival_sites, ..GeneratorSettings::with_seed(key.seed, key.size) }
}

/// Generates the map of `key` and colours one pixel per tile.
fn make_preview(key: PreviewKey) -> PreviewResult {
    let map = generate(&preview_generator(key));
    let pixels = map
        .positions()
        .map(|p| {
            let t = map.tile(p);
            let [r, g, b] = natural_rgb(t).map(|v| v.clamp(0.0, 255.0) as u8);
            Color32::from_rgb(r, g, b)
        })
        .collect();
    PreviewResult { key, pixels, size: map.size(), start: map.start, stats: map_stats(&map) }
}

fn map_stats(map: &WorldMap) -> MapStats {
    let total = map.tiles().len().max(1) as f32;
    let mut water = 0u32;
    let mut forest = 0u32;
    let mut mountains = 0u32;
    for t in map.tiles() {
        if t.is_water() {
            water += 1;
        }
        if matches!(t.vegetation, Vegetation::Tree { .. }) {
            forest += 1;
        }
        if matches!(t.terrain, Terrain::Rock | Terrain::Snow) {
            mountains += 1;
        }
    }
    let mut deposits: Vec<(DepositKind, u32)> = DepositKind::ALL
        .iter()
        .map(|&kind| (kind, map.count_in_radius(map.start, START_RADIUS, |t| t.deposit.is_some_and(|d| d.kind == kind)) as u32))
        .filter(|(_, n)| *n > 0)
        .collect();
    deposits.sort_by_key(|d| std::cmp::Reverse(d.1));
    MapStats {
        water: water as f32 / total,
        forest: forest as f32 / total,
        mountains: mountains as f32 / total,
        deposits,
        fresh_water_near_start: map.count_in_radius(map.start, START_RADIUS, |t| t.has_fresh_water()) > 0,
    }
}

/// The seed of a text: a number is used as it is, any other text is hashed (FNV-1a), so
/// "Mein Reich" always gives the same world.
pub fn seed_of(text: &str) -> Option<u64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if let Ok(n) = text.parse::<u64>() {
        return Some(n);
    }
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    Some(hash)
}

/// A random seed (presentation only: the process's random hash keys and the clock).
pub fn random_seed() -> u64 {
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    hasher.write_u128(nanos);
    hasher.finish() % 1_000_000_000
}

pub struct NewGameForm {
    seed_text: String,
    size: WorldSize,
    difficulty: Difficulty,
    events: bool,
    scenario: ScenarioId,
    preview: Preview,
}

impl NewGameForm {
    pub fn new(defs: &Defs) -> Self {
        Self {
            seed_text: random_seed().to_string(),
            size: WorldSize::Medium,
            difficulty: Difficulty::Normal,
            events: true,
            scenario: defs.default_scenario(),
            preview: Preview::default(),
        }
    }

    /// The settings of the new game, if the seed is valid.
    pub fn settings(&self, defs: &Defs) -> Option<GameSettings> {
        let seed = seed_of(&self.seed_text)?;
        let mut settings = GameSettings::new(defs, seed, self.size).with_events(self.events);
        settings.difficulty = self.difficulty;
        settings.scenario = self.scenario;
        Some(settings)
    }

    fn preview_key(&self, defs: &Defs) -> Option<PreviewKey> {
        // The same generator settings as the game (one settlement site per AI kingdom of this
        // map size, not the scenario's upper limit).
        let generator = kingdoms_sim::state::world_generator_settings(defs, &self.settings(defs)?);
        Some(PreviewKey { seed: generator.seed, size: generator.size, rival_sites: generator.rival_sites })
    }

    /// Draws the page content.
    pub fn show(&mut self, ui: &mut Ui, defs: &Arc<Defs>, now: f64) -> NewGameAction {
        if let Some(key) = self.preview_key(defs) {
            self.preview.request(key, now);
        }
        let generating = self.preview.update(ui.ctx(), now);
        if generating {
            ui.ctx().request_repaint();
        }
        page_title(ui, "Neues Spiel", None);
        let footer = 48.0;
        let body_height = (ui.available_height() - footer).max(200.0);
        let mut seed_focused = false;
        let body_top = ui.cursor().top();
        ui.horizontal_top(|ui| {
            let form_width = (ui.available_width() * 0.46).clamp(320.0, 430.0);
            ui.allocate_ui_with_layout(Vec2::new(form_width, body_height), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(form_width);
                egui::ScrollArea::vertical().id_salt("new_game_form").max_height(body_height).show(ui, |ui| {
                    seed_focused = self.form(ui, defs);
                });
            });
            ui.add_space(14.0);
            ui.allocate_ui_with_layout(ui.available_size().min(Vec2::new(f32::INFINITY, body_height)), egui::Layout::top_down(egui::Align::Min), |ui| {
                self.preview_panel(ui, generating, now);
            });
        });
        // The buttons stay at the bottom whatever the preview statistics need.
        let used = ui.cursor().top() - body_top;
        if used < body_height {
            ui.add_space(body_height - used);
        }
        ui.add_space(6.0);
        let settings = self.settings(defs);
        let mut action = NewGameAction::None;
        ui.horizontal(|ui| {
            if page_button(ui, "Zurück", false, true).on_hover_text("Esc").clicked() {
                action = NewGameAction::Back;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let start = page_button(ui, "Spiel starten", true, settings.is_some()).on_hover_text("Eingabetaste");
                if start.clicked()
                    && let Some(s) = settings.clone()
                {
                    action = NewGameAction::Start(s);
                }
            });
        });
        if matches!(action, NewGameAction::None) {
            let (esc, enter) = ui.ctx().input_mut(|i| {
                (i.consume_key(egui::Modifiers::NONE, egui::Key::Escape), !seed_focused && i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
            });
            if esc {
                action = NewGameAction::Back;
            } else if enter && let Some(s) = settings {
                action = NewGameAction::Start(s);
            }
        }
        action
    }

    /// The form; returns true while the seed field has the keyboard.
    fn form(&mut self, ui: &mut Ui, defs: &Defs) -> bool {
        field_label(ui, "Welt-Seed");
        let mut focused = false;
        ui.horizontal(|ui| {
            let edit = egui::TextEdit::singleline(&mut self.seed_text)
                .desired_width(ui.available_width() - 120.0)
                .char_limit(MAX_SEED_CHARS)
                .hint_text("Zahl oder beliebiger Text");
            let response = ui.add(edit);
            focused = response.has_focus();
            if page_button(ui, "Zufällig", false, true).on_hover_text("Einen zufälligen Seed würfeln").clicked() {
                self.seed_text = random_seed().to_string();
            }
        });
        match seed_of(&self.seed_text) {
            None => {
                ui.colored_label(Palette::BAD, "Bitte einen Seed eingeben.");
            }
            Some(seed) if self.seed_text.trim().parse::<u64>().is_err() => note(ui, format!("Aus dem Text wird der Seed {seed}.")),
            Some(_) => note(ui, "Derselbe Seed erzeugt immer dieselbe Welt."),
        }
        ui.add_space(8.0);
        field_label(ui, "Kartengröße (Felder)");
        egui::Grid::new("new_game_sizes").num_columns(2).spacing([18.0, 4.0]).show(ui, |ui| {
            for (i, size) in WorldSize::ALL.into_iter().enumerate() {
                let n = size.tiles();
                ui.radio_value(&mut self.size, size, format!("{} · {n} × {n}", size_name(size)));
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
        ui.add_space(8.0);
        field_label(ui, "Schwierigkeit");
        ui.horizontal(|ui| {
            for d in Difficulty::ALL {
                ui.selectable_value(&mut self.difficulty, d, d.display_name());
            }
        });
        let chance = defs.rules().events.difficulty_chance_percent.get(self.difficulty);
        note(ui, format!("Zufällige Ereignisse treten mit {chance} % der normalen Häufigkeit auf."));
        ui.add_space(6.0);
        ui.checkbox(&mut self.events, "Zufällige Ereignisse (Brände, Seuchen, Händler, Flüchtlinge …)");
        ui.add_space(8.0);
        field_label(ui, "Szenario");
        let scenarios = defs.scenarios();
        if scenarios.len() > 1 {
            let current = scenarios.get(self.scenario.index()).map_or("–", |s| s.name.as_str());
            egui::ComboBox::from_id_salt("new_game_scenario").selected_text(current).width(ui.available_width() - 8.0).show_ui(ui, |ui| {
                for s in scenarios {
                    ui.selectable_value(&mut self.scenario, s.id, &s.name);
                }
            });
        }
        if let Some(s) = scenarios.get(self.scenario.index()) {
            if scenarios.len() <= 1 {
                ui.label(RichText::new(&s.name).strong());
            }
            if !s.description.is_empty() {
                note(ui, s.description.clone());
            }
            note(ui, format!("Start: {} Siedler, {} Gold.", s.settlers, s.gold));
        }
        focused
    }

    fn preview_panel(&self, ui: &mut Ui, generating: bool, now: f64) {
        field_label(ui, "Vorschau");
        let width = ui.available_width();
        let height = (width * 0.5).min(ui.available_height() - 140.0).max(120.0);
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        let painter = ui.painter_at(rect.expand(4.0));
        painter.rect_filled(rect, CornerRadius::same(6), Palette::PARCHMENT_LIGHT);
        painter.rect_stroke(rect, CornerRadius::same(6), Stroke::new(1.0, Palette::PARCHMENT_SHADE), egui::StrokeKind::Inside);
        let diamond = Rect::from_center_size(rect.center(), Vec2::new(rect.width().min(rect.height() * 2.0) - 16.0, 0.0));
        let diamond = Rect::from_center_size(rect.center(), Vec2::new(diamond.width(), diamond.width() * 0.5));
        match &self.preview.image {
            Some(image) => {
                let to_screen = |t: egui::Vec2| {
                    let n = image.size.as_vec2();
                    let x = (t.x - t.y + n.y) / (n.x + n.y);
                    let y = (t.x + t.y) / (n.x + n.y);
                    Pos2::new(diamond.left() + x * diamond.width(), diamond.top() + y * diamond.height())
                };
                let n = image.size.as_vec2();
                let corners = [to_screen(Vec2::ZERO), to_screen(Vec2::new(n.x, 0.0)), to_screen(Vec2::new(n.x, n.y)), to_screen(Vec2::new(0.0, n.y))];
                let uvs = [Pos2::new(0.0, 0.0), Pos2::new(1.0, 0.0), Pos2::new(1.0, 1.0), Pos2::new(0.0, 1.0)];
                let tint = if generating { Color32::from_white_alpha(140) } else { Color32::WHITE };
                let mut mesh = egui::Mesh::with_texture(image.texture.id());
                for (pos, uv) in corners.iter().zip(uvs) {
                    mesh.vertices.push(egui::epaint::Vertex { pos: *pos, uv, color: tint });
                }
                mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
                painter.add(egui::Shape::mesh(mesh));
                painter.add(egui::Shape::closed_line(corners.to_vec(), Stroke::new(1.2, Palette::WOOD)));
                let start = to_screen(Vec2::new(image.start.x as f32 + 0.5, image.start.y as f32 + 0.5));
                painter.circle_stroke(start, 7.0, Stroke::new(2.0, Palette::INK));
                painter.circle_filled(start, 4.0, Palette::GOLD_BRIGHT);
                response.on_hover_text("Der goldene Punkt markiert den Startplatz des Hauptquartiers.");
            }
            None => {
                ui.painter().text(rect.center() + Vec2::new(0.0, 28.0), egui::Align2::CENTER_CENTER, "Vorschau wird erzeugt …", heading_font(14.0), Palette::INK_SOFT);
            }
        }
        if generating {
            spinner(&painter, rect.center(), 14.0, now);
        }
        ui.add_space(6.0);
        if let Some(image) = &self.preview.image {
            stats_block(ui, &image.stats);
        }
    }
}

fn size_name(size: WorldSize) -> &'static str {
    match size {
        WorldSize::Small => "Klein",
        WorldSize::Medium => "Mittel",
        WorldSize::Large => "Groß",
        WorldSize::Huge => "Riesig",
    }
}

fn percent(v: f32) -> String {
    format!("{} %", (v * 100.0).round() as u32)
}

fn stats_block(ui: &mut Ui, stats: &MapStats) {
    section_header(ui, "Landschaft");
    egui::Grid::new("new_game_stats").num_columns(2).spacing([16.0, 2.0]).show(ui, |ui| {
        crate::ui::widgets::stat_row(ui, "Wasser", percent(stats.water));
        crate::ui::widgets::stat_row(ui, "Wald", percent(stats.forest));
        crate::ui::widgets::stat_row(ui, "Fels und Gebirge", percent(stats.mountains));
        let fresh = if stats.fresh_water_near_start { "Fluss oder See in der Nähe" } else { "kein Fluss oder See in der Nähe" };
        crate::ui::widgets::stat_row(ui, "Startgebiet", fresh);
    });
    let near: Vec<String> = stats.deposits.iter().map(|(k, _)| k.display_name().to_owned()).collect();
    let text = if near.is_empty() { "keine Vorkommen".to_owned() } else { near.join(", ") };
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Vorkommen am Start:").color(Palette::INK_SOFT));
        ui.label(text);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_from_numbers_and_text() {
        assert_eq!(seed_of("42"), Some(42));
        assert_eq!(seed_of(" 7 "), Some(7));
        assert_eq!(seed_of(""), None);
        assert_eq!(seed_of("   "), None);
        let a = seed_of("Mein Reich");
        assert!(a.is_some());
        assert_eq!(a, seed_of("Mein Reich"), "text seeds are stable");
        assert_ne!(a, seed_of("Mein Reich 2"));
        assert!(random_seed() < 1_000_000_000);
    }

    /// The preview shows the same map the game will generate for the chosen settings.
    #[test]
    fn preview_matches_the_generated_game_world() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        let defs = match Defs::load(&dir) {
            Ok(d) => d,
            Err(e) => panic!("shipped game data must load: {e}"),
        };
        let mut form = NewGameForm::new(&defs);
        form.seed_text = "1234".into();
        form.size = WorldSize::Small;
        let Some(key) = form.preview_key(&defs) else { panic!("valid seed") };
        let preview = make_preview(key);
        let Some(settings) = form.settings(&defs) else { panic!("valid settings") };
        let state = kingdoms_sim::state::GameState::new(&defs, settings);
        assert_eq!(preview.size, state.world().size());
        assert_eq!(preview.start, state.world().start);
        assert_eq!(preview.pixels.len(), (preview.size.x * preview.size.y) as usize);
        assert!(preview.stats.water >= 0.0 && preview.stats.water <= 1.0);
        // The generator gets exactly the game's settings, including one settlement site per AI
        // kingdom of the map size (whose surroundings are cleared of vegetation), not the
        // scenario's upper limit.
        for size in [WorldSize::Small, WorldSize::Medium, WorldSize::Large, WorldSize::Huge] {
            form.size = size;
            let Some(key) = form.preview_key(&defs) else { panic!("valid seed") };
            let Some(settings) = form.settings(&defs) else { panic!("valid settings") };
            assert_eq!(preview_generator(key), kingdoms_sim::state::world_generator_settings(&defs, &settings), "{size:?}");
        }
        form.size = WorldSize::Small;
        let Some(key) = form.preview_key(&defs) else { panic!("valid seed") };
        let limit = defs.scenarios()[form.scenario.index()].rival_kingdoms;
        assert!(key.rival_sites < u32::from(limit), "a small map has fewer kingdoms ({}) than the scenario allows ({limit})", key.rival_sites);
    }
}
