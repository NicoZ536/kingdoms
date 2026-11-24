//! Notification feed from the simulation's notifications: fading cards on the right (left of
//! the right bar and the info panel), the full list of the game for the „Benachrichtigungen“
//! panel with an unread counter (kept with the saves, `save::slots::SavedNotification`), and
//! toasts (top centre) for rejected commands and save/load results.
//!
//! Grouping, calming and links go by what the simulation says a notification is about – its
//! machine-readable kind and building (`Notification::kind`, `Notification::building`,
//! docs/game_design.md §2 Meldungen) – never by its German text:
//! * A **state** (a waiting state, `NotificationKind::is_waiting_state`, or goods lost for want
//!   of storage room) belongs to its subject: kind and building, else the map tile, else the
//!   whole kingdom. A new notification about the same subject replaces the earlier card and list
//!   entry (newest text, counted „×2“ …); on the same calendar day it updates them quietly
//!   instead of popping up again (dangers always pop up).
//! * Everything else is **news**: only the same news again (same kind and wording, e.g. one
//!   „Fertiggestellt: Stadtmauer“ per finished wall piece) is counted on one card, which then
//!   leads to the newest place.
//! * A click leads to the building (selected, camera there) while it stands, else to the tile.
//! * The „Meldungen“ panel filters by kind (`show_history`).
//!
//! How serious a notification is comes from the simulation (a forester in a dense forest, for
//! instance, reports plain information).

use std::collections::{BTreeMap, VecDeque};

use glam::IVec2;
use kingdoms_engine::egui::{self, Align2, Color32, RichText};
use kingdoms_sim::buildings::BuildingId;
use kingdoms_sim::event::{Notification, NotificationKind, Severity, SimEvent};
use kingdoms_sim::state::GameState;
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
    /// What the notification is about (from the simulation).
    kind: NotificationKind,
    text: String,
    position: Option<IVec2>,
    /// The building it is about, if any (a click selects it while it stands).
    building: Option<BuildingId>,
    /// Tick of the (last) arrival.
    tick: u64,
    /// Game time ("Tag 3 · 07:30").
    stamp: String,
    shown_at: f64,
    /// The same state or news arrived again (see [`Entry::same_as`]).
    repeats: u32,
    /// Height of the card when it was last drawn or measured (`None`: not yet).
    height: Option<f32>,
}

/// Where a reported state is: a building, else a map tile, else the whole kingdom.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Place {
    Building(BuildingId),
    Tile(i32, i32),
    Kingdom,
}

/// The subject of a reported state: its kind and place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Subject {
    kind: NotificationKind,
    place: Place,
}

/// True for kinds that report a lasting state of their subject rather than news: the
/// simulation's waiting states and goods lost for want of storage room.
fn is_state(kind: NotificationKind) -> bool {
    kind.is_waiting_state() || kind == NotificationKind::StorageFull
}

impl Entry {
    fn new(n: &Notification, tick: u64, now: f64) -> Self {
        Entry {
            severity: n.severity,
            kind: n.kind,
            text: n.text.clone(),
            position: n.position,
            building: n.building,
            tick,
            stamp: stamp(tick),
            shown_at: now,
            repeats: 0,
            height: None,
        }
    }

    /// True if `other` stands for the same entry of the list: the same state of the same
    /// subject, or the same news again (same kind and wording).
    fn same_as(&self, other: &Entry) -> bool {
        match (self.state(), other.state()) {
            (Some(a), Some(b)) => a == b,
            (None, None) => self.kind == other.kind && self.text == other.text,
            _ => false,
        }
    }

    /// The subject of the state this entry reports (`None` for news).
    fn state(&self) -> Option<Subject> {
        let place = match (self.building, self.position) {
            (Some(b), _) => Place::Building(b),
            (None, Some(p)) => Place::Tile(p.x, p.y),
            (None, None) => Place::Kingdom,
        };
        is_state(self.kind).then_some(Subject { kind: self.kind, place })
    }

    /// The card's heading: the kind, or the severity for notifications of no particular kind.
    fn heading(&self) -> &'static str {
        match self.kind {
            NotificationKind::General => self.severity.display_name(),
            kind => kind.display_name(),
        }
    }

    /// Where a click leads: the building while it stands, else the tile.
    fn link(&self, state: &GameState) -> Option<Link> {
        match (self.building, self.position) {
            (Some(b), _) if state.building(b).is_some() => Some(Link::Building(b)),
            (_, Some(p)) => Some(Link::Tile(p)),
            _ => None,
        }
    }
}

/// Where a click on a notification leads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Link {
    Building(BuildingId),
    Tile(IVec2),
}

impl Link {
    fn hover_text(self) -> &'static str {
        match self {
            Link::Building(_) => "Klick: Gebäude auswählen",
            Link::Tile(_) => "Klick: Kamera springt dorthin",
        }
    }

    fn follow(self, actions: &mut Vec<HudAction>) {
        actions.push(match self {
            Link::Building(b) => HudAction::SelectBuilding(b),
            Link::Tile(p) => HudAction::Focus(p),
        });
        actions.push(HudAction::Sound("ui_click"));
    }
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
    /// Every notification of the session, newest first (states merged like the cards).
    history: VecDeque<Entry>,
    /// Shown notifications since the panel was last open.
    unread: usize,
    toasts: VecDeque<Toast>,
    /// Game day on which a state of a subject was last reported (calming repeats).
    last_day: BTreeMap<Subject, u64>,
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
                kind: n.kind,
                text: n.text.clone(),
                position: n.position.map(|(x, y)| IVec2::new(x, y)),
                building: n.building,
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
                kind: e.kind,
                building: e.building,
            })
            .collect()
    }

    /// False if `entry` reports a state whose subject was already reported on the calendar day
    /// of its tick (from midnight to midnight, like the date in the top bar; dangers and news
    /// always pass); records the arrival otherwise.
    fn first_today(&mut self, entry: &Entry) -> bool {
        let Some(subject) = entry.state() else { return true };
        let day = GameClock::at_tick(entry.tick).total_days();
        if entry.severity != Severity::Danger && self.last_day.get(&subject) == Some(&day) {
            return false;
        }
        self.last_day.insert(subject, day);
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
                    let entry = Entry::new(n, logged.tick, now);
                    if !self.first_today(&entry) {
                        // The same state again today: the list and a shown card take the news
                        // quietly.
                        update_quietly(&mut self.history, &entry);
                        update_quietly(&mut self.entries, &entry);
                        continue;
                    }
                    push_merged(&mut self.history, entry.clone(), MAX_HISTORY);
                    if self.filter.shows(entry.severity) {
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

    /// Adds a notification at the top. A state about the subject of a shown one, or the same
    /// news again, replaces it (counting the repeats), so the list stays newest first.
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
        let state = input.session.state();
        let cards = match right {
            Some(right) if !self.entries.is_empty() => Some(self.show_entries(ctx, state, now, right - FEED_WIDTH, top, bottom, actions)),
            _ => None,
        };
        if !self.toasts.is_empty() {
            self.show_toasts(ctx, now);
        }
        cards
    }

    /// The kinds in the list with their number of entries, in declaration order (the choices of
    /// the panel's filter).
    pub fn kinds_in_history(&self) -> Vec<(NotificationKind, usize)> {
        let mut counts: BTreeMap<NotificationKind, usize> = BTreeMap::new();
        for entry in &self.history {
            *counts.entry(entry.kind).or_default() += 1;
        }
        counts.into_iter().collect()
    }

    /// Every notification of the session, newest first (`important_only`: warnings and
    /// dangers; `kind`: only that kind); a click on one leads to its building or place.
    pub fn show_history(&self, ui: &mut egui::Ui, state: &GameState, important_only: bool, kind: Option<NotificationKind>, actions: &mut Vec<HudAction>) {
        let mut shown = 0;
        for (i, entry) in self.history.iter().enumerate() {
            if important_only && !matches!(entry.severity, Severity::Warning | Severity::Danger) {
                continue;
            }
            if kind.is_some_and(|k| k != entry.kind) {
                continue;
            }
            shown += 1;
            let link = entry.link(state);
            let response = ui.push_id(("history", i), |ui| entry_card(ui, entry, link, 1.0, ui.available_width())).inner;
            if response.clicked()
                && let Some(link) = link
            {
                link.follow(actions);
            }
        }
        if shown == 0 {
            let text = if self.history.is_empty() { "Noch keine Meldungen." } else { "Keine Meldungen mit dieser Auswahl." };
            ui.label(RichText::new(text).italics().color(Palette::INK_SOFT));
        }
    }

    /// Draws as many cards (newest first) as fit between `top` and `bottom`, from `left`;
    /// returns the rectangle they cover.
    #[allow(clippy::too_many_arguments)]
    fn show_entries(&mut self, ctx: &egui::Context, state: &GameState, now: f64, left: f32, top: f32, bottom: f32, actions: &mut Vec<HudAction>) -> egui::Rect {
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
                        let link = entry.link(state);
                        entry.height = Some(entry_card(ui, entry, link, 1.0, FEED_WIDTH).rect.height());
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
                    let link = entry.link(state);
                    let response = entry_card(ui, entry, link, alpha as f32, FEED_WIDTH);
                    entry.height = Some(response.rect.height());
                    if response.hovered() {
                        keep_alive.push(i);
                    }
                    if response.clicked()
                        && let Some(link) = link
                    {
                        link.follow(actions);
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

/// Puts `entry` at the top of `list`. An entry it stands for ([`Entry::same_as`]) is replaced
/// (counting the repeats), so the list stays newest first; at most `max` entries stay.
fn push_merged(list: &mut VecDeque<Entry>, mut entry: Entry, max: usize) {
    if let Some(i) = list.iter().position(|e| e.same_as(&entry))
        && let Some(old) = list.remove(i)
    {
        entry.repeats = old.repeats + 1;
        if entry.building.is_none() && entry.position.is_none() {
            entry.building = old.building;
            entry.position = old.position;
        }
    }
    list.push_front(entry);
    while list.len() > max {
        list.pop_back();
    }
}

/// Takes the newest text, time and severity of `entry` into the listed entry about the same
/// state, counting the repeat, without moving it or making it pop up again.
fn update_quietly(list: &mut VecDeque<Entry>, entry: &Entry) {
    let subject = entry.state();
    if let Some(old) = list.iter_mut().find(|e| subject.is_some() && e.state() == subject) {
        old.repeats += 1;
        old.severity = entry.severity;
        old.tick = entry.tick;
        old.stamp.clone_from(&entry.stamp);
        if old.text != entry.text {
            old.text.clone_from(&entry.text);
            // Measured again before the card is placed.
            old.height = None;
        }
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

fn entry_card(ui: &mut egui::Ui, entry: &Entry, link: Option<Link>, alpha: f32, width: f32) -> egui::Response {
    let accent = severity_color(entry.severity);
    let inner = egui::Frame::new()
        .fill(Palette::PARCHMENT.gamma_multiply(0.96 * alpha))
        .stroke(egui::Stroke::new(1.0, Palette::WOOD.gamma_multiply(alpha)))
        .corner_radius(egui::CornerRadius::same(5))
        .inner_margin(egui::Margin { left: 12, right: 8, top: 5, bottom: 5 })
        .show(ui, |ui| {
            ui.set_width(width - 20.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(entry.heading()).small().strong().color(accent.gamma_multiply(alpha)))
                    .on_hover_text(entry.severity.display_name());
                ui.label(RichText::new(&entry.stamp).small().color(Palette::INK_SOFT.gamma_multiply(alpha)));
                if entry.repeats > 0 {
                    ui.label(RichText::new(format!("×{}", entry.repeats + 1)).small().color(Palette::INK_SOFT.gamma_multiply(alpha)));
                }
                if link.is_some() {
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
    match link {
        Some(link) => response.on_hover_text(link.hover_text()),
        None => response,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defs() -> std::sync::Arc<kingdoms_sim::defs::Defs> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        match kingdoms_sim::defs::Defs::load(&dir) {
            Ok(d) => std::sync::Arc::new(d),
            Err(e) => panic!("shipped game data must load: {e}"),
        }
    }

    fn entry(kind: NotificationKind, text: &str, stamp: &str) -> Entry {
        Entry {
            severity: Severity::Warning,
            kind,
            text: text.to_owned(),
            position: None,
            building: None,
            tick: 0,
            stamp: stamp.to_owned(),
            shown_at: 0.0,
            repeats: 0,
            height: None,
        }
    }

    /// News of the general kind: never grouped.
    fn news(text: &str, stamp: &str) -> Entry {
        entry(NotificationKind::General, text, stamp)
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

    /// Tick of `hour`:00 on calendar day `day` of the first spring (tick 0 is 06:00 of day 1).
    fn clock(day: u64, hour: u64) -> u64 {
        match GameClock::from_date(1, kingdoms_sim::time::Season::Spring, day, hour, 0) {
            Some(c) => c.tick,
            None => panic!("day {day}, {hour}:00 lies after the game start"),
        }
    }

    #[test]
    fn a_state_is_calmed_by_its_subject_not_its_text() {
        let mut f = feed();
        let sawmill = BuildingId::from_raw_parts(4, 1);
        let at = |kind: NotificationKind, text: &str, building: Option<BuildingId>, tick: u64| {
            let mut e = entry(kind, text, "x");
            e.building = building;
            e.position = Some(IVec2::new(4, 5));
            e.tick = tick;
            e
        };
        let missing = |text: &str, tick: u64| at(NotificationKind::MissingInput, text, Some(sawmill), tick);
        assert!(f.first_today(&missing("Rohstoffmangel: Dem Gebäude „Sägewerk“ fehlt Holz.", clock(2, 5))));
        // Another text about the same state of the same building the same day (before and after
        // 06:00 and late in the day).
        assert!(!f.first_today(&missing("Rohstoffmangel: Dem Gebäude „Sägewerk“ fehlt Holz und Stein.", clock(2, 7))));
        assert!(!f.first_today(&missing("anders", clock(2, 23))));
        assert!(f.first_today(&at(NotificationKind::MissingInput, "anders", Some(BuildingId::from_raw_parts(9, 1)), clock(2, 23))), "another building");
        assert!(f.first_today(&at(NotificationKind::NoCarriers, "anders", Some(sawmill), clock(2, 23))), "another state");
        assert!(f.first_today(&missing("anders", clock(3, 1))), "the next day begins at midnight");
        // News always pass, even with the same text and place.
        let harvest = |tick| at(NotificationKind::Agriculture, "Feldhof: Ernte eingebracht – 12 Weizen.", None, tick);
        assert!(f.first_today(&harvest(clock(3, 2))));
        assert!(f.first_today(&harvest(clock(3, 3))));
        // Dangers always pass.
        let mut famine = at(NotificationKind::Famine, "Hunger!", None, clock(3, 2));
        famine.severity = Severity::Danger;
        assert!(f.first_today(&famine));
        famine.tick += 1;
        assert!(f.first_today(&famine), "dangers always pass");
    }

    #[test]
    fn a_calmed_state_updates_its_entry_quietly() {
        let mut f = feed();
        let mut first = entry(NotificationKind::StorageFull, "Kein Lagerplatz: 3 Holz gingen verloren.", "Frühling 2 · 07:00");
        first.position = Some(IVec2::new(7, 7));
        first.shown_at = 5.0;
        first.height = Some(60.0);
        f.push(first.clone());
        f.push(news("Neue Siedler", "Frühling 2 · 08:00"));
        let mut again = first.clone();
        again.text = "Kein Lagerplatz: 5 Holz gingen verloren.".to_owned();
        again.stamp = "Frühling 2 · 09:00".to_owned();
        again.shown_at = 50.0;
        update_quietly(&mut f.entries, &again);
        let card = &f.entries[1];
        assert_eq!((card.text.as_str(), card.stamp.as_str(), card.repeats), ("Kein Lagerplatz: 5 Holz gingen verloren.", "Frühling 2 · 09:00", 1));
        assert_eq!(card.shown_at, 5.0, "it does not pop up again");
        assert_eq!(card.height, None, "a new text is measured again");
        // News are never updated in place.
        let before: Vec<String> = f.entries.iter().map(|e| e.text.clone()).collect();
        update_quietly(&mut f.entries, &news("Neue Siedler", "später"));
        assert_eq!(f.entries.iter().map(|e| e.text.clone()).collect::<Vec<_>>(), before);
        assert_eq!(f.entries[0].repeats, 0);
    }

    /// The list goes into a save and comes back with the loaded game.
    #[test]
    fn the_history_survives_saving() {
        let mut f = feed();
        let mut e = entry(NotificationKind::Famine, "Nahrungsmangel", "x");
        e.tick = 5000;
        e.position = Some(IVec2::new(3, -2));
        f.history.push_front(e);
        let mut e = entry(NotificationKind::Population, "Neue Siedler", "x");
        e.severity = Severity::Good;
        e.tick = 7000;
        e.repeats = 2;
        e.building = Some(BuildingId::from_raw_parts(12, 3));
        f.history.push_front(e);
        let saved = f.history_for_save();
        assert_eq!(saved.len(), 2);
        assert_eq!(saved[0].text, "Neue Siedler");
        assert_eq!((saved[0].kind, saved[0].building), (NotificationKind::Population, Some(BuildingId::from_raw_parts(12, 3))));
        assert_eq!((saved[1].kind, saved[1].position), (NotificationKind::Famine, Some((3, -2))));
        let defs = defs();
        let settings = kingdoms_sim::state::GameSettings::new(&defs, 3, kingdoms_sim::world::WorldSize::Small);
        let state = kingdoms_sim::state::GameState::new(&defs, settings);
        let session = Session::from_state(defs, state, 0).with_restored_notifications(saved.clone());
        let restored = Feed::new(&session);
        assert_eq!(restored.history_for_save(), saved);
        assert_eq!(restored.history[0].stamp, stamp(7000));
        assert_eq!(restored.unread(), 0, "old news are not unread");
        assert!(restored.entries.is_empty(), "no cards pop up again");
        // A list written before kinds existed reads as general notifications.
        let old = "(tick: 5, severity: Warning, text: \"Alt\", position: None, repeats: 0)";
        match ron::from_str::<SavedNotification>(old) {
            Ok(n) => assert_eq!((n.kind, n.building), (NotificationKind::General, None)),
            Err(e) => panic!("an old entry must load: {e}"),
        }
    }

    #[test]
    fn a_state_of_the_same_subject_moves_to_the_top() {
        let mut f = feed();
        let sawmill = BuildingId::from_raw_parts(4, 1);
        let missing = |text: &str, stamp: &str| {
            let mut e = entry(NotificationKind::MissingInput, text, stamp);
            e.building = Some(sawmill);
            e
        };
        f.push(missing("Dem Sägewerk fehlt Holz.", "Frühling 1 · 06:00"));
        f.push(news("Neue Siedler", "Frühling 1 · 08:00"));
        f.push(news("Sturm", "Frühling 2 · 10:00"));
        f.push(missing("Dem Sägewerk fehlt Holz und Stein.", "Frühling 3 · 06:00"));
        let order: Vec<(&str, &str, u32)> = f.entries.iter().map(|e| (e.text.as_str(), e.stamp.as_str(), e.repeats)).collect();
        assert_eq!(
            order,
            vec![
                ("Dem Sägewerk fehlt Holz und Stein.", "Frühling 3 · 06:00", 1),
                ("Sturm", "Frühling 2 · 10:00", 0),
                ("Neue Siedler", "Frühling 1 · 08:00", 0),
            ]
        );
        // The same news again is counted on its card; the same words of another kind are other
        // news.
        f.push(news("Sturm", "Frühling 4 · 10:00"));
        assert_eq!(f.entries.iter().filter(|e| e.text == "Sturm").map(|e| e.repeats).collect::<Vec<_>>(), vec![1]);
        f.push(entry(NotificationKind::Weather, "Sturm", "Frühling 4 · 11:00"));
        assert_eq!(f.entries.iter().filter(|e| e.text == "Sturm").count(), 2);
        // The oldest notification is dropped first, not the state that just came again.
        for i in 0..MAX_ENTRIES {
            f.push(news(&format!("Meldung {i}"), "Sommer 1 · 00:00"));
            f.push(missing("Dem Sägewerk fehlt Holz.", "Sommer 1 · 06:00"));
        }
        assert_eq!(f.entries.len(), MAX_ENTRIES);
        assert_eq!(f.entries[0].text, "Dem Sägewerk fehlt Holz.");
        assert_eq!(f.entries[0].repeats, 1 + MAX_ENTRIES as u32);
        // The same news about another building: one card that leads to the newest.
        let mut walls = feed();
        for i in 0..3 {
            let mut e = entry(NotificationKind::Construction, "Fertiggestellt: Stadtmauer", "x");
            e.building = Some(BuildingId::from_raw_parts(20 + i, 1));
            e.position = Some(IVec2::new(i as i32, 0));
            walls.push(e);
        }
        assert_eq!(walls.entries.len(), 1);
        assert_eq!((walls.entries[0].repeats, walls.entries[0].building), (2, Some(BuildingId::from_raw_parts(22, 1))));
        // A kingdom-wide state (no building, no place) is one subject.
        let mut g = feed();
        g.push(entry(NotificationKind::NoFuel, "Brennholz wird knapp …", "a"));
        g.push(entry(NotificationKind::NoFuel, "Kälte: …", "b"));
        assert_eq!(g.entries.len(), 1);
        assert_eq!((g.entries[0].text.as_str(), g.entries[0].repeats), ("Kälte: …", 1));
    }

    #[test]
    fn a_click_leads_to_the_building_while_it_stands() {
        let defs = defs();
        let state = kingdoms_sim::state::GameState::new(&defs, kingdoms_sim::state::GameSettings::new(&defs, 3, kingdoms_sim::world::WorldSize::Small));
        let Some((hall, _)) = state.buildings().iter().next() else { panic!("the start has a headquarters") };
        let mut e = entry(NotificationKind::Construction, "Fertiggestellt: Rathaus", "x");
        assert_eq!(e.link(&state), None);
        e.position = Some(IVec2::new(2, 3));
        assert_eq!(e.link(&state), Some(Link::Tile(IVec2::new(2, 3))));
        e.building = Some(hall);
        assert_eq!(e.link(&state), Some(Link::Building(hall)));
        // A demolished building (stale id) leaves the place.
        e.building = Some(BuildingId::from_raw_parts(9_999, 7));
        assert_eq!(e.link(&state), Some(Link::Tile(IVec2::new(2, 3))));
        let mut actions = Vec::new();
        Link::Building(hall).follow(&mut actions);
        assert!(matches!(actions.first(), Some(HudAction::SelectBuilding(b)) if *b == hall));
        // The heading names the kind; notifications of no particular kind name their severity.
        assert_eq!(e.heading(), "Bau");
        assert_eq!(news("x", "x").heading(), "Warnung");
    }

    #[test]
    fn the_panel_offers_the_kinds_in_the_list() {
        let mut f = feed();
        f.history.push_front(entry(NotificationKind::Trade, "a", "x"));
        f.history.push_front(entry(NotificationKind::Construction, "b", "x"));
        f.history.push_front(entry(NotificationKind::Trade, "c", "x"));
        assert_eq!(f.kinds_in_history(), vec![(NotificationKind::Construction, 1), (NotificationKind::Trade, 2)]);
    }

    /// The notification filter of the settings keeps plain information out of the feed.
    #[test]
    fn filtered_notifications_stay_out_of_the_feed() {
        use kingdoms_sim::command::Command;
        use kingdoms_sim::state::GameSettings;
        use kingdoms_sim::world::WorldSize;

        let defs = defs();
        let started = |filter: NotificationFilter| {
            let mut session = Session::new(defs.clone(), GameSettings::new(&defs, 3, WorldSize::Small));
            let mut f = Feed::new(&session);
            f.filter = filter;
            // Starting the first affordable research is plain information of the research kind.
            let tech = defs.techs().iter().find(|t| t.prerequisites.is_empty() && t.cost_gold <= 500).map(|t| t.id);
            let Some(tech) = tech else { panic!("the data has a technology without prerequisites") };
            session.queue_command(Command::QueueResearch { tech });
            session.tick();
            f.read_events(&session, 0.0);
            let research = |e: &Entry| e.kind == NotificationKind::Research && e.severity == Severity::Info;
            // The panel lists every notification, the unread counter only the shown ones.
            assert!(f.history.iter().any(research));
            assert_eq!(f.unread(), f.entries.len());
            f.entries.iter().any(research)
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
