//! Icons for the HUD: sprites of the `icons` atlas (`res_<resource>`, `bld_<building>`,
//! `ui_<name>`), road previews from the `roads` atlas and field stages from the `fields` atlas
//! (`field_<sprite>`), registered once as egui textures.

use std::collections::BTreeMap;

use kingdoms_engine::assets::AssetManager;
use kingdoms_engine::egui;
use kingdoms_engine::rendering::Renderer;
use kingdoms_sim::defs::Defs;

use crate::ui::widgets::IconRef;

/// `ui_<name>` icons the HUD uses.
const UI_ICONS: &[&str] = &[
    "population", "gold", "happiness", "food", "wood", "stone", "iron", "date", "season_spring", "season_summer",
    "season_autumn", "season_winter", "pause", "play", "speed2", "speed4", "build", "roads", "farm", "production",
    "military", "research", "events", "notifications", "tasks", "overview", "demolish", "upgrade", "rotate", "move",
    "info", "close", "warning", "trade", "diplomacy", "finance", "statistics", "storage", "settings", "save", "load",
    "inspect", "grid", "territory", "economy", "medicine", "administration", "rank",
];

/// Icon lookup by short name (`"res_wood"`, `"bld_sawmill"`, `"ui_gold"`, `"road_gravel"`).
#[derive(Default)]
pub struct Icons {
    map: BTreeMap<String, IconRef>,
}

impl Icons {
    /// Registers the atlas textures with egui and resolves every icon the HUD can show.
    /// Missing atlases or sprites leave the icon out (widgets then draw a fallback).
    pub fn load(renderer: &mut Renderer, assets: &AssetManager, defs: &Defs) -> Self {
        let mut icons = Self::default();
        let mut names: Vec<String> = UI_ICONS.iter().map(|n| format!("ui_{n}")).collect();
        names.extend(defs.resources().iter().map(|r| format!("res_{}", r.key)));
        names.extend(defs.buildings().iter().map(|b| format!("bld_{}", b.key)));
        names.extend((0..super::heraldry::CRESTS).map(super::heraldry::crest_icon));
        icons.register(renderer, assets, "icons", names.iter().map(|n| (n.clone(), format!("icons/{n}"))));
        // Roads: the straight east-west piece (mask E|W = 10) as a preview of each type.
        let roads = defs.roads().iter().map(|r| {
            let sprite = if r.bridge.is_some() { "roads/bridge_x".to_owned() } else { r.sprite(10) };
            (format!("road_{}", r.key), sprite)
        });
        icons.register(renderer, assets, "roads", roads.collect::<Vec<_>>().into_iter());
        // Field stages of every crop plus fallow soil and pasture (farm panel).
        let mut fields: Vec<String> = vec!["soil".to_owned(), "pasture".to_owned()];
        for crop in defs.crops() {
            fields.extend((0..=4).map(|stage| format!("{}_{stage}", crop.sprite)));
        }
        fields.dedup();
        icons.register(renderer, assets, "fields", fields.into_iter().map(|f| (format!("field_{f}"), format!("fields/{f}"))));
        log::info!("HUD icons: {} resolved", icons.map.len());
        icons
    }

    fn register(
        &mut self,
        renderer: &mut Renderer,
        assets: &AssetManager,
        atlas: &str,
        sprites: impl Iterator<Item = (String, String)>,
    ) {
        let Some(texture) = assets.atlas_texture(atlas) else {
            log::warn!("HUD icons: atlas '{atlas}' is not loaded");
            return;
        };
        let Some(egui_texture) = renderer.register_egui_texture(texture) else {
            log::warn!("HUD icons: atlas '{atlas}' cannot be used by the UI");
            return;
        };
        for (name, sprite) in sprites {
            let Some(id) = assets.try_sprite(&sprite) else {
                log::warn!("HUD icon sprite '{sprite}' is missing");
                continue;
            };
            let region = assets.region(id);
            if region.texture != texture {
                continue;
            }
            let [u0, v0, u1, v1] = region.uv;
            let uv = egui::Rect::from_min_max(egui::pos2(u0, v0), egui::pos2(u1, v1));
            let aspect = region.size.x / region.size.y.max(1.0);
            self.map.insert(name, IconRef { texture: egui_texture, uv, aspect });
        }
    }

    /// The icon with short name `name`.
    pub fn get(&self, name: &str) -> Option<IconRef> {
        self.map.get(name).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `ui_*` icon the HUD asks for exists in the generated `icons` atlas.
    #[test]
    fn ui_icons_exist_in_the_atlas() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/atlases/icons.ron");
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => panic!("{}: {e}", path.display()),
        };
        for name in UI_ICONS {
            assert!(text.contains(&format!("\"ui_{name}\"")), "ui_{name} is missing in icons.ron");
        }
        for i in 0..super::super::heraldry::CRESTS {
            let name = super::super::heraldry::crest_icon(i);
            assert!(text.contains(&format!("\"{name}\"")), "{name} is missing in icons.ron");
        }
    }
}
