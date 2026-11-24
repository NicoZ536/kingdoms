//! Citizen window („Bürger“): one citizen from `report::citizen_detail` – name, age, sex and
//! life stage, family and household, home and workplace, what the citizen does right now
//! (German activity text, destination, position, carried goods, last commute), needs,
//! happiness with its factors, health (and whether the citizen is sick), work efficiency,
//! education, friends and the household's savings with its wealth class.
//!
//! Buttons: follow with the camera (the world scene keeps the camera on the walker until the
//! player pans, presses Esc or closes the window), look at the citizen once, select the home or
//! the workplace. Relatives are links that open their own window.
//!
//! Opened by clicking a walker on the map (inspect tool), a resident in a house's info panel or
//! a citizen in the „Bevölkerung“ tab of the management window.

use kingdoms_engine::egui::{self, Align, Layout, Pos2, RichText, Ui};
use kingdoms_sim::population::{Activity, CitizenId, Destination, Purpose, Sex};
use kingdoms_sim::report::{self, CitizenDetail, CitizenLink};

use super::{HudAction, HudInput, Icons, MARGIN, TOP_BAR_HEIGHT};
use crate::actions::Action;
use crate::ui::hud::format::{decimal, int};
use crate::ui::hud::info_panel::wealth_color;
use crate::ui::theme::{self, Palette, heading_font};
use crate::ui::widgets::{icon_button, mini_bar, progress_bar, rating_color, section_header};

/// Inner width of the window.
const WIDTH: f32 = 316.0;
/// Width the info panel occupies at the right edge (panel, frame and margin).
const INFO_PANEL_SPACE: f32 = 340.0 + 32.0;

/// German summary of where a citizen is ("im Holzhaus", "unterwegs bei Feld 12, 40").
pub fn whereabouts(detail: &CitizenDetail, inside_name: Option<&str>) -> String {
    let tile = detail.position.tile();
    match (detail.inside, inside_name) {
        (Some(_), Some(name)) => format!("Im Gebäude: {name}"),
        (Some(_), None) => "In einem Gebäude".to_owned(),
        (None, _) => format!("Draußen bei Feld {}, {}", tile.x, tile.y),
    }
}

/// German line for the destination, if the citizen is heading somewhere else than where it is.
pub fn destination_line(detail: &CitizenDetail) -> Option<String> {
    let destination = detail.destination?;
    if let Destination::Building { building, .. } = destination
        && detail.inside == Some(building)
    {
        return None;
    }
    Some(match (destination, detail.destination_name.as_deref()) {
        (Destination::MapEdge(_), _) => "Ziel: verlässt das Königreich".to_owned(),
        (Destination::Spot { tile, .. }, Some(name)) => format!("Ziel: Arbeitsplatz bei Feld {}, {} ({name})", tile.x, tile.y),
        (Destination::Spot { tile, .. }, None) => format!("Ziel: Feld {}, {}", tile.x, tile.y),
        (Destination::Building { .. }, Some(name)) => format!("Ziel: {name}"),
        (Destination::Building { .. }, None) => "Ziel: ein Gebäude".to_owned(),
        (Destination::Tile(tile), _) => format!("Ziel: Feld {}, {}", tile.x, tile.y),
    })
}

/// State of the citizen window.
#[derive(Default)]
pub struct CitizenWindow {
    open: Option<CitizenId>,
    /// Frames left in which the window is raised above the other windows (after opening).
    raise_frames: u8,
}

impl CitizenWindow {
    /// The citizen shown, if the window is open.
    pub fn citizen(&self) -> Option<CitizenId> {
        self.open
    }

    /// Opens the window for `citizen` (or closes it with `None`).
    pub fn set(&mut self, citizen: Option<CitizenId>) {
        if citizen.is_some() && citizen != self.open {
            self.raise_frames = 3;
        }
        self.open = citizen;
    }

    /// Draws the window and collects the player's actions. Closes itself when the citizen no
    /// longer lives in the kingdom.
    /// Draws the window (if open); it first appears left of the info panel's place, whose
    /// right edge is at `right`, and grows at most down to `bottom` (above the bottom bar).
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        input: &HudInput<'_>,
        icons: &Icons,
        right: f32,
        bottom: f32,
        actions: &mut Vec<HudAction>,
    ) {
        let Some(id) = self.open else { return };
        let state = input.session.state();
        let defs = input.session.defs();
        let Some(detail) = report::citizen_detail(state, defs, id) else {
            self.open = None;
            actions.push(HudAction::CloseCitizen);
            return;
        };
        let screen = ctx.content_rect();
        let x = (right - INFO_PANEL_SPACE - WIDTH - 28.0).max(screen.left() + MARGIN);
        let y = screen.top() + TOP_BAR_HEIGHT + MARGIN;
        let max_height = window_max_height(y, bottom);
        let mut close = false;
        let shown = egui::Window::new("Bürger")
            .id(egui::Id::unique("hud_citizen_window"))
            .title_bar(false)
            .frame(theme::parchment_frame())
            .resizable(false)
            .collapsible(false)
            .default_pos(Pos2::new(x, y))
            .show(ctx, |ui| {
                ui.set_width(WIDTH);
                ui.set_max_height(max_height);
                close = header(ui, input, icons, &detail, actions);
                egui::ScrollArea::vertical().max_height(max_height - 120.0).auto_shrink([false, true]).show(ui, |ui| {
                    ui.set_width(WIDTH - 6.0);
                    let sexes = |links: &[&CitizenLink]| -> Vec<Sex> {
                        links.iter().filter_map(|l| state.population().citizen(l.citizen).map(|c| c.sex())).collect()
                    };
                    let labels = KinLabels {
                        partner: kin_label(&sexes(&detail.partner.iter().collect::<Vec<_>>()), ["Ehemann", "Ehefrau", "Ehepartner", "Ehepartner"]),
                        children: kin_label(&sexes(&detail.children.iter().collect::<Vec<_>>()), ["Sohn", "Tochter", "Kinder", "Töchter"]),
                        friends: kin_label(&sexes(&detail.friends.iter().collect::<Vec<_>>()), ["Freund", "Freundin", "Freunde", "Freundinnen"]),
                    };
                    body(ui, &detail, &labels, actions);
                });
            });
        if let Some(shown) = shown
            && self.raise_frames > 0
        {
            self.raise_frames -= 1;
            ctx.move_to_top(shown.response.layer_id);
        }
        if close {
            self.open = None;
            actions.push(HudAction::CloseCitizen);
            actions.push(HudAction::Sound("ui_close"));
        }
    }
}

/// Name, age and the buttons; returns true if the close button was clicked.
fn header(ui: &mut Ui, input: &HudInput<'_>, icons: &Icons, d: &CitizenDetail, actions: &mut Vec<HudAction>) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        if let Some(icon) = icons.get("ui_population") {
            icon.show(ui, 34.0);
        }
        ui.vertical(|ui| {
            ui.label(RichText::new(&d.name).font(heading_font(19.0)).color(Palette::INK));
            ui.label(
                RichText::new(format!("{}, {} · {}", d.sex_label, crate::ui::hud::format::years(d.age), d.stage_label)).color(Palette::INK_SOFT),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
            if icon_button(ui, icons.get("ui_close"), "×", false, 26.0)
                .on_hover_text(format!("Schließen ({})", input.hint(Action::Cancel)))
                .clicked()
            {
                close = true;
            }
        });
    });
    // What the citizen does right now.
    theme::inset_frame().show(ui, |ui| {
        ui.set_width(WIDTH - 16.0);
        let text = if d.activity_text.is_empty() { d.activity_label.clone() } else { d.activity_text.clone() };
        let (color, note) = activity_note(d);
        let response = ui.label(RichText::new(text).strong().color(color));
        if let Some(note) = note {
            response.on_hover_text(note);
        }
        if let Some(line) = destination_line(d) {
            ui.label(RichText::new(line).color(Palette::INK_SOFT));
        }
        let inside_name = d
            .inside
            .and_then(|b| input.session.state().building(b))
            .and_then(|b| input.session.defs().buildings().get(b.kind().index()))
            .map(|def| def.name.as_str());
        ui.label(RichText::new(whereabouts(d, inside_name)).small().color(Palette::INK_SOFT));
        if let Some(carried) = d.carried {
            let defs = input.session.defs();
            if let Some(def) = defs.resources().get(carried.resource.index()) {
                ui.horizontal(|ui| {
                    if let Some(icon) = icons.get(&format!("res_{}", def.key)) {
                        icon.show(ui, 18.0);
                    }
                    ui.label(RichText::new(format!("Trägt {} {}", carried.amount, def.name)).color(Palette::INK));
                });
            }
        }
    });
    // Camera and navigation.
    ui.horizontal_wrapped(|ui| {
        let following = input.following == Some(d.citizen);
        let follow = ui.add(egui::Button::new(if following { "Folgt …" } else { "Folgen" }).selected(following)).on_hover_text(
            "Die Kamera folgt dem Bürger. Verschieben der Karte, Esc oder Schließen beenden das Folgen.",
        );
        if follow.clicked() {
            actions.push(HudAction::FollowCitizen(if following { None } else { Some(d.citizen) }));
            actions.push(HudAction::Sound("ui_click"));
        }
        if ui.button("Zeigen").on_hover_text("Kamera einmal zum Bürger").clicked() {
            actions.push(HudAction::LookAtCitizen(d.citizen));
            actions.push(HudAction::Sound("ui_click"));
        }
        let home = ui.add_enabled(d.home.is_some(), egui::Button::new("Zuhause"));
        if let Some(home_id) = d.home {
            if home.on_hover_text(d.home_name.clone().unwrap_or_default()).clicked() {
                actions.push(HudAction::SelectBuilding(home_id));
                actions.push(HudAction::Sound("ui_click"));
            }
        } else {
            home.on_disabled_hover_text("Obdachlos");
        }
        let work = ui.add_enabled(d.job.is_some(), egui::Button::new("Arbeitsplatz"));
        if let Some(job) = d.job.as_ref() {
            if work.on_hover_text(&job.building_name).clicked() {
                actions.push(HudAction::SelectBuilding(job.building));
                actions.push(HudAction::Sound("ui_click"));
            }
        } else {
            work.on_disabled_hover_text("Ohne Arbeitsplatz");
        }
    });
    close
}

/// Colour of the activity line and an explanation for the activities of the daily routine that
/// events change (sick, in hospital, sheltering from enemies, festival).
pub fn activity_note(d: &CitizenDetail) -> (egui::Color32, Option<&'static str>) {
    let walking_to = |purpose: Purpose| matches!(d.destination, Some(Destination::Building { purpose: p, .. }) if p == purpose);
    match d.activity {
        Activity::Sick => (Palette::BAD, Some("Krank: bleibt zu Hause und arbeitet nicht, bis die Gesundheit wieder steigt.")),
        Activity::Treatment => (Palette::WARN, Some("Wird im Krankenhaus gepflegt und erholt sich doppelt so schnell.")),
        Activity::Sheltering => (Palette::WARN, Some("Feinde in der Nähe: sucht Schutz zu Hause oder im Hauptquartier.")),
        Activity::Festival => (Palette::GOOD, Some("Festtag: feiert am Nachmittag statt zu arbeiten.")),
        Activity::Walking if walking_to(Purpose::Shelter) => {
            (Palette::WARN, Some("Feinde in der Nähe: flieht nach Hause oder ins Hauptquartier."))
        }
        Activity::Walking if walking_to(Purpose::Treatment) => (Palette::WARN, Some("Krank: geht ins Krankenhaus in Reichweite.")),
        _ if d.sick => (Palette::BAD, Some("Krank: Gesundheit unter der Schwelle – arbeitet nicht.")),
        _ => (Palette::INK, None),
    }
}

/// The „Arbeit“ entry of a citizen without a job: sick leave (the job was given up after a day
/// of illness) or simply none.
fn no_job_text(d: &CitizenDetail) -> (&'static str, egui::Color32) {
    if d.sick_leave {
        ("Krankgeschrieben – sucht nach der Genesung neue Arbeit", Palette::WARN)
    } else {
        ("keine", Palette::INK_SOFT)
    }
}

/// Colour of the training line: still learning is a (temporary) loss.
fn training_color(percent: u8) -> egui::Color32 {
    if percent < 100 { Palette::WARN } else { Palette::INK }
}

fn link(ui: &mut Ui, label: &str, people: &[&CitizenLink], actions: &mut Vec<HudAction>) {
    ui.label(RichText::new(label).color(Palette::INK_SOFT));
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        for p in people {
            if ui.link(RichText::new(&p.name).color(Palette::INFO)).on_hover_text("Bürger anzeigen").clicked() {
                actions.push(HudAction::InspectCitizen(p.citizen));
                actions.push(HudAction::Sound("ui_click"));
            }
        }
    });
    ui.end_row();
}

/// Labels of the relatives and friends by their sex.
struct KinLabels {
    partner: &'static str,
    children: &'static str,
    friends: &'static str,
}

/// German label for relatives of the sexes `sexes`, from `[one man, one woman, several,
/// several women]` – e.g. „Freund“, „Freundin“, „Freunde“, „Freundinnen“.
pub fn kin_label(sexes: &[Sex], words: [&'static str; 4]) -> &'static str {
    match sexes {
        [Sex::Male] => words[0],
        [Sex::Female] => words[1],
        list if !list.is_empty() && list.iter().all(|s| *s == Sex::Female) => words[3],
        _ => words[2],
    }
}

fn body(ui: &mut Ui, d: &CitizenDetail, labels: &KinLabels, actions: &mut Vec<HudAction>) {
    section_header(ui, "Familie und Arbeit");
    egui::Grid::new(("citizen_rows", d.citizen.index())).num_columns(2).spacing([12.0, 3.0]).show(ui, |ui| {
        ui.label(RichText::new("Haushalt").color(Palette::INK_SOFT));
        ui.label(format!("Familie {}", d.family_name));
        ui.end_row();
        ui.label(RichText::new("Zuhause").color(Palette::INK_SOFT));
        match (&d.home_name, d.home) {
            (Some(name), Some(home)) => {
                if ui.link(RichText::new(name).color(Palette::INFO)).on_hover_text("Haus auswählen").clicked() {
                    actions.push(HudAction::SelectBuilding(home));
                }
            }
            _ => {
                ui.colored_label(Palette::BAD, "obdachlos");
            }
        }
        ui.end_row();
        if let Some(partner) = d.partner.as_ref() {
            link(ui, labels.partner, &[partner], actions);
        }
        let parents: Vec<&CitizenLink> = d.mother.iter().chain(d.father.iter()).collect();
        if !parents.is_empty() {
            link(ui, "Eltern", &parents, actions);
        }
        if !d.children.is_empty() {
            let children: Vec<&CitizenLink> = d.children.iter().collect();
            link(ui, labels.children, &children, actions);
        }
        if !d.friends.is_empty() {
            let friends: Vec<&CitizenLink> = d.friends.iter().collect();
            link(ui, labels.friends, &friends, actions);
        }
        ui.label(RichText::new("Ersparnisse").color(Palette::INK_SOFT));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.label(format!("{} Gold", int(d.household_savings)));
            ui.label(RichText::new(format!("· {}", d.wealth_label)).color(wealth_color(d.wealth)));
        })
        .response
        .on_hover_text("Ersparnisse des ganzen Haushalts. Arme Haushalte sind unzufrieden, wohlhabende zufriedener.");
        ui.end_row();
        ui.label(RichText::new("Arbeit").color(Palette::INK_SOFT));
        match d.job.as_ref() {
            Some(job) => {
                let text = if job.title == job.building_name { job.title.clone() } else { format!("{} ({})", job.title, job.building_name) };
                if ui.link(RichText::new(text).color(Palette::INFO)).on_hover_text("Arbeitsplatz auswählen").clicked() {
                    actions.push(HudAction::SelectBuilding(job.building));
                }
            }
            None => {
                let (text, color) = no_job_text(d);
                ui.label(RichText::new(text).color(color));
            }
        }
        ui.end_row();
        if let Some(training) = d.training.as_ref() {
            ui.label(RichText::new("Einarbeitung").color(Palette::INK_SOFT));
            ui.label(RichText::new(&training.text).color(training_color(training.percent))).on_hover_text(
                "Neue Fach- und Gelehrtenarbeiter leisten anfangs weniger und lernen ihre Arbeit in wenigen Arbeitstagen. Die Erfahrung gilt für die ganze Berufsgruppe.",
            );
            ui.end_row();
        }
        if let Some(minutes) = d.last_commute_minutes {
            ui.label(RichText::new("Arbeitsweg").color(Palette::INK_SOFT));
            ui.label(format!("{} (zuletzt)", crate::ui::hud::format::duration_minutes(u64::from(minutes))));
            ui.end_row();
        }
        ui.label(RichText::new("Arbeitskraft").color(Palette::INK_SOFT));
        ui.label(format!("{} %", d.work_efficiency_percent))
            .on_hover_text("Leistung bei der Arbeit nach Gesundheit und Zufriedenheit");
        ui.end_row();
        ui.label(RichText::new("Bildung").color(Palette::INK_SOFT));
        ui.label(if d.educated { "gebildet".to_owned() } else { format!("{} von 100", d.education) });
        ui.end_row();
    });
    section_header(ui, "Gesundheit und Zufriedenheit");
    ui.horizontal(|ui| {
        ui.label(RichText::new("Gesundheit").color(Palette::INK_SOFT));
        let text = if d.sick { format!("{} · krank", d.health) } else { d.health.to_string() };
        progress_bar(ui, f32::from(d.health) / 100.0, rating_color(f32::from(d.health)), &text);
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new("Zufriedenheit").color(Palette::INK_SOFT));
        progress_bar(ui, f32::from(d.happiness) / 100.0, rating_color(f32::from(d.happiness)), &d.happiness.to_string());
    });
    if !d.happiness_factors.is_empty() {
        egui::Grid::new(("citizen_happiness", d.citizen.index())).num_columns(2).spacing([12.0, 1.0]).show(ui, |ui| {
            for f in &d.happiness_factors {
                ui.label(RichText::new(&f.label).small().color(Palette::INK_SOFT));
                let color = if f.points < 0.0 { Palette::BAD } else { Palette::INK };
                let sign = if f.points > 0.0 { "+" } else { "" };
                ui.label(RichText::new(format!("{sign}{}", decimal(f64::from(f.points), 1))).small().color(color));
                ui.end_row();
            }
        });
    }
    if !d.needs.is_empty() {
        section_header(ui, "Bedürfnisse");
        egui::Grid::new(("citizen_needs", d.citizen.index())).num_columns(4).spacing([6.0, 3.0]).show(ui, |ui| {
            for (i, need) in d.needs.iter().enumerate() {
                ui.label(RichText::new(&need.label).small().color(Palette::INK));
                mini_bar(ui, f32::from(need.average) / 100.0, rating_color(f32::from(need.average)), 58.0)
                    .on_hover_text(format!("{}: {} von 100", need.label, need.average));
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
    }
}

/// Room for the window's content between its top `y` and `bottom` (the frame's margins and
/// border take the rest); never less than 240 points.
fn window_max_height(y: f32, bottom: f32) -> f32 {
    const FRAME: f32 = 2.0 * 12.0 + 2.0 * 2.0;
    (bottom - y - FRAME).max(240.0)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use kingdoms_sim::defs::Defs;
    use kingdoms_sim::state::{GameSettings, GameState};
    use kingdoms_sim::world::WorldSize;

    use super::*;

    fn detail() -> CitizenDetail {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        let defs = match Defs::load(&dir) {
            Ok(defs) => Arc::new(defs),
            Err(e) => panic!("shipped game data must load: {e}"),
        };
        let state = GameState::new(&defs, GameSettings::new(&defs, 7, WorldSize::Small));
        let Some((id, _)) = state.population().citizens().next() else { panic!("no settlers") };
        match report::citizen_detail(&state, &defs, id) {
            Some(d) => d,
            None => panic!("no detail"),
        }
    }

    #[test]
    fn the_window_ends_above_the_bottom_bar() {
        // 1600 × 900: the bottom bar starts at about y = 840.
        let (y, bottom) = (TOP_BAR_HEIGHT + MARGIN, 840.0 - MARGIN);
        let height = window_max_height(y, bottom);
        assert!(y + height + 28.0 <= bottom, "{height}");
        assert_eq!(window_max_height(600.0, 700.0), 240.0, "a minimum on tiny screens");
    }

    #[test]
    fn relatives_are_named_by_their_sex() {
        let friends = ["Freund", "Freundin", "Freunde", "Freundinnen"];
        assert_eq!(kin_label(&[Sex::Male], friends), "Freund");
        assert_eq!(kin_label(&[Sex::Female], friends), "Freundin");
        assert_eq!(kin_label(&[Sex::Female, Sex::Male], friends), "Freunde");
        assert_eq!(kin_label(&[Sex::Female, Sex::Female], friends), "Freundinnen");
        assert_eq!(kin_label(&[], friends), "Freunde");
        assert_eq!(kin_label(&[Sex::Female], ["Ehemann", "Ehefrau", "Ehepartner", "Ehepartner"]), "Ehefrau");
        assert_eq!(kin_label(&[Sex::Male, Sex::Male], ["Sohn", "Tochter", "Kinder", "Töchter"]), "Kinder");
    }

    #[test]
    fn activity_notes_explain_sick_and_festival() {
        let mut d = detail();
        d.activity = Activity::Festival;
        assert_eq!(activity_note(&d).0, Palette::GOOD);
        d.activity = Activity::Sick;
        assert!(activity_note(&d).1.is_some_and(|t| t.starts_with("Krank")));
        d.activity = Activity::Working;
        d.sick = false;
        assert_eq!(activity_note(&d), (Palette::INK, None));
    }

    #[test]
    fn sick_leave_and_training_are_shown() {
        let mut d = detail();
        // The start settlers are trained; a job shows its training text.
        if let Some(t) = d.training.as_ref() {
            assert!(!t.text.is_empty());
            assert_eq!(training_color(t.percent), if t.percent < 100 { Palette::WARN } else { Palette::INK });
        }
        assert_eq!(training_color(86), Palette::WARN);
        d.job = None;
        d.sick_leave = false;
        assert_eq!(no_job_text(&d), ("keine", Palette::INK_SOFT));
        d.sick_leave = true;
        assert_eq!(no_job_text(&d).0, "Krankgeschrieben – sucht nach der Genesung neue Arbeit");
    }

    #[test]
    fn whereabouts_and_destination_texts() {
        let mut d = detail();
        d.inside = None;
        d.destination = None;
        let tile = d.position.tile();
        assert_eq!(whereabouts(&d, None), format!("Draußen bei Feld {}, {}", tile.x, tile.y));
        assert_eq!(destination_line(&d), None);
        if let Some(home) = d.home {
            d.inside = Some(home);
            assert_eq!(whereabouts(&d, Some("Einfaches Holzhaus")), "Im Gebäude: Einfaches Holzhaus");
            // Staying inside its destination: no destination line.
            d.destination = Some(Destination::Building { building: home, purpose: kingdoms_sim::population::Purpose::Home });
            assert_eq!(destination_line(&d), None);
            d.inside = None;
            d.destination_name = Some("Einfaches Holzhaus".into());
            assert_eq!(destination_line(&d).as_deref(), Some("Ziel: Einfaches Holzhaus"));
        }
        d.destination = Some(Destination::MapEdge(glam::IVec2::ZERO));
        assert_eq!(destination_line(&d).as_deref(), Some("Ziel: verlässt das Königreich"));
    }
}
