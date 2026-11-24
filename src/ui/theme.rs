//! Visual theme: medieval parchment, dark wood and subtle gold accents with modern legibility.
//!
//! Windows and cards are parchment with dark ink; HUD bars are dark wood with parchment text.
//! Fonts: Cinzel (headings, OFL) and Alegreya (body, OFL) from `assets/fonts`, falling back to
//! egui's built-in fonts when the files are missing.

use std::path::Path;
use std::sync::Arc;

use kingdoms_engine::egui;
use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Shadow, Stroke, TextStyle, Visuals};

/// Smallest screen in UI points the HUD is laid out for: a UI scale that would leave fewer
/// points on the window is lowered by the engine ([`EngineContext::set_ui_min_size`]).
///
/// [`EngineContext::set_ui_min_size`]: kingdoms_engine::core::app::EngineContext::set_ui_min_size
pub const LAYOUT_MIN_SIZE: glam::Vec2 = glam::Vec2::new(1280.0, 720.0);

/// Theme colours (sRGB).
pub struct Palette;

impl Palette {
    pub const PARCHMENT: Color32 = Color32::from_rgb(236, 222, 186);
    pub const PARCHMENT_LIGHT: Color32 = Color32::from_rgb(246, 236, 210);
    pub const PARCHMENT_DARK: Color32 = Color32::from_rgb(214, 194, 150);
    pub const PARCHMENT_SHADE: Color32 = Color32::from_rgb(190, 166, 120);
    pub const INK: Color32 = Color32::from_rgb(52, 36, 22);
    pub const INK_SOFT: Color32 = Color32::from_rgb(105, 82, 58);
    pub const WOOD_DARK: Color32 = Color32::from_rgb(43, 30, 20);
    pub const WOOD: Color32 = Color32::from_rgb(68, 47, 30);
    pub const WOOD_LIGHT: Color32 = Color32::from_rgb(98, 70, 44);
    pub const GOLD: Color32 = Color32::from_rgb(200, 160, 75);
    pub const GOLD_BRIGHT: Color32 = Color32::from_rgb(236, 196, 104);
    pub const GOLD_DARK: Color32 = Color32::from_rgb(140, 106, 40);
    pub const GOOD: Color32 = Color32::from_rgb(78, 140, 60);
    pub const GOOD_LIGHT: Color32 = Color32::from_rgb(140, 200, 110);
    pub const WARN: Color32 = Color32::from_rgb(196, 132, 32);
    pub const BAD: Color32 = Color32::from_rgb(170, 52, 40);
    pub const BAD_LIGHT: Color32 = Color32::from_rgb(232, 110, 90);
    pub const INFO: Color32 = Color32::from_rgb(64, 104, 150);
    /// Text on wood panels.
    pub const ON_WOOD: Color32 = Color32::from_rgb(240, 226, 192);
    pub const ON_WOOD_SOFT: Color32 = Color32::from_rgb(196, 176, 140);
    /// Frosty text on wood panels (temperatures at or below 0 °C).
    pub const FROST: Color32 = Color32::from_rgb(170, 208, 240);
}

/// Colours of the AI kingdoms by their colour index (`Kingdom::color`, 0–6): fields of their
/// crests (`icons/crest_<i>`), their borders, banners and minimap territories. The same values
/// are painted by the asset generator (`tools/assetgen/src/palette.rs`, `KINGDOM_COLORS`).
pub const KINGDOM_COLORS: [[u8; 3]; 7] = [
    [176, 38, 48],
    [44, 86, 172],
    [48, 124, 64],
    [118, 58, 146],
    [212, 124, 34],
    [24, 132, 142],
    [196, 72, 132],
];

/// The player's colour (banners of own soldiers, own territory on the minimap).
pub const PLAYER_COLOR: [u8; 3] = [236, 186, 72];

/// Colour of bandits (the ring under their feet, their banner): bone white, which stands out on
/// dark ground and grass and differs from every kingdom and the player.
pub const BANDIT_COLOR: [u8; 3] = [228, 220, 198];

/// UI colour of kingdom colour index `index` (indices beyond the palette wrap around).
pub fn kingdom_color(index: u8) -> Color32 {
    let [r, g, b] = KINGDOM_COLORS[usize::from(index) % KINGDOM_COLORS.len()];
    Color32::from_rgb(r, g, b)
}

/// Font family used for headings (falls back to proportional when not installed).
pub fn heading_family() -> FontFamily {
    FontFamily::Name("heading".into())
}

pub fn heading_font(size: f32) -> FontId {
    FontId::new(size, heading_family())
}

/// Loads the theme fonts from `fonts_dir` (missing files are logged, defaults remain usable).
pub fn install_fonts(ctx: &egui::Context, fonts_dir: &Path) {
    let mut fonts = FontDefinitions::default();
    let mut load = |name: &str, file: &str| -> bool {
        match std::fs::read(fonts_dir.join(file)) {
            Ok(bytes) => {
                fonts.font_data.insert(name.to_owned(), Arc::new(FontData::from_owned(bytes)));
                true
            }
            Err(err) => {
                log::warn!("font {} not loaded ({err}); using built-in font", fonts_dir.join(file).display());
                false
            }
        }
    };
    let has_body = load("alegreya", "Alegreya.ttf");
    let has_heading = load("cinzel", "Cinzel.ttf");
    if has_body {
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "alegreya".to_owned());
    }
    let mut heading = Vec::new();
    if has_heading {
        heading.push("cinzel".to_owned());
    }
    // Fall back to the proportional stack (covers symbols / missing glyphs).
    heading.extend(fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default());
    fonts.families.insert(heading_family(), heading);
    ctx.set_fonts(fonts);
}

/// Applies the global style (spacing, text sizes, colours) to `ctx`.
pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Light);
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, heading_font(22.0)),
            (TextStyle::Body, FontId::proportional(16.0)),
            (TextStyle::Button, FontId::proportional(16.0)),
            (TextStyle::Small, FontId::proportional(13.0)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        let s = &mut style.spacing;
        s.item_spacing = egui::vec2(8.0, 6.0);
        s.button_padding = egui::vec2(10.0, 5.0);
        s.window_margin = Margin::same(12);
        s.interact_size.y = 26.0;
        s.slider_width = 160.0;
        s.scroll.bar_width = 8.0;
        style.visuals = visuals();
    });
}

fn visuals() -> Visuals {
    let mut v = Visuals::light();
    v.override_text_color = None;
    v.window_fill = Palette::PARCHMENT;
    v.panel_fill = Palette::PARCHMENT;
    v.extreme_bg_color = Palette::PARCHMENT_LIGHT;
    v.faint_bg_color = Palette::PARCHMENT_DARK;
    v.code_bg_color = Palette::PARCHMENT_DARK;
    v.window_stroke = Stroke::new(2.0, Palette::WOOD);
    v.window_corner_radius = CornerRadius::same(6);
    v.menu_corner_radius = CornerRadius::same(4);
    v.window_shadow = Shadow { offset: [3, 6], blur: 16, spread: 0, color: Color32::from_black_alpha(110) };
    v.popup_shadow = Shadow { offset: [2, 4], blur: 10, spread: 0, color: Color32::from_black_alpha(90) };
    v.hyperlink_color = Palette::INFO;
    v.selection.bg_fill = Palette::GOLD.gamma_multiply(0.75);
    v.selection.stroke = Stroke::new(1.0, Palette::INK);
    v.warn_fg_color = Palette::WARN;
    v.error_fg_color = Palette::BAD;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = Palette::PARCHMENT;
    w.noninteractive.weak_bg_fill = Palette::PARCHMENT_DARK;
    w.noninteractive.bg_stroke = Stroke::new(1.0, Palette::PARCHMENT_SHADE);
    w.noninteractive.fg_stroke = Stroke::new(1.0, Palette::INK);
    w.noninteractive.corner_radius = CornerRadius::same(4);

    w.inactive.bg_fill = Palette::PARCHMENT_DARK;
    w.inactive.weak_bg_fill = Palette::PARCHMENT_DARK;
    w.inactive.bg_stroke = Stroke::new(1.0, Palette::GOLD_DARK);
    w.inactive.fg_stroke = Stroke::new(1.0, Palette::INK);
    w.inactive.corner_radius = CornerRadius::same(4);

    w.hovered.bg_fill = Palette::GOLD_BRIGHT.gamma_multiply(0.85);
    w.hovered.weak_bg_fill = Palette::GOLD_BRIGHT.gamma_multiply(0.7);
    w.hovered.bg_stroke = Stroke::new(1.5, Palette::GOLD_DARK);
    w.hovered.fg_stroke = Stroke::new(1.5, Palette::INK);
    w.hovered.corner_radius = CornerRadius::same(4);

    w.active.bg_fill = Palette::GOLD;
    w.active.weak_bg_fill = Palette::GOLD;
    w.active.bg_stroke = Stroke::new(1.5, Palette::WOOD);
    w.active.fg_stroke = Stroke::new(2.0, Palette::INK);
    w.active.corner_radius = CornerRadius::same(4);

    w.open.bg_fill = Palette::PARCHMENT_DARK;
    w.open.weak_bg_fill = Palette::PARCHMENT_DARK;
    w.open.bg_stroke = Stroke::new(1.0, Palette::GOLD_DARK);
    w.open.fg_stroke = Stroke::new(1.0, Palette::INK);
    v
}

/// Frame for HUD bars and docks: dark wood with a gold trim line.
pub fn wood_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(Palette::WOOD_DARK.gamma_multiply(0.97))
        .stroke(Stroke::new(1.5, Palette::GOLD_DARK))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(10, 6))
        .shadow(Shadow { offset: [0, 3], blur: 10, spread: 0, color: Color32::from_black_alpha(120) })
}

/// Frame for windows, cards and tooltips: parchment with a wood border.
pub fn parchment_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(Palette::PARCHMENT)
        .stroke(Stroke::new(2.0, Palette::WOOD))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(12))
        .shadow(Shadow { offset: [3, 6], blur: 16, spread: 0, color: Color32::from_black_alpha(110) })
}

/// Inset area inside parchment (lists, tables).
pub fn inset_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(Palette::PARCHMENT_LIGHT)
        .stroke(Stroke::new(1.0, Palette::PARCHMENT_SHADE))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::same(8))
}

/// Switches text colours for content drawn on wood.
pub fn on_wood(ui: &mut egui::Ui) {
    ui.visuals_mut().override_text_color = Some(Palette::ON_WOOD);
    let w = &mut ui.visuals_mut().widgets;
    w.inactive.weak_bg_fill = Palette::WOOD;
    w.inactive.bg_fill = Palette::WOOD;
    w.inactive.fg_stroke = Stroke::new(1.0, Palette::ON_WOOD);
    w.inactive.bg_stroke = Stroke::new(1.0, Palette::GOLD_DARK);
    w.hovered.weak_bg_fill = Palette::WOOD_LIGHT;
    w.hovered.bg_fill = Palette::WOOD_LIGHT;
    w.hovered.fg_stroke = Stroke::new(1.5, Palette::GOLD_BRIGHT);
    w.hovered.bg_stroke = Stroke::new(1.5, Palette::GOLD);
    w.active.weak_bg_fill = Palette::GOLD_DARK;
    w.active.bg_fill = Palette::GOLD_DARK;
    w.active.fg_stroke = Stroke::new(1.5, Palette::ON_WOOD);
    w.noninteractive.fg_stroke = Stroke::new(1.0, Palette::ON_WOOD);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The crests and banners of the asset generator use the same kingdom colours as the UI.
    #[test]
    fn kingdom_colours_match_the_asset_generator() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tools/assetgen/src/palette.rs");
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let line = text.lines().find(|l| l.starts_with("pub const KINGDOM_COLORS")).unwrap_or_else(|| panic!("no KINGDOM_COLORS in palette.rs"));
        let values: Vec<[u8; 3]> = line
            .split("0x")
            .skip(1)
            .map(|v| {
                let hex = u32::from_str_radix(&v[..6], 16).unwrap_or_else(|e| panic!("{v}: {e}"));
                [(hex >> 16) as u8, (hex >> 8) as u8, hex as u8]
            })
            .collect();
        assert_eq!(values, KINGDOM_COLORS.to_vec());
        assert_eq!(kingdom_color(7), kingdom_color(0), "indices wrap around");
    }
}
