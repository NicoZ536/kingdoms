//! Game actions, their default key bindings, German names for the settings and conflict
//! detection for rebinding.

use kingdoms_engine::input::{ActionMap, Binding, KeyCode, Modifiers, MouseButton};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Action {
    PanUp,
    PanDown,
    PanLeft,
    PanRight,
    PanDrag,
    ZoomIn,
    ZoomOut,
    TogglePause,
    SpeedNormal,
    SpeedDouble,
    SpeedQuadruple,
    ToggleDebug,
    ToggleGrid,
    ToggleTerritory,
    Select,
    Screenshot,
    CenterHome,
    Cancel,
    /// Right-click on the map: cancels the tool step (a click, not a camera drag).
    ToolCancel,
    ToggleBuildMenu,
    DemolishTool,
    MoveTool,
    Rotate,
    QuickSave,
    QuickLoad,
    /// Management window tabs (the key of the shown tab closes the window).
    WindowEconomy,
    WindowStorage,
    WindowProduction,
    WindowFinance,
    WindowTrade,
    WindowDiplomacy,
    WindowMilitary,
    WindowWorkforce,
    WindowPopulation,
    WindowAgriculture,
    WindowResearch,
    WindowStatistics,
    /// Shows or hides the decision dialog of the pending event (otherwise the events panel).
    ToggleEvents,
}

/// Sections of the key binding list in the settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ActionGroup {
    Camera,
    Time,
    Map,
    Tools,
    Windows,
    System,
}

impl ActionGroup {
    pub const ALL: [ActionGroup; 6] =
        [ActionGroup::Camera, ActionGroup::Time, ActionGroup::Map, ActionGroup::Tools, ActionGroup::Windows, ActionGroup::System];

    /// German section title.
    pub const fn title(self) -> &'static str {
        match self {
            ActionGroup::Camera => "Kamera",
            ActionGroup::Time => "Zeit",
            ActionGroup::Map => "Karte",
            ActionGroup::Tools => "Werkzeuge",
            ActionGroup::Windows => "Reichsverwaltung",
            ActionGroup::System => "Spiel",
        }
    }
}

impl Action {
    /// Every action in the order of the settings list.
    pub const ALL: [Action; 38] = [
        Action::PanUp,
        Action::PanDown,
        Action::PanLeft,
        Action::PanRight,
        Action::PanDrag,
        Action::ZoomIn,
        Action::ZoomOut,
        Action::CenterHome,
        Action::TogglePause,
        Action::SpeedNormal,
        Action::SpeedDouble,
        Action::SpeedQuadruple,
        Action::Select,
        Action::ToggleGrid,
        Action::ToggleTerritory,
        Action::ToggleBuildMenu,
        Action::DemolishTool,
        Action::MoveTool,
        Action::Rotate,
        Action::ToolCancel,
        Action::Cancel,
        Action::ToggleEvents,
        Action::WindowEconomy,
        Action::WindowStorage,
        Action::WindowProduction,
        Action::WindowFinance,
        Action::WindowTrade,
        Action::WindowDiplomacy,
        Action::WindowMilitary,
        Action::WindowWorkforce,
        Action::WindowPopulation,
        Action::WindowAgriculture,
        Action::WindowResearch,
        Action::WindowStatistics,
        Action::QuickSave,
        Action::QuickLoad,
        Action::Screenshot,
        Action::ToggleDebug,
    ];

    /// German name in the key binding list.
    pub const fn label(self) -> &'static str {
        match self {
            Action::PanUp => "Ansicht nach oben",
            Action::PanDown => "Ansicht nach unten",
            Action::PanLeft => "Ansicht nach links",
            Action::PanRight => "Ansicht nach rechts",
            Action::PanDrag => "Ansicht ziehen",
            Action::ZoomIn => "Hineinzoomen",
            Action::ZoomOut => "Herauszoomen",
            Action::CenterHome => "Zum Hauptquartier",
            Action::TogglePause => "Pause / weiter",
            Action::SpeedNormal => "Normales Tempo",
            Action::SpeedDouble => "Doppeltes Tempo",
            Action::SpeedQuadruple => "Vierfaches Tempo",
            Action::Select => "Auswählen / bauen",
            Action::ToggleGrid => "Raster ein/aus",
            Action::ToggleTerritory => "Gebietsgrenze ein/aus",
            Action::ToggleBuildMenu => "Baumenü",
            Action::DemolishTool => "Abriss-Werkzeug",
            Action::MoveTool => "Versetzen",
            Action::Rotate => "Gebäude drehen",
            Action::ToolCancel => "Werkzeugschritt abbrechen",
            Action::Cancel => "Zurück / Menü",
            Action::ToggleEvents => "Ereignis-Entscheidung",
            Action::WindowEconomy => "Wirtschaft",
            Action::WindowStorage => "Lager & Waren",
            Action::WindowProduction => "Produktion",
            Action::WindowFinance => "Finanzen",
            Action::WindowTrade => "Handel",
            Action::WindowDiplomacy => "Diplomatie",
            Action::WindowMilitary => "Militär",
            Action::WindowWorkforce => "Arbeitskräfte",
            Action::WindowPopulation => "Bevölkerung",
            Action::WindowAgriculture => "Landwirtschaft & Wetter",
            Action::WindowResearch => "Forschung",
            Action::WindowStatistics => "Statistik",
            Action::QuickSave => "Schnellspeichern",
            Action::QuickLoad => "Schnellladen",
            Action::Screenshot => "Bildschirmfoto",
            Action::ToggleDebug => "Debug-Fenster",
        }
    }

    pub const fn group(self) -> ActionGroup {
        match self {
            Action::PanUp
            | Action::PanDown
            | Action::PanLeft
            | Action::PanRight
            | Action::PanDrag
            | Action::ZoomIn
            | Action::ZoomOut
            | Action::CenterHome => ActionGroup::Camera,
            Action::TogglePause | Action::SpeedNormal | Action::SpeedDouble | Action::SpeedQuadruple => ActionGroup::Time,
            Action::Select | Action::ToggleGrid | Action::ToggleTerritory => ActionGroup::Map,
            Action::ToggleBuildMenu | Action::DemolishTool | Action::MoveTool | Action::Rotate | Action::ToolCancel => {
                ActionGroup::Tools
            }
            Action::ToggleEvents
            | Action::WindowEconomy
            | Action::WindowStorage
            | Action::WindowProduction
            | Action::WindowFinance
            | Action::WindowTrade
            | Action::WindowDiplomacy
            | Action::WindowMilitary
            | Action::WindowWorkforce
            | Action::WindowPopulation
            | Action::WindowAgriculture
            | Action::WindowResearch
            | Action::WindowStatistics => ActionGroup::Windows,
            Action::Cancel | Action::QuickSave | Action::QuickLoad | Action::Screenshot | Action::ToggleDebug => {
                ActionGroup::System
            }
        }
    }

    /// Keyboard actions the player may rebind. Mouse actions and Esc (which also closes every
    /// dialog and cancels the "press a key" prompt) stay fixed.
    pub const fn rebindable(self) -> bool {
        !matches!(self, Action::PanDrag | Action::Select | Action::ToolCancel | Action::Cancel)
    }
}

/// Most key bindings an action can have.
pub const MAX_BINDINGS: usize = 2;

/// True if `a` and `b` would fire on the same key press: the same key, and either one ignores
/// modifiers or both require the same ones.
pub fn bindings_overlap(a: &Binding, b: &Binding) -> bool {
    bindings_overlap_on(a, b, kingdoms_engine::input::COMMAND_BLOCKS_PLAIN_KEYS)
}

/// [`bindings_overlap`] on a platform where Command blocks bindings without it
/// (`command_blocks`, macOS): there a Command combination never overlaps a binding without
/// Command.
pub fn bindings_overlap_on(a: &Binding, b: &Binding, command_blocks: bool) -> bool {
    fn key_and_mods(b: &Binding) -> Option<(KeyCode, Modifiers)> {
        match *b {
            Binding::Key(k) => Some((k, Modifiers::NONE)),
            Binding::KeyWith(k, m) => Some((k, m)),
            Binding::Mouse(_) => None,
        }
    }
    match (a, b) {
        (Binding::Mouse(x), Binding::Mouse(y)) => x == y,
        _ => match (key_and_mods(a), key_and_mods(b)) {
            (Some((ka, ma)), Some((kb, mb))) => {
                ka == kb && !(command_blocks && ma.logo != mb.logo) && (ma == Modifiers::NONE || mb == Modifiers::NONE || ma == mb)
            }
            _ => false,
        },
    }
}

/// Other actions already bound to something that overlaps `binding` (see [`bindings_overlap`]).
/// Mouse buttons are shared on purpose (dragging versus clicking) and never conflict.
pub fn conflicts(map: &ActionMap<Action>, binding: &Binding, action: Action) -> Vec<Action> {
    if matches!(binding, Binding::Mouse(_)) {
        return Vec::new();
    }
    let mut found: Vec<Action> = Action::ALL
        .into_iter()
        .filter(|a| *a != action && map.bindings(*a).iter().any(|b| bindings_overlap(b, binding)))
        .collect();
    found.dedup();
    found
}

/// German label of a key (`Leertaste`, `Pfeil ↑`, `Entf`, `F5`, `W` …).
pub fn key_name(key: KeyCode) -> String {
    let fixed = match key {
        KeyCode::Space => "Leertaste",
        KeyCode::Escape => "Esc",
        KeyCode::Delete => "Entf",
        KeyCode::Insert => "Einfg",
        KeyCode::Home => "Pos1",
        KeyCode::End => "Ende",
        KeyCode::PageUp => "Bild ↑",
        KeyCode::PageDown => "Bild ↓",
        KeyCode::ArrowUp => "Pfeil ↑",
        KeyCode::ArrowDown => "Pfeil ↓",
        KeyCode::ArrowLeft => "Pfeil ←",
        KeyCode::ArrowRight => "Pfeil →",
        KeyCode::Enter => "Eingabe",
        KeyCode::NumpadEnter => "Num Eingabe",
        KeyCode::Backspace => "Rücktaste",
        KeyCode::Tab => "Tab",
        KeyCode::CapsLock => "Feststell",
        KeyCode::Minus => "−",
        KeyCode::Equal => "+",
        KeyCode::NumpadAdd => "Num +",
        KeyCode::NumpadSubtract => "Num −",
        KeyCode::NumpadMultiply => "Num ×",
        KeyCode::NumpadDivide => "Num ÷",
        KeyCode::NumpadDecimal => "Num ,",
        KeyCode::Comma => ",",
        KeyCode::Period => ".",
        KeyCode::Slash => "/",
        KeyCode::Backslash => "\\",
        KeyCode::Semicolon => ";",
        KeyCode::Quote => "'",
        KeyCode::BracketLeft => "[",
        KeyCode::BracketRight => "]",
        KeyCode::Backquote => "^",
        KeyCode::IntlBackslash => "<",
        KeyCode::PrintScreen => "Druck",
        KeyCode::Pause => "Pause",
        _ => "",
    };
    if !fixed.is_empty() {
        return fixed.to_owned();
    }
    let raw = format!("{key:?}");
    if let Some(n) = raw.strip_prefix("Numpad") {
        return format!("Num {n}");
    }
    raw.strip_prefix("Key").or_else(|| raw.strip_prefix("Digit")).unwrap_or(&raw).to_owned()
}

/// German label of a binding (`Strg+S`, `Mittlere Maustaste` …).
pub fn binding_label(binding: &Binding) -> String {
    match binding {
        Binding::Key(k) => key_name(*k),
        Binding::KeyWith(k, m) => {
            let mut parts = Vec::new();
            // Mac keyboards label them „ctrl“ and „option“.
            let mac = cfg!(target_os = "macos");
            if m.ctrl {
                parts.push(if mac { "Ctrl" } else { "Strg" }.to_owned());
            }
            if m.alt {
                parts.push(if mac { "Option" } else { "Alt" }.to_owned());
            }
            if m.shift {
                parts.push("Shift".to_owned());
            }
            if m.logo {
                parts.push(if cfg!(target_os = "macos") { "Cmd" } else { "Super" }.to_owned());
            }
            parts.push(key_name(*k));
            parts.join("+")
        }
        Binding::Mouse(b) => match b {
            MouseButton::Left => "Linke Maustaste".to_owned(),
            MouseButton::Right => "Rechte Maustaste".to_owned(),
            MouseButton::Middle => "Mittlere Maustaste".to_owned(),
            other => format!("Maustaste {other:?}"),
        },
    }
}

/// Keys that cannot be bound on their own (modifiers, and Esc which is reserved).
pub fn bindable_key(key: KeyCode) -> bool {
    !matches!(
        key,
        KeyCode::ShiftLeft
            | KeyCode::ShiftRight
            | KeyCode::ControlLeft
            | KeyCode::ControlRight
            | KeyCode::AltLeft
            | KeyCode::AltRight
            | KeyCode::SuperLeft
            | KeyCode::SuperRight
            | KeyCode::Meta
            | KeyCode::Hyper
            | KeyCode::Fn
            | KeyCode::FnLock
            | KeyCode::Escape
            | KeyCode::ContextMenu
    )
}

pub fn default_bindings() -> ActionMap<Action> {
    default_bindings_for(cfg!(target_os = "macos"))
}

/// The default bindings; `mac` adds Command shortcuts for the F keys, which need the fn key on
/// Mac keyboards (Cmd+S quicksave, Cmd+L quickload, Cmd+P screenshot, Cmd+D debug overlay), and
/// the Mac's delete key (Backspace; forward delete needs fn) for demolishing.
pub fn default_bindings_for(mac: bool) -> ActionMap<Action> {
    use Action::*;
    let mut m = ActionMap::new();
    if mac {
        let cmd = Modifiers { logo: true, ..Modifiers::NONE };
        m.bind(QuickSave, Binding::KeyWith(KeyCode::KeyS, cmd))
            .bind(QuickLoad, Binding::KeyWith(KeyCode::KeyL, cmd))
            .bind(Screenshot, Binding::KeyWith(KeyCode::KeyP, cmd))
            .bind(ToggleDebug, Binding::KeyWith(KeyCode::KeyD, cmd))
            .bind(DemolishTool, Binding::Key(KeyCode::Backspace));
    }
    m.bind(PanUp, Binding::Key(KeyCode::KeyW))
        .bind(PanUp, Binding::Key(KeyCode::ArrowUp))
        .bind(PanDown, Binding::Key(KeyCode::KeyS))
        .bind(PanDown, Binding::Key(KeyCode::ArrowDown))
        .bind(PanLeft, Binding::Key(KeyCode::KeyA))
        .bind(PanLeft, Binding::Key(KeyCode::ArrowLeft))
        .bind(PanRight, Binding::Key(KeyCode::KeyD))
        .bind(PanRight, Binding::Key(KeyCode::ArrowRight))
        .bind(PanDrag, Binding::Mouse(MouseButton::Middle))
        .bind(PanDrag, Binding::Mouse(MouseButton::Right))
        .bind(ZoomIn, Binding::Key(KeyCode::Equal))
        .bind(ZoomIn, Binding::Key(KeyCode::NumpadAdd))
        .bind(ZoomOut, Binding::Key(KeyCode::Minus))
        .bind(ZoomOut, Binding::Key(KeyCode::NumpadSubtract))
        .bind(TogglePause, Binding::Key(KeyCode::Space))
        .bind(SpeedNormal, Binding::Key(KeyCode::Digit1))
        .bind(SpeedDouble, Binding::Key(KeyCode::Digit2))
        .bind(SpeedQuadruple, Binding::Key(KeyCode::Digit3))
        .bind(ToggleDebug, Binding::Key(KeyCode::F3))
        .bind(ToggleGrid, Binding::Key(KeyCode::KeyG))
        .bind(ToggleTerritory, Binding::Key(KeyCode::KeyT))
        .bind(Select, Binding::Mouse(MouseButton::Left))
        .bind(Screenshot, Binding::Key(KeyCode::F12))
        .bind(CenterHome, Binding::Key(KeyCode::Home))
        .bind(CenterHome, Binding::KeyWith(KeyCode::KeyH, Modifiers::NONE))
        .bind(Cancel, Binding::Key(KeyCode::Escape))
        .bind(ToolCancel, Binding::Mouse(MouseButton::Right))
        .bind(ToggleBuildMenu, Binding::Key(KeyCode::KeyB))
        .bind(DemolishTool, Binding::Key(KeyCode::Delete))
        .bind(DemolishTool, Binding::Key(KeyCode::KeyX))
        .bind(MoveTool, Binding::Key(KeyCode::KeyM))
        .bind(Rotate, Binding::Key(KeyCode::KeyR))
        .bind(QuickSave, Binding::Key(KeyCode::F5))
        .bind(QuickLoad, Binding::Key(KeyCode::F9))
        .bind(ToggleEvents, Binding::Key(KeyCode::KeyE))
        // Management window: O(ekonomie), L(ager), P(roduktion), F(inanzen), K(ontor), I (Diplomatie:
        // Inland/Ausland), Q(uartiere: Militär), J(obs), V(olk), N(ahrung/Natur), U(niversität),
        // C(hronik).
        .bind(WindowEconomy, Binding::Key(KeyCode::KeyO))
        .bind(WindowStorage, Binding::Key(KeyCode::KeyL))
        .bind(WindowProduction, Binding::Key(KeyCode::KeyP))
        .bind(WindowFinance, Binding::Key(KeyCode::KeyF))
        .bind(WindowTrade, Binding::Key(KeyCode::KeyK))
        .bind(WindowDiplomacy, Binding::Key(KeyCode::KeyI))
        .bind(WindowMilitary, Binding::Key(KeyCode::KeyQ))
        .bind(WindowWorkforce, Binding::Key(KeyCode::KeyJ))
        .bind(WindowPopulation, Binding::Key(KeyCode::KeyV))
        .bind(WindowAgriculture, Binding::Key(KeyCode::KeyN))
        .bind(WindowResearch, Binding::Key(KeyCode::KeyU))
        .bind(WindowStatistics, Binding::Key(KeyCode::KeyC));
    m
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;

    #[test]
    fn keys_are_not_bound_twice() {
        let map = default_bindings();
        let mut seen: BTreeMap<String, Action> = BTreeMap::new();
        for action in Action::ALL {
            for binding in map.bindings(action) {
                // Mouse buttons are shared on purpose (right button: drag pans, click cancels).
                if matches!(binding, Binding::Mouse(_)) {
                    continue;
                }
                if let Some(other) = seen.insert(binding.label(), action) {
                    panic!("{} is bound to {other:?} and {action:?}", binding.label());
                }
                assert!(conflicts(&map, binding, action).is_empty(), "{action:?}");
            }
        }
        assert_eq!(map.hint(Action::ToggleBuildMenu).as_deref(), Some("B"));
        assert_eq!(map.hint(Action::QuickSave).as_deref(), Some("F5"));
        assert_eq!(map.hint(Action::WindowFinance).as_deref(), Some("F"));
    }

    #[test]
    fn every_action_is_listed_once_and_bound() {
        let map = default_bindings();
        let unique: BTreeSet<Action> = Action::ALL.into_iter().collect();
        assert_eq!(unique.len(), Action::ALL.len());
        for action in Action::ALL {
            assert!(!map.bindings(action).is_empty(), "{action:?} has no default binding");
            assert!(!action.label().is_empty());
            assert!(map.bindings(action).len() <= MAX_BINDINGS, "{action:?}");
        }
    }

    #[test]
    fn overlapping_bindings_conflict() {
        let ctrl = Modifiers { ctrl: true, ..Modifiers::NONE };
        let shift = Modifiers { shift: true, ..Modifiers::NONE };
        let f = Binding::Key(KeyCode::KeyF);
        assert!(bindings_overlap(&f, &Binding::Key(KeyCode::KeyF)));
        // A plain key fires with any modifiers held.
        assert!(bindings_overlap(&f, &Binding::KeyWith(KeyCode::KeyF, ctrl)));
        assert!(!bindings_overlap(&Binding::KeyWith(KeyCode::KeyF, shift), &Binding::KeyWith(KeyCode::KeyF, ctrl)));
        assert!(bindings_overlap(&Binding::KeyWith(KeyCode::KeyF, ctrl), &Binding::KeyWith(KeyCode::KeyF, ctrl)));
        assert!(!bindings_overlap(&f, &Binding::Key(KeyCode::KeyG)));
        assert!(!bindings_overlap(&f, &Binding::Mouse(MouseButton::Left)));

        let map = default_bindings();
        // F opens the finances: binding it to the build menu conflicts, to the finances not.
        assert_eq!(conflicts(&map, &f, Action::ToggleBuildMenu), vec![Action::WindowFinance]);
        assert!(conflicts(&map, &f, Action::WindowFinance).is_empty());
        assert_eq!(conflicts(&map, &Binding::KeyWith(KeyCode::KeyF, ctrl), Action::Rotate), vec![Action::WindowFinance]);
        assert!(conflicts(&map, &Binding::Key(KeyCode::KeyZ), Action::Rotate).is_empty());
        // H for "home" is stored with empty modifiers and still overlaps a plain H.
        assert_eq!(conflicts(&map, &Binding::Key(KeyCode::KeyH), Action::Rotate), vec![Action::CenterHome]);
    }

    #[test]
    fn mac_defaults_add_command_shortcuts_without_conflicts() {
        let cmd = Modifiers { logo: true, ..Modifiers::NONE };
        let mac = default_bindings_for(true);
        for (action, key) in [(Action::QuickSave, KeyCode::KeyS), (Action::QuickLoad, KeyCode::KeyL), (Action::Screenshot, KeyCode::KeyP), (Action::ToggleDebug, KeyCode::KeyD)] {
            assert!(mac.bindings(action).contains(&Binding::KeyWith(key, cmd)), "{action:?}");
        }
        assert!(mac.bindings(Action::QuickSave).contains(&Binding::Key(KeyCode::F5)), "F5 stays");
        assert!(mac.bindings(Action::DemolishTool).contains(&Binding::Key(KeyCode::Backspace)));
        let label = if cfg!(target_os = "macos") { "Cmd+S" } else { "Super+S" };
        assert_eq!(mac.hint(Action::QuickSave).as_deref(), Some(label));
        for a in Action::ALL {
            for b in mac.bindings(a).iter().filter(|b| !matches!(b, Binding::Mouse(_))) {
                for other in Action::ALL.into_iter().filter(|o| *o != a) {
                    assert!(!mac.bindings(other).iter().any(|c| bindings_overlap_on(b, c, true)), "{a:?} and {other:?} share {b:?}");
                }
            }
        }
        // Cmd+S and S (pan down) are separate on macOS, not where a plain key fires with any
        // modifier – which is why other platforms keep only the F keys.
        let save = Binding::KeyWith(KeyCode::KeyS, cmd);
        assert!(!bindings_overlap_on(&save, &Binding::Key(KeyCode::KeyS), true));
        assert!(bindings_overlap_on(&save, &Binding::Key(KeyCode::KeyS), false));
        let other = default_bindings_for(false);
        assert!(Action::ALL.iter().all(|a| !other.bindings(*a).iter().any(|b| matches!(b, Binding::KeyWith(_, m) if m.logo))));
    }

    #[test]
    fn german_key_names() {
        assert_eq!(key_name(KeyCode::Space), "Leertaste");
        assert_eq!(key_name(KeyCode::KeyW), "W");
        assert_eq!(key_name(KeyCode::Digit3), "3");
        assert_eq!(key_name(KeyCode::Numpad7), "Num 7");
        assert_eq!(key_name(KeyCode::F5), "F5");
        let ctrl_s = Binding::KeyWith(KeyCode::KeyS, Modifiers { ctrl: true, shift: true, ..Modifiers::NONE });
        assert_eq!(binding_label(&ctrl_s), "Strg+Shift+S");
        assert_eq!(binding_label(&Binding::Mouse(MouseButton::Middle)), "Mittlere Maustaste");
        assert!(!bindable_key(KeyCode::ShiftLeft) && !bindable_key(KeyCode::Escape) && bindable_key(KeyCode::KeyQ));
        assert!(Action::Rotate.rebindable() && !Action::Cancel.rebindable() && !Action::Select.rebindable());
    }
}
