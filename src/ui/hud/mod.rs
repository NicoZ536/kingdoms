//! The in-game HUD in the layout of the master design (§13): top bar (population, gold, food,
//! wood, stone, iron, happiness, season, date; management window buttons and menu), left bar
//! (Bauen, Straßen, Landwirtschaft, Produktion, Militär with the build flyout, Forschung), right
//! bar (Ereignisse, Benachrichtigungen, Aufgaben, Übersicht with their panels), bottom bar
//! (selection tools, speed, pause, time of day), the info panel of the selection, the management
//! window („Reichsverwaltung“), the decision dialogs of events and diplomatic proposals, the
//! rank-up celebration, notification cards and toasts, minimap, markers of approaching enemies,
//! the banner of the order tool, the world tooltip of the active tool, the pause menu (the defeat
//! screen once the game is lost) and confirmation dialogs.
//!
//! The HUD only reads the session and returns [`HudAction`]s; the world scene turns them into
//! commands for the session's queue, tool changes, camera moves, saves and sounds. Every element
//! shows real simulation state; nothing here changes the simulation directly.

mod bottom_bar;
mod build_menu;
mod citizen_window;
mod dialogs;
mod event_dialog;
mod farm_panel;
mod feed;
pub mod format;
mod heraldry;
mod icons;
mod info_panel;
mod management;
mod minimap;
mod overview;
mod proposal_dialog;
mod right_bar;
mod tasks;
mod tooltip;
mod top_bar;
mod war_markers;

use glam::{IVec2, Vec2};
use kingdoms_engine::core::time::GameSpeed;
use kingdoms_engine::egui;
use kingdoms_engine::input::ActionMap;
use kingdoms_engine::math::Rect;
use kingdoms_sim::buildings::BuildingId;
use kingdoms_sim::command::Command;
use kingdoms_sim::defs::{BuildingTypeId, RoadTypeId};
use kingdoms_sim::diplomacy::KingdomId;
use kingdoms_sim::population::CitizenId;
use kingdoms_sim::report::{ProposalRow, ThreatRow};

pub use build_menu::BuildTab;
pub use icons::Icons;
pub use management::Tab as WindowTab;
pub use minimap::{Minimap, natural_rgb};
pub use right_bar::RightPanel;

use crate::actions::Action;
use crate::session::Session;
use crate::tools::{DemolishRequest, OrderPick, ToolController};
use crate::view::picking::{Hover, Selection};

/// Height of the top bar in points (other elements are placed below it).
pub const TOP_BAR_HEIGHT: f32 = 44.0;
/// Gap between HUD elements and the screen edge.
pub const MARGIN: f32 = 8.0;
/// Gap between neighbouring panels on the right.
const PANEL_GAP: f32 = 6.0;

/// The HUD's docked areas (bars, side panels, notification cards, minimap): unlike the floating
/// windows the player brings to the front, they must not cover a decision dialog.
const DOCKED_AREAS: [&str; 11] = [
    "hud_top_bar",
    "hud_left_bar",
    "hud_build_flyout",
    "hud_right_bar",
    "hud_right_panel",
    "hud_info_panel",
    "hud_feed",
    "hud_proposal_card",
    "hud_bottom_bar",
    "hud_minimap",
    "hud_order_banner",
];

/// True if one of the docked areas is the topmost area of the middle layer, e.g. a side panel
/// opened or clicked after a decision dialog opened; the dialog then moves back on top.
pub(super) fn docked_area_on_top(ctx: &egui::Context) -> bool {
    let top = ctx.memory(|m| m.areas().top_layer_id(egui::Order::Middle));
    top.is_some_and(|layer| DOCKED_AREAS.iter().any(|name| egui::Id::unique(*name) == layer.id))
}

/// A tool the HUD asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolRequest {
    Inspect,
    Build(BuildingTypeId),
    Road(RoadTypeId),
    Demolish,
    /// Move tool; with a building it is picked up right away.
    Move(Option<BuildingId>),
}

/// What the player did in the HUD this frame.
#[derive(Clone, Debug, PartialEq)]
pub enum HudAction {
    /// Queue a simulation command.
    Command(Command),
    /// Switch the map tool.
    Tool(ToolRequest),
    /// Close the info panel.
    Deselect,
    /// Select a building (info panel) and centre the camera on it.
    SelectBuilding(BuildingId),
    /// Open the management window at a tab, or close it if that tab is shown.
    ToggleWindow(management::Tab),
    /// Open the management window at a tab (it stays open if shown).
    OpenWindow(management::Tab),
    /// Show or hide the decision dialog of the pending event.
    EventDialog(bool),
    /// Show or hide the tile grid.
    ToggleGrid,
    /// Show or hide the territory border.
    ToggleTerritory,
    /// Centre the camera on a tile.
    Focus(IVec2),
    /// Centre the camera on a world point (minimap).
    LookAt(Vec2),
    /// Change the game speed.
    Speed(GameSpeed),
    /// The pause menu opened or closed.
    Menu(bool),
    QuickSave,
    QuickLoad,
    /// Pause menu: the save dialog, the load list, the settings.
    OpenSave,
    OpenLoad,
    OpenSettings,
    /// Pause menu: back to the main menu (asks first if progress is unsaved).
    MainMenu,
    /// Quit the game (asks first if progress is unsaved).
    Quit,
    /// Play a UI sound.
    Sound(&'static str),
    /// Open the citizen window for a citizen.
    InspectCitizen(CitizenId),
    /// The citizen window was closed (also stops following).
    CloseCitizen,
    /// Follow a citizen with the camera (`None` stops).
    FollowCitizen(Option<CitizenId>),
    /// Centre the camera on a citizen once.
    LookAtCitizen(CitizenId),
    /// Pick the place of a company's order on the map (order tool).
    PickOrder { company: u32, pick: OrderPick },
    /// Finish the patrol route being picked.
    FinishPatrol,
    /// Open the diplomacy tab at a kingdom.
    OpenKingdom(KingdomId),
    /// Show the decision dialog of a diplomatic proposal.
    ShowProposal(u32),
    /// The player hid the proposal card („Später“): its proposals wait in the Diplomatie tab.
    ProposalPostponed,
    /// Open a panel of the right bar.
    OpenPanel(RightPanel),
}

/// What [`Hud::close_panel`] closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Closed {
    Nothing,
    /// The proposal card (its proposals are postponed).
    ProposalCard,
    /// A dialog, a panel or the build flyout.
    Other,
}

/// Everything the HUD reads in a frame.
pub struct HudInput<'a> {
    pub session: &'a Session,
    pub tools: &'a ToolController,
    pub selection: Option<Selection>,
    pub hover: Hover,
    /// Seconds the cursor has rested on the hovered tile.
    pub hover_rest: f32,
    pub speed: GameSpeed,
    /// World rectangle the camera shows (minimap frame, labels on the map).
    pub view: Rect,
    /// Fraction of the next tick the map shows walkers at (labels above them follow it).
    pub alpha: f32,
    pub actions: &'a ActionMap<Action>,
    /// Real time in seconds (fades, toasts).
    pub now: f64,
    /// Pointer position on the map in points (world tooltip).
    pub pointer: Option<egui::Pos2>,
    /// Description of the quicksave file, if one exists.
    pub quicksave: Option<&'a str>,
    /// How much progress is unsaved (pause menu).
    pub save_status: Option<&'a str>,
    /// The citizen the camera follows.
    pub following: Option<CitizenId>,
    /// The tile grid is shown.
    pub show_grid: bool,
    /// The territory border is shown.
    pub show_territory: bool,
}

impl HudInput<'_> {
    /// Key hint for an action, e.g. "B".
    pub fn hint(&self, action: Action) -> String {
        self.actions.bindings(action).first().map(crate::actions::binding_label).unwrap_or_default()
    }
}

/// The HUD state.
pub struct Hud {
    icons: Icons,
    pub minimap: Minimap,
    build: build_menu::BuildMenu,
    bottom: bottom_bar::BottomBar,
    right: right_bar::RightBar,
    events: event_dialog::EventDialog,
    /// The pending event, refreshed with the other reports.
    pending: management::Cached<Option<kingdoms_sim::report::PendingEventReport>>,
    proposal_dialog: proposal_dialog::ProposalDialog,
    /// Diplomatic proposals waiting for an answer.
    proposals: management::Cached<Vec<ProposalRow>>,
    /// Enemy groups approaching or on the map (edge markers).
    threats: management::Cached<Vec<ThreatRow>>,
    /// Name of the next rank and the progress towards it (Aufgaben button).
    rank: management::Cached<Option<(String, u8)>>,
    rank_dialog: tasks::RankDialog,
    feed: feed::Feed,
    top: top_bar::TopBar,
    info: info_panel::InfoPanel,
    menu_open: bool,
    /// The menu was opened since the last `show` (its Esc must not close it again).
    menu_fresh: bool,
    /// Pause menu entry selected with the keyboard.
    menu_focus: usize,
    /// A dialog of the world scene (save, load, settings, confirmation) is open: the map
    /// ignores input and the pause menu waits behind it.
    blocking: bool,
    confirm: Option<DemolishRequest>,
    management: management::Management,
    citizen: citizen_window::CitizenWindow,
}

impl Hud {
    pub fn new(icons: Icons, minimap: Minimap, session: &Session) -> Self {
        Self {
            icons,
            minimap,
            build: build_menu::BuildMenu::new(),
            bottom: bottom_bar::BottomBar::default(),
            right: right_bar::RightBar::default(),
            events: event_dialog::EventDialog::default(),
            pending: management::Cached::default(),
            proposal_dialog: proposal_dialog::ProposalDialog::default(),
            proposals: management::Cached::default(),
            threats: management::Cached::default(),
            rank: management::Cached::default(),
            rank_dialog: tasks::RankDialog::new(session),
            feed: feed::Feed::new(session),
            top: top_bar::TopBar::default(),
            info: info_panel::InfoPanel,
            menu_open: false,
            menu_fresh: false,
            menu_focus: 0,
            blocking: false,
            confirm: None,
            management: management::Management::default(),
            citizen: citizen_window::CitizenWindow::default(),
        }
    }

    /// Forgets session-specific state after a game was loaded.
    pub fn reset(&mut self, session: &Session) {
        let filter = self.feed.filter;
        self.feed = feed::Feed::new(session);
        self.feed.filter = filter;
        self.top = top_bar::TopBar::default();
        self.confirm = None;
        self.management.reset();
        self.citizen.set(None);
        self.right.reset();
        self.pending = management::Cached::default();
        self.events = event_dialog::EventDialog::default();
        self.proposal_dialog = proposal_dialog::ProposalDialog::default();
        self.proposals = management::Cached::default();
        self.threats = management::Cached::default();
        self.rank = management::Cached::default();
        self.rank_dialog = tasks::RankDialog::new(session);
    }

    /// Opens the diplomacy tab at kingdom `id`.
    pub fn open_kingdom(&mut self, id: KingdomId) {
        self.management.show_kingdom(id);
    }

    /// Shows the decision dialog of proposal `serial`.
    pub fn show_proposal(&mut self, serial: u32) {
        self.proposal_dialog.show_proposal(serial);
    }

    /// True while the management window shows the military tab (company orders on the map).
    pub fn military_open(&self) -> bool {
        self.management.open_tab() == Some(management::Tab::Military)
    }

    /// Opens the citizen window for `citizen` (`None` closes it).
    pub fn inspect_citizen(&mut self, citizen: Option<CitizenId>) {
        self.citizen.set(citizen);
    }

    /// The citizen shown in the citizen window.
    pub fn inspected_citizen(&self) -> Option<CitizenId> {
        self.citizen.citizen()
    }

    /// Closes the citizen window; returns true if it was open.
    pub fn close_citizen(&mut self) -> bool {
        let open = self.citizen.citizen().is_some();
        self.citizen.set(None);
        open
    }

    /// Opens the management window at `tab` or closes it if `tab` is shown; returns true if
    /// the window is open afterwards.
    pub fn toggle_window(&mut self, tab: management::Tab) -> bool {
        self.management.toggle(tab)
    }

    /// Opens the management window at `tab` (keeps it open if `tab` is shown).
    pub fn open_window(&mut self, tab: management::Tab) {
        if self.management.open_tab() != Some(tab) {
            self.management.toggle(tab);
        }
    }

    /// Closes the management window; returns true if it was open.
    pub fn close_window(&mut self) -> bool {
        self.management.close()
    }

    /// True while a modal dialog (pause menu, confirmation) is open; the map then ignores keys.
    pub fn has_modal(&self) -> bool {
        self.menu_open || self.confirm.is_some() || self.blocking || self.management.dialog_open()
    }

    /// A dialog of the world scene is open (see [`Hud::has_modal`]).
    pub fn set_blocking(&mut self, blocking: bool) {
        self.blocking = blocking;
    }

    pub fn menu_open(&self) -> bool {
        self.menu_open
    }

    /// True while the pause menu is the only modal dialog (its quicksave and quickload keys
    /// work then, as its buttons say).
    pub fn only_menu_open(&self) -> bool {
        self.menu_open && self.confirm.is_none() && !self.blocking && !self.management.dialog_open()
    }

    /// Opens or closes the pause menu.
    pub fn set_menu_open(&mut self, open: bool) {
        self.menu_fresh = open && !self.menu_open;
        if self.menu_fresh {
            self.menu_focus = 0;
        }
        self.menu_open = open;
    }

    /// Asks the player to confirm a demolition.
    pub fn ask_demolish(&mut self, request: DemolishRequest) {
        self.confirm = Some(request);
    }

    /// Toggles the build flyout of the last category (`B`).
    pub fn toggle_build_menu(&mut self) -> bool {
        self.build.toggle_last()
    }

    /// Opens a build category (developer option `KINGDOMS_PANEL`).
    pub fn open_build(&mut self, tab: BuildTab) {
        if !self.build.toggle(tab) {
            self.build.toggle(tab);
        }
    }

    /// Opens a panel of the right bar (developer option `KINGDOMS_PANEL`).
    pub fn open_panel(&mut self, panel: RightPanel) {
        if self.right.open_panel() != Some(panel) {
            self.right.toggle(panel);
        }
    }

    /// Closes the event dialog, the proposal card (like „Später“), a panel of the right bar or
    /// the build flyout (innermost first; Esc); says what was closed.
    pub fn close_panel(&mut self) -> Closed {
        if self.rank_dialog.is_open() {
            self.rank_dialog.close();
            return Closed::Other;
        }
        if self.events.is_open() {
            self.events.set_open(false);
            return Closed::Other;
        }
        if self.proposal_dialog.is_open() {
            self.proposal_dialog.close();
            return Closed::ProposalCard;
        }
        if self.right.close() || self.build.close() { Closed::Other } else { Closed::Nothing }
    }

    /// True while the proposal card shows a proposal (also while it waits behind an event
    /// decision or the rank celebration).
    pub fn proposal_card_open(&self) -> bool {
        self.proposal_dialog.is_open()
    }

    /// `E`: shows or hides the decision dialog when an event is pending, otherwise the events
    /// panel; returns true if something opened.
    pub fn toggle_events(&mut self, input_pending: bool) -> bool {
        if input_pending {
            let open = !self.events.is_open();
            self.events.set_open(open);
            open
        } else {
            self.right.toggle(RightPanel::Events)
        }
    }

    /// True while an event waits for a decision (as of the last frame).
    pub fn event_pending(&self) -> bool {
        self.pending.peek().is_some_and(Option::is_some)
    }

    /// Shows or hides the decision dialog of the pending event.
    pub fn set_event_dialog(&mut self, open: bool) {
        self.events.set_open(open);
    }

    /// Which notifications the feed shows (settings).
    pub fn set_notification_filter(&mut self, filter: crate::settings::NotificationFilter) {
        self.feed.filter = filter;
    }

    /// The notification list, newest first, to keep with a save.
    pub fn notification_history(&self) -> Vec<crate::save::slots::SavedNotification> {
        self.feed.history_for_save()
    }

    /// Shows a short message (command errors, saves).
    pub fn toast(&mut self, text: impl Into<String>, now: f64, error: bool) {
        self.feed.toast(text.into(), now, error);
    }

    /// Draws the HUD and collects the player's actions. Order: bars first (their rectangles
    /// place the panels), then the panels on the right from the bar inwards – info panel,
    /// panel of the right bar, notification cards – then windows and dialogs.
    pub fn show(&mut self, ctx: &egui::Context, input: &HudInput<'_>) -> Vec<HudAction> {
        let mut actions = Vec::new();
        let screen = ctx.content_rect();
        self.feed.read_events(input.session, input.now);
        let (state, defs) = (input.session.state(), input.session.defs());
        let pending = self.pending.get(input, || kingdoms_sim::report::pending_event(state, defs)).clone();
        self.events.update(pending.as_ref());
        let proposals = self.proposals.get(input, || kingdoms_sim::report::pending_proposals(state, defs)).clone();
        self.proposal_dialog.update(&proposals, self.events.is_open() || self.rank_dialog.is_open());
        let threats = self.threats.get(input, || kingdoms_sim::report::threats_report(state, defs)).clone();
        let rank = self
            .rank
            .get(input, || {
                let r = kingdoms_sim::report::rank_report(state, defs);
                r.next.map(|n| (n.name, n.percent))
            })
            .clone();
        self.rank_dialog.read_events(input.session, input.now);
        self.top.show(ctx, input, &self.icons, self.menu_open, self.management.open_tab(), &mut actions);
        let bottom_bar = self.bottom.show(ctx, input, &self.icons, &mut actions);
        let research_open = self.management.open_tab() == Some(management::Tab::Research);
        let left_bar = self.build.show(ctx, input, &self.icons, research_open, &mut actions);
        let minimap = self.minimap.show(ctx, input, &mut actions);
        let badges = right_bar::Badges {
            pending: pending.as_ref(),
            dialog_open: self.events.is_open(),
            unread: self.feed.unread(),
            proposals: &proposals,
            rank,
        };
        let strip = self.right.show_bar(ctx, input, &self.icons, &badges, &mut actions);
        // Panels on the right, from the bar inwards; each stops above whatever lies below it.
        let bottom_for = |left: f32, right: f32| {
            let mut bottom = screen.bottom() - MARGIN;
            for rect in [minimap, bottom_bar] {
                if rect.is_positive() && rect.left() < right && rect.right() > left {
                    bottom = bottom.min(rect.top() - MARGIN);
                }
            }
            bottom
        };
        let mut right = strip.left() - PANEL_GAP;
        let citizen_right = right;
        let mut confirm = None;
        if let Some(info) = self.info.show(ctx, input, &self.icons, right, &mut actions, &mut confirm) {
            right = info.left() - PANEL_GAP;
        }
        if confirm.is_some() {
            self.confirm = confirm;
        }
        let place = right_bar::PanelPlace { right, bottom: bottom_for(right - right_bar::PANEL_WIDTH, right) };
        if let Some(panel) = self.right.show_panel(ctx, input, &self.icons, &mut self.feed, place, &mut actions) {
            right = panel.left() - PANEL_GAP;
        }
        // The card of the diplomatic proposals heads the column of the notification cards;
        // event decisions and the rank celebration keep priority (it waits behind them).
        let mut column_top = TOP_BAR_HEIGHT + MARGIN;
        let mut column_left = right;
        if !self.rank_dialog.is_open()
            && !self.events.is_open()
            && let Some(card) = self.proposal_dialog.show(ctx, input, &self.icons, &proposals, right, column_top, &mut actions)
        {
            column_top = card.bottom() + PANEL_GAP;
            column_left = column_left.min(card.left() - PANEL_GAP);
        }
        let cards = (self.right.open_panel() != Some(RightPanel::Notifications)).then_some(right);
        let cards = self.feed.show(ctx, input, cards, column_top, bottom_for(right - feed::FEED_WIDTH, right), &mut actions);
        if let Some(cards) = cards {
            column_left = column_left.min(cards.left() - PANEL_GAP);
        }
        // The map between the left bar and the cards (or panels) on the right: the order
        // banner and the threat markers stay inside it, clear of the cards.
        let free = war_markers::free_area(screen, left_bar, column_left, bottom_bar);
        let banner = war_markers::order_banner(ctx, input, free, &mut actions);
        war_markers::show_threats(ctx, input, &threats, free, banner, &mut actions);
        self.management.show(ctx, input, &self.icons, &mut actions);
        // The citizen window stays above the bottom bar (speed and clock).
        let citizen_bottom = if bottom_bar.is_positive() { bottom_bar.top() - MARGIN } else { screen.bottom() - MARGIN };
        self.citizen.show(ctx, input, &self.icons, citizen_right, citizen_bottom, &mut actions);
        // The celebration of a new rank goes first; decisions wait behind it.
        if self.rank_dialog.is_open() {
            self.rank_dialog.show(ctx, input, &self.icons, &mut actions);
        } else {
            self.events.show(ctx, input, &self.icons, pending.as_ref(), &mut actions);
        }
        tooltip::show(ctx, input, &self.icons);
        if let Some(request) = self.confirm.clone()
            && !dialogs::confirm_demolish(ctx, input, &self.icons, &request, &mut actions)
        {
            self.confirm = None;
        }
        if self.menu_open && !self.blocking {
            let still_open = match state.game_over() {
                Some(over) => dialogs::defeat_screen(ctx, input, over, self.menu_fresh, &mut self.menu_focus, &mut actions),
                None => dialogs::pause_menu(ctx, input, self.menu_fresh, &mut self.menu_focus, &mut actions),
            };
            if !still_open {
                self.menu_open = false;
                actions.push(HudAction::Menu(false));
            }
        }
        self.menu_fresh = false;
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A side panel opened after a decision dialog lies on top of it: the dialog notices and
    /// moves back above (see `event_dialog`, `proposal_dialog`).
    #[test]
    fn a_docked_panel_above_a_dialog_is_noticed() {
        let ctx = egui::Context::default();
        let frame = |panel: bool| {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let shown = egui::Window::new("Ereignis").id(egui::Id::unique("hud_event_dialog")).show(ui.ctx(), |ui| {
                    ui.label("Brand");
                });
                if panel {
                    egui::Area::new(egui::Id::unique("hud_right_panel")).show(ui.ctx(), |ui| {
                        ui.label("Aufgaben");
                    });
                }
                if let Some(shown) = shown
                    && docked_area_on_top(ui.ctx())
                {
                    ui.ctx().move_to_top(shown.response.layer_id);
                }
            });
            // No renderer in this test: the font texture is not uploaded anywhere.
            output.textures_delta.clear();
        };
        frame(false);
        assert!(!docked_area_on_top(&ctx));
        frame(true);
        assert!(docked_area_on_top(&ctx), "the panel opened later is on top");
        frame(true);
        frame(true);
        assert!(!docked_area_on_top(&ctx), "the dialog moved back above the panel");
    }
}
