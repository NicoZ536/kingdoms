//! Credits: what the game is built with – libraries with their licenses (as declared in the
//! crates' Cargo metadata), the fonts with their license files, and the honest state of graphics
//! and audio. No names are listed that the project's files do not name.

use kingdoms_engine::egui::{self, RichText, Ui};

use super::{page_button, page_title};
use crate::ui::theme::{Palette, heading_font};
use crate::ui::widgets::section_header;

/// Library, version, purpose, license (SPDX expression from the crate's Cargo metadata).
const LIBRARIES: &[(&str, &str, &str, &str)] = &[
    ("wgpu", "30", "Grafikschnittstelle (Vulkan, Metal, DirectX 12)", "MIT OR Apache-2.0"),
    ("naga", "30", "Shader-Übersetzung (Teil von wgpu)", "MIT OR Apache-2.0"),
    ("winit", "0.30", "Fenster und Eingabe", "Apache-2.0"),
    ("egui, egui-wgpu, egui-winit", "0.37", "Benutzeroberfläche", "MIT OR Apache-2.0"),
    ("ab_glyph", "0.2", "Schriftdarstellung (über egui)", "Apache-2.0"),
    ("glam", "0.34", "Vektor- und Matrixrechnung", "MIT OR Apache-2.0"),
    ("image, png", "0.25, 0.18", "Laden der Grafik-Atlanten", "MIT OR Apache-2.0"),
    ("rodio", "0.23", "Audiowiedergabe und Mischung", "MIT OR Apache-2.0"),
    ("cpal", "0.18", "Audiogeräte", "Apache-2.0"),
    ("symphonia", "0.5", "Audiodekodierung", "MPL-2.0"),
    ("serde", "1", "Serialisierung", "MIT OR Apache-2.0"),
    ("ron", "0.12", "Spieldaten, Einstellungen, Atlanten", "MIT OR Apache-2.0"),
    ("bincode", "2", "Spielstand-Format", "MIT"),
    ("crc32fast", "1.5", "Prüfsummen der Spielstände", "MIT OR Apache-2.0"),
    ("rand, rand_chacha", "0.10", "Deterministischer Zufall der Simulation", "MIT OR Apache-2.0"),
    ("libm", "0.2", "Plattformunabhängige Mathematik der Weltgenerierung", "MIT"),
    ("jiff", "1.0", "Ortszeit in der Spielstandliste", "Unlicense OR MIT"),
    ("bytemuck", "1", "GPU-Datenlayout", "Zlib OR Apache-2.0 OR MIT"),
    ("pollster", "1", "Start der Grafikschnittstelle", "MIT OR Apache-2.0"),
    ("uuid", "1", "Spielkennung in den Spielständen", "Apache-2.0 OR MIT"),
    ("log, env_logger", "0.4, 0.11", "Protokoll", "MIT OR Apache-2.0"),
    ("anyhow, thiserror", "1, 2", "Fehlerbehandlung", "MIT OR Apache-2.0"),
];

/// Draws the credits page content; returns true when the player wants to go back.
pub fn show(ui: &mut Ui) -> bool {
    page_title(ui, "Credits", Some(concat!("Version ", env!("CARGO_PKG_VERSION"))));
    let footer = 46.0;
    let height = (ui.available_height() - footer).max(80.0);
    egui::ScrollArea::vertical().max_height(height).min_scrolled_height(height).auto_shrink([false, false]).show(ui, |ui| {
        ui.set_width(ui.available_width() - 12.0);
        ui.label(RichText::new("KINGDOMS – Rise of an Empire").font(heading_font(20.0)).color(Palette::INK));
        ui.label(
            "Ein mittelalterliches Städtebau- und Königreich-Simulationsspiel, vollständig in Rust geschrieben – \
             mit eigener Engine und einer deterministischen Simulation, ohne fertige Spiel-Engine.",
        );
        section_header(ui, "Technik");
        ui.label("Engine (kingdoms_engine), Simulation (kingdoms_sim), Spiel und Grafik-Generator sind Teil dieses Projekts.");
        ui.add_space(4.0);
        egui::Grid::new("credits_libraries").num_columns(3).striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
            for title in ["Bibliothek", "Aufgabe", "Lizenz"] {
                ui.label(RichText::new(title).strong().color(Palette::INK_SOFT));
            }
            ui.end_row();
            for (name, version, purpose, license) in LIBRARIES {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(*name).strong());
                    ui.label(RichText::new(*version).color(Palette::INK_SOFT));
                });
                ui.label(*purpose);
                ui.label(RichText::new(*license).color(Palette::INK_SOFT));
                ui.end_row();
            }
        });
        ui.add_space(2.0);
        ui.label(
            RichText::new("Dazu die Rust-Standardbibliothek und weitere, indirekt genutzte Bibliotheken; ihre Lizenzen nennt `cargo metadata`.")
                .small()
                .color(Palette::INK_SOFT),
        );
        section_header(ui, "Schriften");
        ui.label(RichText::new("Cinzel").strong());
        ui.label("Überschriften. Gestaltet von Natanael Gama; Copyright 2020 The Cinzel Project Authors. SIL Open Font License 1.1 (assets/fonts/Cinzel-OFL.txt).");
        ui.add_space(4.0);
        ui.label(RichText::new("Alegreya").strong());
        ui.label("Fließtext. Gestaltet von Huerta Tipográfica; Copyright 2011 The Alegreya Project Authors. SIL Open Font License 1.1 (assets/fonts/Alegreya-OFL.txt).");
        section_header(ui, "Grafik");
        ui.label("Prozedural erzeugte Platzhalter-Grafik (tools/assetgen): Gelände, Gebäude, Bewohner, Symbole und Effekte entstehen deterministisch aus Code. Finale Grafiken ersetzen sie später unter denselben Namen.");
        section_header(ui, "Audio");
        ui.label("Das Klangsystem ist vollständig angebunden (getrennte Kanäle, Musikstimmungen, Umgebungsklänge, Sprachausgabe), es liegen aber noch keine Audiodateien bei – das Spiel ist derzeit stumm.");
        ui.add_space(8.0);
    });
    ui.add_space(6.0);
    let mut back = false;
    ui.horizontal(|ui| {
        back = page_button(ui, "Zurück", true, true).clicked();
    });
    back || super::escape_pressed(ui.ctx())
}
