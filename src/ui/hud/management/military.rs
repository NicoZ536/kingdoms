//! „Militär“: the kingdom's soldiers and their enemies (docs/game_design.md §11). Soldiers by
//! unit type with health, morale, equipment and pay; recruitment at every military building with
//! the simulation's costs and reasons and the recruits in training; companies with their members,
//! order and values, created, filled, given orders and disbanded; threats with their arrival
//! time and a camera jump; the player's wars, campaigns and battle reports.
//!
//! Orders that need a place on the map (defend a building or an area, patrol a route) hand over
//! to the order tool ([`crate::tools::Tool::Order`]): the window closes, the player picks on the
//! map, the command goes out and the window comes back. Every other control queues its command
//! directly; rejections come back from the simulation as toasts.

use std::collections::{BTreeMap, BTreeSet};

use kingdoms_engine::egui::{self, Align, Layout, RichText, Ui};
use kingdoms_sim::buildings::BuildingId;
use kingdoms_sim::command::Command;
use kingdoms_sim::defs::Defs;
use kingdoms_sim::diplomacy::KingdomId;
use kingdoms_sim::military::{self, Order, SoldierStage};
use kingdoms_sim::population::CitizenId;
use kingdoms_sim::report::{self, CompanyRow, MilitaryReport, RecruitOption, SoldierRow};
use kingdoms_sim::state::GameState;

use super::table::{self, CellAlign, ROW_HEIGHT};
use super::{Cached, Navigate, TabContext, card_row, command, hint, select};
use crate::tools::OrderPick;
use crate::ui::hud::format::{count, decimal, int};
use crate::ui::hud::heraldry::crest;
use crate::ui::hud::{HudAction, HudInput, Icons};
use crate::ui::theme::{self, Palette, heading_font};
use crate::ui::widgets::{Mark, mark, mini_bar, rating_color, section_header};

/// Default radius (tiles) of an area to defend.
pub const DEFAULT_AREA_RADIUS: u8 = 6;
/// Width of the strength column („20 Kämpfer · 6 Milizen, …“) of the threats.
const THREAT_STRENGTH_WIDTH: f32 = 250.0;

/// A military building with the units it trains.
struct Barracks {
    id: BuildingId,
    name: String,
    icon: String,
    options: Vec<RecruitOption>,
    /// Recruits training here: name, unit, progress in percent.
    training: Vec<(String, String, u8)>,
}

/// A kingdom at war a company can march against.
struct Target {
    id: KingdomId,
    name: String,
    color: u8,
    at_war: bool,
}

struct Data {
    report: MilitaryReport,
    barracks: Vec<Barracks>,
    targets: Vec<Target>,
    max_patrol_points: u8,
    /// Provisions of each campaign by id: units left and units taken along.
    provisions: Vec<(u32, u32, u32)>,
}

/// Units of food a campaign still carries and how many it took along (`Campaign::provisions`
/// less what its soldiers have eaten on the way).
pub fn provisions_left(campaign: &kingdoms_sim::military::Campaign, defs: &Defs) -> (u32, u32) {
    let taken: u32 = campaign.provisions().iter().map(|m| m.amount).sum();
    (taken.saturating_sub(campaign.provisions_eaten(defs)), taken)
}

/// „Proviant 34 von 60“ (or that none was taken, e.g. while the company still musters).
pub fn provisions_text(left: u32, taken: u32) -> String {
    if taken == 0 { "noch kein Proviant".to_owned() } else { format!("Proviant {left} von {taken}") }
}

/// Recruits of `building` in training: name, unit name, progress in percent.
pub fn training_at(state: &GameState, defs: &Defs, building: BuildingId) -> Vec<(String, String, u8)> {
    state
        .military()
        .soldiers()
        .filter_map(|(id, s)| match s.stage() {
            SoldierStage::Training { building: b, done_milli } if b == building => {
                let unit = defs.units().get(s.unit().index());
                let total = unit.map_or(1, |u| u.training_ticks.max(1)) * 1000;
                let percent = u8::try_from((done_milli * 100 / total).min(100)).unwrap_or(100);
                Some((state.population().full_name(id).unwrap_or_default(), unit.map_or_else(String::new, |u| u.name.clone()), percent))
            }
            _ => None,
        })
        .collect()
}

/// German costs of one recruit: gold and equipment.
pub fn recruit_cost_text(defs: &Defs, option: &RecruitOption) -> String {
    let mut parts = vec![format!("{} Gold", option.cost_gold)];
    parts.extend(option.materials.iter().map(|m| format!("{} {}", m.amount, defs.resources().get(m.resource.index()).map_or("?", |r| r.name.as_str()))));
    parts.join(", ")
}

/// The units a military building trains as rows with costs, values and buttons for one recruit
/// and as many as possible now (shared by the military tab and the info panel).
pub fn recruit_rows(ui: &mut Ui, defs: &Defs, building: BuildingId, options: &[RecruitOption], actions: &mut Vec<HudAction>) {
    for o in options {
        let possible = o.problem.is_none();
        egui::Frame::new()
            .fill(if possible { Palette::PARCHMENT_LIGHT } else { Palette::PARCHMENT_DARK.gamma_multiply(0.6) })
            .stroke(egui::Stroke::new(1.0, Palette::PARCHMENT_SHADE))
            .corner_radius(egui::CornerRadius::same(4))
            .inner_margin(egui::Margin::symmetric(6, 3))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&o.name).strong().color(Palette::INK));
                    ui.label(RichText::new(&o.class_name).small().color(Palette::INK_SOFT));
                });
                let ranged = if o.ranged > 0 { format!(" · Fernkampf {} (Reichweite {})", o.ranged, o.range) } else { String::new() };
                ui.label(RichText::new(format!("Angriff {} · Verteidigung {}{ranged}", o.attack, o.defense)).small().color(Palette::INK_SOFT));
                ui.add(
                    egui::Label::new(
                        RichText::new(format!(
                            "Je Rekrut: {} · {} · Sold {} Gold/Tag · Ausbildung {} h",
                            recruit_cost_text(defs, o),
                            count(u32::from(o.population), "Bewohner", "Bewohner"),
                            o.upkeep,
                            decimal(f64::from(o.training_hours), 1)
                        ))
                        .small()
                        .color(Palette::INK),
                    )
                    .wrap(),
                )
                .on_hover_text(&o.description);
                if let Some(problem) = &o.problem {
                    ui.add(egui::Label::new(RichText::new(problem).small().color(Palette::BAD)).wrap());
                }
                ui.horizontal(|ui| {
                    let one = ui.add_enabled(possible, egui::Button::new("1 ausbilden"));
                    let one = match &o.problem {
                        Some(p) => one.on_disabled_hover_text(p.as_str()),
                        None => one,
                    };
                    if one.clicked() {
                        command(actions, Command::Recruit { building, unit: o.unit, count: 1 });
                    }
                    if o.max_now > 1 && ui.button(format!("{} ausbilden", o.max_now)).on_hover_text("So viele, wie jetzt möglich sind").clicked() {
                        command(actions, Command::Recruit { building, unit: o.unit, count: o.max_now });
                    }
                });
            });
    }
}

/// State of the tab.
#[derive(Default)]
pub struct MilitaryTab {
    data: Cached<Data>,
    controls: Controls,
}

/// What the player set in the tab (separate from the cached data).
struct Controls {
    /// Soldiers ticked in the list without company.
    picked: BTreeSet<CitizenId>,
    /// Kingdom chosen for a campaign, per company.
    target: BTreeMap<u32, KingdomId>,
    /// Campaign as a siege, per company.
    siege: BTreeMap<u32, bool>,
    /// Companies whose member list is shown.
    expanded: BTreeSet<u32>,
    /// Radius of an area to defend.
    area_radius: u8,
    /// Soldiers to dismiss after the player confirms.
    dismiss: Option<Vec<CitizenId>>,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            picked: BTreeSet::new(),
            target: BTreeMap::new(),
            siege: BTreeMap::new(),
            expanded: BTreeSet::new(),
            area_radius: DEFAULT_AREA_RADIUS,
            dismiss: None,
        }
    }
}

impl MilitaryTab {
    /// True while the dismissal question is open.
    pub fn dialog_open(&self) -> bool {
        self.controls.dismiss.is_some()
    }

    pub fn show(&mut self, ui: &mut Ui, ctx: &TabContext<'_, '_>, actions: &mut Vec<HudAction>) -> Option<Navigate> {
        let input = ctx.input;
        let (state, defs) = (input.session.state(), input.session.defs());
        let data = self.data.get(input, || compute(state, defs));
        let r = &data.report;
        let mut nav = None;
        card_row(
            ui,
            &[
                ("Soldaten", int(i64::from(r.soldiers)), Palette::INK),
                ("Bereit", int(i64::from(r.ready)), Palette::GOOD),
                ("In Ausbildung", int(i64::from(r.in_training)), Palette::INFO),
                ("Auf Feldzug", int(i64::from(r.away)), Palette::INK),
                ("Verwundet", int(i64::from(r.wounded)), if r.wounded > 0 { Palette::WARN } else { Palette::INK }),
                ("Sold je Tag", format!("{} Gold", int(i64::from(r.pay_per_day))), if r.unpaid { Palette::BAD } else { Palette::INK }),
            ],
        );
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(&r.strength_text).color(Palette::INK_SOFT));
            if r.crew > 0 {
                ui.label(RichText::new(format!("· {} an Belagerungsgerät", count(r.crew, "Mann", "Mann"))).color(Palette::INK_SOFT));
            }
        });
        if r.unpaid {
            warning(ui, "Der Sold kann nicht bezahlt werden: Die Moral der Soldaten sinkt.");
        }
        if let Some(alarm) = &r.alarm_text {
            warning(ui, alarm);
        }
        // Threats first: they are what needs the player now.
        section_header(ui, "Bedrohungen");
        if r.threats.is_empty() {
            ui.horizontal(|ui| {
                mark(ui, Mark::Ok);
                ui.label(RichText::new("Keine Feinde in Sicht.").color(Palette::GOOD));
            });
        }
        for (i, t) in r.threats.iter().enumerate() {
            let color = t.kingdom.and_then(|k| state.diplomacy().kingdom(k)).map(|k| k.color());
            // The composition of a mixed army is long: it wraps instead of ending in „…“.
            let strength = RichText::new(format!("{} · {}", count(u32::from(t.size), "Kämpfer", "Kämpfer"), t.composition_text)).small();
            let height = table::wrapped_height(ui, strength.clone(), THREAT_STRENGTH_WIDTH, ROW_HEIGHT + 8.0);
            table::row(ui, ("threat", t.warband), height, i % 2 == 1, false, |ui| {
                table::cell(ui, 230.0, CellAlign::Left, |ui| {
                    match color {
                        Some(c) => {
                            crest(ui, ctx.icons, 22.0, c);
                        }
                        None => {
                            mark(ui, Mark::Error);
                        }
                    }
                    ui.label(RichText::new(&t.kind_text).strong().color(Palette::BAD));
                });
                table::wrapped(ui, THREAT_STRENGTH_WIDTH, strength);
                table::text(ui, 200.0, CellAlign::Left, format!("{} · {}", t.stage_text, t.eta_text));
                table::text(ui, 160.0, CellAlign::Left, RichText::new(format!("Ziel: {}", t.target_name)).small());
                table::cell(ui, 100.0, CellAlign::Right, |ui| {
                    if ui.small_button("Kamera hin").clicked() {
                        actions.push(HudAction::Focus(t.position));
                        actions.push(HudAction::Sound("ui_click"));
                    }
                });
            });
        }
        if !r.wars.is_empty() {
            section_header(ui, "Kriege");
            for w in &r.wars {
                theme::inset_frame().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        if let Some(k) = state.diplomacy().kingdom(w.kingdom) {
                            crest(ui, ctx.icons, 26.0, k.color());
                        }
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&w.text).color(Palette::INK));
                            let declared = if w.declared_by_player { "von Euch erklärt" } else { "von ihnen erklärt" };
                            ui.label(
                                RichText::new(format!(
                                    "{declared} · {} geschickt · {} daheim · {} auf Euch zu · eigene Feldzüge: {}",
                                    count(u32::from(w.armies_sent), "Heer", "Heere"),
                                    count(w.enemy_soldiers, "Soldat", "Soldaten"),
                                    count(w.threats, "Heer", "Heere"),
                                    w.campaigns
                                ))
                                .small()
                                .color(Palette::INK_SOFT),
                            );
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button("Diplomatie").on_hover_text("Frieden anbieten oder Bedingungen stellen").clicked() {
                                nav = Some(Navigate::Kingdom(w.kingdom));
                            }
                        });
                    });
                });
            }
        }
        section_header(ui, "Einheiten");
        if r.units.is_empty() {
            hint(ui, "Noch keine Soldaten. Baut eine Kaserne (Militär, links) und bildet dort Rekruten aus.");
        } else {
            const W: [f32; 8] = [170.0, 100.0, 80.0, 90.0, 80.0, 120.0, 120.0, 120.0];
            table::header(
                ui,
                &[
                    ("Einheit", W[0], CellAlign::Left),
                    ("Gattung", W[1], CellAlign::Left),
                    ("Bereit", W[2], CellAlign::Right),
                    ("Ausbildung", W[3], CellAlign::Right),
                    ("Feldzug", W[4], CellAlign::Right),
                    ("Gesundheit", W[5], CellAlign::Left),
                    ("Moral", W[6], CellAlign::Left),
                    ("Ausrüstung", W[7], CellAlign::Left),
                ],
            );
            for (i, u) in r.units.iter().enumerate() {
                table::row(ui, ("unit", u.unit.index()), ROW_HEIGHT, i % 2 == 1, false, |ui| {
                    table::text(ui, W[0], CellAlign::Left, RichText::new(&u.name).strong());
                    table::text(ui, W[1], CellAlign::Left, RichText::new(&u.class_name).small().color(Palette::INK_SOFT));
                    table::text(ui, W[2], CellAlign::Right, u.ready.to_string());
                    table::text(ui, W[3], CellAlign::Right, u.training.to_string());
                    table::text(ui, W[4], CellAlign::Right, u.away.to_string());
                    value_bar(ui, W[5], u32::from(u.health_percent));
                    value_bar(ui, W[6], u32::from(u.morale));
                    value_bar(ui, W[7], u32::from(u.equipment_percent));
                });
            }
        }
        self.controls.companies(ui, ctx, data, actions);
        self.controls.unassigned(ui, ctx, &data.report.unassigned, &data.report.companies, actions);
        section_header(ui, "Rekrutierung");
        if data.barracks.is_empty() {
            hint(ui, "Kein fertiges Militärgebäude bildet aus. Kaserne, Bogenschützenposten und Stallungen findet Ihr im Baumenü unter „Militär“.");
        }
        for b in &data.barracks {
            theme::inset_frame().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    if let Some(icon) = ctx.icons.get(&b.icon) {
                        icon.show(ui, 26.0);
                    }
                    ui.label(RichText::new(&b.name).font(heading_font(15.0)).color(Palette::INK));
                    let slots = defs.rules().military.training_slots;
                    ui.label(RichText::new(format!("{} von {slots} Ausbildungsplätzen belegt", b.training.len())).small().color(Palette::INK_SOFT));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("Zeigen").clicked() {
                            select(actions, b.id);
                        }
                    });
                });
                recruit_rows(ui, defs, b.id, &b.options, actions);
                for (name, unit, percent) in &b.training {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{name} ({unit})")).small().color(Palette::INK));
                        mini_bar(ui, f32::from(*percent) / 100.0, Palette::INFO, 120.0);
                        ui.label(RichText::new(format!("{percent} %")).small().color(Palette::INK_SOFT));
                    });
                }
            });
            ui.add_space(3.0);
        }
        if !r.campaigns.is_empty() {
            section_header(ui, "Feldzüge");
            for (i, c) in r.campaigns.iter().enumerate() {
                table::row(ui, ("campaign", c.id), ROW_HEIGHT, i % 2 == 1, false, |ui| {
                    let kind = if c.siege { "Belagerung" } else { "Feldzug" };
                    table::text(ui, 300.0, CellAlign::Left, RichText::new(format!("{}: {kind} gegen {}", c.company_name, c.kingdom_name)).strong());
                    table::text(ui, 220.0, CellAlign::Left, format!("{} · {}", c.stage_text, c.eta_text));
                    table::text(ui, 120.0, CellAlign::Left, count(c.soldiers, "Soldat", "Soldaten"));
                    if let Some(&(_, left, taken)) = data.provisions.iter().find(|p| p.0 == c.id) {
                        let color = if taken > 0 && left * 5 < taken { Palette::WARN } else { Palette::INK_SOFT };
                        table::cell(ui, 160.0, CellAlign::Left, |ui| {
                            ui.label(RichText::new(provisions_text(left, taken)).small().color(color)).on_hover_text(
                                "Nahrung, die die Kompanie mitgenommen hat; jeder Soldat isst unterwegs seinen Anteil. Was beim Heimkehren übrig ist, kommt ins Lager zurück.",
                            );
                        });
                    }
                });
            }
        }
        section_header(ui, "Kampfberichte");
        if r.battles.is_empty() {
            hint(ui, "Noch keine Kämpfe.");
        }
        for (i, b) in r.battles.iter().take(12).enumerate() {
            let color = match b.outcome {
                military::BattleOutcome::Victory => Palette::GOOD,
                military::BattleOutcome::Defeat => Palette::BAD,
                military::BattleOutcome::Draw => Palette::WARN,
            };
            let response = table::row(ui, ("battle", b.serial), ROW_HEIGHT + 2.0, i % 2 == 1, true, |ui| {
                table::text(ui, 150.0, CellAlign::Left, RichText::new(&b.date_text).small().color(Palette::INK_SOFT));
                table::text(ui, 220.0, CellAlign::Left, RichText::new(format!("{} gegen {}", b.kind_name, b.enemy)).strong());
                table::text(ui, 110.0, CellAlign::Left, RichText::new(&b.outcome_name).strong().color(color));
                table::text(
                    ui,
                    260.0,
                    CellAlign::Left,
                    RichText::new(format!(
                        "Verluste: {} von {} eigenen, {} von {}",
                        b.own_losses,
                        b.own_units,
                        b.enemy_losses,
                        if b.enemy_units == 1 { "1 Feind".to_owned() } else { format!("{} Feinden", b.enemy_units) }
                    ))
                    .small(),
                );
                table::cell(ui, ui.available_width(), CellAlign::Left, |ui| {
                    ui.add(egui::Label::new(RichText::new(&b.text).small().color(Palette::INK_SOFT)).truncate());
                });
            });
            if response.on_hover_text(format!("{}\nKlick: zum Ort des Kampfes", b.text)).clicked() {
                actions.push(HudAction::Focus(b.position));
                actions.push(HudAction::Sound("ui_click"));
            }
        }
        self.controls.dismiss_dialog(ui.ctx(), input, actions);
        nav
    }
}

impl Controls {
    /// Company cards with values, members and orders.
    fn companies(&mut self, ui: &mut Ui, ctx: &TabContext<'_, '_>, data: &Data, actions: &mut Vec<HudAction>) {
        let (state, defs) = (ctx.input.session.state(), ctx.input.session.defs());
        section_header(ui, "Kompanien");
        if data.report.companies.is_empty() {
            hint(ui, "Noch keine Kompanie. Wählt unten bereite Soldaten aus und gründet eine Kompanie, um ihr Befehle zu geben.");
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new("Umkreis beim Verteidigen eines Gebiets:").small().color(Palette::INK_SOFT));
            ui.add(egui::DragValue::new(&mut self.area_radius).range(1..=12).suffix(" Felder"));
        });
        for c in &data.report.companies {
            theme::inset_frame().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&c.name).font(heading_font(16.0)).color(Palette::INK));
                    ui.label(RichText::new(&c.order_text).color(Palette::INFO));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some(p) = c.position
                            && ui.small_button("Kamera hin").clicked()
                        {
                            actions.push(HudAction::Focus(p));
                            actions.push(HudAction::Sound("ui_click"));
                        }
                    });
                });
                let units = if c.units_text.is_empty() { "keine Soldaten".to_owned() } else { c.units_text.clone() };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(units).small().color(Palette::INK));
                    ui.label(RichText::new(format!("· Sold {} Gold/Tag", c.pay_per_day)).small().color(Palette::INK_SOFT));
                    ui.label(RichText::new("Gesundheit").small().color(Palette::INK_SOFT));
                    mini_bar(ui, f32::from(c.health_percent) / 100.0, rating_color(f32::from(c.health_percent)), 60.0);
                    ui.label(RichText::new("Moral").small().color(Palette::INK_SOFT));
                    mini_bar(ui, f32::from(c.morale) / 100.0, rating_color(f32::from(c.morale)), 60.0);
                    ui.label(RichText::new("Ausrüstung").small().color(Palette::INK_SOFT));
                    mini_bar(ui, f32::from(c.equipment_percent) / 100.0, rating_color(f32::from(c.equipment_percent)), 60.0);
                });
                self.order_buttons(ui, state, defs, ctx.icons, c, &data.targets, data.max_patrol_points, actions);
                let open = self.expanded.contains(&c.id);
                let label = if open { "Mitglieder ausblenden" } else { "Mitglieder zeigen" };
                if ui.small_button(format!("{label} ({})", c.members.len())).clicked() {
                    if open {
                        self.expanded.remove(&c.id);
                    } else {
                        self.expanded.insert(c.id);
                    }
                }
                if open {
                    for (i, s) in c.members.iter().enumerate() {
                        soldier_line(ui, s, i, actions, |ui, actions| {
                            if c.campaign.is_none() && ui.small_button("Herauslösen").on_hover_text("Aus der Kompanie nehmen").clicked() {
                                command(actions, Command::AssignSoldiers { soldiers: vec![s.citizen], company: None });
                            }
                        });
                    }
                }
            });
            ui.add_space(3.0);
        }
    }

    /// Order buttons of one company with the simulation's reasons for impossible orders.
    #[allow(clippy::too_many_arguments)]
    fn order_buttons(
        &mut self,
        ui: &mut Ui,
        state: &GameState,
        defs: &Defs,
        icons: &Icons,
        c: &CompanyRow,
        targets: &[Target],
        max_patrol_points: u8,
        actions: &mut Vec<HudAction>,
    ) {
        let empty = c.members.is_empty();
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Befehl:").small().color(Palette::INK_SOFT));
            let station = military::order_problem(state, defs, c.id, &Order::Station);
            if order_button(ui, "Bereitschaft", "Am Posten bereitstehen und Feinde im Umkreis angreifen", station.as_deref(), c.order == Order::Station) {
                command(actions, Command::CompanyOrder { company: c.id, order: Order::Station });
            }
            let campaigning = c.campaign.is_some();
            let busy = campaigning.then(|| format!("{} ist auf einem Feldzug; befehlt zuerst den Rückzug.", c.name));
            if order_button(
                ui,
                "Verteidigen …",
                "Auf der Karte ein Gebäude (Besatzung) oder einen Ort (Gebiet im eingestellten Umkreis) wählen",
                busy.as_deref(),
                matches!(c.order, Order::Defend(_)),
            ) {
                actions.push(HudAction::PickOrder { company: c.id, pick: OrderPick::Defend { radius: self.area_radius } });
            }
            let hint = format!("Auf der Karte 2 bis {max_patrol_points} Wegpunkte setzen; die Kompanie läuft sie im Kreis ab");
            if order_button(ui, "Patrouille …", &hint, busy.as_deref(), matches!(c.order, Order::Patrol { .. })) {
                actions.push(HudAction::PickOrder { company: c.id, pick: OrderPick::Patrol });
            }
            let retreat = military::order_problem(state, defs, c.id, &Order::Retreat);
            if order_button(ui, "Rückzug", "Zurück zum Hauptquartier; ein Feldzug kehrt um", retreat.as_deref(), c.order == Order::Retreat) {
                command(actions, Command::CompanyOrder { company: c.id, order: Order::Retreat });
            }
            let disband = campaigning.then(|| format!("{} ist auf einem Feldzug.", c.name));
            if order_button(ui, "Auflösen", "Die Soldaten bleiben Soldaten ohne Kompanie", disband.as_deref(), false) {
                command(actions, Command::DisbandCompany { company: c.id });
            }
        });
        if targets.is_empty() {
            ui.label(RichText::new("Feldzüge und Belagerungen gibt es nur gegen ein Reich, mit dem Ihr im Krieg seid.").small().color(Palette::INK_SOFT));
            return;
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new("Feldzug gegen").small().color(Palette::INK_SOFT));
            let chosen = self
                .target
                .get(&c.id)
                .copied()
                .filter(|id| targets.iter().any(|t| t.id == *id))
                .or_else(|| targets.iter().find(|t| t.at_war).or(targets.first()).map(|t| t.id));
            let Some(chosen) = chosen else { return };
            let name = targets.iter().find(|t| t.id == chosen).map_or("", |t| t.name.as_str());
            let mut pick = chosen;
            egui::ComboBox::from_id_salt(("campaign_target", c.id)).selected_text(name).width(150.0).show_ui(ui, |ui| {
                for t in targets {
                    let label = if t.at_war { format!("{} (Krieg)", t.name) } else { t.name.clone() };
                    ui.selectable_value(&mut pick, t.id, label);
                }
            });
            self.target.insert(c.id, pick);
            let mut siege = self.siege.get(&c.id).copied().unwrap_or(false);
            ui.checkbox(&mut siege, "Belagerung").on_hover_text("Braucht Belagerungsgerät; ein Erfolg erzwingt Frieden mit Gebietsabtretung");
            self.siege.insert(c.id, siege);
            let order = Order::Campaign { kingdom: pick, siege };
            let problem = if empty { Some("Die Kompanie hat keine Soldaten.".to_owned()) } else { military::order_problem(state, defs, c.id, &order) };
            if let Some(t) = targets.iter().find(|t| t.id == pick) {
                crest(ui, icons, 22.0, t.color);
            }
            if order_button(ui, "Ausrücken", "Die Kompanie sammelt sich am Kartenrand und zieht los", problem.as_deref(), false) {
                command(actions, Command::CompanyOrder { company: c.id, order });
            }
            if let Some(p) = problem {
                ui.label(RichText::new(p).small().color(Palette::BAD));
            }
        });
    }

    /// Ready soldiers without a company and recruits, with creating, assigning and dismissing.
    fn unassigned(&mut self, ui: &mut Ui, ctx: &TabContext<'_, '_>, soldiers: &[SoldierRow], companies: &[CompanyRow], actions: &mut Vec<HudAction>) {
        section_header(ui, "Ohne Kompanie");
        self.picked.retain(|id| soldiers.iter().any(|s| s.citizen == *id));
        if soldiers.is_empty() {
            hint(ui, "Alle Soldaten gehören einer Kompanie an.");
            return;
        }
        let ready: Vec<&SoldierRow> = soldiers.iter().filter(|s| ctx.input.session.state().military().soldier(s.citizen).is_some_and(|x| x.is_ready())).collect();
        ui.horizontal_wrapped(|ui| {
            if ui.small_button("Alle Bereiten wählen").clicked() {
                self.picked = ready.iter().map(|s| s.citizen).collect();
            }
            if ui.small_button("Auswahl leeren").clicked() {
                self.picked.clear();
            }
            let picked: Vec<CitizenId> = self.picked.iter().copied().collect();
            let label = format!("Neue Kompanie ({})", picked.len());
            if ui.add_enabled(!picked.is_empty(), egui::Button::new(label)).clicked() {
                command(actions, Command::CreateCompany { soldiers: picked.clone() });
                self.picked.clear();
            }
            for c in companies.iter().filter(|c| c.campaign.is_none()) {
                if ui.add_enabled(!picked.is_empty(), egui::Button::new(format!("Zu „{}“", c.name))).clicked() {
                    command(actions, Command::AssignSoldiers { soldiers: picked.clone(), company: Some(c.id) });
                    self.picked.clear();
                }
            }
            if ui.add_enabled(!picked.is_empty(), egui::Button::new("Entlassen …")).on_hover_text("Zurück ins zivile Leben").clicked() {
                self.dismiss = Some(picked);
                actions.push(HudAction::Sound("ui_open"));
            }
        });
        for (i, s) in soldiers.iter().enumerate() {
            let ready = ready.iter().any(|r| r.citizen == s.citizen);
            let mut ticked = self.picked.contains(&s.citizen);
            soldier_line(ui, s, i, actions, |ui, _| {
                if ui.add_enabled(ready, egui::Checkbox::without_text(&mut ticked)).on_disabled_hover_text("Rekruten in Ausbildung können noch keiner Kompanie angehören").changed() {
                    if ticked {
                        self.picked.insert(s.citizen);
                    } else {
                        self.picked.remove(&s.citizen);
                    }
                }
            });
        }
    }

    /// Asks before dismissing soldiers.
    fn dismiss_dialog(&mut self, ctx: &egui::Context, input: &HudInput<'_>, actions: &mut Vec<HudAction>) {
        let Some(soldiers) = self.dismiss.clone() else { return };
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::unique("military_dismiss")).frame(theme::parchment_frame().inner_margin(egui::Margin::same(18))).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(RichText::new("Soldaten entlassen?").font(heading_font(20.0)).color(Palette::INK));
            let names: Vec<String> = soldiers.iter().filter_map(|id| input.session.state().population().full_name(*id)).take(6).collect();
            ui.label(format!("{} kehren ins zivile Leben zurück und suchen sich wieder Arbeit.", count(u32::try_from(soldiers.len()).unwrap_or(0), "Soldat", "Soldaten")));
            ui.label(RichText::new(names.join(", ")).small().color(Palette::INK_SOFT));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(RichText::new("Entlassen").strong()).clicked() {
                    command(actions, Command::DismissSoldiers { soldiers: soldiers.clone() });
                    self.picked.clear();
                    close = true;
                }
                if ui.button("Abbrechen").clicked() {
                    close = true;
                }
            });
        });
        if close || modal.should_close() {
            self.dismiss = None;
        }
    }
}

/// The tab's data.
fn compute(state: &GameState, defs: &Defs) -> Data {
    let report = report::military_report(state, defs);
    let barracks = state
        .buildings()
        .iter()
        .filter(|(_, b)| b.is_complete())
        .filter_map(|(id, b)| {
            let def = defs.buildings().get(b.kind().index())?;
            if def.trains.is_empty() {
                return None;
            }
            Some(Barracks {
                id,
                name: def.name.clone(),
                icon: format!("bld_{}", def.key),
                options: report::recruit_options(state, defs, id),
                training: training_at(state, defs, id),
            })
        })
        .collect();
    let targets = state
        .diplomacy()
        .kingdoms()
        .filter(|(_, k)| k.at_war())
        .map(|(id, k)| Target { id, name: k.name().to_owned(), color: k.color(), at_war: k.at_war() })
        .collect();
    let provisions = state
        .military()
        .campaigns()
        .iter()
        .map(|c| {
            let (left, taken) = provisions_left(c, defs);
            (c.id(), left, taken)
        })
        .collect();
    Data { report, barracks, targets, max_patrol_points: defs.rules().military.max_patrol_points, provisions }
}

/// A button for an order: highlighted when it is the current order, disabled with the reason.
fn order_button(ui: &mut Ui, label: &str, hint: &str, problem: Option<&str>, current: bool) -> bool {
    let text = if current { RichText::new(label).strong().color(Palette::INK) } else { RichText::new(label) };
    let response = ui.add_enabled(problem.is_none(), egui::Button::new(text).selected(current));
    let response = match problem {
        Some(p) => response.on_disabled_hover_text(p),
        None => response.on_hover_text(hint),
    };
    response.clicked()
}

/// A bar with a percentage in a table cell.
fn value_bar(ui: &mut Ui, width: f32, percent: u32) {
    table::cell(ui, width, CellAlign::Left, |ui| {
        let f = percent.min(100) as f32 / 100.0;
        mini_bar(ui, f, rating_color(percent.min(100) as f32), width - 50.0);
        ui.label(RichText::new(format!("{percent} %")).small().color(Palette::INK_SOFT));
    });
}

/// One soldier: name, unit, state, values, activity; `extra` adds controls at the end.
fn soldier_line(ui: &mut Ui, s: &SoldierRow, i: usize, actions: &mut Vec<HudAction>, extra: impl FnOnce(&mut Ui, &mut Vec<HudAction>)) {
    let response = table::row(ui, ("soldier", s.citizen.index()), ROW_HEIGHT, i % 2 == 1, true, |ui| {
        table::cell(ui, 40.0, CellAlign::Left, |ui| extra(ui, actions));
        table::text(ui, 190.0, CellAlign::Left, RichText::new(&s.name).color(Palette::INK));
        table::text(ui, 130.0, CellAlign::Left, RichText::new(&s.unit_name).small());
        table::text(ui, 150.0, CellAlign::Left, RichText::new(&s.stage_text).small().color(Palette::INK_SOFT));
        table::text(
            ui,
            250.0,
            CellAlign::Left,
            RichText::new(format!("Gesundheit {} % · Moral {} · Erfahrung {} · {} besiegt", s.health_percent, s.morale, s.experience, s.kills)).small(),
        );
        table::cell(ui, ui.available_width(), CellAlign::Left, |ui| {
            ui.add(egui::Label::new(RichText::new(&s.activity_text).small().color(Palette::INK_SOFT)).truncate());
        });
    });
    if response.on_hover_text("Klick: Bürgerfenster").clicked() {
        actions.push(HudAction::InspectCitizen(s.citizen));
        actions.push(HudAction::Sound("ui_select"));
    }
}

/// A warning line.
fn warning(ui: &mut Ui, text: &str) {
    ui.horizontal_top(|ui| {
        mark(ui, Mark::Warning);
        ui.add(egui::Label::new(RichText::new(text).color(Palette::WARN)).wrap());
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::hud::management::tests::test_defs;
    use kingdoms_sim::state::GameSettings;
    use kingdoms_sim::world::WorldSize;

    #[test]
    fn a_new_kingdom_has_no_soldiers_but_targets() {
        let defs = test_defs();
        let state = GameState::new(&defs, GameSettings::new(&defs, 7, WorldSize::Small));
        let data = compute(&state, &defs);
        assert_eq!(data.report.soldiers, 0);
        assert!(data.barracks.is_empty(), "the start has no military building");
        assert!(data.targets.is_empty(), "nobody is at war at the start");
        assert!(data.max_patrol_points >= 2);
        assert!(data.provisions.is_empty(), "no campaign at the start");
        assert_eq!(provisions_text(34, 60), "Proviant 34 von 60");
        assert_eq!(provisions_text(0, 0), "noch kein Proviant");
        // Costs read like the data.
        let Some(barracks) = defs.buildings().iter().find(|b| !b.trains.is_empty()) else { panic!("a building trains units") };
        let Some(unit) = barracks.trains.first().and_then(|u| defs.units().get(u.index())) else { panic!("unit") };
        let option = RecruitOption {
            unit: unit.id,
            name: unit.name.clone(),
            description: String::new(),
            class_name: String::new(),
            cost_gold: unit.cost.gold,
            materials: unit.cost.materials.clone(),
            population: unit.population,
            upkeep: unit.upkeep_per_day,
            training_hours: unit.training_hours,
            attack: unit.attack,
            defense: unit.defense,
            ranged: unit.ranged,
            range: unit.range,
            max_now: 0,
            problem: None,
        };
        let text = recruit_cost_text(&defs, &option);
        assert!(text.starts_with(&format!("{} Gold", unit.cost.gold)), "{text}");
        assert_eq!(text.matches(", ").count(), unit.cost.materials.len());
    }
}
