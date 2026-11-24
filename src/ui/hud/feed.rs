//! Notification feed from the simulation's notifications: fading cards on the right (left of
//! the right bar and the info panel), the full list of the game for the „Benachrichtigungen“
//! panel with an unread counter (kept with the saves, `save::slots::SavedNotification`), and
//! toasts (top centre) for rejected commands and save/load results.
//!
//! Calming: a notification that arrives again with the same text and place on the same game day
//! is dropped (dangers excepted). How serious a notification is comes from the simulation (a
//! forester in a dense forest, for instance, reports plain information).

use std::collections::{BTreeMap, VecDeque};

use glam::IVec2;
use kingdoms_engine::egui::{self, Align2, Color32, RichText};
use kingdoms_sim::event::{Severity, SimEvent};
use kingdoms_sim::time::GameClock;

use super::{HudAction, HudInput, TOP_BAR_HEIGHT};
use crate::save::slots::{MAX_SAVED_NOTIFICATIONS, SavedNotification};
use crate::session::{EventCursor, Session};
use crate::settings::NotificationFilter;
use crate::ui::theme::Palette;
use crate::ui::widgets::{Mark, mark};

/// Notifications kept in the feed.
const MAX_ENTRIES: usize = 6;
/// Notifications kept for the „Benachrichtigungen“ panel (and with a save).
const MAX_HISTORY: usize = MAX_SAVED_NOTIFICATIONS;
/// Seconds a notification stays fully visible.
const ENTRY_SECONDS: f64 = 24.0;
/// Seconds of the fade-out at the end.
const FADE_SECONDS: f64 = 3.0;
/// Toasts on screen at once.
const MAX_TOASTS: usize = 3;
/// Seconds a toast stays.
const TOAST_SECONDS: f64 = 4.5;
pub const FEED_WIDTH: f32 = 300.0;
/// Height assumed for a card that could not be measured (a card with four lines of text).
const UNMEASURED_HEIGHT: f32 = 130.0;
/// Vertical gap between two cards.
const ENTRY_GAP: f32 = 4.0;

#[derive(Clone, Debug)]
struct Entry {
    severity: Severity,
    text: String,
    position: Option<IVec2>,
    /// Tick of the (last) arrival.
    tick: u64,
    /// Game time ("Tag 3 · 07:30").
    stamp: String,
    shown_at: f64,
    /// Same text arrived again while shown.
    repeats: u32,
    /// Height of the card when it was last drawn or measured (`None`: not yet).
    height: Option<f32>,
}

#[derive(Clone, Debug)]
struct Toast {
    text: String,
    error: bool,
    shown_at: f64,
}

pub struct Feed {
    cursor: EventCursor,
    /// Which notifications are shown (settings).
    pub filter: NotificationFilter,
    entries: VecDeque<Entry>,
    /// Every notification of the session, newest first (repeats merged like the cards).
    history: VecDeque<Entry>,
    /// Shown notifications since the panel was last open.
    unread: usize,
    toasts: VecDeque<Toast>,
    /// Game day on which a notification (text and place) last arrived (calming repeats).
    last_day: BTreeMap<(String, Option<(i32, i32)>), u64>,
}

/// Game time of tick `tick` as the cards show it ("Frühling 3 · 07:30").
fn stamp(tick: u64) -> String {
    let clock = GameClock::at_tick(tick);
    format!("{} {} · {}", clock.season().display_name(), clock.day_of_season(), clock.time_string())
}

impl Feed {
    /// Starts with the notifications still in the session's log (e.g. of the warp ticks) and
    /// the notification list a loaded save was written with.
    pub fn new(session: &Session) -> Self {
        let history = session
            .restored_notifications()
            .iter()
            .take(MAX_HISTORY)
            .map(|n| Entry {
                severity: n.severity,
                text: n.text.clone(),
                position: n.position.map(|(x, y)| IVec2::new(x, y)),
                tick: n.tick,
                stamp: stamp(n.tick),
                shown_at: 0.0,
                repeats: n.repeats,
                height: None,
            })
            .collect();
        Self {
            cursor: EventCursor::default(),
            filter: NotificationFilter::All,
            entries: VecDeque::new(),
            history,
            unread: 0,
            toasts: VecDeque::new(),
            last_day: BTreeMap::new(),
        }
    }

    /// The notification list for a save, newest first.
    pub fn history_for_save(&self) -> Vec<SavedNotification> {
        self.history
            .iter()
            .map(|e| SavedNotification {
                tick: e.tick,
                severity: e.severity,
                text: e.text.clone(),
                position: e.position.map(|p| (p.x, p.y)),
                repeats: e.repeats,
            })
            .collect()
    }

    /// False if the same notification (text and place) already arrived on the calendar day of
    /// `tick` (from midnight to midnight, like the date in the top bar; dangers always pass);
    /// records the arrival otherwise.
    fn first_today(&mut self, severity: Severity, text: &str, position: Option<IVec2>, tick: u64) -> bool {
        let day = GameClock::at_tick(tick).total_days();
        let key = (text.to_owned(), position.map(|p| (p.x, p.y)));
        if severity != Severity::Danger && self.last_day.get(&key) == Some(&day) {
            return false;
        }
        self.last_day.insert(key, day);
        // Old days are of no use any more.
        if self.last_day.len() > 4 * MAX_HISTORY {
            self.last_day.retain(|_, d| *d == day);
        }
        true
    }

    /// Notifications that arrived since the panel was last open.
    pub fn unread(&self) -> usize {
        self.unread
    }

    /// The panel shows everything.
    pub fn mark_read(&mut self) {
        self.unread = 0;
    }

    /// Takes new notifications and command rejections from the event log.
    pub fn read_events(&mut self, session: &Session, now: f64) {
        let read = session.events().read(&mut self.cursor);
        let mut pending = Vec::new();
        for logged in read.events {
            match &logged.event {
                SimEvent::Notification(n) => {
                    let severity = n.severity;
                    if !self.first_today(severity, &n.text, n.position, logged.tick) {
                        continue;
                    }
                    let entry = Entry {
                        severity,
                        text: n.text.clone(),
                        position: n.position,
                        tick: logged.tick,
                        stamp: stamp(logged.tick),
                        shown_at: now,
                        repeats: 0,
                        height: None,
                    };
                    push_merged(&mut self.history, entry.clone(), MAX_HISTORY);
                    if self.filter.shows(severity) {
                        self.unread += 1;
                        pending.push(entry);
                    }
                }
                SimEvent::CommandRejected { reason, .. } => self.toast(reason.to_string(), now, true),
                _ => {}
            }
        }
        for entry in pending {
            self.push(entry);
        }
    }

    /// Adds a notification at the top. A notification with the same text as a shown one
    /// replaces it (counting the repeats), so the list stays newest first.
    fn push(&mut self, entry: Entry) {
        push_merged(&mut self.entries, entry, MAX_ENTRIES);
    }

    pub fn toast(&mut self, text: String, now: f64, error: bool) {
        self.toasts.retain(|t| t.text != text);
        self.toasts.push_back(Toast { text, error, shown_at: now });
        while self.toasts.len() > MAX_TOASTS {
            self.toasts.pop_front();
        }
    }

    /// Draws the cards with their right edge at `right` (`None`: hidden while the panel with
    /// all notifications is open) between `top` and `bottom`, and the toasts; returns the
    /// rectangle the cards cover, if any are shown.
    pub fn show(&mut self, ctx: &egui::Context, input: &HudInput<'_>, right: Option<f32>, top: f32, bottom: f32, actions: &mut Vec<HudAction>) -> Option<egui::Rect> {
        let now = input.now;
        self.entries.retain(|e| now - e.shown_at < ENTRY_SECONDS + FADE_SECONDS);
        self.toasts.retain(|t| now - t.shown_at < TOAST_SECONDS);
        let cards = match right {
            Some(right) if !self.entries.is_empty() => Some(self.show_entries(ctx, now, right - FEED_WIDTH, top, bottom, actions)),
            _ => None,
        };
        if !self.toasts.is_empty() {
            self.show_toasts(ctx, now);
        }
        cards
    }

    /// Every notification of the session, newest first (`important_only`: warnings and
    /// dangers); a click on one with a place moves the camera there.
    pub fn show_history(&self, ui: &mut egui::Ui, important_only: bool, actions: &mut Vec<HudAction>) {
        let mut shown = 0;
        for (i, entry) in self.history.iter().enumerate() {
            if important_only && !matches!(entry.severity, Severity::Warning | Severity::Danger) {
                continue;
            }
            shown += 1;
            let response = ui.push_id(("history", i), |ui| entry_card(ui, entry, 1.0, ui.available_width())).inner;
            if response.clicked()
                && let Some(tile) = entry.position
            {
                actions.push(HudAction::Focus(tile));
                actions.push(HudAction::Sound("ui_click"));
            }
        }
        if shown == 0 {
            ui.label(RichText::new("Noch keine Meldungen.").italics().color(Palette::INK_SOFT));
        }
    }

    /// Draws as many cards (newest first) as fit between `top` and `bottom`, from `left`;
    /// returns the rectangle they cover.
    fn show_entries(&mut self, ctx: &egui::Context, now: f64, left: f32, top: f32, bottom: f32, actions: &mut Vec<HudAction>) -> egui::Rect {
        let mut keep_alive = Vec::new();
        // New cards are laid out once invisibly (off screen) to know their height before the
        // number of cards that fit is decided.
        if self.entries.iter().any(|e| e.height.is_none()) {
            egui::Area::new(egui::Id::unique("hud_feed_measure"))
                .fixed_pos(egui::pos2(-2.0 * FEED_WIDTH, top))
                .constrain(false)
                .interactable(false)
                .order(egui::Order::Background)
                .show(ctx, |ui| {
                    ui.set_invisible();
                    ui.set_width(FEED_WIDTH);
                    for entry in self.entries.iter_mut().filter(|e| e.height.is_none()) {
                        entry.height = Some(entry_card(ui, entry, 1.0, FEED_WIDTH).rect.height());
                    }
                });
        }
        let visible = fitting(self.entries.iter().map(|e| e.height.unwrap_or(UNMEASURED_HEIGHT)), bottom - top);
        let shown = egui::Area::new(egui::Id::unique("hud_feed"))
            .fixed_pos(egui::pos2(left, top))
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                ui.set_width(FEED_WIDTH);
                ui.spacing_mut().item_spacing.y = ENTRY_GAP;
                for (i, entry) in self.entries.iter_mut().enumerate().take(visible) {
                    let age = now - entry.shown_at;
                    let alpha = if age > ENTRY_SECONDS { (1.0 - (age - ENTRY_SECONDS) / FADE_SECONDS).clamp(0.0, 1.0) } else { 1.0 };
                    let response = entry_card(ui, entry, alpha as f32, FEED_WIDTH);
                    entry.height = Some(response.rect.height());
                    if response.hovered() {
                        keep_alive.push(i);
                    }
                    if response.clicked()
                        && let Some(tile) = entry.position
                    {
                        actions.push(HudAction::Focus(tile));
                        actions.push(HudAction::Sound("ui_click"));
                    }
                }
            });
        // Hovered entries do not fade while the player reads them.
        for i in keep_alive {
            if let Some(e) = self.entries.get_mut(i) {
                e.shown_at = e.shown_at.max(now - ENTRY_SECONDS * 0.5);
            }
        }
        shown.response.rect
    }

    fn show_toasts(&self, ctx: &egui::Context, now: f64) {
        egui::Area::new(egui::Id::unique("hud_toasts"))
            .anchor(Align2::CENTER_TOP, egui::vec2(0.0, TOP_BAR_HEIGHT + 14.0))
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                for toast in &self.toasts {
                    let age = now - toast.shown_at;
                    let alpha = ((TOAST_SECONDS - age) / 0.6).clamp(0.0, 1.0) as f32;
                    let accent = if toast.error { Palette::BAD_LIGHT } else { Palette::GOOD_LIGHT };
                    egui::Frame::new()
                        .fill(Palette::WOOD_DARK.gamma_multiply(0.95 * alpha))
                        .stroke(egui::Stroke::new(1.5, accent.gamma_multiply(alpha)))
                        .corner_radius(egui::CornerRadius::same(6))
                        .inner_margin(egui::Margin::symmetric(14, 7))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                mark(ui, if toast.error { Mark::Error } else { Mark::Ok });
                                ui.label(RichText::new(&toast.text).size(16.0).color(Palette::ON_WOOD.gamma_multiply(alpha)));
                            });
                        });
                }
            });
    }
}

/// Puts `entry` at the top of `list`. An entry with the same text as a listed one replaces it
/// (counting the repeats), so the list stays newest first; at most `max` entries stay.
fn push_merged(list: &mut VecDeque<Entry>, mut entry: Entry, max: usize) {
    if let Some(i) = list.iter().position(|e| e.text == entry.text)
        && let Some(old) = list.remove(i)
    {
        entry.repeats = old.repeats + 1;
        entry.position = entry.position.or(old.position);
        entry.height = old.height;
    }
    list.push_front(entry);
    while list.len() > max {
        list.pop_back();
    }
}

/// Number of cards of `heights` (newest first) that fit into `room` points; at least one.
fn fitting(heights: impl Iterator<Item = f32>, room: f32) -> usize {
    let mut used = 0.0;
    let mut n = 0;
    for h in heights.take(MAX_ENTRIES) {
        let next = used + h + if n > 0 { ENTRY_GAP } else { 0.0 };
        if n > 0 && next > room {
            break;
        }
        used = next;
        n += 1;
    }
    n
}

pub fn severity_color(severity: Severity) -> Color32 {
    match severity {
        Severity::Info => Palette::INFO,
        Severity::Good => Palette::GOOD,
        Severity::Warning => Palette::WARN,
        Severity::Danger => Palette::BAD,
    }
}

fn entry_card(ui: &mut egui::Ui, entry: &Entry, alpha: f32, width: f32) -> egui::Response {
    let accent = severity_color(entry.severity);
    let inner = egui::Frame::new()
        .fill(Palette::PARCHMENT.gamma_multiply(0.96 * alpha))
        .stroke(egui::Stroke::new(1.0, Palette::WOOD.gamma_multiply(alpha)))
        .corner_radius(egui::CornerRadius::same(5))
        .inner_margin(egui::Margin { left: 12, right: 8, top: 5, bottom: 5 })
        .show(ui, |ui| {
            ui.set_width(width - 20.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(entry.severity.display_name()).small().strong().color(accent.gamma_multiply(alpha)));
                ui.label(RichText::new(&entry.stamp).small().color(Palette::INK_SOFT.gamma_multiply(alpha)));
                if entry.repeats > 0 {
                    ui.label(RichText::new(format!("×{}", entry.repeats + 1)).small().color(Palette::INK_SOFT.gamma_multiply(alpha)));
                }
                if entry.position.is_some() {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        mark(ui, Mark::Next);
                    });
                }
            });
            ui.label(RichText::new(&entry.text).color(Palette::INK.gamma_multiply(alpha)));
        });
    // Severity stripe on the left edge.
    let rect = inner.response.rect;
    let stripe = egui::Rect::from_min_max(rect.min + egui::vec2(2.0, 3.0), egui::pos2(rect.min.x + 6.0, rect.max.y - 3.0));
    ui.painter().rect_filled(stripe, egui::CornerRadius::same(2), accent.gamma_multiply(alpha));
    let response = inner.response.interact(egui::Sense::click());
    if entry.position.is_some() {
        response.on_hover_text("Klick: Kamera springt dorthin")
    } else {
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(text: &str, stamp: &str) -> Entry {
        Entry {
            severity: Severity::Warning,
            text: text.to_owned(),
            position: None,
            tick: 0,
            stamp: stamp.to_owned(),
            shown_at: 0.0,
            repeats: 0,
            height: None,
        }
    }

    fn feed() -> Feed {
        Feed {
            cursor: EventCursor::default(),
            filter: NotificationFilter::All,
            entries: VecDeque::new(),
            history: VecDeque::new(),
            unread: 0,
            toasts: VecDeque::new(),
            last_day: BTreeMap::new(),
        }
    }

    #[test]
    fn repeats_on_the_same_day_are_calmed() {
        let mut f = feed();
        let at = Some(IVec2::new(4, 5));
        // Tick 0 is 06:00 of the first day; `clock(day, hour)` is that hour of a calendar day.
        let clock = |day: u64, hour: u64| match GameClock::from_date(1, kingdoms_sim::time::Season::Spring, day, hour, 0) {
            Some(c) => c.tick,
            None => panic!("day {day}, {hour}:00 lies after the game start"),
        };
        let text = "Försterhaus: kein Platz";
        assert!(f.first_today(Severity::Warning, text, at, clock(2, 5)));
        assert!(!f.first_today(Severity::Warning, text, at, clock(2, 7)), "again the same day (before and after 06:00)");
        assert!(!f.first_today(Severity::Warning, text, at, clock(2, 23)), "late on the same day");
        assert!(f.first_today(Severity::Warning, text, Some(IVec2::new(9, 9)), clock(2, 23)), "another building");
        assert!(f.first_today(Severity::Warning, text, at, clock(3, 1)), "the next day begins at midnight");
        assert!(f.first_today(Severity::Danger, "Feuer!", at, clock(3, 2)));
        assert!(f.first_today(Severity::Danger, "Feuer!", at, clock(3, 2) + 1), "dangers always pass");
    }

    /// The list goes into a save and comes back with the loaded game.
    #[test]
    fn the_history_survives_saving() {
        let mut f = feed();
        let mut e = entry("Nahrungsmangel", "x");
        e.tick = 5000;
        e.position = Some(IVec2::new(3, -2));
        f.history.push_front(e);
        let mut e = entry("Neue Siedler", "x");
        e.severity = Severity::Good;
        e.tick = 7000;
        e.repeats = 2;
        f.history.push_front(e);
        let saved = f.history_for_save();
        assert_eq!(saved.len(), 2);
        assert_eq!(saved[0].text, "Neue Siedler");
        assert_eq!(saved[1].position, Some((3, -2)));
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        let defs = match kingdoms_sim::defs::Defs::load(&dir) {
            Ok(d) => std::sync::Arc::new(d),
            Err(e) => panic!("shipped game data must load: {e}"),
        };
        let settings = kingdoms_sim::state::GameSettings::new(&defs, 3, kingdoms_sim::world::WorldSize::Small);
        let state = kingdoms_sim::state::GameState::new(&defs, settings);
        let session = Session::from_state(defs, state, 0).with_restored_notifications(saved.clone());
        let restored = Feed::new(&session);
        assert_eq!(restored.history_for_save(), saved);
        assert_eq!(restored.history[0].stamp, stamp(7000));
        assert_eq!(restored.unread(), 0, "old news are not unread");
        assert!(restored.entries.is_empty(), "no cards pop up again");
    }

    #[test]
    fn repeated_notifications_move_to_the_top() {
        let mut f = feed();
        f.push(entry("Nahrungsmangel", "Frühling 1 · 06:00"));
        f.push(entry("Neue Siedler", "Frühling 1 · 08:00"));
        f.push(entry("Sturm", "Frühling 2 · 10:00"));
        f.push(entry("Nahrungsmangel", "Frühling 3 · 06:00"));
        let order: Vec<(&str, &str, u32)> = f.entries.iter().map(|e| (e.text.as_str(), e.stamp.as_str(), e.repeats)).collect();
        assert_eq!(
            order,
            vec![
                ("Nahrungsmangel", "Frühling 3 · 06:00", 1),
                ("Sturm", "Frühling 2 · 10:00", 0),
                ("Neue Siedler", "Frühling 1 · 08:00", 0),
            ]
        );
        // The oldest notification is dropped first, not the one that just came again.
        for i in 0..MAX_ENTRIES {
            f.push(entry(&format!("Meldung {i}"), "Sommer 1 · 00:00"));
            f.push(entry("Nahrungsmangel", "Sommer 1 · 06:00"));
        }
        assert_eq!(f.entries.len(), MAX_ENTRIES);
        assert_eq!(f.entries[0].text, "Nahrungsmangel");
        assert_eq!(f.entries[0].repeats, 1 + MAX_ENTRIES as u32);
    }

    /// The notification filter of the settings keeps plain information out of the feed.
    #[test]
    fn filtered_notifications_stay_out_of_the_feed() {
        use kingdoms_sim::command::Command;
        use kingdoms_sim::state::GameSettings;
        use kingdoms_sim::world::WorldSize;

        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        let defs = match kingdoms_sim::defs::Defs::load(&dir) {
            Ok(d) => std::sync::Arc::new(d),
            Err(e) => panic!("shipped game data must load: {e}"),
        };
        let started = |filter: NotificationFilter| {
            let mut session = Session::new(defs.clone(), GameSettings::new(&defs, 3, WorldSize::Small));
            let mut f = Feed::new(&session);
            f.filter = filter;
            // Starting the first affordable research is plain information ("Forschung begonnen").
            let tech = defs.techs().iter().find(|t| t.prerequisites.is_empty() && t.cost_gold <= 500).map(|t| t.id);
            let Some(tech) = tech else { panic!("the data has a technology without prerequisites") };
            session.queue_command(Command::QueueResearch { tech });
            session.tick();
            f.read_events(&session, 0.0);
            // The panel lists every notification, the unread counter only the shown ones.
            assert!(f.history.iter().any(|e| e.text.starts_with("Forschung begonnen")));
            assert_eq!(f.unread(), f.entries.len());
            f.entries.iter().any(|e| e.text.starts_with("Forschung begonnen"))
        };
        assert!(started(NotificationFilter::All));
        assert!(!started(NotificationFilter::Important));
        assert!(!started(NotificationFilter::Warnings));
    }

    #[test]
    fn only_cards_that_fit_are_shown() {
        // 720p: about 470 points between the top bar and the build bar.
        assert_eq!(fitting([110.0, 120.0, 66.0, 130.0, 66.0].into_iter(), 470.0), 4);
        assert_eq!(fitting([110.0, 120.0, 66.0, 130.0, 66.0].into_iter(), 310.0), 3);
        assert_eq!(fitting([200.0, 66.0].into_iter(), 150.0), 1, "always the newest card");
        assert_eq!(fitting(std::iter::repeat(40.0), 10_000.0), MAX_ENTRIES);
    }
}
