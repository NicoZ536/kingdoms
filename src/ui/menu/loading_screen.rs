//! The loading screen: title, a parchment card with the loading stages (done, running,
//! waiting), an optional progress bar, the elapsed time and a rotating hint about the controls.
//! Errors replace the stages with a German message and a way back to the main menu.

use kingdoms_engine::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Stroke, Vec2};
use kingdoms_engine::input::ActionMap;

use super::{diamond, page_button, paint_title, spinner, wood_backdrop};
use crate::actions::{Action, binding_label};
use crate::ui::theme::{self, Palette, heading_font};

/// State of one loading stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageState {
    Done,
    Active,
    Waiting,
}

/// What the loading screen shows this frame.
pub struct LoadingView<'a> {
    pub heading: &'a str,
    pub stages: Vec<(String, StageState)>,
    /// Progress of the running stage (0–1), if known.
    pub progress: Option<f32>,
    /// Seconds since loading started.
    pub elapsed: f64,
    /// German error message: loading failed.
    pub error: Option<&'a str>,
}

/// Seconds a hint stays.
const HINT_SECONDS: f64 = 7.0;

/// Hints about the controls, with the keys as currently bound.
fn hints(actions: &ActionMap<Action>) -> Vec<String> {
    let key = |a: Action| actions.bindings(a).first().map(binding_label).unwrap_or_else(|| "–".to_owned());
    vec![
        format!("Mit {} öffnet Ihr das Baumenü; {} dreht ein Gebäude vor dem Platzieren.", key(Action::ToggleBuildMenu), key(Action::Rotate)),
        format!(
            "{} hält das Spiel an, {} · {} · {} wählen das Tempo.",
            key(Action::TogglePause),
            key(Action::SpeedNormal),
            key(Action::SpeedDouble),
            key(Action::SpeedQuadruple)
        ),
        format!("{} speichert schnell, {} lädt den Schnellspielstand.", key(Action::QuickSave), key(Action::QuickLoad)),
        "Ein Klick auf einen Bewohner öffnet sein Bürgerfenster – mit „Folgen“ begleitet ihn die Kamera.".to_owned(),
        format!(
            "Die Reichsverwaltung zeigt Lager ({}), Produktion ({}), Finanzen ({}) und mehr.",
            key(Action::WindowStorage),
            key(Action::WindowProduction),
            key(Action::WindowFinance)
        ),
        "Straßen zieht Ihr mit gedrückter linker Maustaste; ein Rechtsklick bricht ab.".to_owned(),
        "In den Einstellungen lassen sich alle Tasten neu belegen.".to_owned(),
    ]
}

/// Draws the loading screen; returns true if the player asked to go back (after an error).
pub fn show(ctx: &egui::Context, view: &LoadingView<'_>, actions: &ActionMap<Action>) -> bool {
    let screen = ctx.content_rect();
    wood_backdrop(ctx, 1.0);
    let painter = ctx.layer_painter(egui::LayerId::background());
    // Thin gold frame inset from the screen edge.
    let frame = screen.shrink(14.0);
    painter.rect_stroke(frame, CornerRadius::same(4), Stroke::new(1.0, Palette::GOLD_DARK.gamma_multiply(0.7)), egui::StrokeKind::Inside);
    for corner in [frame.left_top(), frame.right_top(), frame.left_bottom(), frame.right_bottom()] {
        diamond(&painter, corner, 6.0, Palette::GOLD_DARK);
    }

    let scale = (screen.height() / 900.0).clamp(0.7, 1.1);
    let title_width = 74.0 * scale * 5.6;
    paint_title(&painter, Pos2::new(screen.center().x - title_width * 0.5, screen.top() + screen.height() * 0.12), scale);

    let mut back = false;
    let card = Vec2::new(520.0, 0.0);
    egui::Area::new(egui::Id::unique("loading_card"))
        .order(egui::Order::Middle)
        .anchor(Align2::CENTER_CENTER, Vec2::new(0.0, screen.height() * 0.08))
        .show(ctx, |ui| {
            theme::parchment_frame().inner_margin(egui::Margin::symmetric(26, 18)).show(ui, |ui| {
                ui.set_width(card.x);
                ui.label(egui::RichText::new(view.heading).font(heading_font(22.0)).color(Palette::INK));
                ui.add_space(6.0);
                if let Some(error) = view.error {
                    ui.horizontal_top(|ui| {
                        crate::ui::widgets::mark(ui, crate::ui::widgets::Mark::Error);
                        ui.add(egui::Label::new(egui::RichText::new(error).color(Palette::BAD)).wrap());
                    });
                    ui.add_space(12.0);
                    back = page_button(ui, "Zurück zum Hauptmenü", true, true).clicked() || super::escape_pressed(ui.ctx());
                    return;
                }
                for (text, state) in &view.stages {
                    stage_row(ui, text, *state, view.elapsed);
                }
                ui.add_space(8.0);
                progress_line(ui, view.progress, view.elapsed);
            });
        });

    let hints = hints(actions);
    if !hints.is_empty() && view.error.is_none() {
        let i = (view.elapsed / HINT_SECONDS) as usize % hints.len();
        let pos = Pos2::new(screen.center().x, screen.bottom() - 54.0);
        let font = FontId::proportional(16.0);
        painter.text(pos + Vec2::new(1.0, 1.5), Align2::CENTER_CENTER, &hints[i], font.clone(), Color32::from_black_alpha(160));
        painter.text(pos, Align2::CENTER_CENTER, &hints[i], font, Palette::ON_WOOD);
        painter.text(pos - Vec2::new(0.0, 24.0), Align2::CENTER_CENTER, "Hinweis", heading_font(13.0), Palette::GOLD);
    }
    back
}

fn stage_row(ui: &mut egui::Ui, text: &str, state: StageState, t: f64) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::hover());
        let painter = ui.painter();
        match state {
            StageState::Done => {
                painter.circle_filled(rect.center(), 7.5, Palette::GOOD);
                let c = rect.center();
                let white = Stroke::new(1.8, Color32::WHITE);
                painter.line_segment([c + Vec2::new(-3.5, 0.0), c + Vec2::new(-1.0, 3.0)], white);
                painter.line_segment([c + Vec2::new(-1.0, 3.0), c + Vec2::new(3.8, -3.2)], white);
            }
            StageState::Active => spinner(painter, rect.center(), 7.0, t),
            StageState::Waiting => {
                painter.circle_stroke(rect.center(), 6.0, Stroke::new(1.2, Palette::PARCHMENT_SHADE));
            }
        }
        let color = match state {
            StageState::Done => Palette::INK_SOFT,
            StageState::Active => Palette::INK,
            StageState::Waiting => Palette::PARCHMENT_SHADE,
        };
        let text = egui::RichText::new(text).color(color);
        ui.label(if state == StageState::Active { text.strong() } else { text });
    });
}

/// A progress bar (known progress) or a sweeping band (unknown), with the elapsed time.
fn progress_line(ui: &mut egui::Ui, progress: Option<f32>, t: f64) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 12.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(5), Palette::PARCHMENT_SHADE.gamma_multiply(0.55));
    match progress {
        Some(p) => {
            let p = p.clamp(0.0, 1.0);
            if p > 0.0 {
                let fill = Rect::from_min_size(rect.min, Vec2::new(rect.width() * p, rect.height()));
                painter.rect_filled(fill, CornerRadius::same(5), Palette::GOLD);
            }
        }
        None => {
            // A band sweeping back and forth.
            let w = rect.width() * 0.25;
            let phase = ((t * 0.7).fract() as f32 * 2.0 - 1.0).abs();
            let x = rect.left() + (rect.width() - w) * (1.0 - phase);
            painter.rect_filled(Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(w, rect.height())), CornerRadius::same(5), Palette::GOLD);
        }
    }
    painter.rect_stroke(rect, CornerRadius::same(5), Stroke::new(1.0, Palette::WOOD), egui::StrokeKind::Inside);
    ui.add_space(2.0);
    let seconds = t.max(0.0) as u64;
    let text = match progress {
        Some(p) => format!("{} % · {seconds} s", (p.clamp(0.0, 1.0) * 100.0).round() as u32),
        None => format!("{seconds} s"),
    };
    ui.label(egui::RichText::new(text).small().color(Palette::INK_SOFT));
}
