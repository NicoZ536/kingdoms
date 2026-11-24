//! „Forschung“: the tech tree by area with prerequisite connectors, the state of every
//! technology (erforscht, in Arbeit, vorgemerkt, verfügbar, gesperrt with the missing
//! prerequisites), the research queue (reorder, remove), the research rate with its sources and
//! the details of the chosen technology.
//!
//! Every change is a `QueueResearchWithPrerequisites` (a technology together with its missing
//! prerequisites, all or nothing), `CancelResearch` or `MoveResearch` command; the simulation
//! validates it, and its German rejection appears as a toast. Only the arrangement of the tree
//! is computed here ([`layout_tree`]); states, costs, times and effect texts come from
//! [`report::research_report`], the order in which a locked technology would be queued from
//! [`research::queue_plan`] – the plan the command carries out.

use std::collections::{BTreeMap, BTreeSet};

use kingdoms_engine::egui::{self, Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};
use kingdoms_sim::buildings::BuildingId;
use kingdoms_sim::command::Command;
use kingdoms_sim::defs::{BuildingRole, TechArea, TechId};
use kingdoms_sim::report::{self, ResearchReport, TechRow, TechState};
use kingdoms_sim::research::{self, ResearchStatus};
use kingdoms_sim::time::GameClock;

use super::{Cached, TabContext, card_row, command, hint, select};
use crate::ui::chart::{Chart, Series, ValueFormat};
use crate::ui::hud::HudAction;
use crate::ui::theme::{self, Palette, heading_font};
use crate::ui::widgets::{Glyph, Mark, glyph_button, mark, mini_bar, section_header};

/// Size of a node of the tree.
const NODE: Vec2 = Vec2::new(132.0, 58.0);
/// Horizontal gap between two columns (room for the connectors).
const COLUMN_GAP: f32 = 22.0;
/// Vertical gap between two nodes of a lane.
const ROW_GAP: f32 = 8.0;
/// Height of the title line of a lane.
const LANE_HEAD: f32 = 22.0;
/// Space below the last row of a lane.
const LANE_PAD: f32 = 8.0;
/// Space left of the first column.
const TREE_PAD: f32 = 6.0;
/// Width of the column with queue and details next to the tree.
const SIDE_WIDTH: f32 = 292.0;

/// Where a technology sits in the tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodePlace {
    pub tech: TechId,
    pub area: TechArea,
    /// Column = length of the longest prerequisite chain before it (0 = no prerequisites), so
    /// every connector runs from left to right, also between areas.
    pub column: usize,
    /// Row inside the lane of its area.
    pub row: usize,
}

/// One lane (area) of the tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lane {
    pub area: TechArea,
    /// Rows of the lane (at least one).
    pub rows: usize,
}

/// Arrangement of the tech tree: one horizontal lane per area (in [`TechArea::ALL`] order,
/// only areas with technologies), columns by prerequisite depth.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TreeLayout {
    pub nodes: Vec<NodePlace>,
    pub lanes: Vec<Lane>,
    pub columns: usize,
}

/// Arranges technologies given as `(tech, area, prerequisites)` (in report order). Inside a
/// lane, a technology takes the row of its first prerequisite from the same area if that row is
/// free in its column (chains stay on one line), otherwise the nearest free row.
pub fn layout_tree(techs: &[(TechId, TechArea, Vec<TechId>)]) -> TreeLayout {
    let index: BTreeMap<TechId, usize> = techs.iter().enumerate().map(|(i, (t, _, _))| (*t, i)).collect();
    // Longest prerequisite chain by relaxation (a cycle in broken data cannot loop forever).
    let mut column = vec![0usize; techs.len()];
    for _ in 0..techs.len() {
        let mut changed = false;
        for (i, (_, _, prerequisites)) in techs.iter().enumerate() {
            let depth = prerequisites.iter().filter_map(|p| index.get(p)).map(|&j| column[j] + 1).max().unwrap_or(0);
            if depth != column[i] {
                column[i] = depth;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let columns = column.iter().max().map_or(0, |c| c + 1);
    let mut row = vec![usize::MAX; techs.len()];
    let mut lanes = Vec::new();
    for area in TechArea::ALL {
        let members: Vec<usize> = (0..techs.len()).filter(|&i| techs[i].1 == area).collect();
        if members.is_empty() {
            continue;
        }
        let mut rows = 1;
        for col in 0..columns {
            let mut taken: BTreeSet<usize> = BTreeSet::new();
            for &i in members.iter().filter(|&&i| column[i] == col) {
                // The row of the first prerequisite in this lane, else the top row.
                let preferred = techs[i]
                    .2
                    .iter()
                    .filter_map(|p| index.get(p))
                    .find(|&&j| techs[j].1 == area && row[j] != usize::MAX)
                    .map_or(0, |&j| row[j]);
                let free = (0..)
                    .flat_map(|d: usize| [preferred.checked_add(d), preferred.checked_sub(d)])
                    .flatten()
                    .find(|r| !taken.contains(r))
                    .unwrap_or(preferred);
                taken.insert(free);
                row[i] = free;
                rows = rows.max(free + 1);
            }
        }
        lanes.push(Lane { area, rows });
    }
    let nodes = techs.iter().enumerate().map(|(i, (tech, area, _))| NodePlace { tech: *tech, area: *area, column: column[i], row: row[i] }).collect();
    TreeLayout { nodes, lanes, columns }
}

impl TreeLayout {
    /// Top of each lane relative to the tree's origin, and the total height.
    fn lane_tops(&self) -> (Vec<f32>, f32) {
        let mut tops = Vec::with_capacity(self.lanes.len());
        let mut y = 0.0;
        for lane in &self.lanes {
            tops.push(y);
            y += LANE_HEAD + lane.rows as f32 * (NODE.y + ROW_GAP) + LANE_PAD;
        }
        (tops, y)
    }

    /// Size of the whole tree in points.
    pub fn size(&self) -> Vec2 {
        let width = TREE_PAD * 2.0 + self.columns as f32 * (NODE.x + COLUMN_GAP) - COLUMN_GAP;
        Vec2::new(width.max(NODE.x), self.lane_tops().1)
    }

    /// Rectangle of every node relative to the tree's origin, in node order.
    pub fn node_rects(&self) -> Vec<Rect> {
        let (tops, _) = self.lane_tops();
        self.nodes
            .iter()
            .map(|n| {
                let lane = self.lanes.iter().position(|l| l.area == n.area).unwrap_or(0);
                let top = tops.get(lane).copied().unwrap_or(0.0);
                let min = Pos2::new(
                    TREE_PAD + n.column as f32 * (NODE.x + COLUMN_GAP),
                    top + LANE_HEAD + n.row as f32 * (NODE.y + ROW_GAP),
                );
                Rect::from_min_size(min, NODE)
            })
            .collect()
    }
}

/// A research building and its contribution.
#[derive(Clone, Debug)]
struct SourceRow {
    building: BuildingId,
    name: String,
    current_milli: u64,
    daily_milli: u64,
}

/// Everything the tab shows, refreshed with the report.
struct Data {
    report: ResearchReport,
    layout: TreeLayout,
    rects: Vec<Rect>,
    /// Technologies that build on each technology.
    dependents: BTreeMap<TechId, Vec<TechId>>,
    sources: Vec<SourceRow>,
    headquarters: Option<String>,
    /// Day labels and points of the closed days.
    history: (Vec<String>, Vec<f64>),
    /// What queuing each technology would queue ([`research::queue_plan`]: missing
    /// prerequisites first; empty for researched and queued ones).
    plans: BTreeMap<TechId, Vec<TechId>>,
}

impl Data {
    fn compute(context: &TabContext<'_, '_>) -> Self {
        let state = context.input.session.state();
        let defs = context.input.session.defs();
        let report = report::research_report(state, defs);
        let triples: Vec<(TechId, TechArea, Vec<TechId>)> =
            report.techs.iter().map(|t| (t.tech, t.area, t.prerequisites.iter().map(|p| p.tech).collect())).collect();
        let layout = layout_tree(&triples);
        let rects = layout.node_rects();
        let mut dependents: BTreeMap<TechId, Vec<TechId>> = BTreeMap::new();
        for t in &report.techs {
            for p in &t.prerequisites {
                dependents.entry(p.tech).or_default().push(t.tech);
            }
        }
        let sources = report
            .rate
            .sources
            .iter()
            .map(|s| SourceRow {
                building: s.building,
                name: building_name(context, s.building),
                current_milli: s.current_milli,
                daily_milli: s.daily_milli,
            })
            .collect();
        let headquarters = state
            .buildings()
            .iter()
            .find(|(_, b)| defs.buildings().get(b.kind().index()).is_some_and(|d| d.role == BuildingRole::Headquarters))
            .and_then(|(_, b)| defs.buildings().get(b.kind().index()))
            .map(|d| d.name.clone());
        let history = (
            report.history.iter().map(|d| super::day_label(d.day)).collect(),
            report.history.iter().map(|d| d.points_milli as f64 / 1000.0).collect(),
        );
        let plans = report.techs.iter().map(|t| (t.tech, research::queue_plan(state, defs, t.tech))).collect();
        Self { report, layout, rects, dependents, sources, headquarters, history, plans }
    }

    fn row(&self, tech: TechId) -> Option<&TechRow> {
        self.report.techs.iter().find(|t| t.tech == tech)
    }
}

fn building_name(context: &TabContext<'_, '_>, id: BuildingId) -> String {
    let state = context.input.session.state();
    let defs = context.input.session.defs();
    state.building(id).and_then(|b| defs.buildings().get(b.kind().index())).map_or_else(|| "Gebäude".to_owned(), |d| d.name.clone())
}

/// German number with one decimal from milli-units, e.g. „1,5“.
fn points(milli: u64) -> String {
    let tenths = (milli + 50) / 100;
    format!("{},{}", tenths / 10, tenths % 10)
}

/// Colour of a technology state (stripe, chip).
fn state_color(state: TechState) -> Color32 {
    match state {
        TechState::Researched => Palette::GOOD,
        TechState::InProgress => Palette::GOLD_DARK,
        TechState::Queued => Palette::GOLD,
        TechState::Available => Palette::INFO,
        TechState::Locked => Palette::PARCHMENT_SHADE,
    }
}

/// Icon of an area.
pub fn area_icon(area: TechArea) -> &'static str {
    match area {
        TechArea::Agriculture => "ui_farm",
        TechArea::Architecture => "ui_build",
        TechArea::Crafts => "ui_production",
        TechArea::Mining => "res_iron_ore",
        TechArea::Trade => "ui_trade",
        TechArea::Medicine => "ui_medicine",
        TechArea::Military => "ui_military",
        TechArea::Administration => "ui_administration",
    }
}

/// Second line of a node: what matters in its state.
fn node_note(row: &TechRow) -> String {
    match row.state {
        TechState::Researched => "Erforscht".to_owned(),
        TechState::InProgress => format!("In Arbeit · {} %", row.progress_percent),
        TechState::Queued => match row.eta_hours {
            Some(h) => format!("Vorgemerkt · {}", short_duration(h)),
            None => "Vorgemerkt".to_owned(),
        },
        TechState::Available | TechState::Locked => format!("{} Gold · {} Punkte", row.cost_gold, row.points),
    }
}

/// Short duration for the nodes, e.g. „14 h“ or „3 Tage“ (the game's shared form).
pub fn short_duration(hours: u64) -> String {
    crate::ui::hud::format::duration_hours(hours)
}

/// What the player may do with a technology right now.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Offer {
    /// Queue it (and its missing prerequisites first).
    Queue(Vec<TechId>),
    /// It is queued: move it to the front or remove it.
    Queued { position: u8 },
    /// Nothing to do (researched), or why it cannot be queued.
    None(Option<String>),
}

fn offer(data: &Data, row: &TechRow) -> Offer {
    match row.state {
        TechState::Researched => Offer::None(None),
        TechState::InProgress | TechState::Queued => Offer::Queued { position: row.queue_position.unwrap_or(0) },
        TechState::Available | TechState::Locked => {
            let plan = data.plans.get(&row.tech).cloned().unwrap_or_default();
            let room = usize::from(data.report.max_queue).saturating_sub(data.report.queue.len());
            if plan.len() > room {
                Offer::None(Some(format!(
                    "Die Forschungsliste fasst höchstens {} Technologien{}.",
                    data.report.max_queue,
                    if plan.len() > 1 { format!(" – mit Voraussetzungen wären es {} mehr", plan.len()) } else { String::new() }
                )))
            } else {
                Offer::Queue(plan)
            }
        }
    }
}

/// The research tab.
#[derive(Default)]
pub struct ResearchTab {
    data: Cached<Data>,
    selected: Option<TechId>,
    /// Node under the pointer in the last frame (highlights its connectors).
    hovered: Option<TechId>,
}

impl ResearchTab {
    pub fn show(&mut self, ui: &mut Ui, context: &TabContext<'_, '_>, actions: &mut Vec<HudAction>) {
        let data = self.data.get(context.input, || Data::compute(context));
        // Technologies the player queued while the game is paused (applied with the next tick).
        let pending: BTreeSet<TechId> = context
            .input
            .session
            .queued_commands()
            .iter()
            .filter_map(|c| match c {
                Command::QueueResearch { tech }
                | Command::QueueResearchWithPrerequisites { tech }
                | Command::CancelResearch { tech }
                | Command::MoveResearch { tech, .. } => Some(*tech),
                _ => None,
            })
            .collect();
        if self.selected.is_none() {
            self.selected = data.report.queue.first().map(|q| q.tech).or_else(|| {
                data.report.techs.iter().find(|t| t.state == TechState::Available).map(|t| t.tech)
            });
        }
        summary(ui, data);
        ui.add_space(4.0);
        let height = (ui.clip_rect().bottom() - ui.cursor().top() - 6.0).max(280.0);
        let mut select_tech = None;
        let mut hovered = None;
        ui.horizontal_top(|ui| {
            let tree_width = (ui.available_width() - SIDE_WIDTH - 10.0).max(300.0);
            let column = egui::Layout::top_down(egui::Align::Min);
            ui.allocate_ui_with_layout(Vec2::new(tree_width, height), column, |ui| {
                theme::inset_frame().inner_margin(egui::Margin::same(2)).show(ui, |ui| {
                    egui::ScrollArea::both()
                        .id_salt("research_tree")
                        .max_width(tree_width - 6.0)
                        .max_height(height - 6.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let (clicked, hover) = tree(ui, context, data, self.selected, self.hovered, &pending, actions);
                            select_tech = clicked;
                            hovered = hover;
                        });
                });
            });
            ui.allocate_ui_with_layout(Vec2::new(SIDE_WIDTH, height), column, |ui| {
                egui::ScrollArea::vertical().id_salt("research_side").max_height(height).auto_shrink([false, false]).show(ui, |ui| {
                    ui.set_width(SIDE_WIDTH - 12.0);
                    if let Some(t) = side_panel(ui, context, data, self.selected, &pending, actions) {
                        select_tech = Some(t);
                    }
                });
            });
        });
        if select_tech.is_some() {
            self.selected = select_tech;
        }
        self.hovered = hovered;
    }
}

/// Cards and the status line above the tree.
fn summary(ui: &mut Ui, data: &Data) {
    let r = &data.report;
    let (current, progress) = match r.status {
        ResearchStatus::Researching(t) | ResearchStatus::WaitingForGold { tech: t, .. } => {
            let row = data.row(t);
            let name = row.map_or_else(String::new, |t| t.name.clone());
            let progress = match (r.status, row) {
                (ResearchStatus::WaitingForGold { missing, .. }, _) => format!("wartet auf {missing} Gold"),
                (_, Some(row)) => match row.eta_hours {
                    Some(h) => format!("{} % · noch {}", row.progress_percent, short_duration(h)),
                    None => format!("{} % · steht still", row.progress_percent),
                },
                _ => String::new(),
            };
            (name, progress)
        }
        ResearchStatus::Idle => ("Keine".to_owned(), "Punkte verfallen".to_owned()),
    };
    let idle = matches!(r.status, ResearchStatus::Idle);
    card_row(
        ui,
        &[
            ("Aktuelle Forschung", current, if idle { Palette::WARN } else { Palette::INK }),
            ("Fortschritt", progress, if idle { Palette::WARN } else { Palette::INK }),
            ("Forschungsleistung", format!("{} Punkte/h", points(r.rate.current_milli)), Palette::INK),
            ("Erwartet pro Tag", format!("{} Punkte", points(r.rate.daily_milli)), Palette::INK),
            ("Erforscht", format!("{} von {}", r.researched, r.total), Palette::GOOD),
        ],
    );
    let color = match r.status {
        ResearchStatus::Idle => Palette::WARN,
        ResearchStatus::WaitingForGold { .. } => Palette::BAD,
        ResearchStatus::Researching(_) => Palette::INK,
    };
    ui.add_space(2.0);
    ui.label(RichText::new(&r.status_text).color(color));
    ui.label(RichText::new(&r.rate_text).small().color(Palette::INK_SOFT));
}

/// Draws the tree; returns the clicked and the hovered technology.
fn tree(
    ui: &mut Ui,
    context: &TabContext<'_, '_>,
    data: &Data,
    selected: Option<TechId>,
    hovered: Option<TechId>,
    pending: &BTreeSet<TechId>,
    actions: &mut Vec<HudAction>,
) -> (Option<TechId>, Option<TechId>) {
    let size = data.layout.size();
    let (area, _) = ui.allocate_exact_size(Vec2::new(size.x.max(ui.available_width()), size.y), Sense::hover());
    let origin = area.min.to_vec2();
    let painter = ui.painter_at(ui.clip_rect());
    // Lanes: alternating bands with the area's icon, name and count.
    let (tops, _) = data.layout.lane_tops();
    for (i, (lane, top)) in data.layout.lanes.iter().zip(&tops).enumerate() {
        let height = LANE_HEAD + lane.rows as f32 * (NODE.y + ROW_GAP) + LANE_PAD;
        let band = Rect::from_min_size(Pos2::new(area.left(), area.top() + top), Vec2::new(area.width(), height));
        let fill = if i % 2 == 0 { Palette::PARCHMENT_LIGHT } else { Palette::PARCHMENT.gamma_multiply(0.55) };
        painter.rect_filled(band, CornerRadius::ZERO, fill);
        painter.line_segment([band.left_top(), band.right_top()], Stroke::new(1.0, Palette::PARCHMENT_SHADE));
        let icon_rect = Rect::from_min_size(band.min + Vec2::new(TREE_PAD, 3.0), Vec2::splat(16.0));
        if let Some(icon) = context.icons.get(area_icon(lane.area)) {
            icon.paint_fit(&painter, icon_rect, Color32::WHITE);
        }
        let in_lane: Vec<&TechRow> = data.report.techs.iter().filter(|t| t.area == lane.area).collect();
        let done = in_lane.iter().filter(|t| t.state == TechState::Researched).count();
        let name = painter.text(
            Pos2::new(icon_rect.right() + 6.0, icon_rect.center().y),
            egui::Align2::LEFT_CENTER,
            lane.area.display_name(),
            heading_font(13.0),
            Palette::INK,
        );
        painter.text(
            Pos2::new(name.right() + 10.0, icon_rect.center().y),
            egui::Align2::LEFT_CENTER,
            format!("{done} von {} erforscht", in_lane.len()),
            egui::FontId::proportional(12.0),
            Palette::INK_SOFT,
        );
    }
    // Connectors below the nodes; those of the hovered or selected node in gold.
    let focus = hovered.or(selected);
    let rect_of = |tech: TechId| data.layout.nodes.iter().position(|n| n.tech == tech).and_then(|i| data.rects.get(i)).map(|r| r.translate(origin));
    for t in &data.report.techs {
        let Some(to) = rect_of(t.tech) else { continue };
        for p in &t.prerequisites {
            let Some(from) = rect_of(p.tech) else { continue };
            let lit = focus == Some(t.tech) || focus == Some(p.tech);
            let color = if lit {
                Palette::GOLD_DARK
            } else if p.researched {
                Palette::GOOD.gamma_multiply(0.7)
            } else {
                Palette::INK_SOFT.gamma_multiply(0.45)
            };
            connector(&painter, from.right_center(), to.left_center(), Stroke::new(if lit { 2.4 } else { 1.4 }, color));
        }
    }
    let mut clicked = None;
    let mut hover = None;
    for (node, rect) in data.layout.nodes.iter().zip(&data.rects) {
        let Some(row) = data.row(node.tech) else { continue };
        let rect = rect.translate(origin);
        let response = ui.interact(rect, ui.make_persistent_id(("research_tech", node.tech)), Sense::click());
        if response.hovered() {
            hover = Some(node.tech);
        }
        paint_node(ui, row, rect, selected == Some(node.tech), response.hovered(), pending.contains(&node.tech));
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_ui(|ui| node_tooltip(ui, row));
        if response.double_clicked() {
            if let Offer::Queue(_) = offer(data, row)
                && !pending.contains(&row.tech)
            {
                command(actions, Command::QueueResearchWithPrerequisites { tech: row.tech });
            }
            clicked = Some(node.tech);
        } else if response.clicked() {
            clicked = Some(node.tech);
            actions.push(HudAction::Sound("ui_select"));
        }
    }
    (clicked, hover)
}

/// Curved connector with an arrow head at `to`.
fn connector(painter: &egui::Painter, from: Pos2, to: Pos2, stroke: Stroke) {
    let dx = ((to.x - from.x) * 0.5).max(14.0);
    let points = [from, from + Vec2::new(dx, 0.0), to - Vec2::new(dx, 0.0), to - Vec2::new(5.0, 0.0)];
    painter.add(egui::epaint::CubicBezierShape::from_points_stroke(points, false, Color32::TRANSPARENT, stroke));
    let head = vec![to, to + Vec2::new(-6.0, -3.5), to + Vec2::new(-6.0, 3.5)];
    painter.add(egui::Shape::convex_polygon(head, stroke.color, Stroke::NONE));
}

#[allow(clippy::too_many_arguments)]
fn paint_node(ui: &Ui, row: &TechRow, rect: Rect, selected: bool, hovered: bool, pending: bool) {
    let painter = ui.painter();
    let fill = match row.state {
        TechState::Researched => Color32::from_rgb(214, 226, 176),
        TechState::InProgress => Color32::from_rgb(246, 222, 150),
        TechState::Queued => Color32::from_rgb(246, 232, 190),
        TechState::Available => Palette::PARCHMENT_LIGHT,
        TechState::Locked => Palette::PARCHMENT_DARK,
    };
    let radius = CornerRadius::same(5);
    painter.rect_filled(rect, radius, fill);
    // State stripe on the left edge.
    let stripe = Rect::from_min_max(rect.min, Pos2::new(rect.left() + 5.0, rect.bottom()));
    painter.rect_filled(stripe, CornerRadius { nw: 5, sw: 5, ne: 0, se: 0 }, state_color(row.state));
    // Progress along the bottom edge (also kept progress of a removed technology).
    if row.state != TechState::Researched && row.progress_percent > 0 {
        let width = (rect.width() - 8.0) * f32::from(row.progress_percent) / 100.0;
        let bar = Rect::from_min_size(Pos2::new(rect.left() + 6.0, rect.bottom() - 5.0), Vec2::new(width, 3.0));
        painter.rect_filled(bar, CornerRadius::same(1), Palette::GOLD_DARK);
    }
    let stroke = if selected {
        Stroke::new(2.5, Palette::GOLD_DARK)
    } else if hovered {
        Stroke::new(1.8, Palette::GOLD)
    } else {
        Stroke::new(1.0, Palette::WOOD.gamma_multiply(0.7))
    };
    painter.rect_stroke(rect, radius, stroke, egui::StrokeKind::Inside);
    // Badge at the top right: queue position.
    let mut badge_right = rect.right() - 4.0;
    if let Some(position) = row.queue_position {
        let c = Pos2::new(badge_right - 8.0, rect.top() + 10.0);
        painter.circle_filled(c, 8.0, if position == 0 { Palette::GOLD_DARK } else { Palette::GOLD });
        painter.text(c, egui::Align2::CENTER_CENTER, (position + 1).to_string(), egui::FontId::proportional(11.0), Palette::PARCHMENT_LIGHT);
        badge_right -= 18.0;
    }
    let text_color = if row.state == TechState::Locked { Palette::INK_SOFT } else { Palette::INK };
    let name_width = badge_right - rect.left() - 11.0;
    // At most two lines of name above the note line.
    let mut job = egui::text::LayoutJob::simple(row.name.clone(), egui::FontId::proportional(13.0), text_color, name_width);
    job.wrap.max_rows = 2;
    let galley = painter.layout_job(job);
    painter.galley(Pos2::new(rect.left() + 9.0, rect.top() + 3.0), galley, text_color);
    let note = if pending { "Änderung wartet …".to_owned() } else { node_note(row) };
    painter.text(
        Pos2::new(rect.left() + 9.0, rect.bottom() - 7.0),
        egui::Align2::LEFT_BOTTOM,
        note,
        egui::FontId::proportional(11.0),
        if row.state == TechState::Locked { Palette::INK_SOFT } else { state_color(row.state).gamma_multiply(1.0) },
    );
}

fn node_tooltip(ui: &mut Ui, row: &TechRow) {
    ui.set_max_width(300.0);
    ui.label(RichText::new(&row.name).font(heading_font(16.0)).color(Palette::INK));
    ui.label(RichText::new(format!("{} · {}", row.area_name, row.state.display_name())).small().color(Palette::INK_SOFT));
    ui.label(RichText::new(&row.description).italics().color(Palette::INK_SOFT));
    effects_list(ui, row);
    ui.label(format!("Kosten: {} Gold{} · {} Forschungspunkte", row.cost_gold, if row.paid { " (bezahlt)" } else { "" }, row.points));
    if !row.missing.is_empty() {
        ui.colored_label(Palette::BAD, format!("Fehlt: {}", row.missing.join(", ")));
    }
    let hint_text = match row.state {
        TechState::Available => "Doppelklick: einreihen",
        TechState::Locked => "Doppelklick: mit den fehlenden Voraussetzungen einreihen",
        _ => "Klick: Einzelheiten rechts",
    };
    ui.label(RichText::new(hint_text).small().color(Palette::INK_SOFT));
}

/// The effect lines of the simulation.
fn effects_list(ui: &mut Ui, row: &TechRow) {
    for effect in &row.effects {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            mark(ui, Mark::Next);
            ui.add(egui::Label::new(RichText::new(effect).color(Palette::INK)).wrap());
        });
    }
}

/// Queue, sources and the chosen technology; returns a technology to select.
fn side_panel(
    ui: &mut Ui,
    context: &TabContext<'_, '_>,
    data: &Data,
    selected: Option<TechId>,
    pending: &BTreeSet<TechId>,
    actions: &mut Vec<HudAction>,
) -> Option<TechId> {
    let mut select_tech = None;
    let r = &data.report;
    if let Some(row) = selected.and_then(|t| data.row(t)) {
        section_header(ui, "Auswahl");
        details(ui, context, data, row, pending.contains(&row.tech), actions, &mut select_tech);
    }
    section_header(ui, &format!("Forschungsliste ({} von {})", r.queue.len(), r.max_queue));
    if r.queue.is_empty() {
        hint(ui, "Noch nichts eingereiht. Wählt eine Technologie im Baum und reiht sie ein (Doppelklick oder Knopf unten).");
    }
    let last = r.queue.len().saturating_sub(1);
    for (i, q) in r.queue.iter().enumerate() {
        let position = u8::try_from(i).unwrap_or(u8::MAX);
        let waiting = pending.contains(&q.tech);
        theme::inset_frame().inner_margin(egui::Margin::symmetric(6, 2)).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            // Buttons first from the right, the name truncated into the rest.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                {
                    if glyph_button(ui, Glyph::Cross, !waiting).on_hover_text("Aus der Liste nehmen (Fortschritt und bezahltes Gold bleiben)").clicked() {
                        command(actions, Command::CancelResearch { tech: q.tech });
                    }
                    if glyph_button(ui, Glyph::Down, !waiting && i < last).on_hover_text("Nach hinten").clicked() {
                        command(actions, Command::MoveResearch { tech: q.tech, position: position.saturating_add(1) });
                    }
                    if glyph_button(ui, Glyph::Up, !waiting && i > 0).on_hover_text("Nach vorn").clicked() {
                        command(actions, Command::MoveResearch { tech: q.tech, position: position.saturating_sub(1) });
                    }
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let text = RichText::new(format!("{}. {}", i + 1, q.name)).color(if i == 0 { Palette::INK } else { Palette::INK_SOFT }).strong();
                    if ui.add(egui::Label::new(text).truncate().sense(Sense::click())).clicked() {
                        select_tech = Some(q.tech);
                    }
                });
            });
            ui.horizontal(|ui| {
                mini_bar(ui, f32::from(q.progress_percent) / 100.0, Palette::GOLD_DARK, 90.0);
                let eta = match q.eta_hours {
                    Some(h) => format!("{} % · fertig in {}", q.progress_percent, report::duration_text(h)),
                    None => format!("{} % · ohne Forschungspunkte", q.progress_percent),
                };
                ui.label(RichText::new(if waiting { "Änderung wartet …".to_owned() } else { eta }).small().color(Palette::INK_SOFT));
            });
        });
    }
    section_header(ui, "Quellen");
    egui::Grid::new("research_sources").num_columns(2).spacing([10.0, 2.0]).show(ui, |ui| {
        let hq = data.headquarters.clone().unwrap_or_else(|| "Hauptquartier".to_owned());
        ui.label(RichText::new(hq).color(Palette::INK_SOFT));
        ui.label(format!("{} Punkte/h", points(r.rate.base_milli)));
        ui.end_row();
        for s in &data.sources {
            let link = ui.add(egui::Label::new(RichText::new(&s.name).color(Palette::INFO).underline()).sense(Sense::click()));
            if link.on_hover_text(format!("Gebäude zeigen · erwartet {} Punkte am Tag", points(s.daily_milli))).clicked() {
                select(actions, s.building);
            }
            ui.label(format!("{} Punkte/h", points(s.current_milli)));
            ui.end_row();
        }
        if r.rate.bonus_percent != 0 {
            ui.label(RichText::new("Forschungsbonus").color(Palette::INK_SOFT));
            ui.label(format!("{} %", crate::ui::hud::format::signed(i64::from(r.rate.bonus_percent))));
            ui.end_row();
        }
    });
    if data.sources.is_empty() {
        hint(ui, "Schulen mit Gelehrten forschen zusätzlich (je volle Schicht bis zu 48 Punkte am Tag).");
    }
    if !data.history.1.is_empty() {
        section_header(ui, "Punkte je Tag");
        let series = [Series::bar("Forschungspunkte", Palette::GOLD_DARK, data.history.1.clone())];
        Chart::new("research_history", &data.history.0, &series).height(110.0).format(ValueFormat::Decimal).show(ui);
    }
    select_tech
}

fn details(
    ui: &mut Ui,
    context: &TabContext<'_, '_>,
    data: &Data,
    row: &TechRow,
    waiting: bool,
    actions: &mut Vec<HudAction>,
    select_tech: &mut Option<TechId>,
) {
    ui.horizontal(|ui| {
        if let Some(icon) = context.icons.get(area_icon(row.area)) {
            icon.show(ui, 22.0);
        }
        ui.label(RichText::new(&row.name).font(heading_font(18.0)).color(Palette::INK));
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new(&row.area_name).small().color(Palette::INK_SOFT));
        ui.label(RichText::new(row.state.display_name()).small().strong().color(state_color(row.state)));
    });
    ui.add(egui::Label::new(RichText::new(&row.description).italics().color(Palette::INK_SOFT)).wrap());
    ui.add_space(2.0);
    effects_list(ui, row);
    ui.add_space(2.0);
    egui::Grid::new("research_detail").num_columns(2).spacing([10.0, 2.0]).show(ui, |ui| {
        let gold = if row.paid { format!("{} Gold (bezahlt)", row.cost_gold) } else { format!("{} Gold beim Beginn", row.cost_gold) };
        crate::ui::widgets::stat_row(ui, "Kosten", gold);
        crate::ui::widgets::stat_row(
            ui,
            "Forschungspunkte",
            format!("{} ({} %)", row.points, if row.state == TechState::Researched { 100 } else { row.progress_percent }),
        );
        match row.state {
            TechState::Researched => {
                let when = row.researched_tick.map_or_else(|| "zu Beginn".to_owned(), |t| GameClock::at_tick(t).date_string());
                crate::ui::widgets::stat_row(ui, "Erforscht", when);
            }
            _ => {
                if let Some(h) = row.eta_hours {
                    crate::ui::widgets::stat_row(ui, "Fertig in", report::duration_text(h));
                }
                let duration = row.duration_hours.map_or_else(|| "ohne Forschungspunkte unbestimmt".to_owned(), report::duration_text);
                crate::ui::widgets::stat_row(ui, "Dauer allein", duration);
            }
        }
    });
    if !row.prerequisites.is_empty() {
        ui.label(RichText::new("Voraussetzungen").small().color(Palette::INK_SOFT));
        for p in &row.prerequisites {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                mark(ui, if p.researched { Mark::Ok } else { Mark::Error });
                if ui.add(egui::Label::new(RichText::new(&p.name).color(Palette::INFO)).sense(Sense::click())).clicked() {
                    *select_tech = Some(p.tech);
                }
            });
        }
    }
    if let Some(next) = data.dependents.get(&row.tech).filter(|d| !d.is_empty()) {
        ui.label(RichText::new("Ermöglicht").small().color(Palette::INK_SOFT));
        ui.horizontal_wrapped(|ui| {
            for t in next {
                if let Some(d) = data.row(*t)
                    && ui.add(egui::Label::new(RichText::new(&d.name).color(Palette::INFO)).sense(Sense::click())).clicked()
                {
                    *select_tech = Some(*t);
                }
            }
        });
    }
    ui.add_space(4.0);
    if waiting {
        hint(ui, "Die Änderung wird übernommen, sobald das Spiel weiterläuft.");
        return;
    }
    match offer(data, row) {
        Offer::Queue(plan) => {
            let label = if plan.len() > 1 { format!("Mit {} Voraussetzungen einreihen", plan.len() - 1) } else { "Einreihen".to_owned() };
            let names: Vec<String> = plan.iter().filter_map(|t| data.row(*t)).map(|t| t.name.clone()).collect();
            if ui.button(label).on_hover_text(format!("Reihenfolge: {}", names.join(" → "))).clicked() {
                command(actions, Command::QueueResearchWithPrerequisites { tech: row.tech });
            }
        }
        Offer::Queued { position } => {
            ui.horizontal_wrapped(|ui| {
                if position > 0 && ui.button("Als Nächstes erforschen").clicked() {
                    command(actions, Command::MoveResearch { tech: row.tech, position: 0 });
                }
                if ui.button("Aus der Liste nehmen").clicked() {
                    command(actions, Command::CancelResearch { tech: row.tech });
                }
            });
            let after: Vec<&str> = data
                .dependents
                .get(&row.tech)
                .into_iter()
                .flatten()
                .filter_map(|t| data.row(*t))
                .filter(|t| t.queue_position.is_some())
                .map(|t| t.name.as_str())
                .collect();
            if !after.is_empty() {
                hint(ui, format!("Entfernen nimmt auch {} aus der Liste.", after.join(", ")));
            }
        }
        Offer::None(Some(reason)) => {
            ui.colored_label(Palette::BAD, reason);
        }
        Offer::None(None) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kingdoms_sim::state::{GameSettings, GameState};
    use kingdoms_sim::world::WorldSize;

    fn shipped() -> (kingdoms_sim::defs::Defs, ResearchReport) {
        let defs = super::super::tests::test_defs();
        let state = GameState::new(&defs, GameSettings::new(&defs, 7, WorldSize::Small));
        let report = report::research_report(&state, &defs);
        (defs, report)
    }

    fn triples(report: &ResearchReport) -> Vec<(TechId, TechArea, Vec<TechId>)> {
        report.techs.iter().map(|t| (t.tech, t.area, t.prerequisites.iter().map(|p| p.tech).collect())).collect()
    }

    #[test]
    fn the_tree_runs_left_to_right_without_overlaps() {
        let (defs, report) = shipped();
        let layout = layout_tree(&triples(&report));
        assert_eq!(layout.nodes.len(), defs.techs().len());
        let place = |t: TechId| layout.nodes.iter().find(|n| n.tech == t).copied();
        for t in &report.techs {
            let node = place(t.tech).expect("every technology is placed");
            assert_eq!(node.area, t.area);
            assert!(node.row < layout.lanes.iter().find(|l| l.area == t.area).map_or(0, |l| l.rows));
            for p in &t.prerequisites {
                let before = place(p.tech).expect("prerequisite placed");
                assert!(before.column < node.column, "{} must stand right of {}", t.name, p.name);
            }
        }
        // No two nodes share a cell; the rectangles do not overlap.
        let rects = layout.node_rects();
        for (i, a) in rects.iter().enumerate() {
            for b in rects.iter().skip(i + 1) {
                assert!(!a.intersects(*b), "{a:?} overlaps {b:?}");
            }
        }
        let size = layout.size();
        assert!(rects.iter().all(|r| r.max.x <= size.x + 0.01 && r.max.y <= size.y + 0.01));
        // One lane per area, in the canonical order.
        let areas: Vec<TechArea> = layout.lanes.iter().map(|l| l.area).collect();
        assert_eq!(areas, TechArea::ALL.to_vec());
    }

    #[test]
    fn chains_stay_on_one_row() {
        let (defs, report) = shipped();
        let layout = layout_tree(&triples(&report));
        let id = |key: &str| defs.tech_id(key).unwrap_or_else(|| panic!("tech {key}"));
        let place = |key: &str| layout.nodes.iter().find(|n| n.tech == id(key)).copied().unwrap_or_else(|| panic!("{key}"));
        // Pflüge → Fruchtwechsel → Bewässerung form one straight line.
        let plows = place("improved_plows");
        assert_eq!(place("crop_rotation").row, plows.row);
        assert_eq!(place("irrigation").row, plows.row);
        assert_eq!((place("crop_rotation").column, place("irrigation").column), (plows.column + 1, plows.column + 2));
    }

    #[test]
    fn small_trees_are_laid_out_by_depth() {
        let defs = super::super::tests::test_defs();
        let t = |i: usize| defs.techs()[i].id;
        let techs = vec![
            (t(0), TechArea::Agriculture, vec![]),
            (t(1), TechArea::Agriculture, vec![t(0)]),
            (t(2), TechArea::Agriculture, vec![t(0)]),
            (t(3), TechArea::Mining, vec![]),
            (t(4), TechArea::Mining, vec![t(3), t(2)]),
        ];
        let layout = layout_tree(&techs);
        let place: Vec<(usize, usize)> = layout.nodes.iter().map(|n| (n.column, n.row)).collect();
        assert_eq!(place, vec![(0, 0), (1, 0), (1, 1), (0, 0), (2, 0)]);
        assert_eq!(layout.columns, 3);
        assert_eq!(layout.lanes, vec![Lane { area: TechArea::Agriculture, rows: 2 }, Lane { area: TechArea::Mining, rows: 1 }]);
        // A prerequisite cycle in broken data still ends.
        let cyclic = vec![(t(0), TechArea::Trade, vec![t(1)]), (t(1), TechArea::Trade, vec![t(0)])];
        assert_eq!(layout_tree(&cyclic).nodes.len(), 2);
    }

    #[test]
    fn queueing_a_locked_technology_queues_the_simulations_plan_at_once() {
        let defs = super::super::tests::test_defs();
        let mut session = crate::session::Session::new(std::sync::Arc::new(defs.clone()), GameSettings::new(&defs, 7, WorldSize::Small));
        let id = |key: &str| defs.tech_id(key).unwrap_or_else(|| panic!("tech {key}"));
        // Bewässerung needs Fruchtwechsel ← Verbesserte Pflüge ← Getreideanbau; the tab offers
        // exactly the simulation's plan.
        let state = session.state();
        let report = report::research_report(state, &defs);
        let plans: BTreeMap<TechId, Vec<TechId>> =
            report.techs.iter().map(|t| (t.tech, research::queue_plan(state, &defs, t.tech))).collect();
        let data = Data {
            layout: layout_tree(&triples(&report)),
            rects: Vec::new(),
            dependents: BTreeMap::new(),
            sources: Vec::new(),
            headquarters: None,
            history: (Vec::new(), Vec::new()),
            plans,
            report,
        };
        let irrigation = data.row(id("irrigation")).expect("irrigation row");
        let expected = vec![id("grain_farming"), id("improved_plows"), id("crop_rotation"), id("irrigation")];
        assert!(matches!(offer(&data, irrigation), Offer::Queue(plan) if plan == expected));
        // One command queues the whole chain in that order.
        session.queue_command(Command::QueueResearchWithPrerequisites { tech: id("irrigation") });
        session.tick();
        let queued: Vec<TechId> = report::research_report(session.state(), &defs).queue.iter().map(|q| q.tech).collect();
        assert_eq!(queued, expected);
        // Queued technologies need no plan; the next one only itself.
        assert!(research::queue_plan(session.state(), &defs, id("crop_rotation")).is_empty());
        let after = research::queue_plan(session.state(), &defs, id("irrigation"));
        assert!(after.is_empty());
    }

    #[test]
    fn short_durations_read_naturally() {
        assert_eq!(short_duration(0), "1 h");
        assert_eq!(short_duration(14), "14 h");
        assert_eq!(short_duration(48), "2 Tage");
        assert_eq!(short_duration(49), "3 Tage");
        assert_eq!(points(1500), "1,5");
        assert_eq!(points(24_000), "24,0");
    }
}
