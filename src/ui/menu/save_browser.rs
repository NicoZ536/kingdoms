//! The save game browser: loading (main menu and pause menu) and saving under a name (pause
//! menu). Lists every save newest first with its metadata – name, real date, game date,
//! population, gold, buildings, seed, map size, playing time, format and version – grouped by
//! game (the simulation's game id; headings when several games are listed), marks saves that
//! cannot be loaded with the German reason, and asks before deleting or overwriting.

use std::path::{Path, PathBuf};

use kingdoms_engine::egui::{self, CornerRadius, FontId, Pos2, RichText, Sense, Stroke, Ui, Vec2};

use super::{danger_button, note, page_button, page_title};
use crate::save::slots::{self, Compatibility, SaveEntry, SlotKind, check_name, list_saves, play_time_text, saved_at_phrase, saved_at_text};
use crate::ui::hud::format::{count, int};
use crate::ui::theme::{Palette, heading_font};

/// What the browser is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserMode {
    Load,
    Save,
}

/// What the player asked for.
#[derive(Clone, Debug, PartialEq)]
pub enum BrowserAction {
    None,
    Close,
    /// Load the save at the path (its display name for messages).
    Load { path: PathBuf, name: String },
    /// Save under `name` at `path` (checked, confirmed if it overwrites).
    Save { path: PathBuf, name: String },
}

/// A question shown above the buttons.
#[derive(Clone, Debug, PartialEq)]
enum Confirm {
    Delete { path: PathBuf, name: String },
    Overwrite { path: PathBuf, name: String, when: String },
}

pub struct SaveBrowser {
    mode: BrowserMode,
    entries: Vec<SaveEntry>,
    selected: Option<usize>,
    /// Name typed for saving.
    name: String,
    confirm: Option<Confirm>,
    /// Message line (text, is an error).
    message: Option<(String, bool)>,
    /// The name field should take the keyboard (first frame).
    focus_name: bool,
    /// The selection changed by keyboard: scroll it into view.
    scroll_to_selected: bool,
}

impl SaveBrowser {
    /// Lists the saves of `dir`. `name` prefills the name field when saving.
    pub fn new(mode: BrowserMode, dir: &Path, fingerprint: u64, name: String) -> Self {
        let mut browser =
            Self {
            mode,
            entries: Vec::new(),
            selected: None,
            name,
            confirm: None,
            message: None,
            focus_name: mode == BrowserMode::Save,
            scroll_to_selected: false,
        };
        browser.refresh(dir, fingerprint);
        browser
    }

    pub fn mode(&self) -> BrowserMode {
        self.mode
    }

    /// Reads the directory again (keeps the selection by path).
    pub fn refresh(&mut self, dir: &Path, fingerprint: u64) {
        let selected = self.selected.and_then(|i| self.entries.get(i)).map(|e| e.path.clone());
        let mut entries = list_saves(dir, fingerprint);
        if self.mode == BrowserMode::Save {
            entries.retain(|e| e.kind == SlotKind::Manual);
        }
        slots::group_by_game(&mut entries);
        self.entries = entries;
        self.selected = match selected {
            Some(path) => self.entries.iter().position(|e| e.path == path),
            None if self.mode == BrowserMode::Load => self.entries.iter().position(SaveEntry::loadable),
            None => None,
        };
    }

    /// Shows a message under the list (e.g. a failed load).
    pub fn set_message(&mut self, text: impl Into<String>, error: bool) {
        self.message = Some((text.into(), error));
    }

    /// Draws the browser into a page. `busy`: a save is being written (saving waits).
    pub fn show(&mut self, ui: &mut Ui, dir: &Path, fingerprint: u64, busy: bool) -> BrowserAction {
        let title = match self.mode {
            BrowserMode::Load => "Spiel laden",
            BrowserMode::Save => "Spiel speichern",
        };
        let count = self.entries.len();
        let subtitle = match (self.mode, count) {
            (BrowserMode::Load, 0) => "keine Spielstände".to_owned(),
            (BrowserMode::Load, 1) => "1 Spielstand".to_owned(),
            (BrowserMode::Load, n) => format!("{n} Spielstände"),
            (BrowserMode::Save, 0) => "noch keine benannten Spielstände".to_owned(),
            (BrowserMode::Save, 1) => "1 benannter Spielstand".to_owned(),
            (BrowserMode::Save, n) => format!("{n} benannte Spielstände"),
        };
        page_title(ui, title, Some(&subtitle));
        let mut action = BrowserAction::None;
        if self.mode == BrowserMode::Save {
            action = self.name_row(ui, dir, busy);
        }
        let footer = if self.confirm.is_some() { 92.0 } else { 52.0 } + if self.message.is_some() { 24.0 } else { 0.0 };
        let height = (ui.available_height() - footer).max(80.0);
        let mut activate = None;
        egui::ScrollArea::vertical().id_salt("save_browser_list").max_height(height).min_scrolled_height(height).auto_shrink([false, false]).show(
            ui,
            |ui| {
                ui.set_width(ui.available_width() - 10.0);
                if self.entries.is_empty() {
                    ui.add_space(20.0);
                    ui.vertical_centered(|ui| {
                        let text = match self.mode {
                            BrowserMode::Load => "Noch keine Spielstände vorhanden.",
                            BrowserMode::Save => "Noch keine benannten Spielstände – gebt oben einen Namen ein.",
                        };
                        ui.label(RichText::new(text).color(Palette::INK_SOFT));
                    });
                } else if self.mode == BrowserMode::Load && !self.entries.iter().any(SaveEntry::loadable) {
                    ui.label(RichText::new(no_loadable_text(self.entries.len())).color(Palette::BAD));
                    ui.add_space(4.0);
                }
                let names: Vec<String> = self.entries.iter().map(SaveEntry::display_name).collect();
                // Headings only when saves of more than one game are listed.
                let several_games = self.entries.windows(2).any(|w| !slots::same_game(&w[0], &w[1]));
                for i in 0..self.entries.len() {
                    if several_games && (i == 0 || !slots::same_game(&self.entries[i - 1], &self.entries[i])) {
                        let size = self.entries[i..].iter().take_while(|e| slots::same_game(&self.entries[i], e)).count();
                        group_heading(ui, &self.entries[i..i + size], i > 0);
                    }
                    // Two saves with the same name are told apart by their file names.
                    let twin = names.iter().filter(|n| **n == names[i]).count() > 1;
                    let response = entry_row(ui, &self.entries[i], self.selected == Some(i), twin);
                    if response.clicked() {
                        self.select(i);
                    }
                    if response.double_clicked() {
                        activate = Some(i);
                    }
                    if self.selected == Some(i) && self.scroll_to_selected {
                        response.scroll_to_me(None);
                    }
                    ui.add_space(4.0);
                }
                self.scroll_to_selected = false;
            },
        );
        if let Some((text, error)) = &self.message {
            ui.colored_label(if *error { Palette::BAD } else { Palette::GOOD }, text);
        }
        ui.add_space(4.0);
        if let Some(confirm) = self.confirm.clone() {
            if let Some(a) = self.confirm_row(ui, &confirm, dir, fingerprint) {
                action = a;
            }
            return action;
        }
        let selected = self.selected.and_then(|i| self.entries.get(i));
        let has_selection = selected.is_some();
        let load_target = selected.filter(|e| e.loadable()).map(|e| (e.path.clone(), e.display_name()));
        let can_save = !busy && check_name(&self.name).is_ok();
        let mut delete = false;
        let mut save = false;
        ui.horizontal(|ui| {
            if page_button(ui, "Zurück", false, true).on_hover_text("Esc").clicked() {
                action = BrowserAction::Close;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                match self.mode {
                    BrowserMode::Load => {
                        let load = page_button(ui, "Laden", true, load_target.is_some()).on_hover_text("Eingabetaste oder Doppelklick");
                        if load.clicked()
                            && let Some((path, name)) = load_target.clone()
                        {
                            action = BrowserAction::Load { path, name };
                        }
                    }
                    BrowserMode::Save => {
                        save = page_button(ui, "Speichern", true, can_save).on_hover_text("Eingabetaste").clicked();
                    }
                }
                delete = danger_button(ui, "Löschen", has_selection).on_hover_text("Entf").clicked();
            });
        });
        if save {
            action = self.request_save(dir);
        }
        if delete {
            self.ask_delete();
        }
        if action == BrowserAction::None {
            action = self.keys(ui, dir, activate);
        }
        action
    }

    /// Selects a row; when saving, its name goes into the name field.
    fn select(&mut self, i: usize) {
        self.selected = Some(i);
        self.message = None;
        if self.mode == BrowserMode::Save
            && let Some(e) = self.entries.get(i)
        {
            self.name = e.display_name();
        }
    }

    fn name_row(&mut self, ui: &mut Ui, dir: &Path, busy: bool) -> BrowserAction {
        let mut action = BrowserAction::None;
        super::field_label(ui, "Name des Spielstands");
        ui.horizontal(|ui| {
            let edit = egui::TextEdit::singleline(&mut self.name)
                .desired_width(ui.available_width().min(420.0))
                .char_limit(slots::MAX_NAME_CHARS)
                .hint_text("z. B. Mein Königreich");
            let response = ui.add(edit);
            if self.focus_name {
                response.request_focus();
                self.focus_name = false;
            }
            if response.changed() {
                self.message = None;
                self.selected = self.entries.iter().position(|e| check_name(&self.name).is_ok_and(|s| s == e.stem));
            }
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !busy && check_name(&self.name).is_ok() {
                // The Enter that saves must not also answer the overwrite question that appears
                // further down in this same frame.
                ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                action = self.request_save(dir);
            }
        });
        match check_name(&self.name) {
            Err(err) if !self.name.is_empty() => {
                ui.colored_label(Palette::BAD, err.message());
            }
            _ if busy => note(ui, "Ein Spielstand wird gerade geschrieben …"),
            _ => note(ui, "Ein Name, den es schon gibt, überschreibt diesen Spielstand (nach Rückfrage)."),
        }
        ui.add_space(4.0);
        action
    }

    /// Checks the name; a new save goes ahead, an existing one asks first.
    fn request_save(&mut self, dir: &Path) -> BrowserAction {
        match check_name(&self.name) {
            Ok(stem) => {
                let path = slots::save_path(dir, &stem);
                let name = self.name.trim().to_owned();
                match self.entries.iter().find(|e| e.path == path) {
                    Some(existing) => {
                        self.confirm = Some(Confirm::Overwrite { path, name, when: saved_at_phrase(existing.saved_at()) });
                        BrowserAction::None
                    }
                    None => BrowserAction::Save { path, name },
                }
            }
            Err(err) => {
                self.message = Some((err.message().to_owned(), true));
                BrowserAction::None
            }
        }
    }

    fn ask_delete(&mut self) {
        if let Some(e) = self.selected.and_then(|i| self.entries.get(i)) {
            self.confirm = Some(Confirm::Delete { path: e.path.clone(), name: e.display_name() });
        }
    }

    fn confirm_row(&mut self, ui: &mut Ui, confirm: &Confirm, dir: &Path, fingerprint: u64) -> Option<BrowserAction> {
        let (question, yes) = match confirm {
            Confirm::Delete { name, .. } => (format!("„{name}“ wirklich löschen? Das lässt sich nicht rückgängig machen."), "Löschen"),
            Confirm::Overwrite { name, when, .. } => (overwrite_question(name, when), "Überschreiben"),
        };
        let mut result = None;
        egui::Frame::new()
            .fill(Palette::PARCHMENT_DARK)
            .stroke(Stroke::new(1.2, Palette::WARN))
            .corner_radius(CornerRadius::same(5))
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    crate::ui::widgets::mark(ui, crate::ui::widgets::Mark::Warning);
                    ui.label(RichText::new(question).strong());
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let (no, yes_key) = ui.ctx().input_mut(|i| {
                        (i.consume_key(egui::Modifiers::NONE, egui::Key::Escape), i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
                    });
                    if page_button(ui, "Abbrechen", false, true).clicked() || no {
                        self.confirm = None;
                        result = Some(BrowserAction::None);
                    }
                    if danger_button(ui, yes, true).clicked() || yes_key {
                        self.confirm = None;
                        result = Some(match confirm {
                            Confirm::Delete { path, name } => {
                                match slots::delete_save(path) {
                                    Ok(()) => self.message = Some((format!("„{name}“ wurde gelöscht."), false)),
                                    Err(err) => {
                                        log::error!("cannot delete {}: {err}", path.display());
                                        self.message = Some((format!("„{name}“ konnte nicht gelöscht werden."), true));
                                    }
                                }
                                self.selected = None;
                                self.refresh(dir, fingerprint);
                                BrowserAction::None
                            }
                            Confirm::Overwrite { path, name, .. } => BrowserAction::Save { path: path.clone(), name: name.clone() },
                        });
                    }
                });
            });
        result
    }

    /// Up/Down select, Enter loads or saves, Delete asks to delete, Esc closes.
    fn keys(&mut self, ui: &mut Ui, dir: &Path, activate: Option<usize>) -> BrowserAction {
        let typing = ui.ctx().memory(|m| m.focused().is_some());
        let (up, down, enter, delete, esc) = ui.ctx().input_mut(|i| {
            let esc = i.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
            if typing {
                return (false, false, false, false, esc);
            }
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Delete),
                esc,
            )
        });
        if esc {
            return BrowserAction::Close;
        }
        let n = self.entries.len();
        if n > 0 && (up || down) {
            let i = match (self.selected, up) {
                (None, _) => 0,
                (Some(i), true) => (i + n - 1) % n,
                (Some(i), false) => (i + 1) % n,
            };
            self.select(i);
            self.scroll_to_selected = true;
        }
        if delete {
            self.ask_delete();
        }
        let target = activate.or(if enter { self.selected } else { None });
        match (self.mode, target.and_then(|i| self.entries.get(i))) {
            (BrowserMode::Load, Some(e)) if e.loadable() => BrowserAction::Load { path: e.path.clone(), name: e.display_name() },
            (BrowserMode::Load, Some(e)) => {
                if let Compatibility::Incompatible(reason) = &e.compatibility {
                    self.message = Some((format!("„{}“ kann nicht geladen werden: {reason}", e.display_name()), true));
                }
                BrowserAction::None
            }
            (BrowserMode::Save, Some(_)) if activate.is_some() || enter => self.request_save(dir),
            _ => BrowserAction::None,
        }
    }
}

/// Heading above the saves of one game (`group`, newest first): its name – the newest save
/// named by the player, else the scenario –, seed, map size and the number of saves.
fn group_heading(ui: &mut Ui, group: &[SaveEntry], space_above: bool) {
    if space_above {
        ui.add_space(6.0);
    }
    let (title, details) = group_title(group);
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).font(heading_font(16.0)).color(Palette::INK));
        ui.label(RichText::new(details).small().color(Palette::INK_SOFT));
    });
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().line_segment([rect.left_center(), rect.right_center()], Stroke::new(1.0, Palette::GOLD_DARK));
    ui.add_space(3.0);
}

/// Title and details of a group of saves of one game.
fn group_title(group: &[SaveEntry]) -> (String, String) {
    let n = group.len() as u64;
    let Some(Ok(h)) = group.first().map(|e| &e.header) else {
        // Unreadable files are no saves of a game.
        return ("Nicht lesbare Dateien".to_owned(), count(n, "Datei", "Dateien"));
    };
    let saves = count(n, "Spielstand", "Spielstände");
    let named = group.iter().find(|e| e.kind == SlotKind::Manual).map(SaveEntry::display_name);
    let title = match named {
        Some(name) => format!("Spiel „{name}“"),
        None => format!("Spiel: {}", h.summary.scenario),
    };
    (title, format!("Seed {} · {} · {saves}", h.seed, world_size_text(h.summary.world_size)))
}

/// One save in the list: name (with the file name if another save has the same name), kind and
/// date; game details; format and compatibility.
fn entry_row(ui: &mut Ui, e: &SaveEntry, selected: bool, show_file: bool) -> egui::Response {
    let width = ui.available_width();
    let incompatible = match &e.compatibility {
        Compatibility::Compatible => None,
        Compatibility::Incompatible(reason) => Some(reason.as_str()),
    };
    let lines = match (&e.header, incompatible.is_some()) {
        (Ok(_), true) => 4.0,
        (Ok(_), false) | (Err(_), true) => 3.0,
        (Err(_), false) => 2.0,
    };
    let height = 18.0 + 22.0 + (lines - 1.0) * 19.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let painter = ui.painter_at(rect);
    let hovered = response.hovered();
    let fill = if selected {
        Palette::GOLD_BRIGHT.gamma_multiply(0.55)
    } else if hovered {
        Palette::PARCHMENT_LIGHT
    } else {
        Palette::PARCHMENT_LIGHT.gamma_multiply(0.92)
    };
    painter.rect_filled(rect, CornerRadius::same(5), fill);
    let stroke = if selected { Stroke::new(1.8, Palette::GOLD_DARK) } else { Stroke::new(1.0, Palette::PARCHMENT_SHADE) };
    painter.rect_stroke(rect, CornerRadius::same(5), stroke, egui::StrokeKind::Inside);
    let ink = if incompatible.is_some() { Palette::INK_SOFT } else { Palette::INK };
    let left = rect.left() + 12.0;
    let mut y = rect.top() + 7.0;
    // Line 1: name, kind badge, real date.
    let name = painter.text(Pos2::new(left, y), egui::Align2::LEFT_TOP, e.display_name(), heading_font(17.0), ink);
    if show_file && e.header.is_ok() {
        let file = format!("Datei {}", file_name(&e.path));
        painter.text(Pos2::new(name.right() + 10.0, y + 3.0), egui::Align2::LEFT_TOP, file, FontId::proportional(14.0), Palette::INK_SOFT);
    }
    painter.text(Pos2::new(rect.right() - 12.0, y + 1.0), egui::Align2::RIGHT_TOP, saved_at_text(e.saved_at()), FontId::proportional(15.0), Palette::INK_SOFT);
    y += 24.0;
    let small = FontId::proportional(14.0);
    match &e.header {
        Ok(h) => {
            let mut details = vec![h.summary.date.clone()];
            match &e.meta {
                Some(m) => {
                    details.push(format!("{} Einwohner", int(i64::from(m.population))));
                    details.push(format!("{} Gold", int(m.treasury)));
                    details.push(format!("{} Gebäude", int(i64::from(m.buildings))));
                }
                None => details.push("Einwohner und Gold unbekannt".to_owned()),
            }
            painter.text(Pos2::new(left, y), egui::Align2::LEFT_TOP, details.join(" · "), small.clone(), ink);
            y += 19.0;
            let mut more = vec![
                h.summary.scenario.clone(),
                world_size_text(h.summary.world_size),
                format!("Schwierigkeit {}", h.summary.difficulty.display_name()),
                format!("Seed {}", h.seed),
            ];
            if let Some(m) = &e.meta {
                more.push(format!("Spielzeit {}", play_time_text(m.play_seconds)));
            }
            more.push(format!("Format {} · Version {} · {}", h.format_version, h.game_version, megabytes(e.size)));
            painter.text(Pos2::new(left, y), egui::Align2::LEFT_TOP, more.join(" · "), small.clone(), Palette::INK_SOFT);
            y += 19.0;
        }
        Err(_) => {
            painter.text(Pos2::new(left, y), egui::Align2::LEFT_TOP, format!("Datei {}", file_name(&e.path)), small.clone(), Palette::INK_SOFT);
            y += 19.0;
        }
    }
    if let Some(reason) = incompatible {
        painter.text(Pos2::new(left, y), egui::Align2::LEFT_TOP, format!("Nicht ladbar: {reason}"), small, Palette::BAD);
    }
    response
}

fn world_size_text(size: kingdoms_sim::world::WorldSize) -> String {
    let n = size.tiles();
    let name = match size {
        kingdoms_sim::world::WorldSize::Small => "Klein",
        kingdoms_sim::world::WorldSize::Medium => "Mittel",
        kingdoms_sim::world::WorldSize::Large => "Groß",
        kingdoms_sim::world::WorldSize::Huge => "Riesig",
    };
    format!("{name} ({n} × {n})")
}

/// File size as "2,4 MB" (at least one decimal).
fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_048_576.0).replace('.', ",")
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Saves exist, but none can be loaded with this version of the game.
pub fn no_loadable_text(saves: usize) -> String {
    match saves {
        1 => "Es gibt einen Spielstand, aber er kann mit dieser Version nicht geladen werden.".to_owned(),
        n => format!("Es gibt {n} Spielstände, aber keiner kann mit dieser Version geladen werden."),
    }
}

/// „Den Spielstand „Alt“, gespeichert heute um 13:58, überschreiben?“ (`when` from
/// [`saved_at_phrase`]).
fn overwrite_question(name: &str, when: &str) -> String {
    format!("Den Spielstand „{name}“, gespeichert {when}, überschreiben?")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_overwrite_question_reads_naturally_for_every_day() {
        assert_eq!(overwrite_question("alter_stand", "heute um 13:58"), "Den Spielstand „alter_stand“, gespeichert heute um 13:58, überschreiben?");
        let old = overwrite_question("Burg", &saved_at_phrase(1_000_000_000));
        assert!(old.contains(", gespeichert am ") && old.ends_with(", überschreiben?"), "{old}");
    }

    #[test]
    fn unloadable_saves_are_named_as_such() {
        assert!(no_loadable_text(1).starts_with("Es gibt einen Spielstand, aber er"));
        assert_eq!(no_loadable_text(3), "Es gibt 3 Spielstände, aber keiner kann mit dieser Version geladen werden.");
        let broken = SaveEntry {
            path: PathBuf::from("x.ksav"),
            stem: "x".into(),
            kind: SlotKind::Manual,
            header: Err("kaputt".into()),
            meta: None,
            compatibility: Compatibility::Incompatible("kaputt".into()),
            modified_unix: 0,
            size: 0,
        };
        assert_eq!(group_title(&[broken.clone(), broken.clone()]), ("Nicht lesbare Dateien".to_owned(), "2 Dateien".to_owned()));
        assert_eq!(group_title(&[broken]), ("Nicht lesbare Dateien".to_owned(), "1 Datei".to_owned()));
    }
}
