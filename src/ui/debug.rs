//! Developer debug overlay (F3): performance, renderer statistics, simulation state, hovered
//! tile, selection, recent events and the log.

use kingdoms_engine::core::app::EngineContext;
use kingdoms_engine::egui;
use kingdoms_sim::buildings::{Building, BuildingId};
use kingdoms_sim::grid::Direction;
use kingdoms_sim::world::{Tile, Vegetation};

use crate::session::Session;
use crate::view::picking::{Hover, Selection};
use crate::view::world_view::ViewStats;

/// Information the scene passes to the debug window.
pub struct DebugInfo<'a> {
    pub session: &'a Session,
    pub hover: Hover,
    pub selection: Option<Selection>,
    pub view: ViewStats,
    pub extra: Vec<(String, String)>,
}

/// Recent events listed in the window.
const RECENT_EVENTS: usize = 12;

pub fn show(ctx: &EngineContext, ui: &mut egui::Ui, info: &DebugInfo<'_>) {
    egui::Window::new("Debug (F3)")
        .default_pos(egui::pos2(100.0, 60.0))
        .default_width(340.0)
        .default_height(760.0)
        .vscroll(true)
        .resizable(true)
        .show(ui.ctx(), |ui| {
            let m = &ctx.metrics;
            ui.label(format!("{:.0} FPS   {:.2} ms   1% low {:.2} ms", m.fps(), m.average_frame_ms(), m.worst_frame_ms()));
            frame_graph(ui, m.frame_history().collect());
            timing(ctx, ui);
            people_and_daylight(ui, info);
            effects(ui, &info.view);
            simulation(ctx, ui, info.session);
            selection(ui, info);
            world(ui, info);
            renderer(ctx, ui, &info.view);
            events(ui, info.session);
            log(ctx, ui);
        });
}

fn timing(ctx: &EngineContext, ui: &mut egui::Ui) {
    let m = &ctx.metrics;
    egui::CollapsingHeader::new("Timing").default_open(true).show(ui, |ui| {
        egui::Grid::new("timing").striped(true).show(ui, |ui| {
            for (name, t) in m.sections() {
                ui.label(name);
                ui.label(format!("{:.2} ms", t.avg_ms));
                ui.label(format!("max {:.2}", t.peak_ms));
                ui.end_row();
            }
        });
        ui.label(format!(
            "Ticks: {} gesamt, {} in diesem Frame, verworfen {}",
            ctx.time.total_ticks(),
            ctx.time.ticks_this_frame(),
            ctx.time.dropped_ticks()
        ));
    });
}

fn people_and_daylight(ui: &mut egui::Ui, info: &DebugInfo<'_>) {
    egui::CollapsingHeader::new("Bewohner & Tageszeit").default_open(true).show(ui, |ui| {
        let view = &info.view;
        let c = view.citizens;
        let walking = kingdoms_sim::report::walking_summary(info.session.state());
        egui::Grid::new("people").striped(true).show(ui, |ui| {
            row(ui, "Läufer abgefragt / gezeichnet", format!("{} / {} ({} mit Ware)", c.queried, c.drawn, c.carrying));
            row(ui, "Detailstufe", if c.detailed { "voll (Laufbilder, Waren)".to_owned() } else { "einfach (weit herausgezoomt)".to_owned() });
            row(
                ui,
                "Unterwegs / wartet auf Weg / ohne Weg",
                format!("{} / {} / {}", walking.walking, walking.waiting_for_path, walking.no_path),
            );
            row(ui, "In Gebäuden / verlassen das Reich", format!("{} / {}", walking.inside, walking.leaving));
            if let Some(d) = view.daylight {
                let minutes = (d.hour.fract() * 60.0) as u32;
                row(ui, "Tageszeit", format!("{:02}:{minutes:02} · {}", d.hour.floor() as u32, d.phase_name()));
                row(
                    ui,
                    "Sonne",
                    format!("{} – {} ({:.1} h)", clock_text(d.sun.sunrise), clock_text(d.sun.sunset), d.sun.day_length()),
                );
                row(ui, "Tageslicht / Nacht", format!("{:.2} {:.2} {:.2} / {:.2}", d.light[0], d.light[1], d.light[2], d.night));
            }
            let a = view.ambient;
            row(ui, "Umgebungslicht (Wetter × Tag)", format!("{:.2} {:.2} {:.2}", a[0], a[1], a[2]));
            row(ui, "Beleuchtete Gebäude", format!("{} (Stärke {:.2})", view.lights.lit, view.lights.glow));
        });
    });
}

/// "05:30" for 5.5 hours.
fn clock_text(hours: f32) -> String {
    let minutes = (hours * 60.0).round() as u32;
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

fn effects(ui: &mut egui::Ui, view: &ViewStats) {
    egui::CollapsingHeader::new("Wetter & Partikel").default_open(true).show(ui, |ui| {
        let p = view.effects.particles;
        egui::Grid::new("effects").striped(true).show(ui, |ui| {
            row(ui, "Partikel", format!("{} (Emitter {}, neu {})", p.live, p.emitters, p.spawned));
            row(ui, "Verworfen (Obergrenze)", p.dropped.to_string());
            row(ui, "Rauch / Staub / Laubbäume", format!(
                "{} / {} / {}",
                view.effects.smoke_sources, view.effects.dust_sources, view.effects.leaf_trees
            ));
            if let Some(look) = view.look {
                row(ui, "Licht", format!("{:.2} {:.2} {:.2}", look.light[0], look.light[1], look.light[2]));
                row(ui, "Wind (Schwanken/Neigung/Drift)", format!(
                    "{:.2} / {:.1} px / {:.0}, {:.0} px/s",
                    look.sway, look.lean, look.wind.x, look.wind.y
                ));
                row(ui, "Schnee / Nässe", format!("{:.0} % / {:.0} %", look.snow * 100.0, look.wetness * 100.0));
                row(ui, "Regen / Schneefall", format!("{:.2} / {:.2}", look.rain, look.snowfall));
            }
        });
    });
}

fn renderer(ctx: &EngineContext, ui: &mut egui::Ui, view: &ViewStats) {
    egui::CollapsingHeader::new("Renderer").default_open(false).show(ui, |ui| {
        let s = ctx.renderer.stats();
        egui::Grid::new("render").striped(true).show(ui, |ui| {
            row(ui, "Draw Calls", s.draw_calls.to_string());
            row(ui, "Sprites (statisch)", s.sprites_static.to_string());
            row(ui, "Sprites (dynamisch)", s.sprites_dynamic.to_string());
            row(ui, "Batches sichtbar / gecullt", format!("{} / {}", s.static_batches_drawn, s.static_batches_culled));
            row(ui, "CPU-Culling", s.sprites_culled_cpu.to_string());
            row(ui, "Terrain-Sprites", view.terrain_sprites.to_string());
            row(ui, "Vegetation-Sprites", view.vegetation_sprites.to_string());
            row(ui, "Straßen-Sprites", view.road_sprites.to_string());
            row(ui, "Feld-Sprites", view.field_sprites.to_string());
            row(ui, "Weidetiere", view.animals_drawn.to_string());
            row(ui, "Geschosse in der Luft", view.projectiles_drawn.to_string());
            row(ui, "Läufer", view.citizens.drawn.to_string());
            let (ground, structures, fields) = view.rebuilt;
            row(ui, "Neu gebaut (Gelände/Bauten/Felder)", format!("{ground} / {structures} / {fields} Chunks"));
            row(ui, "Grenzsegmente", view.border_segments.to_string());
            row(ui, "Gebäude-Sprites", view.buildings_drawn.to_string());
            row(ui, "Texturen", s.textures.to_string());
            row(ui, "Instanzpuffer", format!("{} KiB", s.instance_buffer_bytes / 1024));
            row(ui, "MSAA", format!("{}x", ctx.renderer.sample_count()));
            row(ui, "Übersprungene Frames", s.frames_skipped.to_string());
        });
        let g = &ctx.gpu_info;
        ui.small(format!("{} · {} · {}", g.name, g.backend, g.device_type));
        ui.small(format!("{} · {}", g.surface_format, g.present_mode));
    });
}

fn simulation(ctx: &EngineContext, ui: &mut egui::Ui, session: &Session) {
    egui::CollapsingHeader::new("Simulation").default_open(true).show(ui, |ui| {
        let state = session.state();
        let clock = state.clock();
        let timing = session.tick_timing();
        let buildings = state.buildings();
        let sites = buildings.iter().filter(|(_, b)| !b.is_complete()).count();
        let workforce = state.workforce();
        egui::Grid::new("sim").striped(true).show(ui, |ui| {
            row(ui, "Datum", clock.date_string());
            row(ui, "Uhrzeit", format!("{} Uhr", clock.time_string()));
            row(ui, "Tick", format!("{} · Tempo {}", clock.tick, ctx.time.speed().label()));
            row(
                ui,
                "Tickzeit",
                format!("{:.3} ms (Ø {:.3}, max {:.3})", timing.last_ms(), timing.average_ms(), timing.max_ms()),
            );
            row(
                ui,
                "Bevölkerung",
                format!(
                    "{} ({} erwerbsfähig, {} beschäftigt)",
                    workforce.population(),
                    workforce.available(),
                    workforce.employed()
                ),
            );
            row(ui, "Gold", format!("{}", state.economy().treasury()));
            row(ui, "Gebäude", format!("{} (davon {sites} im Bau)", buildings.len()));
            row(ui, "Straßenfelder", state.roads().len().to_string());
            row(ui, "Gebiet", format!("{} Tiles", state.territory().owned_tiles()));
            row(ui, "Befehle wartend", session.queued_commands().len().to_string());
            row(ui, "Ereignisse", format!("{} im Log, {} gesamt", session.events().len(), session.events().total()));
        });
    });
}

fn world(ui: &mut egui::Ui, info: &DebugInfo<'_>) {
    egui::CollapsingHeader::new("Welt").default_open(true).show(ui, |ui| {
        for (k, v) in &info.extra {
            ui.label(format!("{k}: {v}"));
        }
        let map = info.session.state().world();
        ui.label(format!("Seed {} · {}×{} Tiles · Start {}", map.seed(), map.width(), map.height(), map.start));
        match info.hover.tile.and_then(|p| map.get(p).map(|t| (p, t))) {
            Some((p, t)) => tile_info(ui, "hover_tile", p, t),
            None => {
                ui.label("Maus über keinem Tile");
            }
        }
        if let Some(b) = info.hover.building.and_then(|id| info.session.state().building(id)) {
            let name = info.session.defs().buildings().get(b.kind().index()).map_or("?", |d| d.name.as_str());
            ui.label(format!("Gebäude unter der Maus: {name}"));
        }
        if let Some(name) = info.hover.citizen.and_then(|id| info.session.state().population().full_name(id)) {
            ui.label(format!("Bürger unter der Maus: {name}"));
        }
    });
}

fn selection(ui: &mut egui::Ui, info: &DebugInfo<'_>) {
    egui::CollapsingHeader::new("Auswahl").default_open(true).show(ui, |ui| {
        let state = info.session.state();
        match info.selection {
            None => {
                ui.label("Nichts ausgewählt (Linksklick wählt aus, Esc hebt auf)");
            }
            Some(Selection::Tile(p)) => match state.world().get(p) {
                Some(t) => tile_info(ui, "selected_tile", p, t),
                None => {
                    ui.label("Tile außerhalb der Karte");
                }
            },
            Some(Selection::Building(id)) => match state.building(id) {
                Some(b) => building_info(ui, info.session, id, b),
                None => {
                    ui.label("Gebäude existiert nicht mehr");
                }
            },
        }
    });
}

fn building_info(ui: &mut egui::Ui, session: &Session, id: BuildingId, b: &Building) {
    let defs = session.defs();
    let state = session.state();
    let name = defs.buildings().get(b.kind().index()).map_or("?", |d| d.name.as_str());
    egui::Grid::new("building").striped(true).show(ui, |ui| {
        row(ui, "Gebäude", format!("{name} ({id:?})"));
        let paused = if b.is_paused() { ", pausiert" } else { "" };
        row(ui, "Status", format!("{}{paused}", b.status().display_name()));
        row(ui, "Grundfläche", b.rect().to_string());
        row(ui, "Drehung", format!("{} · Eingang nach {} bei {}", b.rotation(), direction_name(b.facing()), b.entrance()));
        row(ui, "Straßenanschluss", if state.has_road_access(id) { "ja".into() } else { "nein".into() });
        row(ui, "Arbeitskräfte", format!("{} / {}", b.staffed(), b.workers_wanted(defs)));
        if let Some(site) = b.construction_site() {
            row(ui, "Baufortschritt", format!("{:.0} % (Stufe {})", site.progress() * 100.0, site.stage()));
        }
        if let Some(p) = b.production() {
            row(ui, "Produktion", p.status().display_name().to_owned());
        }
        let goods = b.all_goods();
        if !goods.is_empty() {
            let list: Vec<String> = goods
                .iter()
                .map(|g| format!("{} {}", g.amount, defs.resources().get(g.resource.index()).map_or("?", |r| r.name.as_str())))
                .collect();
            row(ui, "Inhalt", list.join(", "));
        }
    });
}

fn events(ui: &mut egui::Ui, session: &Session) {
    egui::CollapsingHeader::new("Ereignisse").default_open(false).show(ui, |ui| {
        for e in session.events().recent().take(RECENT_EVENTS) {
            let text = match e.event.notification() {
                Some(n) => n.text.clone(),
                None => format!("{:?}", e.event),
            };
            let text: String = text.chars().take(90).collect();
            ui.label(egui::RichText::new(format!("#{} t{}: {text}", e.seq, e.tick)).monospace().small());
        }
    });
}

fn log(ctx: &EngineContext, ui: &mut egui::Ui) {
    egui::CollapsingHeader::new("Log").default_open(false).show(ui, |ui| {
        egui::ScrollArea::vertical().max_height(160.0).stick_to_bottom(true).show(ui, |ui| {
            for line in ctx.log.recent(80) {
                let color = match line.level {
                    log::Level::Error => egui::Color32::from_rgb(230, 90, 80),
                    log::Level::Warn => egui::Color32::from_rgb(230, 190, 80),
                    _ => ui.visuals().text_color(),
                };
                ui.label(egui::RichText::new(line.text).monospace().small().color(color));
            }
        });
    });
}

/// German name of a map direction.
fn direction_name(d: Direction) -> &'static str {
    match d {
        Direction::North => "Norden",
        Direction::East => "Osten",
        Direction::South => "Süden",
        Direction::West => "Westen",
    }
}

fn row(ui: &mut egui::Ui, k: &str, v: String) {
    ui.label(k);
    ui.label(v);
    ui.end_row();
}

fn tile_info(ui: &mut egui::Ui, id: &str, p: glam::IVec2, t: &Tile) {
    egui::Grid::new(id).striped(true).show(ui, |ui| {
        row(ui, "Tile", format!("{}, {}", p.x, p.y));
        row(ui, "Terrain", t.terrain.display_name().to_owned());
        row(ui, "Biom", t.biome.display_name().to_owned());
        row(ui, "Höhe", t.elevation.to_string());
        row(ui, "Fruchtbarkeit", t.fertility.to_string());
        row(ui, "Feuchtigkeit", t.moisture.to_string());
        row(ui, "Temperatur", t.temperature.to_string());
        let veg = match t.vegetation {
            Vegetation::None => "–".to_owned(),
            Vegetation::Tree { kind, growth } => format!("Baum {kind:?} ({growth})"),
            Vegetation::Stump { hours } => format!("Baumstumpf ({hours} h)"),
            Vegetation::Bush => "Busch".to_owned(),
            Vegetation::Boulder => "Felsbrocken".to_owned(),
        };
        row(ui, "Bewuchs", veg);
        row(
            ui,
            "Vorkommen",
            t.deposit.map(|d| format!("{} ({})", d.kind.display_name(), d.amount)).unwrap_or_else(|| "–".into()),
        );
        row(ui, "Fluss", if t.river { "ja".into() } else { "nein".into() });
    });
}

fn frame_graph(ui: &mut egui::Ui, history: Vec<f32>) {
    let desired = egui::vec2(ui.available_width().max(200.0), 46.0);
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, egui::Color32::from_black_alpha(90));
    if history.len() < 2 {
        return;
    }
    let max_ms = history.iter().copied().fold(33.4f32, f32::max);
    let y_of = |ms: f32| rect.bottom() - (ms / max_ms).min(1.0) * rect.height();
    for target in [16.7f32, 33.3] {
        let y = y_of(target);
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(1.0, egui::Color32::from_white_alpha(30)),
        );
    }
    let n = history.len();
    let points: Vec<egui::Pos2> = history
        .iter()
        .enumerate()
        .map(|(i, ms)| egui::pos2(rect.left() + rect.width() * i as f32 / (n - 1) as f32, y_of(*ms)))
        .collect();
    painter.add(egui::Shape::line(points, egui::Stroke::new(1.5, egui::Color32::from_rgb(120, 200, 120))));
}
