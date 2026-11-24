//! The settings panel (main menu and pause menu): tabs Audio, Grafik, Spiel and Steuerung.
//! Every change applies at once (the caller pushes audio and graphics changes to the engine);
//! the files are written when the panel closes. Key bindings: click a key to rebind it, the
//! next key press (with Ctrl/Alt/Shift) becomes the binding; a key used elsewhere asks before
//! it moves; Esc cancels.

use kingdoms_engine::audio::{AudioChannel, AudioSettings};
use kingdoms_engine::core::config::{EngineConfig, GraphicsConfig};
use kingdoms_engine::egui::{self, Color32, CornerRadius, RichText, Sense, Stroke, Ui, Vec2};
use kingdoms_engine::input::{ActionMap, Binding, KeyCode, Modifiers};

use super::{danger_button, note, page_button, page_title};
use crate::actions::{Action, ActionGroup, MAX_BINDINGS, binding_label, bindable_key, bindings_overlap, conflicts, default_bindings};
use crate::settings::{AutosaveInterval, DisplaySettings, GameplaySettings, NotificationFilter, ParticleDensity, Settings};
use crate::ui::theme::{Palette, heading_font};
use crate::ui::widgets::section_header;

/// Width of the action names in the key list.
const LABEL_WIDTH: f32 = 210.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsTab {
    Audio,
    Graphics,
    Game,
    Controls,
}

impl SettingsTab {
    pub const ALL: [SettingsTab; 4] = [SettingsTab::Audio, SettingsTab::Graphics, SettingsTab::Game, SettingsTab::Controls];

    pub const fn title(self) -> &'static str {
        match self {
            SettingsTab::Audio => "Audio",
            SettingsTab::Graphics => "Grafik",
            SettingsTab::Game => "Spiel",
            SettingsTab::Controls => "Steuerung",
        }
    }
}

/// Everything the panel edits, borrowed for one frame.
pub struct SettingsContext<'a> {
    pub settings: &'a mut Settings,
    pub actions: &'a mut ActionMap<Action>,
    pub engine: &'a mut EngineConfig,
    pub msaa_supported: bool,
    pub audio_device: bool,
    /// Audio files present under `assets/audio`.
    pub audio_files: usize,
    /// The largest UI scale the window allows (the HUD needs 1280 × 720 points).
    pub ui_scale_limit: f32,
    /// Keys pressed this frame (also those the UI consumed) and the held modifiers.
    pub raw_keys: &'a [KeyCode],
    pub modifiers: Modifiers,
}

/// What changed this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SettingsChanges {
    /// Push the audio settings to the engine.
    pub audio: bool,
    /// Apply the graphics settings (vsync, MSAA, UI scale, fullscreen).
    pub graphics: bool,
    /// The player closed the panel.
    pub close: bool,
}

/// A key waiting for confirmation because other actions use it.
#[derive(Clone, Debug, PartialEq)]
struct PendingBinding {
    action: Action,
    slot: usize,
    binding: Binding,
    others: Vec<Action>,
}

pub struct SettingsPanel {
    tab: SettingsTab,
    /// Waiting for a key for this action and binding slot.
    capture: Option<(Action, usize)>,
    pending: Option<PendingBinding>,
    /// UI scale while its slider is dragged (applied on release).
    ui_scale_draft: Option<f32>,
    dirty: bool,
}

impl SettingsPanel {
    pub fn new(tab: SettingsTab) -> Self {
        Self { tab, capture: None, pending: None, ui_scale_draft: None, dirty: false }
    }

    /// True while a key binding waits for a key (the caller ignores that key otherwise).
    pub fn is_capturing(&self) -> bool {
        self.capture.is_some() || self.pending.is_some()
    }

    /// True once if anything changed since the last call (save the files).
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    pub fn show(&mut self, ui: &mut Ui, cx: &mut SettingsContext<'_>) -> SettingsChanges {
        let mut changes = SettingsChanges::default();
        if self.capture.is_some() {
            self.capture_key(ui, cx);
        }
        page_title(ui, "Einstellungen", None);
        ui.horizontal(|ui| {
            for tab in SettingsTab::ALL {
                if tab_button(ui, tab.title(), tab == self.tab).clicked() && tab != self.tab {
                    self.tab = tab;
                    self.capture = None;
                    self.pending = None;
                }
            }
        });
        ui.add_space(6.0);
        let footer = 50.0;
        let height = (ui.available_height() - footer).max(80.0);
        // A solid bar that stays visible whenever a tab is longer than the page (the floating
        // default only appears under the mouse, so cut-off options would look missing).
        ui.spacing_mut().scroll = egui::style::ScrollStyle { foreground_color: true, bar_width: 8.0, ..egui::style::ScrollStyle::solid() };
        egui::ScrollArea::vertical()
            .id_salt(("settings_scroll", self.tab.title()))
            .max_height(height)
            .min_scrolled_height(height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(ui.available_width() - 12.0);
                match self.tab {
                    SettingsTab::Audio => changes.audio |= self.audio(ui, cx),
                    SettingsTab::Graphics => changes.graphics |= self.graphics(ui, cx),
                    SettingsTab::Game => self.game(ui, cx),
                    SettingsTab::Controls => self.controls(ui, cx),
                }
            });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let reset = page_button(ui, "Standard wiederherstellen", false, true)
                .on_hover_text(format!("Setzt die Einstellungen des Reiters „{}“ zurück.", self.tab.title()));
            if reset.clicked() {
                let c = self.reset_tab(cx);
                changes.audio |= c.audio;
                changes.graphics |= c.graphics;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if page_button(ui, "Schließen", true, true).on_hover_text("Esc").clicked() {
                    changes.close = true;
                }
            });
        });
        if !self.is_capturing() && super::escape_pressed(ui.ctx()) {
            changes.close = true;
        }
        if changes.close {
            self.capture = None;
            self.pending = None;
        }
        self.dirty |= changes.audio || changes.graphics;
        changes
    }

    fn reset_tab(&mut self, cx: &mut SettingsContext<'_>) -> SettingsChanges {
        self.dirty = true;
        match self.tab {
            SettingsTab::Audio => {
                cx.engine.audio = AudioSettings::default();
                SettingsChanges { audio: true, ..Default::default() }
            }
            SettingsTab::Graphics => {
                cx.engine.graphics = GraphicsConfig::default();
                cx.engine.window.fullscreen = false;
                cx.settings.display = DisplaySettings::default();
                SettingsChanges { graphics: true, ..Default::default() }
            }
            SettingsTab::Game => {
                cx.settings.game = GameplaySettings::default();
                SettingsChanges::default()
            }
            SettingsTab::Controls => {
                *cx.actions = default_bindings();
                cx.settings.set_bindings(cx.actions);
                self.capture = None;
                self.pending = None;
                SettingsChanges::default()
            }
        }
    }

    fn audio(&mut self, ui: &mut Ui, cx: &mut SettingsContext<'_>) -> bool {
        let mut changed = false;
        let a = &mut cx.engine.audio;
        section_header(ui, "Lautstärke");
        egui::Grid::new("settings_audio").num_columns(2).spacing([24.0, 10.0]).show(ui, |ui| {
            ui.label(RichText::new("Gesamt").strong());
            changed |= volume_slider(ui, &mut a.master);
            ui.end_row();
            for channel in AudioChannel::ALL {
                ui.label(channel.display_name());
                changed |= volume_slider(ui, a.channel_mut(channel));
                ui.end_row();
            }
        });
        ui.add_space(6.0);
        changed |= ui.checkbox(&mut a.muted, "Stumm (alle Kanäle)").changed();
        ui.add_space(8.0);
        if !cx.audio_device {
            note(ui, "Kein Audiogerät gefunden: die Einstellungen werden gespeichert, zu hören ist nichts.");
        }
        if cx.audio_files == 0 {
            note(ui, "Es liegen noch keine Audiodateien bei; die Lautstärken wirken, sobald Klänge vorhanden sind.");
        }
        self.dirty |= changed;
        changed
    }

    fn graphics(&mut self, ui: &mut Ui, cx: &mut SettingsContext<'_>) -> bool {
        let mut changed = false;
        section_header(ui, "Anzeige");
        changed |= ui.checkbox(&mut cx.engine.window.fullscreen, "Vollbild (randloses Fenster über den ganzen Bildschirm)").changed();
        changed |= ui.checkbox(&mut cx.engine.graphics.vsync, "Vertikale Synchronisation (kein Bildreißen, sparsam)").changed();
        ui.add_enabled_ui(!cx.engine.graphics.vsync, |ui| {
            ui.horizontal(|ui| {
                ui.label("Bildrate begrenzen auf");
                let current = fps_label(cx.engine.graphics.max_fps);
                egui::ComboBox::from_id_salt("settings_fps").selected_text(current).show_ui(ui, |ui| {
                    for fps in [30, 60, 120, 144, 240, 0] {
                        changed |= ui.selectable_value(&mut cx.engine.graphics.max_fps, fps, fps_label(fps)).changed();
                    }
                });
            });
        });
        let mut msaa = cx.engine.graphics.msaa_samples >= 4;
        let response = ui.add_enabled(cx.msaa_supported, egui::Checkbox::new(&mut msaa, "Kantenglättung (4× MSAA)"));
        let response = response.on_disabled_hover_text("Die Grafikkarte unterstützt keine 4-fache Kantenglättung.");
        if response.changed() {
            cx.engine.graphics.msaa_samples = if msaa { 4 } else { 1 };
            changed = true;
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Größe der Oberfläche");
            let mut value = self.ui_scale_draft.unwrap_or(cx.engine.graphics.ui_scale) * 100.0;
            let slider = ui.add(egui::Slider::new(&mut value, 75.0..=150.0).step_by(5.0).fixed_decimals(0).suffix(" %"));
            if slider.dragged() {
                self.ui_scale_draft = Some(value / 100.0);
            } else if slider.drag_stopped() || slider.changed() {
                self.ui_scale_draft = None;
                cx.engine.graphics.ui_scale = (value / 100.0).clamp(0.5, 3.0);
                changed = true;
            }
        });
        note(ui, "Die Oberflächengröße wird beim Loslassen des Reglers übernommen.");
        if let Some(text) = ui_scale_limit_note(self.ui_scale_draft.unwrap_or(cx.engine.graphics.ui_scale), cx.ui_scale_limit) {
            ui.label(RichText::new(text).small().color(Palette::WARN));
        }
        section_header(ui, "Details");
        ui.horizontal(|ui| {
            ui.label("Partikel (Rauch, Staub, Laub, Regen, Schnee)");
            for density in ParticleDensity::ALL {
                self.dirty |= ui.selectable_value(&mut cx.settings.display.particles, density, density.label()).changed();
            }
        });
        self.dirty |= ui.checkbox(&mut cx.settings.display.day_night, "Tag und Nacht (Licht nach Tageszeit, beleuchtete Fenster)").changed();
        self.dirty |= changed;
        changed
    }

    fn game(&mut self, ui: &mut Ui, cx: &mut SettingsContext<'_>) {
        let g = &mut cx.settings.game;
        let mut changed = false;
        section_header(ui, "Speichern");
        ui.horizontal(|ui| {
            ui.label("Automatisch speichern");
            egui::ComboBox::from_id_salt("settings_autosave").selected_text(g.autosave.label()).show_ui(ui, |ui| {
                for choice in AutosaveInterval::CHOICES {
                    changed |= ui.selectable_value(&mut g.autosave, choice, choice.label()).changed();
                }
            });
        });
        note(ui, "Drei automatische Spielstände werden reihum überschrieben. Minuten zählen nur, solange das Spiel läuft.");
        section_header(ui, "Automatisch anhalten");
        changed |= ui.checkbox(&mut g.pause_on_event, "Wenn ein Ereignis eine Entscheidung verlangt").changed();
        changed |= ui.checkbox(&mut g.pause_on_danger, "Bei Gefahrenmeldungen (rot markiert)").changed();
        section_header(ui, "Kamera");
        changed |= ui.checkbox(&mut g.edge_scroll, "Bewegen, wenn die Maus den Bildschirmrand berührt").changed();
        changed |= ui.checkbox(&mut g.trackpad_pan, "Trackpad: Wischen mit zwei Fingern verschiebt die Karte").changed();
        note(
            ui,
            if g.trackpad_pan {
                "Zoomen mit zwei Fingern (zusammenziehen oder spreizen), dem Mausrad oder + und −."
            } else {
                "Wischen mit zwei Fingern zoomt wie das Mausrad; zusammenziehen oder spreizen zoomt ebenfalls."
            },
        );
        ui.horizontal(|ui| {
            ui.label("Geschwindigkeit");
            let mut percent = g.camera_speed * 100.0;
            if ui.add(egui::Slider::new(&mut percent, 50.0..=200.0).step_by(10.0).fixed_decimals(0).suffix(" %")).changed() {
                g.camera_speed = (percent / 100.0).clamp(0.5, 2.0);
                changed = true;
            }
        });
        section_header(ui, "Meldungen");
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 18.0;
            for filter in NotificationFilter::ALL {
                changed |= ui.radio_value(&mut g.notifications, filter, filter.label()).changed();
            }
        });
        note(ui, "Hinweise zu abgelehnten Befehlen und zum Speichern erscheinen immer.");
        self.dirty |= changed;
    }

    fn controls(&mut self, ui: &mut Ui, cx: &mut SettingsContext<'_>) {
        if let Some(pending) = self.pending.clone() {
            self.pending_row(ui, cx, &pending);
            ui.add_space(6.0);
        }
        note(
            ui,
            if cfg!(target_os = "macos") {
                "Klickt auf eine Taste, um sie neu zu belegen (mit Cmd, Alt, Ctrl oder Shift möglich). Rechtsklick entfernt eine Belegung."
            } else {
                "Klickt auf eine Taste, um sie neu zu belegen (mit Strg, Alt oder Shift möglich). Rechtsklick entfernt eine Belegung."
            },
        );
        let defaults = default_bindings();
        for group in ActionGroup::ALL {
            section_header(ui, group.title());
            egui::Grid::new(("settings_keys", group.title())).num_columns(3).spacing([12.0, 6.0]).show(ui, |ui| {
                for action in Action::ALL.into_iter().filter(|a| a.group() == group) {
                    let overlaps: Vec<Action> =
                        cx.actions.bindings(action).iter().flat_map(|b| conflicts(cx.actions, b, action)).collect();
                    let label = RichText::new(action.label());
                    let label = if overlaps.is_empty() { label } else { label.color(Palette::BAD) };
                    // A fixed label width keeps the key columns of all sections aligned.
                    let response = ui.horizontal(|ui| {
                        ui.set_width(LABEL_WIDTH);
                        ui.label(label)
                    }).inner;
                    if !overlaps.is_empty() {
                        let names: Vec<&str> = overlaps.iter().map(|a| a.label()).collect();
                        response.on_hover_text(format!("Dieselbe Taste löst auch aus: {}", names.join(", ")));
                    }
                    ui.horizontal(|ui| {
                        let list = cx.actions.bindings(action).to_vec();
                        if !action.rebindable() {
                            let text = list.iter().map(binding_label).collect::<Vec<_>>().join(" · ");
                            key_chip(ui, &text, false, false).on_hover_text("Fest belegt");
                            return;
                        }
                        for slot in 0..MAX_BINDINGS {
                            let capturing = self.capture == Some((action, slot));
                            let text = match (capturing, list.get(slot)) {
                                (true, _) => "Taste drücken … (Esc: abbrechen)".to_owned(),
                                (false, Some(b)) => binding_label(b),
                                (false, None) => "–".to_owned(),
                            };
                            let chip = key_chip(ui, &text, capturing, true);
                            if chip.clicked() {
                                self.capture = Some((action, slot.min(list.len())));
                                self.pending = None;
                            }
                            if chip.secondary_clicked() && slot < list.len() {
                                let mut rest = list.clone();
                                rest.remove(slot);
                                set_bindings(cx.actions, action, &rest);
                                cx.settings.set_bindings(cx.actions);
                                self.dirty = true;
                            }
                            if slot >= list.len() {
                                break;
                            }
                        }
                    });
                    let is_default = cx.actions.bindings(action) == defaults.bindings(action);
                    if action.rebindable() && !is_default {
                        if ui.small_button("Standard").on_hover_text(defaults.bindings(action).iter().map(binding_label).collect::<Vec<_>>().join(" · ")).clicked() {
                            let list = defaults.bindings(action).to_vec();
                            self.assign_all(cx, action, &list);
                        }
                    } else {
                        ui.label("");
                    }
                    ui.end_row();
                }
            });
        }
    }

    /// Takes the next key while waiting for one.
    fn capture_key(&mut self, ui: &mut Ui, cx: &mut SettingsContext<'_>) {
        // Nothing else may react to the keys meant for the binding.
        ui.ctx().input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Key { .. } | egui::Event::Text(_))));
        let Some((action, slot)) = self.capture else { return };
        for &key in cx.raw_keys {
            if key == KeyCode::Escape {
                self.capture = None;
                return;
            }
            if !bindable_key(key) {
                continue;
            }
            let m = cx.modifiers;
            let binding = if m == Modifiers::NONE { Binding::Key(key) } else { Binding::KeyWith(key, m) };
            self.capture = None;
            let others = conflicts(cx.actions, &binding, action);
            if others.is_empty() {
                self.assign(cx, action, slot, binding);
            } else {
                self.pending = Some(PendingBinding { action, slot, binding, others });
            }
            return;
        }
    }

    fn pending_row(&mut self, ui: &mut Ui, cx: &mut SettingsContext<'_>, pending: &PendingBinding) {
        let names: Vec<String> = pending.others.iter().map(|a| format!("„{}“", a.label())).collect();
        egui::Frame::new()
            .fill(Palette::PARCHMENT_DARK)
            .stroke(Stroke::new(1.2, Palette::WARN))
            .corner_radius(CornerRadius::same(5))
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    crate::ui::widgets::mark(ui, crate::ui::widgets::Mark::Warning);
                    ui.label(
                        RichText::new(format!(
                            "{} ist bereits {} zugeordnet. Für „{}“ übernehmen und dort entfernen?",
                            binding_label(&pending.binding),
                            names.join(", "),
                            pending.action.label()
                        ))
                        .strong(),
                    );
                });
                ui.horizontal(|ui| {
                    if danger_button(ui, "Übernehmen", true).clicked() {
                        for other in &pending.others {
                            let rest: Vec<Binding> =
                                cx.actions.bindings(*other).iter().filter(|b| !bindings_overlap(b, &pending.binding)).copied().collect();
                            set_bindings(cx.actions, *other, &rest);
                        }
                        self.pending = None;
                        self.assign(cx, pending.action, pending.slot, pending.binding);
                    }
                    if page_button(ui, "Abbrechen", false, true).clicked() || super::escape_pressed(ui.ctx()) {
                        self.pending = None;
                    }
                });
            });
    }

    fn assign(&mut self, cx: &mut SettingsContext<'_>, action: Action, slot: usize, binding: Binding) {
        let mut list = cx.actions.bindings(action).to_vec();
        list.retain(|b| *b != binding);
        if slot < list.len() {
            list[slot] = binding;
        } else {
            list.push(binding);
        }
        list.truncate(MAX_BINDINGS);
        self.assign_all(cx, action, &list);
    }

    fn assign_all(&mut self, cx: &mut SettingsContext<'_>, action: Action, list: &[Binding]) {
        set_bindings(cx.actions, action, list);
        cx.settings.set_bindings(cx.actions);
        self.dirty = true;
    }
}

/// Replaces the bindings of `action`.
fn set_bindings(map: &mut ActionMap<Action>, action: Action, list: &[Binding]) {
    map.clear(action);
    for b in list {
        map.bind(action, *b);
    }
}

fn fps_label(fps: u32) -> String {
    if fps == 0 { "unbegrenzt".to_owned() } else { format!("{fps} Bilder/s") }
}

fn volume_slider(ui: &mut Ui, value: &mut f32) -> bool {
    let mut percent = (*value * 100.0).round();
    let changed = ui.add(egui::Slider::new(&mut percent, 0.0..=100.0).step_by(1.0).fixed_decimals(0).suffix(" %")).changed();
    if changed {
        *value = (percent / 100.0).clamp(0.0, 1.0);
    }
    changed
}

fn tab_button(ui: &mut Ui, text: &str, active: bool) -> egui::Response {
    let font = heading_font(16.0);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font.clone(), Palette::INK);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(galley.size().x + 32.0, 32.0), Sense::click());
    let painter = ui.painter();
    let hovered = response.hovered();
    let fill = if active {
        Palette::GOLD
    } else if hovered {
        Palette::GOLD_BRIGHT.gamma_multiply(0.6)
    } else {
        Palette::PARCHMENT_DARK
    };
    painter.rect_filled(rect, CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 }, fill);
    let stroke = if active { Stroke::new(1.6, Palette::WOOD) } else { Stroke::new(1.0, Palette::GOLD_DARK) };
    painter.rect_stroke(rect, CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 }, stroke, egui::StrokeKind::Inside);
    painter.text(rect.center(), egui::Align2::CENTER_CENTER, text, font, Palette::INK);
    response
}

/// A key cap button.
fn key_chip(ui: &mut Ui, text: &str, capturing: bool, enabled: bool) -> egui::Response {
    let font = egui::FontId::proportional(15.0);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font.clone(), Palette::INK);
    let width = (galley.size().x + 22.0).max(64.0);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 26.0), sense);
    let painter = ui.painter();
    let hovered = enabled && response.hovered();
    let fill = if capturing {
        Palette::GOLD_BRIGHT
    } else if !enabled {
        Palette::PARCHMENT_DARK.gamma_multiply(0.8)
    } else if hovered {
        Palette::PARCHMENT_LIGHT
    } else {
        Color32::from_rgb(242, 232, 206)
    };
    painter.rect_filled(rect.translate(Vec2::new(0.0, 2.0)), CornerRadius::same(4), Palette::PARCHMENT_SHADE);
    painter.rect_filled(rect, CornerRadius::same(4), fill);
    let stroke = if capturing || hovered { Stroke::new(1.5, Palette::WOOD) } else { Stroke::new(1.0, Palette::GOLD_DARK) };
    painter.rect_stroke(rect, CornerRadius::same(4), stroke, egui::StrokeKind::Inside);
    let color = if enabled { Palette::INK } else { Palette::INK_SOFT };
    painter.text(rect.center(), egui::Align2::CENTER_CENTER, text, font, color);
    response
}

/// Note under the UI scale slider when the window is too small for the chosen scale: the HUD
/// needs 1280 × 720 points, so the scale in effect is lower.
fn ui_scale_limit_note(chosen: f32, limit: f32) -> Option<String> {
    let percent = |v: f32| (v * 100.0).round() as i32;
    (percent(chosen) > percent(limit)).then(|| {
        format!(
            "Bei dieser Fenstergröße wirken nur {} %: die Oberfläche braucht 1280 × 720 Punkte. Ein größeres Fenster erlaubt mehr.",
            percent(limit)
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_too_large_ui_scale_is_explained() {
        assert_eq!(ui_scale_limit_note(1.0, 1.0), None);
        assert_eq!(ui_scale_limit_note(1.25, 1.5), None);
        let Some(text) = ui_scale_limit_note(1.5, 1.0) else { panic!("150 % on 1280 × 720 is limited") };
        assert!(text.contains("nur 100 %"), "{text}");
    }

    fn context<'a>(settings: &'a mut Settings, actions: &'a mut ActionMap<Action>, engine: &'a mut EngineConfig) -> SettingsContext<'a> {
        SettingsContext {
            settings,
            actions,
            engine,
            msaa_supported: true,
            audio_device: false,
            audio_files: 0,
            ui_scale_limit: 3.0,
            raw_keys: &[],
            modifiers: Modifiers::NONE,
        }
    }

    /// Rebinding moves a key from its old action only after confirmation, and the settings
    /// follow the action map.
    #[test]
    fn rebinding_with_conflicts() {
        let mut settings = Settings::default();
        let mut actions = settings.action_map();
        let mut engine = EngineConfig::default();
        let mut panel = SettingsPanel::new(SettingsTab::Controls);
        let mut cx = context(&mut settings, &mut actions, &mut engine);
        // A free key goes straight in (replacing the first binding).
        panel.assign(&mut cx, Action::Rotate, 0, Binding::Key(KeyCode::KeyE));
        assert_eq!(cx.actions.bindings(Action::Rotate), &[Binding::Key(KeyCode::KeyE)]);
        assert_eq!(cx.settings.controls[&Action::Rotate], vec![Binding::Key(KeyCode::KeyE)]);
        // F belongs to the finances: the panel asks.
        let f = Binding::Key(KeyCode::KeyF);
        assert_eq!(conflicts(cx.actions, &f, Action::Rotate), vec![Action::WindowFinance]);
        // Accepting removes it there (as the "Übernehmen" button does).
        let rest: Vec<Binding> = cx.actions.bindings(Action::WindowFinance).iter().filter(|b| !bindings_overlap(b, &f)).copied().collect();
        set_bindings(cx.actions, Action::WindowFinance, &rest);
        panel.assign(&mut cx, Action::Rotate, 1, f);
        assert_eq!(cx.actions.bindings(Action::Rotate), &[Binding::Key(KeyCode::KeyE), f]);
        assert!(cx.actions.bindings(Action::WindowFinance).is_empty());
        assert!(conflicts(cx.actions, &f, Action::Rotate).is_empty());
        assert!(panel.take_dirty() && !panel.take_dirty());
        // Reset brings every default back.
        let changes = panel.reset_tab(&mut cx);
        assert_eq!(changes, SettingsChanges::default());
        assert_eq!(cx.actions.bindings(Action::WindowFinance), &[f]);
        assert_eq!(cx.settings.controls, Settings::default().controls);
    }

    #[test]
    fn resetting_audio_and_graphics_asks_the_engine_to_apply() {
        let mut settings = Settings::default();
        let mut actions = settings.action_map();
        let mut engine = EngineConfig::default();
        engine.audio.master = 0.1;
        engine.graphics.vsync = false;
        settings.display.day_night = false;
        let mut panel = SettingsPanel::new(SettingsTab::Audio);
        let mut cx = context(&mut settings, &mut actions, &mut engine);
        assert!(panel.reset_tab(&mut cx).audio);
        assert_eq!(cx.engine.audio, AudioSettings::default());
        panel.tab = SettingsTab::Graphics;
        assert!(panel.reset_tab(&mut cx).graphics);
        assert!(cx.engine.graphics.vsync && cx.settings.display.day_night);
    }
}
