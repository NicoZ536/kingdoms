# KINGDOMS – Technische Architektur

## 1. Überblick

```
kingdoms/                      Cargo-Workspace
├── crates/engine  (kingdoms_engine)  spielunabhängiges Framework: Lifecycle, Loop, Input,
│                                     wgpu-Renderer, Kamera, Iso-Projektion, Assets, Audio, Debug
├── crates/sim     (kingdoms_sim)     deterministische Simulation ohne Grafik-Abhängigkeiten
├── src/           (kingdoms, bin)    Spiel: Szenen, Darstellung (view), UI, Speichern-IO, Audio-Glue
├── tools/assetgen                    erzeugt Platzhalter-Sprite-Atlanten (deterministisch)
├── tools/macos                       make_app.sh: baut Kingdoms.app (Bundle, Symbol, Signatur)
├── assets/                           Atlanten (PNG + RON-Metadaten), Audio, Fonts
├── data/                             Spieldaten (RON): Ressourcen, Gebäude, Technologien, Ereignisse, …
├── docs/                             Architektur, Game Design, Roadmap, Fortschritt, Assets
├── crates/sim/tests/                 Integrationstests der Simulation (Determinismus, Speichern/Laden,
│                                     Fuzzing, vernünftiger Spieler, Kampagnen)
└── tests/                            nur README: Unit-Tests stehen in jedem Crate neben dem Code
```

**Warum ein Workspace statt eines einzelnen Crates?** Die geforderte strikte Trennung von
Engine, Simulation, Rendering und UI wird vom Compiler erzwungen: `kingdoms_sim` *kann*
weder wgpu noch winit noch egui verwenden. Dadurch ist die Simulation headless testbar,
deterministisch und schnell kompilierbar. Die im Auftrag skizzierten Ordner existieren als
Module in den jeweiligen Crates:

| Ordner laut Auftrag | Ort |
|---|---|
| core, engine, input, rendering, assets, audio | `crates/engine/src/{core,input,rendering,assets,audio}` |
| world, simulation, economy, population, buildings, production, agriculture, diplomacy, military, research, events | `crates/sim/src/…` |
| save | Formate/Validierung `crates/sim/src/save`, Datei-IO `src/save` |
| ui | `src/ui` (HUD, Fenster, Menüs, Theme) |

Abhängigkeitsrichtung (azyklisch): `kingdoms` → `kingdoms_engine`, `kingdoms_sim`;
`kingdoms_assetgen` → `kingdoms_engine` (nur Metadaten-Schema). Engine und Sim kennen sich nicht.

## 2. Engine (`kingdoms_engine`)

### 2.1 Lifecycle & Main Loop (`core::app`)
winit-`ApplicationHandler`. Pro Frame: Zeitmessung → `Game::update` (Eingabe, Kamera) →
`Game::fixed_update` × fällige Ticks → `Game::ui` (egui) → `Game::render` (RenderList) →
Renderer zeichnet Welt + UI → Eingabe-Frame-Zustand zurücksetzen.
* **Fixed Timestep** (`core::time`): Akkumulator, 20 Ticks/s bei 1×, Geschwindigkeiten
  Pause/1×/2×/4×, max. Ticks pro Frame (Spiral-of-Death-Schutz), Interpolationsfaktor `alpha`.
* **Frame-Pacing**: VSync (Fifo) oder Limiter (`max_fps`) via `ControlFlow::WaitUntil`; minimiert → Schlafen.
* **Fenstersymbol** (Linux, Windows): `WindowConfig::icon` (vom Spiel beim Start gesetzt, nie in
  `engine.ron` gespeichert) nennt ein Bild im Assets-Ordner; es wird auf 128 px verkleinert
  (`load_window_icon`), eine fehlende oder kaputte Datei gibt eine Warnung und kein Symbol. macOS
  nutzt das Symbol des App-Bundles.
* **Szenen** (`core::scene`): Stapel mit geteiltem Zustand `S`, Übergänge Push/Pop/Replace/Reset/Quit,
  Overlay-Szenen lassen die darunterliegende Szene rendern.
* **Ladezustand** (`EngineContext::set_loading`): solange das Spiel einen Ladebildschirm meldet,
  zählen Frames nicht für den automatischen Screenshot (`KINGDOMS_AUTOSHOT_FRAME`) – Skripte
  erfassen denselben Moment, egal wie lange das Laden dauert.
* **Skriptläufe** (`Script` in `core::app`): `KINGDOMS_AUTOSHOT_FRAME` nimmt auch eine Liste
  (`30,37` → `<datei>_30.png`, `<datei>_37.png`); `KINGDOMS_PROFILE=<frames>[@<start>]` sammelt
  ab dem Startframe jede Messung jedes Abschnitts (`FrameMetrics::begin_capture`/`end_capture`,
  `ProfileReport`: Mittel, Median, 95. Perzentil, Maximum) und schreibt die Tabelle mit den
  Sprite- und Draw-Call-Zahlen des letzten Frames ins Log; `KINGDOMS_FIXED_DT=<s>` lässt jeden
  Frame genau so lange dauern (`Time::set_fixed_frame`; die Shader-Uhr zählt dann die Frames nach
  dem Laden, `RenderList::shader_time`) – Spielzeit, Animationen, Partikel, Wind und Wasser hängen
  nur von der Frame-Nummer ab, Screenshots zweier Läufe sind pixelgleich.
* **Plattform:** auf macOS ohne das Standard-Menü von winit (dessen „Beenden“ beendete sofort,
  ohne die Rückfrage des Spiels); Cmd+Q geht wie das Schließen des Fensters über
  `Game::on_close_requested`, Cmd+H blendet aus, Cmd+M minimiert (`platform_shortcut`). Ein
  verdecktes Fenster (`WindowEvent::Occluded`, macOS behält dabei seine Größe) läuft mit 10 Frames/s
  weiter (die Simulation holt ihre Ticks nach), ein minimiertes mit Größe 0 schläft wie bisher.
* **Events** (`core::events`): typisierte FIFO-Queues mit Überlaufschutz; Engine-Events (Resize, Fokus, Drop).
* **Konfiguration** (`core::config`): RON, fehlende Felder = Defaults, Sanitizing, atomares Speichern
  (temporäre Datei mit `sync_all` – auf macOS ein volles `F_FULLFSYNC` –, dann Umbenennen).
* **Pfade** (`core::paths`): Content-Root-Suche (`content_candidates`): `KINGDOMS_ROOT`, dann
  `Contents/Resources` des App-Bundles, aus dem das Programm läuft (`bundle_resources`; vor allem
  anderen, damit eine in einem Quellbaum gebaute App ihre eigene Kopie nimmt), dann Exe-Verzeichnis
  und CWD samt Vorfahren. Benutzerverzeichnis je OS (`~/Library/Application Support/Kingdoms` auf
  macOS).
* **Logging** (`core::logging`): env_logger → stderr + Logdatei + Ringpuffer (Debug-Konsole),
  Panic-Hook protokolliert Ort und Nachricht.

### 2.2 Input (`input`)
Tastatur-/Maus-Zustand pro Frame (down / pressed / released, Drag-Erkennung, Klick vs. Drag,
Scroll, Modifikatoren). egui hat Vorrang: Drücken wird blockiert, wenn die UI Zeiger/Tastatur
beansprucht, Loslassen wird immer weitergereicht. `ActionMap<A>` bildet Spielaktionen auf
Bindungen ab (serde-fähig für frei belegbare Tasten). `Input::raw_keys_pressed` liefert die
Tastendrücke des Frames in Ankunftsreihenfolge auch dann, wenn die UI sie verbraucht hat
(„Taste drücken“-Abfragen beim Neubelegen). Weiches Scrollen (`MouseScrollDelta::PixelDelta`,
Trackpads) wird in logische Pixel umgerechnet (`pixel_scroll`, Skalierungsfaktor aus dem Fenster) und
zählt mit 40 px je Zeile auch in `scroll`; Zwei-Finger-Zoom (`WindowEvent::PinchGesture`) summiert
`pinch`. Auf macOS (`COMMAND_BLOCKS_PLAIN_KEYS`) feuert eine Bindung ohne Cmd nicht, solange Cmd
gedrückt ist – Cmd+S speichert, ohne die Karte nach unten zu schieben; anderswo feuert eine Taste
ohne Modifikatoren wie bisher mit jedem.

### 2.3 Rendering (`rendering`)
* wgpu 30, sRGB-Surface, MSAA 4× mit Alpha-to-Coverage (Fallback: Alpha-Test). MSAA lässt sich
  zur Laufzeit umschalten (`Renderer::set_msaa_samples` baut Sprite-Pipelines und Render-Targets
  neu; `msaa_supported` meldet, ob der Adapter 4× kann); `EngineContext::apply_graphics_config`
  übernimmt VSync, MSAA, UI-Skalierung und Vollbild sofort. Mit `EngineContext::set_ui_min_size` nennt das
  Spiel die Mindestgröße seiner Oberfläche in Punkten (1280 × 720); die wirksame UI-Skalierung
  (`effective_ui_scale`, je Frame gesetzt) wird gesenkt – nie erhöht –, solange das Fenster sonst
  weniger Punkte zeigen würde.
* **Kamera** (`camera`): orthografisch, Zoom am Cursor verankert, Grenzen, Pixel-Snapping.
* **Iso-Projektion** (`iso`): 2:1-Rauten 64×32 px, Höhenstufe = 16 px.
* **Tiefenmodell**: statt CPU-Painter-Sortierung schreibt der Fragment-Shader eine
  Iso-Tiefe `D = tx + ty` (flache Flächen: linear in y; aufrechte Objekte: „Zeltfunktion"
  über die Breite = Tiefe der Vorderwände). Dadurch sind Verdeckungen zwischen Terrain,
  Gelände-Höhen, Gebäuden und Figuren korrekt, unabhängig von der Zeichenreihenfolge.
  Normalerweise gilt das Minimum beider Funktionen; Sprites mit `FLAG_DEPTH_MAX` nehmen das
  Maximum („Bodenbewuchs“, `SpriteDepth::ground_cover`: vorne flaches Decal, alles darüber auf
  einer Mindesttiefe – Felder mit Pflanzen überdecken, was hinter dem Feld steht).
  `SpriteDepth::sub_span(u0, u1)` liefert die Tiefe eines kleineren Sprites, das nur den
  waagerechten Ausschnitt `u0..u1` eines aufrechten Sprites bedeckt (Zeltfunktion auf den
  Ausschnitt umgerechnet; z. B. beleuchtete Fenster über einem Gebäude).
  Details und Tests: `crates/engine/src/rendering/iso.rs`.
* **Sprite-Batching**: instanziertes Rendering (12 Vertices je Sprite, Instanzpuffer wächst),
  opake Sprites nach Textur gruppiert (stabiles Zählsortieren über die wenigen Texturen),
  transparente Sprites back-to-front sortiert (Tiefenschlüssel einmal je Sprite berechnet, dann
  `sort_unstable` über Schlüssel, Textur und Index – dieselbe Reihenfolge wie das frühere
  stabile Sortieren). Ordnung, Instanzen und Läufe liegen in wiederverwendeten Puffern
  (`FrameScratch`); auch die Listen einer `RenderList` kommen aus einem kleinen Vorrat je Thread
  (eine fallengelassene Liste leiht der nächsten ihre Puffer). `RenderStats` misst die Schritte
  des Renderers (Warten auf die Surface, Vorbereiten, Kodieren, Abschicken), die App führt sie
  als Abschnitte `render: …`.
* **Statische Batches**: GPU-residente Sprite-Listen (Terrain-Chunks 32×32), Frustum-Culling je Batch.
* **CPU-Culling** dynamischer Sprites gegen das Sichtrechteck.
* **Shader-Effekte**: Wind (Flag), Wasser-Schimmer (Flag), emissiv (ignoriert Umgebungslicht),
  globales Umgebungslicht (`RenderList::ambient`, Tag/Nacht- und Wettertönung).
* **Umgebung** (`RenderList::environment`, `sprite::Environment`, spielunabhängig): Stärke und
  Zeitbasis des Wind-Schwankens, stetige Neigung der Wipfel (Vorzeichen = Richtung),
  Windrichtung auf dem Bildschirm (`wind_dir`, −1 … 1: Böen drücken in diese Richtung und
  laufen in ihr über die Karte; 0 = gleichmäßiges Schwanken), Schneedecke 0–1 und Nässe 0–1.
  `FLAG_WIND` biegt aufrechte Sprites an den oberen Ecken; zusammen mit `FLAG_GROUND`
  (Bodenbewuchs: Felder, Obstgärten) bleibt das Quad stehen und der Fragment-Shader schert nur
  die Pixel über der Tile-Raute nach ihrer Höhe (die Raute selbst bewegt sich nie). `FLAG_GROUND` (Boden-Sprite mit Anker in der Tile-Mitte): die
  Tile-Raute wird dunkler bei Nässe und fleckig weiß bei Schnee (Rauschen in Weltkoordinaten,
  nahtlos über Tiles; `FLAG_TRODDEN` für Wege: nur dünner Schnee); `FLAG_SNOW_ON_GREEN`: grüne
  Pixel (Rasen, Hecken, Laub) eines beliebigen Sprites bekommen die Schneedecke;
  `FLAG_SNOW_ON_ROOF`: Pixel mit der Schneemarke Alpha 254 (`SNOW_MARK_ALPHA`, Dächer und
  Oberseiten, docs/assets.md) bekommen die Schneedecke und werden voll deckend gezeichnet
  (Gebäude, Mauern, Tore, Städte der Nachbarreiche); `FLAG_SHADOW_MARK`: Pixel mit der
  Schattenmarke Alpha 253 (`SHADOW_MARK_ALPHA`, gemalte Schlagschatten) werden auf die Stärke
  `Environment::shadow` abgeschwächt – der Shader nimmt die Mischung des Generators
  (`SHADOW_MARK_RGB` mit `SHADOW_MARK_OPACITY` in sRGB) zurück und mischt neu. Beide Marken
  liest der Shader exakt aus den vier Texeln des bilinearen Filters (`textureGather`, volle
  Auflösung), damit Filterung und Mipmaps die Ränder der einen Marke nicht zur anderen machen.
  Der Shader wird in einem Test mit naga validiert; derselbe Test prüft, dass Flag-Konstanten,
  Markenwerte und Schattenfarbe in WGSL und Rust übereinstimmen.
* **Partikel** (`particles`): CPU-Pool mit harter Obergrenze (Verworfene werden gezählt), Stile
  (`ParticleStyle`: Sprites aus einem Atlas mit Bildwahl über die Lebenszeit, Schleife oder
  Zufall; Lebensdauer, Startgeschwindigkeit mit Streuung, Beschleunigung, Windanteil, seitliches
  Pendeln, Größe und Deckkraft über die Lebenszeit, Tönung, Flags) und Emitter im
  Immediate-Mode (`emit(key, Emitter)` je Frame; Rate, Spawn-Rechteck, Tiefe). Weltpartikel
  werden wie aufrechte Objekte an der Bodentiefe ihrer Quelle tiefengetestet, Emitter außerhalb
  der Sicht (plus Rand) erzeugen nichts; Bildschirmpartikel (logische Pixel) liegen über der
  Welt mit gleicher Bildschirmgröße bei jedem Zoom (Wetter). Jeder Emitter-Schlüssel hat einen
  eigenen SplitMix64-Zufall (deterministisch, reine Darstellung); Geburten werden über den
  Frame verteilt, Zeitschritt höchstens 0,1 s. Gezeichnet im transparenten Pass. Unit-Tests für
  Rate, Lebensdauer, Obergrenze, Determinismus, Bewegung, Culling, Kurven und Tiefe.
* **egui** wird in eine lineare (nicht-sRGB) View der Surface gezeichnet.
* **Screenshots**: GPU-Readback (Surface oder Offscreen-Target) → PNG im Hintergrund-Thread.

### 2.4 Assets (`assets`)
`assets/atlases/index.ron` listet Atlanten; je Atlas eine RON-Datei (`AtlasDef`: Bild,
Sampling, Mipmaps, Sprites mit Rechteck + Anker, Animationen). **Asynchron:**
`AssetManager::start_loading_atlases` startet bis zu vier Decoder-Threads (`AtlasLoading`,
Fortschritt `done/total`), `continue_loading_atlases` registriert je Frame die fertig dekodierten
Atlanten mit Zeitbudget und lädt sie auf dem Main-Thread hoch – streng in Index-Reihenfolge, damit
Sprite-IDs in jedem Lauf gleich sind; ein abgestürzter Decoder wird als Ladefehler gemeldet.
`load_all_atlases` ist die blockierende Variante. Mipmaps gamma-korrekt auf der CPU. Fehlende Sprites → sichtbarer
Magenta-Platzhalter + einmalige Warnung. Je Atlas bleibt eine Alpha-Maske (1 Bit je Pixel,
deckend ab Alpha 128) auf der CPU: `AssetManager::sprite_opaque_at` für Treffertests gegen das
tatsächlich Gezeichnete. Schema: `docs/assets.md`.

### 2.5 Audio (`audio`)
`Audio` (im `EngineContext`) liest `assets/audio/manifest.ron` (`AudioManifest`: Klang-IDs mit
Dateien, Kanal, Lautstärke, Abklingzeit, Höchstzahl, Tonhöhenvariation, Beschreibung und bei
Stimmen dem genauen Wortlaut `text`; Musikstücke mit Stimmungen). Kanäle Musik, Umgebung,
Effekte, Oberfläche, Stimmen mit Lautstärken aus den Einstellungen (`set_settings` wirkt sofort
auf alles Laufende). **Dateien nach Stamm** (`audio::files`): beim Start wird jede Manifest-Datei
einmal aufgelöst – zuerst der genaue Pfad, sonst im selben Ordner eine Datei mit gleichem Stamm
(`stem_key`: Kleinbuchstaben, Folgen anderer Zeichen als Buchstaben/Ziffern = ein `_`) und einer
unterstützten Endung (`.ogg`, `.mp3`, `.wav`, `.flac`; die genannte zuerst). Das Log nennt die
Summe, `log_delivery` die Ersetzungen und die IDs ohne Datei. Musik: Stücke der Stimmung
nacheinander, 2,5 s vor dem Ende (Länge aus dem Decoder) Überblendung ins nächste, ebenso beim
Stimmungswechsel; Umgebung als Endlosschleifen mit weicher Lautstärke; Effekte mit Stereo-Pan und
Abstand; solange eine Stimme spricht, werden Musik (auf 35 %) und Umgebung (halb so stark)
abgesenkt (`duck_step`). `files::audit` dekodiert alle Dateien ohne Gerät (Länge, Abtastrate,
Kanäle, Spitze, Stille an den Enden) für den Lieferbericht des Spiels. Ohne Gerät oder Dateien
bleibt alles stumm.

## 3. Simulation (`kingdoms_sim`)

### 3.1 Prinzipien
* **Deterministisch**: Zufall nur aus gespeicherten `ChaCha8Rng`; keine Iteration über
  `HashMap`/`HashSet` mit ergebnisrelevanter Reihenfolge (`BTreeMap`/`Vec`); keine Wanduhr.
* **Kommandos statt direkter Mutation**: UI/KI erzeugen `Command`s, die zu Beginn des
  nächsten Ticks angewendet werden (Replay-/Netzwerk-/Test-fähig).
* **Ereignisse nach außen**: jeder Tick liefert `SimEvent`s (Benachrichtigungen, Sounds, UI).
* **Datengetrieben**: Gebäude, Ressourcen, Rezepte, Feldfrüchte, Technologien, Einheiten,
  Ereignisse und Szenarien stehen in `data/*.ron` und werden beim Start validiert (`Defs`).
* **ECS-ähnlich**: Entitäten in generationellen Arenen (`Arena<T>` mit `Id<T>`), Systeme als
  Funktionen über den Zustand, räumliche Indizes (Belegungsgitter, Chunk-Listen).

### 3.2 Module
| Modul | Inhalt |
|---|---|
| `world` | Tile-Karte, Terrain, Biome, Vegetation, Vorkommen, Weltgenerator (10 Stufen), Rauschen |
| `ids` | generationelle Arena und typisierte IDs |
| `time` | Kalender (Tick → Minute/Stunde/Tag/Jahreszeit/Jahr) |
| `defs` | Datendefinitionen + Loader + Validierung |
| `command`, `event` | Eingabe-Kommandos, Ausgabe-Ereignisse |
| `state` | `GameState` (gesamter Simulationszustand) + Tick-Pipeline |
| `buildings`, `roads` | Platzierung, Validierung, Bau, Abriss, Ausbau aller Gebäude mit `upgrade` in den Daten (`upgrade_target`, `upgrade_blockers`; Bauarbeiter halten die Plätze: `Building::trade_staff`), Straßennetz |
| `economy`, `production` | Lager, Rezepte, Warteschlangen, Preise, Finanzen, Handel; `production::output` (gemessener Ertrag: Arbeitsgänge und Waren je Tag, heute und die letzten 24 Tage, `ProductionState::output_log`) |
| `nature` | Baumwachstum (Jahreszeit-Faktor), Baumstümpfe, Fischbestand |
| `population` | Bewohner (Arena, `CitizenId`), Haushalte, Wohnungen; Bedürfnisse, Zufriedenheit, Gesundheit, Haushaltsversorgung (Markt/Lager), Demografie, Hausstufen (Ab-/Aufstieg); `movement` (Positionen in 1/256 Tile, Wege, Pfadanfragen mit Budget, Reisende), `routine` (Tagesablauf als Ziele – Festtage, Kranke, Schutz vor Feinden –, Anwesenheit, Tätigkeit), `spatial` (räumlicher Index je Chunk, `CitizenView`), `wealth` (Geldbörse der Haushalte: Einkommen, Steuer, Marktgebühr, Käufe zum Marktpreis, arm/wohlhabend), `friends` (Freundschaften am selben Ort, stündlich per Hash) |
| `workforce` | Arbeitszuteilung der Bewohner auf Arbeitsplätze und Baustellen (Grundbesetzung: Träger, Marktarbeiter, ein Bauarbeiter für die erste Baustelle; Suche nach den Nächstwohnenden über Gruppen je Haus, Ergebnis gleich der früheren Einzelsuche – Referenz `workforce_reference.rs` nur in Tests), Wohnbelegung, Zähler |
| `navigation` | Wegkosten-Raster (Terrain, Straßen/Brücken, Höhen, Gebäude, Mauern/Tore, Bauwerke der KI-Orte; abgeleitet, inkrementell über die Änderungs-Rechtecke von Straßen, Gebäuden und der Land-Karte; jede Aktualisierung meldet die Rechtecke geänderter Tiles, billiger/teurer), Zusammenhangskomponenten, A* (ganzzahlig, vollständige Gleichstandsregel, in Scheiben über mehrere Ticks fortsetzbar), Weggedächtnis `memory` (gespeichert: Wege je ungeordnetem Paar, gepackt 2 Bit je Schritt, mit Kosten und Umriss, am längsten unbenutzte zuerst vergessen, gezielt invalidiert, beim Laden gegen den Raster-Fingerabdruck und Schritt für Schritt geprüft); `prepare` (Raster und Komponenten aktuell, bevor Schritte Komponenten vergleichen) |
| `logistics` | Transportaufträge (`planner`: Abholung, Rücktransport, Belieferung, Baumaterial; Rückstau höchstens 4 je Träger), laufende Träger (`carriers`: Abholen → Tragen → Abladen mit Ladezeit, Zusammenlegen gleicher Aufträge, Rückfracht, Umleiten zu einem Lager mit Platz, Übergabe beim Tod/Abwandern), Heimtragen der Sammler (`Haul`), Fahrtstatistik; Waren unterwegs liegen beim Bewohner (`CarryTrip`) |
| `agriculture` | Farmen mit exklusiv beanspruchten Feld- und Weide-Tiles (`Agriculture`: Farmen je Gebäude, Belegung Tile → Farm), Feldfrüchte mit Wachstum, Stufen, Boden, Feuchte, Zustand; Aussaat und Ernte als Farmarbeit, Frost, Scheune; Darstellung je Tile (`field_at`, `fields_in`, Feld-Dirty-Chunks) |
| `weather` | Tageswetter (eigener `ChaCha8Rng`, Fortbestehen, Stärke), Tagestemperatur in °C (Jahreszeit, Wetter, Hash des Tages, Klima des Startgebiets; Heizen und Frost), Wirkungen (Wachstum, Arbeit im Freien, Heizbedarf, Krankheit, Feldschaden, Dürre-Durst), Jahreszeit-Faktoren der Sammler, Schneedecke/Bodennässe, Trockenperiode (`dry_days`: Tage in Folge ohne Regen, Sturm oder Schnee), Meldungen |
| `research` | Erforschte Technologien und ihre Wirkungen (Abfragen für alle Systeme), Forschungsliste mit Fortschritt (ganzzahlig, Milli-Punkte je Stunde je Tick), Bezahlung beim Beginn, Abschluss mit Wirkung, Forschungsleistung (`research_rate`: Hauptquartier + Schulen nach anwesender Besetzung), Tagesstatistik, deutsche Wirkungstexte |
| `events` | Ereignisse mit Entscheidungen: tägliche Auslosung (eigener `ChaCha8Rng`, Bedingungen, Abklingzeit, Gewicht × Risiko), offenes Ereignis mit Frist, `ResolveEvent`, Konsequenzen (`effects`: Gold/Waren, befristete Modifikatoren mit Quelle `ModifierSource` – Ereignis oder Fest –, Wetter, Flüchtlinge, Krankheit, Gebäudeschaden → Ruinen-Baustelle, Felder, Plünderung mit Ankunft der Bande relativ zur Meldung, Vorkommen, Steuerdeckel), Bedingungen und Voraussetzungstexte (`conditions`, u. a. `RaidGraceOver`, Weltzustand für Ernte und Dürre: `MinDryDays`, `Min-`/`MaxFieldCondition` aus `Agriculture::average_field_condition`, `NoEventThisSeason`), Ereignistext mit Brandherd (`event_text`, `focus_text` mit `{building}`), Verlauf; `festival`: Fest ausrichten (`HoldFestival`, Kosten nach Einwohnern, Abklingzeit, Modifikatoren, Erinnerung „Festgäste“, `FestivalReport`) |
| `diplomacy` | KI-Königreiche (Sitz, Gebiet und Land-Karte `LandMap` mit Besitzer und Bauwerk-Markierung je Tile, Siedlungen aus `Structure`s, abstrakte Wirtschaft `economy`, Wachstum `setup`), Beziehungen mit benannten Teilen und verblassenden Erinnerungen, Beziehungen und Kriege der Reiche untereinander (`relations`), Aktionen des Spielers mit deterministischem Urteil und deutschen Gründen (`actions`), KI-Initiativen, Posteingang, Überfälle (`initiatives`), Handelskarawanen mit Händler und Karren als Läufer (`caravans`); eigener `ChaCha8Rng` |
| `military` | Soldaten als Bewohner (`Soldier`: Einheit, Ausbildung, Gesundheit, Moral, Ausrüstung, Erfahrung, Dienst), Rekrutierung und Entlassung (`recruit`), Kompanien und Befehle, Dienst und Eingreifen (`orders`), Unterhalt: Sold, Rationen, Moral, Neuausrüstung, Heilung (`upkeep`), Bedrohungen: Räuberbanden und KI-Heere als Reisende mit Ziel, Plünderung, Breschen, Eroberung, Kampfberichte (`threats`), Kampfrunden mit Schadensformel und Verteidigungsboni (`combat`), Feldzüge und Belagerungen mit abstrakter Schlacht (`campaign`); eigener `ChaCha8Rng` |
| `ranks` | Herrscherränge mit Anforderungen, Ansehen und Hofhaltung (`court_upkeep`, im Unterhalt gebucht), Niederlage (`GameOver`: Hauptquartier erobert, Bevölkerung 0) |
| `statistics` | Pipeline-Schritt 10: stündliche Historie, Tagesabschluss |
| `save` | versionierte Spielstände (bincode + Prüfsumme), Validierung |

### 3.3 Tick-Pipeline (Reihenfolge ist Teil der Determinismus-Garantie)
1. Kommandos anwenden
2. Kalender, dann Wetter (`weather`: um Mitternacht Tageswetter ziehen und melden, stündlich
   Schneedecke und Bodennässe)
3. Bau (Arbeitszuteilung – zuerst je ein Träger pro Lager, ein Arbeiter pro Markt und ein
   Bauarbeiter für die erste Baustelle mit Material, dann nach Prioritäten, untätige Produktion
   zuletzt –, Baustellen mit Merker „zuletzt gearbeitet“,
   Fertigstellung; automatischer Hausausbau je Haus einmal stündlich in seinem Slot
   `Gebäudeindex mod 200`; Arbeitstempo × Winter × Wetter)
4. Natur (Baumwachstum, Baumstümpfe verrotten, Fischbestand) und Landwirtschaft
   (`agriculture`): nach Änderungen an Straßen, Grundflächen oder neu aktiven Gebäuden Farmen
   abgleichen (neue Farmen beanspruchen sofort, überbaute Tiles werden frei, neue Weidehöfe
   kaufen die halbe Herde); am ersten Wintertick und an jedem Frosttag (≤ 0 °C) Frost; zu Winter- und Frühlingsbeginn
   Obstgarten-Chunks als geändert melden; jede Farm hat einen Stunden-Slot (Gebäudeindex mod 200):
   Tiles nachbeanspruchen (Feldfarmen nur zu Fuß erreichbare), Weidehöfe: Herde an die Weide
   anpassen, im ersten Slot des Tages wachsen lassen; Feldfarmen: Felder zählen Stunden ohne
   Erreichbarkeit (nach 24 frei), eine Stunde Wachstum, im ersten Slot des Tages Feuchte,
   Verdorren, Sturmschaden, Brache-Erholung;
   jeden Tick Farmarbeit der Feldfarmen (Ernte vor Aussaat; untätige Farmen suchen alle 20 Ticks
   nach Arbeit) mit Status
5. Produktion (zuerst `navigation::prepare`; Arbeitsgänge mit den anwesenden Arbeitern ×
   Schichtfaktor, Pufferlogik; Sammler: Ziel nur unter den zu Fuß erreichbaren, Arbeitsplatz am
   Ziel, verschwundenes Ziel → nächstes, Ertrag als `Haul` beim ersten Arbeiter am Platz;
   Feldfarmen arbeiten in Schritt 4; Faktor Jahreszeit/Wetter: Winter −15 %, Arbeit im Freien
   nach Wetter, Fischerei/Weide/Imkerei nach Jahreszeit)
6. Logistik (alle 20 Ticks verwaiste Aufträge verwerfen und planen; dann Gänge der Träger nach
   Auftrags-ID: im Quellgebäude laden → abholen → zum Ziel, im Ziel abladen → zurück, Rückfracht,
   Umleiten; danach freie Träger (anwesend im Lager) den wartenden Aufträgen ihres Lagers zuteilen;
   Bestandsansicht einmal am Ende des Schritts; alle 6 Stunden Warnung „Keine Träger“ für Lager ohne
   Arbeiter, deren Aufträge seit einer Stunde warten)
7. Bevölkerung (`population`): Wohnungen abgleichen (Obdachlose ziehen ein), Haushalte stündlich
   gestaffelt (Obdachlose und am Markt wartende Läufer füllen auf, eine Stunde
   Nahrung/Brennstoff/Waren verbrauchen, Versorgung)
   und einmal täglich (Alter – wer alt wird, verliert sofort seine Arbeit –, Bildung, Tod,
   Geburt, Abwanderung), Bewohner alle 20 Ticks
   gestaffelt (Bedürfnisse, Gesundheit, Zufriedenheit, Ziel des Tagesablaufs → Pfadanfrage,
   Tätigkeit; ein Transport geht vor, Arbeitsplätze im Freien sind `Destination::Spot`; Haushalte
   füllen ihren Vorrat beim Besorgungsgang auf, Obdachlose stündlich), Häuser stündlich gestaffelt (Stufenprüfung, Abstieg), stündlich Zuwanderung (zu Fuß
   vom Kartenrand) und Freundschaften (`friends`), um Mitternacht Hochzeiten. Die
   Arbeitszuteilung läuft in Schritt 3, sobald sich etwas geändert hat.
7b. Bewegung (`population::movement`, jeden Tick): Wegkosten-Raster nachführen (Änderungs-
   Rechtecke von Straßen und Gebäuden; das Weggedächtnis vergisst betroffene Wege), Bewohner aus
   verschwundenen, versetzten oder
   vergrößerten Gebäuden treten an der alten Tür heraus, Pfadanfragen der Reihe nach bis zum
   Knotenbudget je Tick beantworten (es wächst mit der Länge der Warteschlange bis zu einer
   Obergrenze; eine längere Suche läuft im nächsten Tick weiter), dann alle aktiven Läufer nach ID bewegen (Ankunft → im Gebäude,
   Reisende am Kartenrand entfernt; Ankunft zum Besorgungsgang → vormerken; am Ende des Schritts
   füllen alle angekommenen Haushalte in Ankunftsreihenfolge mit einem gemeinsamen
   Dienst-/Lager-Index ihre Vorräte auf, danach kaufen die angekommenen Gäste im Gasthaus Bier). Produktion,
   Feldarbeit, Bau und Träger in den Schritten 3–6 nutzen die Anwesenheit aus dem Stand des vorigen
   Ticks.
8. Wirtschaft (stündlich: zuerst die Geldbörsen der Haushalte in ID-Reihenfolge – Einkommen,
   Steuer, Marktgebühr, um Mitternacht Tagesabschluss –, die Staatskasse bucht genau das
   Gezahlte; dann Löhne, Unterhalt samt Hofhaltung des Rangs, Sold; Handel mit
   Handelsstraßen-Anteil; Bankrottprüfung)
9. Forschung (`research`: um Mitternacht Tagesstatistik schließen; Spitze der Liste bezahlen und
   beginnen – ohne Gold warten –; aktuelle Forschungsleistung aufaddieren; bei vollen Punkten
   abschließen und Wirkungen anwenden), dann Diplomatie (`diplomacy`, nur mit Königreichen: um
   Mitternacht Wirtschaft der Reiche, Gebiets- und Ortswachstum, verblassende Erinnerungen,
   Beziehungen und Kriege der Reiche untereinander; jeden Tick abgelaufene Verträge und
   Vorschläge; stündlich die Initiativen (zur Initiativstunde, je Reich nach ID; Vorschläge,
   Forderungen und Angebote nur, solange der Posteingang offen ist: `initiatives::inbox_open`,
   Zeitpunkt des letzten Vorschlags im Zustand) und Überfälle
   (14 Uhr); dann Karawanen: stündlich Aussenden, jeden Tick Stufen nach Karawanen-ID), dann
   Militär (`military`: um Mitternacht Unterhalt – Soldstatus, Kriegsrationen, Moral,
   Neuausrüstung, Entlassung Alter, Reparatur von Bauwerksschäden –, stündlich Heilung; jeden Tick
   Ausbildung, Ankunft angekündigter Banden und Heere am Kartenrand; alle `round_ticks`: Abgänge,
   bei Feinden auf der Karte Entscheidungen der Feinde, Kampfrunde und erneut Abgänge, dann der
   Dienst aller Soldaten und das Ende fertiger Banden/Heere mit Kampfbericht; zuletzt Feldzüge –
   zur vollen Stunde zuerst die Soldaten-Stunden des Proviants –),
   dann Ereignisse (`events`: abgelaufene Modifikatoren und Steuerdeckel entfernen; offenes Ereignis
   nach Fristende mit der ersten Wahl entscheiden; zur Prüfstunde neues Ereignis auslosen), dann
   Ränge (`ranks`: stündlich Niederlage durch Entvölkerung und Rangaufstieg). Karawanen-Läufer und
   Feinde (Reisende mit Karawane bzw. Bande) bewegen sich in Schritt 7b – Feinde bleiben vor
   fertigen Toren stehen –; der Weltmarkt in Schritt 8 handelt nur ohne Königreiche; der Sold wird
   in Schritt 8 stündlich gebucht. Nach einer Niederlage wendet ein Tick nichts mehr an (alle
   Kommandos abgelehnt, die Uhr steht).
10. Statistiken (`statistics`: stündliche Stichprobe von Schatz, Lagerbestand, Bevölkerung und
    Beschäftigten (rollierend 72 h); um Mitternacht Tagesabschluss: Kassenbuch, Preise,
    Ressourcenstatistik, Handelslimits, Ertragsbuch jedes fertigen Produktionsgebäudes
    (`production::close_output_day`), Bevölkerungszähler (Geburten, Tode nach Ursache, Zu- und
    Abwanderung, Ersparnisse der Haushalte, rollierend 48 Tage); an der ersten Mitternacht einer
    Jahreszeit zusätzlich eine `SeasonSample` der beendeten Jahreszeit in die Langzeitreihe
    (`Statistics::seasons()`: Bevölkerung, Lebensabschnitte, Haushalte, Obdachlose, Beschäftigte,
    Zufriedenheit, Schatz, Einnahmen und Ausgaben der Jahreszeit, Ersparnisse, Lagerbestand je Ware,
    Gebiet, Gebäude, Rang; höchstens 400 Stichproben, danach wird jede zweite verworfen und nur noch
    jede zweite Jahreszeit aufgenommen – die Reihe reicht immer bis zum Spielbeginn zurück) – nach
    allen Buchungen des Ticks); danach Tick-Abschluss:
    Wegkosten-Raster auf alle Änderungen des Ticks nachführen (ein Spielstand enthält nie
    Änderungen, die das Weggedächtnis nicht gesehen hat) und den Anwesenheits-Index der Gebäude
    neu zählen)

### 3.4 Speichern
`SaveGame { header: SaveHeader { magic, format_version, game_version, timestamp, seed, name,
summary }, state: GameState }` → bincode 2 (serde) → Datei mit Prüfsumme (CRC32),
atomar geschrieben (`.tmp` + rename). Laden validiert Magic, Version (Migrationen für ältere
Versionen), Längen (überlaufsicher), Prüfsumme und Konsistenz (Kartengrößen ≤ 384 vor jeder
Allokation, Tile-Anzahl, IDs, Referenzen). Der vollständige Zustand inkl. RNG-Zustand wird
gespeichert → exakte Fortsetzung (Format 6: inkl. Bewohnern, Haushalten, Wohnungen,
Arbeitsindex und Bevölkerungsstatistik; Wetter mit eigenem Zufallsgenerator, Schneedecke und
Bodennässe; Farmen mit Feld-/Weide-Tiles, Feldfrüchten und laufender Arbeit; Baumstümpfe;
Format 7: Position, Vor-Tick-Position, Blickrichtung, Gebäude, Ziel, Weg und Fortschritt jedes
Bewohners, Pfadanfrage-Warteschlange samt Fortschritt der laufenden Suche, Reisende, „Kein
Weg“-Sperrzeiten; Format 8: Transportaufträge mit Träger und Stufe (wartet, holt ab, trägt),
Waren auf den Trägern (`CarryTrip` mit Ladezeit), Fahrtstatistik, Arbeitsplatz der Sammler im
Arbeitsgang, Pendel-Merker für den Arbeitsweg; Format 9: Forschungsliste, Fortschritt und
bezahlte Kosten je Technologie, Abschlüsse, Forschungs-Tagesstatistik; Ereignisse mit offenem
Ereignis, Abklingzeiten, Modifikatoren, Steuerdeckel, Verlauf und eigenem Zufallsgenerator;
Schalter `events_enabled` in den Einstellungen; Ruinen als Baustellen `Repair`; Kassenbuch mit den
Kategorien Forschung und Ereignisse; Format 10: KI-Königreiche mit Land-Karte, Bauwerken,
Wirtschaft, Erinnerungen, Verträgen und Krieg, Beziehungsmatrix und Kriege der Reiche, Posteingang,
Karawanen mit Ladung, Börse und Läufern, diplomatischer Verlauf und eigener Zufallsgenerator,
Karawanen-Läufer unter den Reisenden, Kassenbuch-Kategorie Diplomatie; Format 11: Militär mit
Soldaten, Kompanien und Befehlen, Banden und Heeren mit Kämpfern und Beute, Feldzügen,
Kampfberichten, Bauwerksschäden, Alarm/Aufmarsch und eigenem Zufallsgenerator; Soldaten-Einheit
der Bewohner, Feinde unter den Reisenden, Ziel `Destination::Tile`, Herrscherrang und Niederlage,
Schalter `defeat_enabled`, Ressource `armor`; Format 12: Weggedächtnis mit Fingerabdruck des
Wegkosten-Rasters, Wind (Seed, Richtung, Stärke), Herden der Weidehöfe, Stunden unerreichbarer
Felder, „zuletzt gearbeitet“ der Baustellen; Format 13: Spielkennung (`GameState::game_id`, UUID
v4 aus einem eigenen ChaCha-Strom der Einstellungen, auch im Kopf `SaveHeader::game_id`, beim
Laden gegen den Zustand geprüft), Geldbörse der Haushalte (`Purse`), Freunde der Bewohner,
Gasthaus-Bestand im Gebäudeinventar, Wegzählung der Karawanen (`RouteTally`), Klima-Verschiebung
des Wetters, Ausbau aller Gebäudearten, Produktionsstatus „Wird ausgebaut“, Modifikator
`Festival`, neue Ziele `Shelter`/`Treatment`; Format 14: Ertragsbuch der Produktionsgebäude
(`OutputLog`: Arbeitsgänge und Waren je Tag, bis 24 Tage, beim Laden auf Länge, Ordnung und
Waren-IDs geprüft), Proviant und Soldaten-Stunden der Feldzüge, Wartezähler eingemauerter
abziehender Heere; Format 15: Arbeitserfahrung je Berufskategorie und Krankenstand der Bewohner
(`experience`, `sick_since`, `sick_leave`), Produktionsstatus `NoPlantingSpace`, Ersparnisse im
Tagesverlauf der Bevölkerung, Langzeitreihe der Statistik (`SeasonHistory`, beim Laden auf Länge,
Schrittweite, Ordnung und Warenzahl geprüft); Geldbörsen über 10¹² Gold
(`population::MAX_PURSE_MILLI`) und Soldaten-Stunden über 65 535 je Stunde seit Feldzugbeginn
werden abgelehnt); Format 16: Quelle der Modifikatoren (`ModifierSource`: Ereignis oder Fest)
und Tag des letzten Festes, Erinnerung „Festgäste“, Zeitpunkt des letzten KI-Vorschlags
(Drosselung des Posteingangs; ein Zeitpunkt in der Zukunft wird beim Laden abgelehnt);
Format 17: Trockenperiode des Wetters (`WeatherState::dry_days`, Tage ohne Regen, Sturm oder
Schnee; muss zum heutigen Wetter passen), Ereignis eines erzwungenen Wetters
(`ForcedWeather::event`, `None` = Entwicklerbefehl; muss ein bekanntes Ereignis sein). Ereignistage (letztes Fest, letztes Auftreten je
Ereignis, letztes Ereignis), Beginn eines offenen Ereignisses und Entscheidungen der Chronik
dürfen nicht in der Zukunft liegen, Modifikatoren und Steuerdeckel höchstens 65 535 Tage
dauern – sonst wird der Spielstand abgelehnt. Wegkosten-Raster, Komponenten, räumlicher Index,
Anwesenheits-Index der Gebäude und Liste der aktiven Läufer sind abgeleitet und werden nach dem
Laden neu aufgebaut; das Weggedächtnis wird nur übernommen, wenn der Fingerabdruck des neu
gebauten Rasters stimmt, und jeder Weg wird Schritt für Schritt geprüft (Grenzen, erlaubte
Schritte, Ziel, Kosten, Umriss) – sonst vergessen.
`validate` prüft alle Querverweise Bewohner ↔ Haushalt ↔ Haus ↔ Arbeitsplatz und `staffed`
sowie Farm ↔ Tile-Belegung ↔ Gebäude, keine Felder unter Gebäuden oder Straßen, Wertebereiche,
Positionen auf der Karte und höchstens ein Tile vom Vor-Tick entfernt, Weg passend zur Position,
sichtbare Läufer auf begehbaren Tiles oder Grundflächen, Gebäude der Insassen vorhanden,
Warteschlange = wartende Läufer; Transportaufträge in ID-Ordnung, jeder zugeteilte Auftrag hat
genau einen Träger, der ihn zurück nennt, die Ladung des Trägers entspricht Ware und Menge des
Auftrags, nur tragende Aufträge haben Waren unterwegs, `Haul` immer beladen; Forschung: bekannte
IDs, nichts doppelt eingereiht, Erforschtes weder eingereiht noch bezahlt noch in Arbeit;
Ereignisse: bekannte IDs, Seriennummern, Steuerdeckel im erlaubten Bereich, Verlauf ≤ 32;
Baustellen: nicht in der Zukunft bearbeitet; Farmen: Herde höchstens so groß wie die Tiles,
keine Herde bei Feldfarmen, Erreichbarkeits-Stunden nur für eigene Tiles und unter 24; Wind:
Richtung 0–15, Stärke ≤ 100; Diplomatie: höchstens 7 Reiche, symmetrische Beziehungsmatrix im Bereich, Land-Karte in
Kartengröße ohne unbekannte Besitzer, Gebietsgröße = Tiles der Land-Karte, Bauwerke auf eigenem,
trockenem Land ohne Gebäude und Straßen, Bauwerk-Markierungen = Bauwerke (keine Überlappung), kein
Tile zugleich beim Spieler und einem Reich, Erinnerung je Ursache höchstens einmal, keine Verträge
im Krieg, Vorschläge und Kriege mit bekannten Reichen; Karawanen: eindeutige IDs, sortierte
positive Ladung, keine negative Börse, Läufer existieren und nennen ihre Karawane zurück, keine
Läufer während der Reise außerhalb der Karte); Militär: Soldat ⇔ Bewohner mit derselben Einheit
und ohne Zivilarbeit, Werte im Bereich, Ausbildung nur in vorhandenen Gebäuden und ohne Kompanie,
Kompanie ⇔ Mitglieder gegenseitig, Mannschaft mit vorhandenem Bediener, Feldzug ⇔ abwesende
Soldaten, Banden mit Kämpfern passend zur Stufe, jeder Kämpfer hat einen Reisenden, der die Bande
zurücknennt (und umgekehrt), bekannte Reiche, Verlauf ≤ 32, Schäden nur an vorhandenen
Gebäuden, Rang im Bereich und erreichte Ränge geordnet; Haushalte: keine negativen Beträge in
der Geldbörse; Freunde: vorhanden, sortiert, gegenseitig, nicht man selbst, höchstens
`max_friends`; ein Ausbau nur zum Folgetyp der Daten; Wegzählung der Karawanen auf der Karte;
Spielkennung des Kopfes = Spielkennung des Zustands). Der Daten-Fingerprint umfasst die IDs (auch der Ränge) und die Datenform
(Gebäudegrößen, Produktion/Rezeptanzahl, Lager, Wohnen, Brücken); übrige Balancing-Werte dürfen
sich ändern und werden beim Laden neu angewendet (`GameState::rederive`: Lager- und
Pufferkapazitäten, Gebiet, Lagersicht; überbelegte Häuser nach gesunkener Kapazität geben im
nächsten Tick Haushalte ab; Farmen über ihrer Feld-Höchstzahl geben die entferntesten Tiles frei,
eine nicht mehr angebotene Feldfrucht fällt auf die erste zurück; eingereihte Technologien,
deren Voraussetzungen nach geänderten Daten nicht mehr vor ihnen stehen, fallen aus der Liste).
Namen werden als Text gespeichert,
`names.ron` darf sich ändern, ebenso `kingdoms.ron` (Namen und Herrscher stehen im Spielstand;
Wirtschaftsprofil und Ortsplan wirken ab dem nächsten Tag).

### 3.5 Schnittstellen für Darstellung und Entwicklung
* **Änderungssignale:** `take_dirty_chunks` (Tiles: Terrain, Vegetation inkl. Stümpfe),
  `take_structure_dirty_chunks` + `structure_revision` (Straßen, Mauern/Tore, Grundflächen und
  Eingänge samt Nachbar-Tiles, deren Masken sich ändern), `take_field_dirty_chunks` (Feld-Ebene:
  beansprucht, freigegeben, gesät, geerntet, neue Stufe). Alle Listen werden nicht gespeichert.
* **Felder:** `GameState::agriculture().field_at(p)` / `fields_in(rect)` → `FieldView` mit
  `sprite(defs)`; Wetter: `GameState::weather()` (Wetter, Stärke, Schneedecke, Bodennässe);
  `GameState::building_elevation(id)` (Grundflächen sind eben).
* **Berichte:** `report::{farm_report, agriculture_summary, weather_report, food_forecast}`;
  Laufwege: `report::{commute_report, path_problem, walking_summary}`, `ProductionReport`
  mit `present` und `commute` („Arbeitsweg Ø … min (… % der Schicht)“), `CitizenDetail` mit
  Position, Gebäude, Ziel, `activity_text`, letztem Arbeitsweg, getragener Ware (`carried`),
  `HouseReport::path_problem`; Logistik: `logistics_report` (wartend, holen ab, liefern, zurück,
  Träger gesamt/unterwegs/frei, Einheiten unterwegs, Heimtragen, Ø Transportdauer, Anzahl),
  `carrier_wait(state, building)` („Wartet auf Träger: …“, auch in `ProductionReport`),
  `StorageReport::{idle_carriers, waiting_jobs}`, `Logistics::goods_in_transit`,
  `population::work_area` (wo die Arbeiter eines Gebäudes arbeiten).
* **Läufer (stabil für Renderer):** `population::citizens_in(state, defs, rect)` liefert
  `CitizenView { id: WalkerId, pos, prev_pos: SubPos (1/256 Tile), facing: Direction
  (`sprite_dir()` → `ne|se|sw|nw`), role: CitizenRole (`sprite_key()` → `man`, `woman`,
  `child`, `carrier`, `builder`, `farmer`, `soldier`, `merchant`; wer für ein Lager trägt, ist
  `carrier`), moving, visible, carried: Option<(ResourceId, u32)> }` (Träger mit Ware, Sammler
  mit Ertrag)
  der sichtbaren Läufer im Rechteck (Chunks des räumlichen Index, O(sichtbar)); `citizen_view`,
  `walker_view` für einzelne; `Citizen::motion()` mit Phase, Ziel, Restweg; Interpolation
  zwischen `prev_pos` und `pos` mit dem Tick-Bruchteil. `GameState::navigation()` (Wegkosten,
  Komponenten, `PathStats`).
* **Forschung:** `report::research_report` (`ResearchReport`: Status mit deutschem Text,
  `ResearchRate` mit Quellen und erwarteter Tagesleistung, Liste mit Restzeiten, je Technologie
  `TechRow` mit Bereich, `TechState` Erforscht/In Arbeit/Vorgemerkt/Verfügbar/Gesperrt, fehlenden
  Voraussetzungen, Kosten, Punkten, Fortschritt, `eta_hours`/`duration_hours`, deutschen
  Wirkungstexten; Tagesstatistik), `research::{research_rate, effect_text, hours_for}`,
  `GameState::research()` (Liste, Fortschritt, Abfragen der Wirkungen). Kommandos
  `QueueResearch`, `CancelResearch`, `MoveResearch`; Ereignisse `ResearchQueueChanged`,
  `ResearchStarted`, `ResearchCancelled`, `ResearchCompleted`.
* **Ereignisse:** `report::pending_event` (`PendingEventReport`: Seriennummer, Titel, Text,
  Frist mit deutschem Text, Fokusgebäude/Position, je Wahl Verfügbarkeit mit Grund und
  vorhergesagten Wirkungen), `report::events_report` (Modifikatoren mit Restdauer, Summen für
  Zufriedenheit/Produktion/Feldertrag, Steuerdeckel, Verlauf neueste zuerst),
  `GameState::events()`; `ProductionReport::event_modifier_percent`, `HappinessFactor::Events`;
  Kommando `ResolveEvent { serial, choice }`; Ereignisse `EventStarted`, `EventResolved`,
  `BuildingDamaged`, `BuildingRepaired`, `FieldsDamaged`, `StockLost`, `CitizensFellIll`,
  `DepositDiscovered`; Ruinen: `BuildingStatus::Damaged`, `SitePurpose::Repair`,
  `Building::sprite` → `buildings/rubble_<B>x<T>` bis Stufe 1.
* **Königreiche & Diplomatie:** `GameState::diplomacy()` (`Diplomacy`: `kingdoms()` mit
  `Kingdom` – Name, Herrscher, Persönlichkeit, Farbe 0–6, Sitz, Radius, Einwohner, Soldaten,
  Vorräte, Verträge, Krieg, `structures()` –, `land()` → `LandMap::owner(p)` /
  `has_structure(p)` für Grenz-Overlay und Minikarte, `structures_in(defs, rect)` →
  `(KingdomId, &Structure)` mit `Structure::sprite(defs)`/`rect(defs)` für die KI-Orte,
  `land_revision()`, `proposals()`, `caravans()`, `history()`), `GameState::take_land_dirty_chunks`
  (Grenzen und Bauwerke; nicht gespeichert). Berichte: `report::diplomacy_report` (`KingdomRow`:
  Beziehung mit Zustand und `relation_parts`, Verträge mit Restdauer, Krieg, Einwohner,
  Stärke-Schätzung mit deutschem Vergleich, Gebiet, Wohlstand, gesuchte/angebotene Waren mit
  Preisen, Zugang, Routengefahr, Feinde, Verlauf; Posteingang; Karawanen; Kriege der Reiche),
  `report::kingdom_actions` (`ActionRow`: vorgeschlagene `DiplomaticAction`, Kosten, Hinderungsgrund,
  Aussicht `Judgement` mit Gründen), `report::pending_proposals`, `report::caravans_report`,
  `report::partner_prices`; `diplomacy::{relations, economy, actions, caravans}` für Einzelwerte.
  Kommandos `Diplomacy { kingdom, action }`, `AnswerProposal { serial, accept }`,
  `SendCaravan { kingdom, goods, gold, buy }`. Karawanen-Läufer erscheinen in `citizens_in` als
  Reisende mit `CitizenRole::Merchant` und `CitizenRole::Cart` (`sprite_key` `cart`, `carried` =
  größte Ladung). Ereignisse `KingdomBorderChanged`, `RelationStateChanged`, `TreatySigned`,
  `TreatyEnded`, `DiplomaticReply`, `GiftSent`, `GiftReceived`, `WarDeclared`, `PeaceMade`,
  `AiWarStarted`, `AiPeace`, `ProposalReceived`, `ProposalAnswered`, `BorderIncident`,
  `RaidHappened`, `CaravanDeparted`, `CaravanArrived`, `CaravanTraded`, `CaravanRobbed`,
  `CaravanReturned`; `HappinessFactor::War`, `LedgerCategory::Diplomacy`,
  `PlacementError::ForeignKingdom`/`TileBlock::ForeignKingdom`.
* **Militär:** `GameState::military()` (`Military`: `soldiers()`/`soldier(id)` → `Soldier` mit
  Einheit, Stufe Ausbildung/Bereit, Kompanie, Posten, Gesundheit, Moral, Ausrüstung, Erfahrung,
  Abschüssen, Dienst, Feldzug; `companies()` → `Company` mit Name, Mitgliedern, `Order`;
  `warbands()` → `Warband` (Räuber oder Heer eines Reichs, Stufe Anmarsch mit Ankunftstick /
  Angriff / Flucht / Abzug nach Frieden (`WarbandStage::Withdrawing`, `is_hostile()` falsch),
  Kämpfer mit Reisenden, Beute und Gold, Ziel, `fought()`); `campaigns()`; `battles()` →
  `BattleReport`; `damage_of(building)`, `enemies_on_map()`, `available_count()`),
  `military::{recruit_problem, order_problem, campaign_problem, warband_eta, bandit_band,
  fighter_stats, unit_strength, strength, alert_active, order_text, fighter_name,
  fighter_count_text, unit_count_text, unit_role, DEFEND_RADIUS}` (Anzahl mit Mehrzahl aus
  `UnitDef::plural`). Bedrohungen wählen Einstieg und Ziel nach Erreichbarkeit
  (Zusammenhangskomponenten des Wegrasters; Tore gelten als passierbar und werden aufgebrochen),
  laufen zum Eingang oder einem freien Feld am Gebäude und wiederholen gescheiterte Wegsuchen;
  ohne Weg und ohne Mauer nehmen sie ein anderes erreichbares Ziel oder ziehen ab.
  Berichte: `report::{military_report, recruit_options, threats_report, battle_reports,
  war_status, campaigns_report, rank_report}`. Läufer: neue `CitizenRole::{Archer, Knight, Siege,
  Raider}` (`sprite_key` `archer`, `knight`, `siege`, `raider`; Soldaten nach Einheitenklasse),
  `CitizenView::hostile` (Feinde markieren), `Traveler::warband()`, `Citizen::soldier_unit()`,
  `Destination::Tile(p)`, `Activity::{Training, OnDuty, Fighting, Campaign}`,
  `DeathCause::Battle`, `DamageCause::Attack`. Kommandos `Recruit`, `DismissSoldiers`,
  `CreateCompany`, `AssignSoldiers`, `DisbandCompany`, `CompanyOrder`, `DebugSpawnThreat`;
  Ereignisse `SoldiersRecruited`, `SoldierTrained`, `SoldierDismissed`, `CompanyChanged`,
  `CampaignChanged`, `ThreatApproaching` (mit Ankunftstick), `ThreatArrived`, `EnemyFell`,
  `BattleEnded`, `StructureDestroyed`, `RankAchieved`, `GameOver`.
* **Ränge und Niederlage:** `GameState::progress()` (`rank()`, `reached()`, `game_over()`),
  `GameState::game_over()`, `ranks::{current_rank, requirements, wealth, counted_buildings,
  prestige, defeat_text}`, `Defs::ranks()`.
* **Darstellungshilfen (Sim-Lane Schritt 6):**
  * Anwesende: `Population::occupants(building)` – Bewohner und Reisende im Gebäude am Ende des
    letzten Ticks, O(1) (beleuchtete Fenster, Gebäudeansichten).
  * Baustellen: `buildings::site_work(state, defs, building) -> Option<SiteWork>` (`Working`,
    `Paused`, `WaitingForMaterials`, `NoBuilders`, `LunchBreak`, `BuildersOnTheWay`, `OffShift`;
    `text()` deutsch), `ConstructionSite::{progressed_at(tick), last_work_tick()}` (Staub,
    Hämmern nur, wenn wirklich gearbeitet wurde).
  * Heizen: `Population::heating(house, cold) -> Option<Heating>` (`cold`:
    `WeatherState::needs_heating`) (`Empty`, `NotNeeded`,
    `Burning`, `OutOfFuel`; `smoking()`, `text()`): Rauch nur, wenn das Haus in der letzten
    Stunde Brennstoff verbrannt hat.
  * Wind: `WeatherState::{wind_direction() (0–15, 0 = Norden/−y, im Uhrzeigersinn),
    wind_degrees(), wind_strength() (0–100), wind_vector_permille()}`.
  * Herden: `Farm::{herd(), herd_capacity(defs)}` (Tiere auf der Weide).
  * Obstgärten im Winter: `FieldView::{is_bare(defs, season), seasonal_sprite(defs, season)}` →
    `fields/<sprite>_bare`; die Chunks werden zu Winter- und Frühlingsbeginn über
    `take_field_dirty_chunks` gemeldet.
  * Orte von Verlusten: `SimEvent::StockLost { lost, buildings, position }`,
    `SimEvent::CitizensFellIll { citizens, homes, position }` (Haus mit den meisten Kranken); die
    Ereignis-Meldung trägt diesen Ort.
  * Forschung: `research::queue_plan(state, defs, tech)` (fehlende Voraussetzungen zuerst) und
    `Command::QueueResearchWithPrerequisites { tech }` (alles oder nichts).
  * Niederlage in der Vorschau: `PlacementError::GameOver`, `RoadProblem::GameOver` („Das Spiel
    ist verloren.“) stehen zuerst in Bauplatz- und Straßenprüfung.
  * Weggedächtnis: `Navigation::remembered_paths()`; `PathStats` zählt Treffer weiter.
* **Darstellungshilfen der Spiel-Lücken (Spiel-Schritt 7):**
  * Kampfbild: `Military::last_round()` → `CombatRound` (`tick()`, `shots()` mit `Shot { from,
    to: TileRect, kind: ShotKind::{Arrow, Stone}, player }`, `struck(WalkerId)`,
    `is_current(now, round_ticks)`) – die Fernkampftreffer und Schläger der letzten Kampfrunde;
    reine Darstellungsdaten (nicht gespeichert, `#[serde(skip)]`, nicht im Zustandsvergleich,
    von der Simulation nie gelesen).
  * Tätigkeit der Läufer: `CitizenView::action` → `WalkerAction::{None, Work, Attack}`: `Work`
    für Bewohner, die während der Schicht still an ihrer Arbeitsstelle (`Destination::Spot`
    ihres Betriebs) stehen, `Attack` für alle, die in der aktuellen Kampfrunde zugeschlagen
    oder geschossen haben.
  * Wirtschaftlichkeit: `report::trade_balance(state)` → `TradeBalance` (Aus- und Einfuhr je Tag
    aus dem Kassenbuch, Mittel der letzten 7 Tage, deutscher Satz) und
    `report::building_economics(state, defs, building)` → `BuildingEconomics` (Arbeitsgänge je
    Tag beim aktuellen Arbeitstempo, Wert der Erzeugnisse, Rohstoffe und Werkzeugverschleiß,
    Löhne, Unterhalt, Saldo, Herstellkosten je Einheit des Haupterzeugnisses gegen den
    Marktpreis) zu heutigen Preisen; Löhne und Unterhaltsnachlass teilen sich die Funktionen
    mit `finance::daily_rates`.
* **Sim-Lane Schritt 9 (Feinschliff: Räuber, Posteingang, Goldsenken, Texte):**
  * Räuber: `military::raid_grace_over(state, defs)` (Schonfrist je Schwierigkeit, Bedingung
    `EventCondition::RaidGraceOver`), `military::bandit_band(state, defs)` (Größe nach Einwohnern ×
    Schwierigkeit − Abschreckung), `military::raid_defenders`, `military::bandit_arrival(state,
    defs, reported_tick)` (Ankunft der gemeldeten Bande); Daten `military.bandits.{size_percent,
    defenders_per_bandit, max_deterrence_percent, grace_days, grace_population, warning_hours}`.
  * Posteingang: `diplomacy::initiatives::inbox_open(state, defs)`; Daten
    `diplomacy.ai.{max_open_proposals, first_proposal_day, proposal_gap_hours,
    early_proposal_gap_hours, early_days}`.
  * Goldsenken: `Command::HoldFestival`, `RejectReason::FestivalUnavailable { reason }`,
    `SimEvent::FestivalHeld { gold, guests }`, `report::court_report(state, defs)` →
    `CourtReport { rank, upkeep_per_day, next_upkeep_per_day, upkeep_text, festival }`,
    `report::festival_report` → `FestivalReport { cost, problem, next_day, last_day, active,
    cost_text, effects }`, `events::festival::{festival_cost, festival_problem}`,
    `ranks::court_upkeep`; `ActiveModifier::source` ist jetzt `ModifierSource` (`Event(id)` oder
    `Festival`), `MemoryCause::Festival` („Festgäste“), `Events::last_festival_day()`.
  * Texte: `report::hours_text(hours)` und `report::ticks_text(ticks)` (eine Dauerform für alle
    Texte, §2), `report::duration_text` schreibt „etwa 14 h“.
* **Sim-Lane Schritt 8 (Prüfbefunde, Ausbauketten, Langzeitlauf):**
  * Ausbau: ein Lager im Ausbau beliefert seine Baustelle selbst (`logistics::supply_own_upgrades`
    vor jeder Planungsrunde) und ist weder Quelle noch Ziel neuer Transporte (`carriers_of` = 0);
    Gebiet folgt der Grundfläche beim Start und Abbruch. `buildings::upgrade_service_gaps(state,
    defs, id)` → `Vec<ServiceGap { service, houses }>` mit `text()` („Während des Ausbaus fehlt
    12 Häusern die Versorgung „Markt“.“). `BuildingDef::lineage`/`descends_from(kind)`:
    Forschungsboni und Ausbildung gelten auch für Ausbaustufen (`BonusTarget::matches(def)`).
    20 neue Stufe-2-Typen (82 Gebäudetypen).
  * Qualifikation: `population::training_permille(rules, citizen, category, tier)`,
    `production::worker_permille(defs, def, citizen)` (Gesundheit × Zufriedenheit × Einarbeitung),
    `production::crew_training_permille`; `Citizen::{experience_days, sick_since,
    is_on_sick_leave}`; `ProductionReport::training_percent`, `CitizenDetail::{sick_leave,
    training: Option<TrainingInfo { days, percent, text }>}`.
  * Statistik: `Statistics::seasons()` → `SeasonHistory { samples(), stride() }` mit
    `SeasonSample` (je Jahreszeit; `year()`, `season()`), `PopulationDay::savings`.
  * Handel: `economy::trade::trade_percent(direction, margin, bonus)` – Bonus höchstens bis zur
    Marge (Einkauf nie unter Verkauf).
  * Status `ProductionStatus::NoPlantingSpace` („Kein freier Platz zum Pflanzen“, Försterhaus im
    dichten Wald; gilt als untätig, eine Meldung je Episode).
  * Robustheit: `population::MAX_PURSE_MILLI`, gesättigte Summen in Wohlstand und Bericht,
    `Campaign::provisions_eaten` gesättigt, Soldaten-Stunden beim Laden begrenzt.
* **Sim-Lane Schritt 7b (letzte Punkte, Grenzfälle, Balance):**
  * Kampfbild mit Hieben: `CombatRound::strikes()` → `Strike { attacker: WalkerId, from, target:
    StrikeTarget::{Walker(WalkerId), Building { building, tile }}, to: TileRect, player }`
    (Hiebe gegen Kämpfer, dann die Schläge der Feinde gegen das angegriffene Gebäude – sie zählen
    auch als Schlag), `CombatRound::target_of(walker)` (Ziel des ersten Hiebs, sonst des ersten
    Schusses), `Shot::shooter: Option<WalkerId>` (`None`: Turm). `CitizenView::facing` zeigt bei
    `WalkerAction::Attack` zum Ziel. Weiterhin reine Darstellungsdaten.
  * Gemessener Ertrag: `ProductionState::output_log()` → `OutputLog` (`today()`, `yesterday()`,
    `days()` → `DayOutput { cycles, goods }`, `units_of(resource)`, `units()`;
    `OUTPUT_HISTORY_DAYS` = 24). `BuildingEconomics` hat neu `basis: EconomicsBasis::{Crew,
    Harvests { days }}` und `measured: MeasuredOutput` (Arbeitsgänge und Einheiten gestern und
    heute bisher, möglich je Tag, Mittel über die gespeicherten Tage, Wert, deutscher Satz);
    Feldfarmen und Obstgärten liefern jetzt Wirtschaftlichkeit (Ernten im Mittel, `recipe: None`).
  * Fristen: `report::deadline_clock_text(now, until)` („18:00 Uhr“, „morgen 06:00 Uhr“, …);
    `PendingEventReport::deadline_text` enthält nur noch Uhrzeit und Standardwahl
    („Entscheidung bis morgen 06:00 Uhr, sonst gilt „Kein Interesse“.“), die Restzeit ist
    `minutes_left`. Mengen-Helfer `report::{missing_gold, missing_goods, tiles_text,
    tiles_dative_text}`.
  * Feldzüge: `Campaign::provisions()`, `Campaign::provisions_eaten(defs)`.
* **Sim-Schritt 7 (Lücken der Vollständigkeitsprüfung):**
  * Ausbau: `buildings::upgrade_target(defs, kind)` → (`UpgradeDef` mit Kosten/Stunden, Zieltyp),
    `upgrade_blockers` für jedes Gebäude (`NoUpgrade`, `MaxTier`, `MaxCountReached`, …),
    `Building::{is_upgrading, trade_staff}`, `ProductionStatus::Upgrading` („Wird ausgebaut“);
    `BuildingDef::upgrade` ersetzt `HousingDef::upgrade_to`.
  * Wohlstand: `Household::purse()` → `Purse` (`savings`, gestern/heute verdient/ausgegeben),
    `population::wealth_class` → `WealthClass` (Arm/Auskömmlich/Wohlhabend),
    `HappinessFactor::Wealth`, `HouseholdRow::{savings, wealth_label, earned_yesterday,
    spent_yesterday}`, `CitizenDetail::{household_savings, wealth_label, friends, sick}`,
    `PopulationSummary::{wealth, taverns}`.
  * Gasthaus: `report::shop_report(state, defs, building)` → Bestand je Ware und `problem`
    („Kein Bier vorrätig“).
  * Handelsstraßen: `economy::trade::trade_route_percent(state, defs, post)`,
    `Caravan::route` (`RouteTally::bonus_percent`), `diplomacy::trade_road_bonus_at`,
    `TradeReport::{trade_road_percent, trade_road_text}`.
  * Tagesablauf: `Activity::{Sick, Treatment, Sheltering, Festival}`, `Purpose::{Shelter,
    Treatment}`, `population::is_sick`, `Events::festival_active`, `ModifierKind::Festival`.
  * Temperatur: `WeatherState::{temperature_c, temperature_text, needs_heating, is_frost}`,
    `WeatherReport::{temperature_c, temperature_text, heating_needed, frost}`;
    `Population::heating(house, cold)` nimmt jetzt `needs_heating` statt der Jahreszeit.
  * Texte: `report::since_days_text` („seit heute“, „seit 1 Tag“, „seit 3 Tagen“).
  * Spielkennung: `GameState::game_id()`, `state::game_identity(settings)`,
    `SaveHeader::game_id`.
* **Entwickler-Kommandos** (das Spiel sendet sie nur hinter einem Entwicklerschalter):
  `DebugUnlockAllResearch` (alle Technologien), `DebugSetWeather { weather, intensity, days }`
  (Wetter erzwingen, z. B. für Screenshots von Schnee; derselbe Weg dient dem Ereignis-Wetter),
  `DebugTriggerEvent { event }` (Ereignis sofort, ohne Bedingungen und Abklingzeit, solange keines
  offen ist), `DebugGrant { gold, goods }` (Gold und Waren in die Lager nahe dem Hauptquartier),
  `DebugSpawnHouseholds { count }` (1–50 Haushalte laufen vom Kartenrand ein),
  `DebugSetRelation { kingdom, value }` (Beziehung −100…+100 über eine verblassende Erinnerung),
  `DebugSpawnThreat { kingdom, size }` (Räuber oder ein Heer eines Reichs im Krieg binnen einer
  Stunde am Kartenrand).

### 3.6 Skalierung und Simulation-LOD (Sim-Lane Schritt 6)
Leitlinie: teure Arbeit je Bewohner wird **gestaffelt oder ereignisgesteuert** erledigt, nie
durch Weglassen oder Schätzen – die Wirtschaft rechnet mit denselben Mengen wie ohne LOD, nur
verteilt über Ticks. Jeder Takt ist eine reine Funktion des Ticks (Slot = Index mod Periode), damit
geladene Spiele exakt weiterlaufen.

| Arbeit | Takt |
|---|---|
| Bedürfnisse, Gesundheit, Zufriedenheit, Tagesablauf-Ziel je Bewohner | alle 20 Ticks, Slot nach Arena-Index |
| Verbrauch, Versorgung, Tagesdemografie je Haushalt | stündlich, Slot nach Index (Demografie im ersten Slot des Tages) |
| Stufenprüfung und Abstieg je Haus; automatischer Ausbau je Haus | stündlich, Slot nach Gebäudeindex (Ausbau neu gestaffelt) |
| Feldwachstum, Nachbeanspruchen, Herde, Erreichbarkeit je Farm | stündlich, Slot nach Gebäudeindex |
| Pfadsuche | Warteschlange mit Knotenbudget je Tick (10 000 + 100 je Wartendem, ≤ 25 000), lange Suchen über Ticks fortgesetzt; gemerkte Wege kosten 25 Knoten |
| Bewegung | jeden Tick nur aktive Läufer (sortierte Liste) |
| Besorgungen | gesammelt am Ende des Bewegungsschritts mit einem gemeinsamen Index |
| Logistik-Planung | alle 20 Ticks; Träger jeden Tick nach Auftrags-ID |
| Arbeitszuteilung | nur nach Änderungen (Gebäude, Bewohner, Prioritäten) |
| Wegkosten-Raster, Komponenten | inkrementell über Änderungs-Rechtecke, Komponenten nur bei Passierbarkeitsänderung |
| KI-Reiche | Wirtschaft und Wachstum täglich, Initiativen stündlich, Karawanen je Tick nur ihre Stufe |
| Kampf | alle `round_ticks` (10), nur solange Feinde auf der Karte sind |

**Messstadt** (`scale_tests::scale_performance`, Release, `--test-threads=1`, 4 geteilte Kerne):
Karte „Groß“ 256², 5 KI-Reiche, 999 Gebäude (506 Häuser, 61 Märkte, 46 Lager, 276 Werkstätten
und Höfe, 12 Feldfarmen, Kaserne, Kontor), 5000 Bewohner in 1667 Haushalten, 130 Soldaten, Handelsaufträge
für alle Waren (4 Karawanen gleichzeitig), Krieg mit einem aggressiven Reich, Heer von 120 Mann,
das gegen die Kompanien kämpft; 14 400 Ticks (3 Spieltage) nach einer Stunde Anlauf:

| Messung | Wert |
|---|---|
| Tick Ø / Median / p99 / max | 399 µs / 313 µs / 1,48 ms / 4,76 ms (0 Ticks über 8 ms) |
| größte Schritte Ø | Bevölkerung 123 µs, Produktion 63 µs, Bewegung 51 µs, Wirtschaft 40 µs, Bau 38 µs, Logistik 30 µs |
| größte Einzelspitzen | Bevölkerung 3,5 ms, Logistik 3,4 ms, Produktion 3,1 ms, Bau (Arbeitszuteilung) 3,0 ms, Bewegung 2,9 ms |
| Läufer / Pfad-Warteschlange | bis 3095 gleichzeitig unterwegs; Warteschlange bis 426, Wartezeit Ø 1,4 Ticks, max 7; 85 % Treffer im Weggedächtnis |
| Speicher | RSS 29 MiB (7 MiB vor dem Aufbau) |
| Spielstand | 1,60 MiB; Kodieren 4,7 ms, Laden mit Prüfung und Ableitung 16,9 ms |

`walking_performance` (2045 Bewohner, lange Arbeitswege auf einem Straßengitter): Wartezeit in
der Pfad-Warteschlange Ø 39 → 12 Ticks, max 166 → 98 Ticks durch das gespeicherte Weggedächtnis;
Tick Ø 182 µs. Die Ziele (Ø < 2 ms, max < 8 ms je Tick) sind erfüllt.

**Arbeitszuteilung** (Sim-Lane Schritt 7b, `scale_tests::allocation_performance`, Release, Messstadt
mit 5001 Bewohnern, 3334 im arbeitsfähigen Alter, 969 Stellen): Die Zuteilung bleibt eine
vollständige Neuberechnung, ist aber billiger – das Hauptquartier als Wohnort der Obdachlosen
wird einmal statt je Bewohner gesucht, Bedarf je Gebäude über einen Index statt einer Map, keine Kopien bei Gebäuden ohne
Überschuss, keine Suche ohne freie Stelle, die Arbeitslosen gruppiert nach Haus (eine Entfernung
je Haus und Gebäude, Auswahl der k Nächsten mit `select_nth_unstable` statt k-mal das Minimum).
Ein Differenztest gegen die frühere Implementierung (`workforce_reference.rs`, 24 Zufallsstädte
× 30 Änderungen, 3216 bewegte Stellen) ergibt Byte-gleiche Zustände.

| Fall | vorher Ø (max) | nachher Ø (max) |
|---|---|---|
| nichts geändert | 178 µs (263) | 78 µs (139) |
| ein Betrieb schließt, einer öffnet | 233 µs (492) | 171 µs (246) |
| zwanzig schließen, zwanzig öffnen | 601 µs (3789) | 233 µs (310) |
| alle suchen neu Arbeit | 9557 µs (18 396) | 1265 µs (2627) |

## 4. Spiel-Binary (`kingdoms`)
Das Binary stellt nur dar und sendet Kommandos; Spielregeln liegen ausschließlich in `kingdoms_sim`.

* `game`: `GameShared` (Aktionsbelegung, Einstellungen, Debug-Schalter, **`Arc<Defs>`** – beim
  Start einmal aus `data/` geladen; ungültige Daten brechen den Start mit der vollständigen
  Problemliste ab). `settings_panel` zeigt das Einstellungsfenster, reicht Audio- und
  Grafikänderungen sofort an die Engine weiter und speichert beim Schließen.
* **Szenen und Start:** `scenes::boot` (Ladebildschirm, Atlanten asynchron, 12 ms Upload je Frame)
  → `scenes::main_menu` oder – bei Entwickler-/Testvariablen (`StartMode`, `KINGDOMS_START`) –
  direkt `scenes::loading` → `scenes::world`.
  * `main_menu`: Hintergrund = eigene kleine Session (Seed 7, Klein, Entwicklerstadt nach zwei
    Tagen, klares Wetter; im Worker erzeugt, `WorldView` ohne Gebietsgrenze, läuft weiter, Kamera
    driftet auf einer Ellipse rechts der Knöpfe, 1,5 s Einblendung); Titel, Knöpfe (Fortsetzen
    = neuester ladbarer Spielstand, Neues Spiel, Spiel laden, Einstellungen, Credits, Beenden),
    Seiten als Pergament-Flächen über dem Hintergrund; Pfeiltasten/W/S, Eingabe, Esc.
  * `loading`: Worker-Thread für neues Spiel (`Session::start`), Entwicklerstart
    (`Session::launch` mit Fortschritts-Rückruf, Messstadt-Daten) oder Spielstand (Datei lesen →
    `save::decode` prüfen), Stufen mit Status, Fehler mit deutscher Meldung und „Zurück zum
    Hauptmenü“; die Kartenansicht wird danach auf dem Main-Thread gebaut (ein Frame zeigt die
    Stufe vorher), `ctx.set_loading` während der ganzen Zeit.
  * `world`: zusätzlich Pausenmenü-Dialoge als egui-Modals (Speichern/Laden über `SaveBrowser`,
    Einstellungen, Rückfrage beim Verlassen mit ungespeichertem Fortschritt inkl. Fenster-X),
    Laden im Hintergrund (Spiel angehalten, Meldung), Autospeichern (`AutosaveInterval`, nur bei
    ungespeicherten Ticks, nie während ein Speichern läuft), automatisches Anhalten
    (`pause_reason`: Ereignis mit Entscheidung, Gefahrenmeldung; eine Pause für ein Ereignis merkt
    sich das Tempo davor – `resume_after_decision` setzt es nach der Entscheidung fort, solange
    der Spieler das Tempo nicht selbst geändert hat), Einstellungen je Frame
    (Randscrollen, Kamerageschwindigkeit, Partikeldichte, Tag/Nacht, Meldungsfilter), Spielzeit.
    Beim Verlassen: laufende Speicherungen abwarten, statische Batches freigeben, Klänge stoppen.
* `settings`: `Settings` in `settings.ron` (Spiel: Autospeichern, automatisch anhalten,
  Randscrollen, Kamerageschwindigkeit, Trackpad-Wischen verschiebt die Karte (`trackpad_pan`,
  Standard auf macOS; sonst zoomt es wie das Mausrad, Zwei-Finger-Zoom immer), Meldungsfilter;
  Darstellung: Partikeldichte, Tag/Nacht;
  Tasten je Aktion als `BTreeMap`, feste Aktionen immer mit Standardbelegung), atomar gespeichert
  (temporäre Datei, `sync_all`, Umbenennen), fehlende/fehlerhafte Werte → Standard. Fenster, Grafik
  und Lautstärken bleiben in `engine.ron` (`EngineConfig`). `actions`: deutsche Namen und Gruppen
  aller Aktionen, `bindings_overlap`/`conflicts` (gleiche Taste und Modifikatoren überschneiden
  sich; eine Taste ohne Modifikatoren feuert auch mit ihnen – auf macOS nicht mit Cmd), deutsche
  Tastennamen (auf dem Mac „Ctrl“, „Option“, „Cmd“). Standardbelegung auf macOS zusätzlich
  Cmd+S/Cmd+L/Cmd+P/Cmd+D für F5/F9/F12/F3 und ⌫ für den Abriss (`default_bindings_for`).
* `session`: **`Session`** = laufendes Spiel: `GameState` + `Defs`, Kommando-Warteschlange
  (`queue_command`, angewendet zu Beginn des nächsten Ticks), `tick()` (genau ein
  `GameState::tick` je Engine-Tick; das Engine-Tempo Pause/1×/2×/4× bestimmt die Tickrate),
  begrenztes **Ereignis-Log** (`EventLog`, Ringpuffer 2048, Leser mit eigenem `EventCursor` und
  Meldung verpasster Ereignisse), gemessene Tickzeiten. `LaunchOptions` aus Umgebungsvariablen
  (Seed, Kartengröße, Warp-Ticks, Entwickler-Siedlung, Kamera/Auswahl, siehe README);
  `dev_settlement` legt die Test-Siedlung nur über echte Kommandos an (`DevPlan::Showcase` für
  `=1`; `DevPlan::Town` für `=2`: eine kleine, über ein Jahr lebensfähige Stadt – stündlich das
  nächste Ziel von `town_goal` per `PlaceBuildingWithAccess` an einer Stelle, die
  `check_placement` und `plan_access_road` erlauben, nur so viele Arbeitsplätze wie Erwachsene,
  Brennholzreserve ab Herbst; begrenzter Auftrag für das Sägewerk, täglich ein Arbeitsgang der
  Köhlerei; vorab `DebugUnlockAllResearch` und zwei Handelsaufträge; ein Test simuliert ein Jahr).
  Entwicklerschalter `KINGDOMS_DEV_UNLOCK` (Forschung) und `KINGDOMS_DEV_WEATHER`
  (`DebugSetWeather` nach den Warp-Ticks, ein zusätzlicher Tick, damit schon das erste Bild das
  Wetter zeigt), `KINGDOMS_DEV_RESEARCH` (`QueueResearch` je Technologie, ein Tick),
  `KINGDOMS_DEV_EVENT` (`DebugTriggerEvent`, ein Tick, mit Wahl zusätzlich `ResolveEvent` und ein
  Tick; Ort im Log), `KINGDOMS_SPEED` (Starttempo, `pause` hält genau nach den Warp-Ticks an),
  `KINGDOMS_PANEL` (Bauleiste oder Seitenfenster beim Start, von der Weltszene aufgelöst).
  `dev_crowd` (`KINGDOMS_DEV_CROWD=<n>`): Messstadt für viele Läufer – die Spieldaten werden
  gelesen und im Speicher um ein Szenario `dev_crowd` ergänzt (`Defs::from_files`; dichtes
  Raster aus 2×2-Gebäuden mit einfeldigen Gassen um eine Burg, auf deren Land; die Welt wird
  dafür einmal zur Probe erzeugt, um den Startpunkt zu kennen), die Simulation startet es wie
  jedes Szenario. Eigener Daten-Fingerabdruck, Spielstände daraus laden nur im selben Modus.
  `dev_showcase` (`KINGDOMS_DEV_SHOWCASE=1`) ebenso: Szenario `dev_showcase` mit einer Burg und je
  einem fertigen Gebäude jeder Art auf Ringen von 5×5-Zellen um sie (Produktion und
  Landwirtschaft innen, dann Öffentliches und Militär, außen ein Haus je Stufe und Stadthäuser für
  alle Arbeiter), allen Technologien, den Zutaten aller Rezepte, Nahrung und Brennholz, ohne
  Rivalen; die Weltszene schaltet die Zufallsereignisse aus. `Session::launch` schreibt jedes
  Gebäude mit Eingangs-Tile ins Log.
* `scenes::world`: Weltszene – Kamera, Tasten, Hover/Auswahl, Werkzeuge, HUD, Klänge,
  `fixed_update` → `Session::tick`, `render` → Synchronisation der Kartenebenen und Zeichnen.
* `view`: Darstellung des Simulationszustands.
  * `world_view`: alle Ebenen + Änderungsverfolgung: Dirty-Chunks der Welt → Vegetation und
    Minikarte immer; Terrain, Felder, Straßen und Gebietsgrenze nur, wenn sich Geländeart oder
    Höhe des Chunks samt Randreihe geändert haben (`ground::GroundSignatures`: genaue Kopie
    dieser Bytes je Chunk – wachsende oder gefällte Bäume, die häufigste Änderung, bauen so
    nicht mehr vier Ebenen neu), Feld-Chunks (`take_field_dirty_chunks` → Feldebene),
    Bau-Chunks (`take_structure_dirty_chunks`, nur abgeholt, wenn sich `structure_revision`
    geändert hat → Straßenebene), `TerritoryChanged` → Grenze, Jahreszeit (Vegetation). Setzt je
    Frame das Umgebungslicht = Wetterlicht aus `effects` × Tageslicht aus `daylight` und die
    Umgebung aus `effects`; die Debug-Statistik zählt die neu gebauten Chunks je Art. Jede
    Ebene wird als eigener Abschnitt gemessen (`DrawTimes`, `draw: …`; feste Tabelle ohne
    Speicheranforderung). Picking:
    Tile, Gebäude (mit Tiefe) und Bürger – ein Bürger gilt nur, wo kein deckendes Gebäudepixel
    vor ihm liegt (durch Lücken zwischen Türmen oder über Dächern bleibt er wählbar).
  * `citizens`: **Läufer** (Bewohner und abwandernde Reisende) aus `GameState::citizens_in` für
    die Tiles des Sichtbereichs (räumlicher Index der Simulation, Chunk-genau; danach
    Bildschirm-Culling). Position zwischen `prev_pos` und `pos` mit dem Interpolationsfaktor
    `Time::alpha` (Pause: aktueller Tick), Sprung über mehr als ein Tile wird nicht
    interpoliert. Höhe = Höhenstufe des Tiles, innerhalb von 0,35 Tiles einer Kante zu einem
    Nachbarn anderer Höhe (höchstens 1,5 Stufen, sonst Klippe) linear zur Mitte beider – stetig
    über die Kante; Brücken: Wasserstufe + Brückendeck (6/16 Stufe). Tiefe wie jedes aufrechte
    Objekt am Fußpunkt (Häuser davor verdecken, davor verdeckt der Läufer das Haus), auf
    Brücken an der Vorderkante der Brücke (sonst verdeckte das Geländer den Läufer).
    Bilder aus den Animationen des `characters`-Atlas über `AssetManager::animation`
    (`<rolle>_<ne|se|sw|nw>_walk` nach `CitizenView::role`/`facing`, `Direction::sprite_dir`);
    das Laufbild folgt dem zurückgelegten Weg (ein Bild je 0,5 Tiles = 8 Bilder/s bei
    Grundtempo 0,2 Tiles/Tick), im Stand Bild 1 (Beine geschlossen). Wer steht und handelt
    (`CitizenView::action` der Simulation), spielt die Schleife seiner Rolle in Spielzeit, je
    Läufer versetzt: `<rolle>_<richtung>_work` an der Arbeitsstelle (Bauarbeiter hämmern,
    Bauern hacken), Sammler mit dem Werkzeug ihres Arbeitsplatzes (`GatherTool::of` nach dem
    Sammeln des Gebäudetyps, je Typ einmal beim Laden: Holzfäller Axt = `_work`, Steinbruch und
    Minen `_pickaxe`, Lehm-/Sandgrube und Förster `_shovel`, Fischerei `_rod`; fehlt die Schleife,
    die Axt), `<rolle>_<richtung>_attack` im Kampf (Speer,
    Bogen, Schwert, Axt, Katapult). Ein weicher Bodenschatten (`shadows`) unter jedem Läufer. Getragene Ware (`carried`) als Warensymbol (10 px) seitlich in der Hand – in
    den Vorderansichten vor, in den Rückansichten hinter dem Körper. LOD unter Zoom 0,45: nur
    Standbild, keine Ware. Markierung (weicher Ring) unter dem Bürger unter der Maus (weiß) und
    dem im Bürgerfenster (gold). Die gezeichneten Läufer bleiben für das Picking (Trefferfläche
    mindestens 16 Bildschirmpixel, vorderster gewinnt). Zeitmessung „citizens“ im
    Debug-Fenster; Abschnitt „Bewohner & Tageszeit“ (abgefragt/gezeichnet, mit Ware, LOD,
    `walking_summary`, Tageszeit, Sonne, Licht, beleuchtete Gebäude).
  * `daylight`: **Tag und Nacht** (reine Darstellung): Schlüsselfarben Nacht (Mondblau) →
    Morgendämmerung (bei Sonnenaufgang) → Morgenlicht (+1 h) → Tageslicht (+2,5 h) …
    Tageslicht (Sonnenuntergang −2 h) → Abendsonne (−0,5 h) → Abenddämmerung (bei
    Sonnenuntergang) → Nacht (+1,25 h), dazwischen weich (Smoothstep). Sonnenauf- und
    -untergang je Jahreszeit (Frühling 5:30–20:00, Sommer 4:45–21:15, Herbst 6:15–19:00,
    Winter 7:45–16:30; die Simulation kennt keine Tageslänge, ihre Arbeitszeiten bleiben);
    gewechselt wird um Mitternacht, also ohne Sprung. Das Licht multipliziert das Wetterlicht
    und wirkt nur auf die Welt (nicht auf die egui-Oberfläche, nicht auf emissive Sprites);
    Partikel werden mitgetönt. `night` (0 Tag – 1 Nacht) steuert die Fensterlichter und den
    Klang.
  * `lights`: **beleuchtete Fenster** nachts: je Gebäude-Sprite mit Fenstern ein Nachtlicht
    aus dem `lights`-Atlas (gleicher Anker wie das Gebäude), transparent, emissiv, Deckkraft
    nach Dämmerung, Tiefe der Wände darunter (`sub_span` + 0,004). Leuchtet, wenn Menschen
    drinnen sind (aus `motion().inside()` einmal je Tick in ein flaches Feld gezählt; nur
    Gebäude im Bild werden betrachtet) oder die Produktion läuft;
    Häuser nur, solange die Bewohner wach sind (alle bis 22 Uhr, dann je Haus eine Schlafenszeit
    bis Mitternacht und eine Aufstehzeit 4:30–6:00, jedes zehnte Haus bleibt wach),
    stillgelegte Gebäude nie.
  * `terrain`: Terrain-Chunks (statische Batches, Übergangsränder, Klippen, Kantenlicht); Land
    und Ränder mit `FLAG_GROUND` (Schnee, Nässe).
  * `vegetation`: Bäume (Art × Jahreszeit × Wachstumsstufe laut `nature::tree_stage`), Baumstümpfe,
    Büsche, Felsbrocken, Vorkommen-Markierungen; statische Batches je Chunk, deterministischer Versatz,
    Spiegelung, Tönung und Windphase je Tile. Im Herbst zusätzlich je Chunk die Kronen der
    Laubbäume (`LeafSource`) für fallendes Laub.
  * `roads`: Straßen (16 Masken je Typ aus `GameState::road_mask`, `FLAG_GROUND | FLAG_TRODDEN`)
    und Brücken; statische Batches, neu gebaut nur für die Bau-Chunks der Simulation (inklusive
    der Nachbarn, deren Masken sich ändern) und Chunks mit geändertem Terrain.
  * `fields`: Feldebene aus `agriculture().fields_in(chunk)` und `FieldView::seasonal_sprite`
    (`fields`-Atlas, Anker in der Tile-Mitte; im Winter kahle Obstbäume `orchard_bare`, deren
    Chunks die Simulation zu Winterbeginn und -ende meldet); statische Batches je Chunk, neu gebaut nur für Feld- und
    Terrain-Chunks. Weiden und Brache sind flache Decals über dem Terrain (Bias wie Vorkommen),
    Feldfrüchte und Obstbäume Bodenbewuchs (`ground_cover`, `FLAG_DEPTH_MAX`): die Pflanzen über
    dem Tile überdecken Bäume und Gebäude dahinter und werden von allem davor verdeckt. Schnee
    und Nässe auf der Raute, Schnee auf grünen Pflanzen; Feldfrüchte und Obstbäume wiegen sich im
    Wind (`FLAG_WIND` am Boden-Sprite, Phase je Tile).
  * `animals`: Weidetiere: je Weidehof so viele Kühe (Viehzucht) oder Schafe, wie die Simulation
    als Herde führt (`Farm::herd`), jedes Tier wandert zwischen Punkten auf und um ein
    „Heimat“-Tile seiner Weide und grast dazwischen – die Positionen sind eine reine Funktion
    von Farm, Tile-Liste und Spielzeit (Ticks + Interpolation), stehen also bei Pause still und
    speichern nichts. Laufbilder aus den `<rolle>_<richtung>_walk`-Animationen, Bodenschatten.
  * `shadows`: weiche Bodenschatten (`overlay/blob_shadow`) unter Läufern, Weidetieren, Bäumen
    und Büschen – transparente Boden-Decals (`SHADOW_BIAS` zwischen Straßen und Grenzen), etwas
    nach rechts versetzt (Licht von links oben), Deckkraft nach Tageslicht und Wetterlicht
    (`shadow_alpha`: volle Sonne 1, Nacht 0,25, Sturmhimmel ≈ 0,35). Die Baumschatten sammelt die
    Vegetationsebene je Chunk (`TreeShadow`).
  * `projectiles`: Pfeile und Steine der letzten Kampfrunde (`Military::last_round`): je Schuss
    von der Position des Schützen (Soldat in Handhöhe, Turm/Garnison an der Spitze) zum Ziel auf
    einem flachen Bogen, Flugdauer nach Entfernung (kürzer als eine Runde), in Spielzeit;
    Pfeil-Sprite nach Flugrichtung (`effects/arrow_<0..15>`), Steine der Belagerungsgeräte
    höher (`effects/stone`).
  * `effects`: Wetteroptik und Partikel aus dem Zustand: `WeatherLook` je Wetter und Stärke
    (Licht, Schneedecke und Nässe aus `WeatherState`, Regen-/Schneedichte) und nach dem Wind der
    Simulation (`ScreenWind`: `wind_vector_permille` auf den Bildschirm projiziert,
    `wind_strength`): Schwanken, Neigung und Böenrichtung der Pflanzen, Drift von Rauch, Laub,
    Regen und Schnee – derselbe Wind, der Brände weiterträgt; über ≈ 2,5 s weich nachgeführt.
    Bildschirmpartikel für Regen (Schräge aus dem Wind, bei Wind nach rechts gespiegelter
    Streifen; Sturm: mehr, schneller) und Schnee; Weltpartikel für Rauch (Emitter-Punkte der
    Sprites aus `assets/atlases/emitters.ron`, gespiegelt mit dem Sprite; Werkstätten, solange
    sie arbeiten, Häuser, deren Ofen brennt – `Population::heating` mit Bewohnern, kaltem Tag
    und Brennstoff, ein Haus ohne Holz bleibt kalt –, andere Gebäude an kalten Tagen,
    stillgelegte nie), Staub über
    Baustellen mit Bauarbeitern, die vorankommen, und fallendes Laub von Laubbäumen im Herbst
    (Kronen aus der Vegetationsebene, bei vielen Bäumen ausgedünnt). Zeitmessung „effects“ im
    Debug-Fenster, Abschnitt „Wetter & Partikel“ mit Partikeln, Emittern, Quellen und Optikwerten.
  * `buildings`: Gebäude, Baustellen, Ausbau (Gerüst der Zielgröße + aktuelles Haus davor),
    Arbeitsschleifen arbeitender Betriebe (`buildings/<id>_work`: Mühlenflügel, Essenfeuer;
    über `AssetManager::animation`, in Spielzeit, je Gebäude versetzt), Winterbild
    (`buildings/<id>_winter`: Obstgarten mit kahlen Bäumen),
    Ruinen (`rubble_<B>x<T>`, solange der Wiederaufbau auf Stufe 0 steht), Mauern/Tore (`wall_mask`), Status-Tönung; jedes Frame aus dem Zustand mit Culling; Rasen und
    Hecken der Grundstücke werden bei Schnee weiß (`FLAG_SNOW_ON_GREEN`), Dächer und Oberseiten
    bekommen die Schneedecke über ihre Schneemarke (`FLAG_SNOW_ON_ROOF`, `buildings::SNOW_FLAGS`).
    Sprite-Ausrichtung: ein Sprite je Typ deckt `(x: T, y: B)` ab, gespiegelt `(x: B, y: T)`;
    nicht-quadratische Typen richten sich nach der gedrehten Grundfläche, quadratische werden
    bei Eingang Nord/Ost gespiegelt. Picking über Belegung und die deckenden Pixel des Sprites
    (Silhouette + Alpha-Maske) mit Tiefenvergleich.
  * `overlay`: Kanten-/Flächen-Decals (Gebietsgrenze, Auswahl, Hover, Raster).
  * `picking`: `Selection` (Gebäude oder Tile, für das Info-Fenster), `Hover`, Tile-Picking mit Höhen.
  * `camera`: RTS-Kamera.
  * Tiefen-Bias-Leiter der Bodendecals (Abstände ≥ 0,003 zu opaken Nachbarn): Vorkommen <
    Straßen < Schatten < Gebietsgrenze < Raster < Hervorhebungen.
  * `preview`: Vorschau der Werkzeuge – Geist des Gebäudes (`GhostLook`: grün, bernsteinfarben
    für einen freien Bauplatz, den kein Lager erreicht – `tools::site_unsupplied`, Bauwarnung
    `NoStorageInReach`, im Tooltip rot statt „Bauplatz geeignet“ –, rot; immer sichtbar), Feldstatus
    je Tile (frei/Rodung/blockiert mit Kreuz), Eingang, vorgeschlagene Zufahrt, geplante Straßen
    als transparente Straßen-Sprites (ungültige Felder rot), Abriss-Markierungen.
* `tools`: **Werkzeug-Zustandsautomat** (`ToolController`, ohne Rendering, unit-getestet):
  Inspizieren, Bauen (Typ + Drehung), Straße (Ziehen), Abriss, Versetzen. Eingabe je Frame als
  `ToolInput` (gefiltert: Mausdruck nur über der Karte, Loslassen immer), Ausgabe `ToolOutput`
  (Kommandos für die Warteschlange, UI-Klänge, Abriss-Rückfrage, Auswahl, Hinweis). Vorschauen
  stammen aus den Prüfungen der Simulation und sind gecacht: `check_placement`/`check_move` bei
  neuer Position/Drehung und alle 10 Ticks, `plan_access_road` (Zufahrts-Vorschlag) bei neuer
  Position, wenn sich das Urteil der Prüfung ändert, sonst alle 100 Ticks, `plan_road` nur bei
  neuem Endfeld. Mit Zufahrt sendet der Klick `PlaceBuildingWithAccess`: die Simulation plant
  die Zufahrt im selben Tick neu und baut sie nur, wenn das Gebäude angenommen wurde.
  Ungültige Klicks gehen trotzdem als Kommando an die Simulation (maßgebliche Ablehnung →
  Hinweis); das Werkzeug bleibt dann aktiv. Erstattungsvorschau, Stilllegen und Versetzen
  fragen die Simulation (`buildings::demolish_refund`, `pause_allowed`, `move_allowed`); nur
  die Abriss-Rückfrage (Hauptquartier, bewohnt, Waren im Gebäude, ≥ 100 Gold) ist eine
  UI-Entscheidung.
* `ui`: Theme (Pergament, dunkles Holz, Goldakzente; Cinzel/Alegreya) und Widgets
  (`resource_chip`, `icon_button`, `progress_bar`, `mini_bar`, `section_header`, gemalte
  Statuszeichen `mark`), Debug-Fenster (F3).
  * `chart`: wiederverwendbares Diagramm im Theme – Linien und gestapelte Balken (positive Werte
    nach oben, negative nach unten) über gemeinsamen Zeitschlitzen, „schöne“ Achsenteilung
    (1/2/2,5/5 × 10ⁿ, bei Stückzahlen und Gold nur ganze Schritte), deutsche Zahlen
    („1.250“, „12,5 Tsd.“), Legende, Hover-Markierung mit allen Werten des Schlitzes;
    Skalierung, Beschriftungsabstand und Trefferprüfung sind reine, getestete Funktionen.
  * `hud`: liest nur die Session und gibt `HudAction`s zurück (Kommando, Werkzeug, Kamera, Tempo,
    Menü, Speichern/Laden, Fenster, Ereignis-Dialog, Raster/Gebietsgrenze, Klang); die Weltszene
    führt sie aus. **Anordnung nach Master-Spezifikation §13:** oben die Leiste, links die
    Bau-Leiste mit Bauleiste daneben, rechts die Leiste Ereignisse/Meldungen/Übersicht, unten
    Werkzeuge/Tempo/Tageszeit, unten rechts die Minikarte. `Hud::show` zeichnet zuerst die Leisten
    (ihre Rechtecke bestimmen den Platz der Fenster), dann rechts von außen nach innen:
    Info-Fenster neben der rechten Leiste, das offene Seitenfenster links davon, die Meldungskarten
    links davon; jedes endet über der Minikarte bzw. der unteren Leiste, wenn es sie waagrecht
    überdeckt. Danach Reichsverwaltung, Bürgerfenster, Ereignis-Dialog, Tooltip, Rückfragen,
    Pausenmenü. Alles passt auf 1280 × 720.
    * `icons`: registriert die Atlanten `icons`, `roads` und `fields` als egui-Texturen
      (`Renderer::register_egui_texture`) und löst `res_*`, `bld_*`, `ui_*`, Straßenvorschauen und
      Feldstufen (`field_<sprite>`) in UV-Rechtecke auf.
    * `top_bar`: in der Reihenfolge der Spezifikation Bevölkerung/Wohnraum, Schatz (Trend aus
      `finance::daily_rates`), Nahrung, Holz, Stein, Eisen (Eisenbarren, Erz im Tooltip), bei
      genug Platz Bretter/Werkzeuge/Ziegel/Lehm, Zufriedenheit mit Faktoren, „Festtag“ während
      eines Festes, Jahreszeit, Tagestemperatur (bei Frost frostblau) und Datum – der Tooltip nennt
      Wetter, Heizen und Frost; rechts die zehn Knöpfe der Reichsverwaltung und das Menü. Tooltips mit Bestand,
      Erzeugung, Verbrauch. Zahlen aus `report::*`, höchstens alle 0,25 s neu berechnet.
    * `build_menu`: senkrechte Leiste links (Bauen = Wohnen + Öffentlich, Straßen = Straßentypen +
      Mauern/Tore, Landwirtschaft, Produktion, Militär, Forschung → Reiter Forschung); die
      Kategorien öffnen rechts daneben die Bauleiste (je Abschnitt Knopfreihen zu sechs, Tooltips
      mit Kosten, Arbeitern, Unterhalt, Rezepten, Diensten; gesperrte Typen mit Grund). `B` öffnet
      die zuletzt gewählte Kategorie. Beschriftungen werden bis 9 pt verkleinert, bis sie passen
      (`fitted_label`).
    * `bottom_bar`: Auswählen, Straße (zuletzt genutzter, sonst bester freigeschalteter Typ),
      Abreißen, Versetzen, Raster, Gebietsgrenze · Pause, 1×, 2×, 4× · Tageszeit als Halbbogen mit
      Sonne (6–18 Uhr) bzw. Mond (`dial_position`, getestet) und Uhrzeit.
    * `right_bar`: Ereignisse (rotes, pulsierendes Abzeichen bei offener Entscheidung; Klick
      öffnet dann den Dialog, sonst das Fenster mit offener Entscheidung, Wirkungen aus
      `events_report` – Modifikatoren mit Restdauer, Summen, Steuerdeckel – und Verlauf der
      letzten Ereignisse mit Wahl und Ergebnis), Meldungen (alle Benachrichtigungen der Sitzung,
      Filter „nur Warnungen und Gefahren“, Zähler ungelesener), Übersicht (`overview`). Ein Fenster
      zur Zeit, 330 Punkte breit, zwischengespeichert wie die Reiter.
    * `overview`: Bevölkerung mit Veränderung heute, Zufriedenheit mit größter Last, Festtag mit
      Restdauer (Modifikator `Festival` aus `events_report`), Schatz und
      Tagesbilanz, Nahrung mit Reichweite (`food_forecast`), Forschung, Gefahren (offene
      Entscheidung, Bankrott oder knapper Schatz, Nahrung unter drei Tagen, Ruinen, negative
      Ereignis-Modifikatoren, Steuerdeckel, Obdachlose, Unzufriedene, stillstehende Forschung;
      nach Schwere sortiert) und die letzten vier Ereignisse. Jede Zeile führt zum passenden
      Reiter, Gebäude oder Dialog. Ein Rang erscheint erst, wenn die Simulation Ränge kennt; der
      Platz „Aufgaben“ folgt mit den Herrscherrängen.
    * `event_dialog`: Karte in der Bildmitte aus `report::pending_event` (Titel, Schwere, Text,
      betroffenes Gebäude mit „zeigen“ – nennt der Text das Gebäude schon, heißt der Link nur
      „Gebäude zeigen“ (`place_link_text`) –, Fristbalken mit `countdown_text`, je Wahl eine Karte mit
      vorhergesagten Wirkungen oder dem Grund, warum sie nicht möglich ist; Klick →
      `ResolveEvent`). Öffnet sich bei jedem neuen Ereignis (Seriennummer) einmal selbst,
      „Später entscheiden“/Esc blendet aus, `E` bzw. der Knopf rechts holen sie zurück; eine
      wartende Wahl (Pause) wird angezeigt. Fußzeile nur mit der Uhrzeit der Frist
      (`deadline_clock_text`), die Restzeit steht einmal im Balken. Liegt eine der festen
      Flächen (Leisten, Seitenfenster, Info-Fenster, Meldungskarten, Minikarte;
      `hud::docked_area_on_top`) obenauf, rückt der Dialog – wie der Anfrage-Dialog – wieder
      darüber; vom Spieler nach vorn geholte Fenster (Reichsverwaltung, Bürger) bleiben vorn.
    * `management`: **Reichsverwaltung** – ein verschiebbares Fenster mit zwölf Reitern (immer
      nur einer sichtbar; Entscheidung: ein gemeinsames Fenster statt zehn einzelner, damit nie
      Fenster übereinander liegen und Tasten/Knöpfe nur den Reiter wechseln). Öffnen über die
      Knöpfe in der Leiste oder die Tasten O/L/P/F/K/J/V/N/U/C (die Taste des offenen Reiters
      schließt), Schließen auch mit Esc (nach dem Werkzeugschritt, vor der Auswahl) und dem
      Schließen-Knopf. Größe höchstens 1040 × 700 (Forschung 1240 × 700), auf kleineren
      Bildschirmen kleiner, Inhalt scrollt; die Reiterleiste hat zwei Zeilen mit benannten
      Gruppen (`Tab::ROWS`: „Wirtschaft“ – Lager, Produktion, Wirtschaft, Finanzen, Handel;
      „Volk“ – Arbeit, Bevölkerung, Landwirtschaft; „Reich“ – Forschung, Diplomatie, Militär,
      Statistik), jeder Reiter mit Symbol und Namen, bei Platzmangel 13 statt 15 pt und erst auf
      sehr schmalen Fenstern nur noch Symbole; Standardlage links der Mitte, damit das Info-Fenster sichtbar bleibt; nach dem
      Öffnen drei Frames nach oben geholt. Zeiger und Mausrad über dem Fenster wirken nie auf
      die Karte (egui-Eingabefilter der Engine). Jeder Reiter speichert seine Daten in
      `Cached<T>` und berechnet sie höchstens alle 0,25 s neu, sobald die Simulation
      weiterlief. Zeilen (`table`: feste Spalten, Zebra, Hover, ganze Zeile klickbar,
      nicht markierbare Texte) wählen Gebäude aus (`HudAction::SelectBuilding` → Auswahl +
      Kamera). Die Fußzeile zeigt den Stand und, im Pausenzustand, wartende Kommandos.
      * `economy` (Wirtschaft): Kennzahlen (Schatz, Tagesbilanz, Warenwert, Preisindex = Mittel
        aus aktuellem/Grundpreis, Engpässe, Betriebe in Arbeit, Handelsbilanz je Tag aus
        `report::trade_balance`), je Produktionskette Ertrag minus Kosten je Tag
        (`report::building_economics` summiert; Feldfarmen und Obstgärten nach ihren Ernten,
        erst ab dem ersten gemessenen Tag; der Tooltip nennt die Grundlage), Angebot und Nachfrage je
        Warengruppe (Bestand, Lagerfüllung, gestern erzeugt/verbraucht, Bilanz, Wert, Preisindex),
        Engpässe (Baustellen brauchen mehr als den Bestand, aufgebraucht bei Verbrauch, fallender
        Bestand unter drei Tagen, Eingangsware wartender Betriebe) und Überschüsse (steigender
        Bestand bei ≥ 75 % Lagerfüllung) – eine Lesehilfe dieses Reiters, keine Spielregel; Klick
        öffnet den Verlauf. Produktionsketten je Gebäudetyp aus `production_report` (arbeiten,
        ruhen, Probleme mit fehlenden Eingangswaren; Klick wählt den ersten Betrieb mit Problem).
      * `storage` (Lager & Waren): `economy_summary` (Bestand, Lagerplatz, für Baustellen
        gebunden, heute erzeugt/verbraucht/Haushalte, Trend des letzten Tages, Preis und
        Preistrend) nach Kategorie gruppiert, filter- und sortierbar; `storage_report`
        (Füllstand, volle Waren, Träger unterwegs/frei, wartende Transporte, Erreichbarkeit),
        `logistics_report` (holen ab, liefern, Rückweg, frei, Einheiten unterwegs, Heimtragen
        der Sammler, Ø Transportdauer, erledigte Transporte); Klick auf eine Ware öffnet ihren
        Verlauf in der Statistik.
      * `production`: alle Produktionsbetriebe aus `production_report`, nach Typ gruppiert
        (zuklappbar), Zustand mit Grund, Belegschaft, Produktivität, „Gestern“ aus
        `BuildingEconomics::measured` (`measured_cell`: Arbeitsgänge des letzten
        abgeschlossenen Tages gegen die mögliche Tageszahl der Belegschaft, „5 von 6,5“, nach
        Anteil gefärbt; bei Feldfarmen und Obstgärten die Ernte des Tages; Tooltip: der Satz der
        Simulation), Arbeitsgang/Auftrag, Priorität; Filter „nur Probleme“.
      * `finance`: `finance_report` – Kassenbuch je `LedgerCategory` (heute, gestern, laufende
        Rate aus `DailyRates`), Verlauf als gestapelte Balken je Kategorie mit Bilanzlinie,
        Steuersatz (Schieberegler/±, `SetTaxRate` beim Loslassen, wartender Satz aus der
        Kommando-Warteschlange), Wirkung auf die Zufriedenheit aus `happiness_report`
        (`HappinessFactor::Taxes`), laufende Steuereinnahmen; Kasten „Hof und Feste“ aus
        `court_report` (Hofhaltung des Rangs, im Unterhalt enthalten, der nächste teurere Rang aus
        `RankDef::court_upkeep`, Festkosten mit Hinderungsgrund, Knopf zum Reiter Bevölkerung).
      * `trade`: `trade_report`, Zustand des Handels (aktives Kontor, sonst Grund: nicht gebaut,
        im Bau, stillgelegt, ohne Arbeiter, nicht verbunden – aus öffentlichem Zustand
        abgeleitet) mit `trade_road_text` (Handelsstraßen-Bonus; die Weltmarktpreise rechnen ihn
        wie die Aufträge ein), Karawanen mit dem Handelsstraßen-Anteil ihres Wegs
        (`RouteTally::bonus_percent`), Hinweis, dass Boni die Marge höchstens bis auf den Preis
        des Partners schließen (`economy::trade::trade_percent`), Aufträge mit Editor (`SetTradeOrder`/`RemoveTradeOrder`; Vorschau mit
        `trade::validate_order`, ungültige Aufträge gehen trotzdem an die Simulation, deren
        Ablehnung als Hinweis erscheint), Ein-/Verkaufspreise aller Waren über
        `trade::trade_unit_price_centi` – mit KI-Königreichen (Handel nur über deren Karawanen)
        stattdessen `partner_quotes`: günstigster Einkauf bei einem Reich mit Handelszugang, das
        die Ware anbietet, bester Verkauf bei einem, das sie sucht (`report::partner_prices`,
        `diplomacy::has_access`), sonst „–“ –, letzte Geschäfte aus `SimEvent::TradeExecuted` (eigener
        Ereigniszeiger, auch bei geschlossenem Fenster).
      * `workforce`: `workforce_report` je Bereich (Bedarf, besetzt, unbesetzt, Priorität per
        `SetWorkforcePriority`), Gesamtzahlen; alle Arbeitsplätze und Baustellen je Bereich mit
        eigener Priorität (`SetProductionPriority`).
      * `population`: `population_summary` (Lebensabschnitte × Geschlecht, Haushalte,
        Wohnraum, Obdachlose, Plätze für Zuwanderer, Bewegung heute/gestern, Verläufe),
        `needs_summary`, `happiness_report` (Faktoren), Häuserliste aus `house_report` und
        `upgrade_blockers`, Wohlstand der Haushalte (`WealthSummary`: Anteile arm/auskömmlich/
        wohlhabend als Balken, Ersparnisse gesamt/Ø/Median, Einnahmen und Ausgaben von gestern,
        Faktor „Wohlstand“ mit den Grenzen aus `WealthRules`), Gasthäuser (`TavernSummary`:
        geöffnet, mit/ohne Bier), Kasten „Fest ausrichten“ aus `festival_report` (Knopf mit
        Kosten → `HoldFestival`, gesperrt mit `problem` als Grund oder solange ein Fest in der
        Kommando-Warteschlange wartet; Kosten aus `FestivalRules`, Wirkungen, „Heute wird
        gefeiert“, letzter Festtag), Bürgerliste (Filter Alle/Unzufrieden/Ohne Arbeit/Obdachlos/Ohne
        Weg/Unterwegs, Namenssuche, höchstens 150 Zeilen mit Tätigkeitstext; Klick öffnet das
        Bürgerfenster).
      * `agriculture`: `weather_report` (Wetter mit gemaltem Symbol, Stärke, Dauer, Tagestemperatur
        mit „Häuser heizen“ und „Frost“, Jahreszeit und Resttage, Schneedecke, Bodennässe, Wind,
        Wirkungen, erzwungenes Wetter mit seinem Grund aus `forced_text`), `food_forecast` (Vorrat
        bis Winterende, `days_of_stock` als Option; Balken „Vorrat 4.594 · Bedarf im Winter 1.728“,
        bei Mangel „… · 528 fehlen“, `winter_bar_text`), `agriculture_summary` (Feldfrüchte mit
        Aussaat- und Wachstumsdaten aus `CropDef`; „Zustand der Felder“ aus `average_condition`,
        gefärbt und erklärt nach den Schwellen der Ernte-Ereignisse in den Ereignisdaten,
        `HarvestThresholds`), Farmen aus `farm_report`.
      * `research` (Forschung): Baum aus `research_report` – `layout_tree` (rein, getestet): eine
        Bahn je Bereich, Spalte = längste Voraussetzungskette (alle Verbindungen laufen nach
        rechts, auch zwischen Bereichen), Zeile = Zeile der ersten Voraussetzung desselben
        Bereichs, sonst die nächste freie (Ketten bleiben gerade). Knoten gemalt (Zustandsfarbe,
        Fortschrittsstrich, Platz in der Liste), Verbindungen als Kurven (die des
        markierten Knotens golden). Klick wählt, Doppelklick reiht ein: ein einziges
        `QueueResearchWithPrerequisites` (alles oder nichts); die Vorschau der Reihenfolge ist
        `research::queue_plan` der Simulation – genau der Plan, den das Kommando ausführt. Seitenspalte:
        Auswahl (Wirkungstexte der Simulation, Kosten, Dauer/Fertig in, Voraussetzungen,
        „Ermöglicht“, Knöpfe Einreihen/Als Nächstes/Entfernen), Forschungsliste (nach vorn/hinten/
        entfernen über `MoveResearch`/`CancelResearch`), Quellen (Hauptquartier, Schulen als Link),
        Punkte je Tag. Kommandos in der Warteschlange (Pause) markieren ihre Technologie als
        „Änderung wartet“; Ablehnungen der Simulation erscheinen als Hinweis.
      * `statistics`: wählbare Verläufe – Bevölkerung, Schatzkammer, Nahrungsvorrat, eine Ware
        (Bestand, Erzeugung, Verbrauch, Handel, Preis), Zufriedenheit, Beschäftigung, Wohlstand
        (Ersparnisse aller Haushalte und je Haushalt, `PopulationDay::savings`), Gebiet und
        Bauten – über die abgeschlossenen Tage (Kassenbuch, Ressourcen- und
        Bevölkerungsstatistik, bis 48), die Stundenwerte (`statistics().hourly()`, 72 h) oder
        „Jahreszeiten (gesamt)“: die Langzeitreihe `statistics().seasons()` über das ganze Spiel
        (`SeasonSample` am Ende jeder Jahreszeit, Achse „J2 Fr“, Tooltip mit Rang; nach dem
        Ausdünnen „am Ende jeder 2. Jahreszeit“). Gebiet und Bauten gibt es nur je Jahreszeit;
        `Metric::supported` wählt für jede Anzeige einen Zeitraum, den sie hat. Der Schatz am
        Tagesende wird aus dem aktuellen Schatz und den Tagesbilanzen zurückgerechnet (jede
        Änderung des Schatzes wird gebucht; ein Test vergleicht mit den Stundenwerten um
        Mitternacht).
    * `citizen_window`: **Bürgerfenster** (verschiebbar, links neben dem Info-Fenster) aus
      `report::citizen_detail`: Name, Geschlecht, Alter, Lebensabschnitt, Tätigkeit mit
      deutschem Text (krank, im Krankenhaus, in Sicherheit, Festtag farbig mit Erklärung –
      `activity_note`), Ziel, Aufenthaltsort, getragene Ware, Haushalt, Zuhause, Ehemann/
      Ehefrau, Eltern, Kinder und Freunde (Bezeichnung nach Geschlecht, `kin_label`; Links öffnen
      deren Fenster), Ersparnisse des Haushalts mit Wohlstand, Arbeit (ohne Stelle nach einem
      Tag Krankheit „Krankgeschrieben – sucht nach der Genesung neue Arbeit“), Einarbeitung
      (`CitizenDetail::training`: „In Einarbeitung: 1 von 3 Tagen, 86 %“, „Eingearbeitet“,
      „Hilfsarbeit – keine Einarbeitung nötig“), letzter Arbeitsweg, Arbeitskraft, Bildung, Gesundheit (mit „krank“), Zufriedenheit mit Faktoren, Bedürfnisse. Knöpfe:
      Folgen (`HudAction::FollowCitizen`; die Weltszene setzt die Kamera in `render` – nach den
      Ticks des Frames – auf die interpolierte Position; Verschieben der Karte, Esc, Schließen,
      ein Kamerasprung oder das Verschwinden des Bürgers beenden das Folgen, Zoomen nicht),
      Zeigen, Zuhause, Arbeitsplatz. Geöffnet durch Klick auf einen Läufer (Prüfwerkzeug:
      `ToolInput::citizen` vor Gebäude und Feld; andere Werkzeuge ignorieren Läufer), auf
      einen Bewohner im Info-Fenster eines Hauses oder auf einen Bürger im Reiter Bevölkerung
      (`HudAction::InspectCitizen`). Schließt sich, wenn der Bürger nicht mehr lebt.
    * `farm_panel`: Farmteil des Info-Fensters für Feldfarmen, Obstgärten und Weidehöfe aus
      `farm_report`: Erklärung des Zustands (wächst, außerhalb der Saison mit Aussaatzeit der
      Frucht, kein Ackerland), Fruchtwahl mit Bild der reifen Stufe (`SetCrop`, nur die
      angebotenen Früchte), Felder bestellt/reif/brach, Wachstum, erwartete Ernte, Reifezeit
      der Frucht, Fruchtbarkeit gegenüber dem natürlichen Wert, Erschöpfung, Feuchte, Zustand,
      Scheune, laufende und letzte Ernte, laufende Arbeit, aufklappbare Einzelliste der Felder
      mit Stufenbild.
    * `info_panel`: Info-Fenster rechts neben der rechten Leiste (Gebäude oder Feld): Status mit Grund (finden die
      Arbeiter keinen Weg, „Kein Weg zur Arbeit“ rot; „Arbeiter auf dem Weg“, „Mittagspause“,
      „Feierabend“ und „Kein freier Platz zum Pflanzen“ neutral), Bau/Ausbau mit Material und Bauarbeitern (zugeteilt und an der
      Baustelle), Arbeiter und Priorität, Produktion (Arbeitsgang, Produktivität mit Faktoren –
      solange neue Fachkräfte lernen, zusätzlich „Einarbeitung“ aus
      `ProductionReport::training_percent`, in der Belegschaft enthalten –,
      Arbeitsweg aus `ProductionReport::commute` mit Einzelheiten im Tooltip, „Wartet auf
      Träger“ aus `carrier_wait`, Wegprobleme aus `path_problem`, Rezeptwahl mit Erklärung der
      Rund-um-die-Uhr-Dauern (Schichtfaktor aus `RoutineHours`), Wirtschaftlichkeit aus
      `report::building_economics` (Tooltip nach `EconomicsBasis`: mögliche Arbeitsgänge der
      Belegschaft oder Ernten im Mittel der gemessenen Tage; Herstellkosten mit Cent) und
      darunter das Gemessene (`MeasuredOutput`: Balken „Gestern N % des Möglichen“, Satz der
      Simulation, Mittel der gespeicherten Tage), Ein-/Ausgangslager, Aufträge
      +1/+5/+10/dauerhaft/leeren), Ausschank des Gasthauses (`shop_report`: Bestand/Platz, „Kein
      Bier vorrätig“), Häuser (Bewohner als Links zum Bürgerfenster, Haushalte mit Ersparnissen,
      Wohlstand und Einnahmen/Ausgaben von gestern, Bedürfnisse, Wegproblem), Lager (Träger
      unterwegs/frei, wartende Transporte, Inhalt), **Ausbau** für jedes Gebäude mit
      `BuildingDef::upgrade` (`upgrade_section`: Ziel, Beschreibung, Voraussetzungen bei Häusern,
      Kosten, Bauzeit, was sich ändert aus `upgrade_changes`, Warnung vor Versorgungslücken aus
      `upgrade_service_gaps` („Während des Ausbaus fehlt 18 Häusern die Versorgung „Markt“.“),
      `UpgradeBuilding` mit den `upgrade_blockers` als Grund (der erste darunter als Satz); die Ausbau-Sperre nur bei Häusern; Baustellentitel
      „Ausbau: Großes Sägewerk“), Gebiet/Dienste; Stilllegen, Versetzen, Abreißen. Die Reichsverwaltung (Produktion) führt
      Betriebe ohne Weg für die Arbeiter als Problem.
    * `feed`: Meldungskarten rechts (links vom Info- bzw. Seitenfenster; ausgeblendet, solange die
      Liste „Meldungen“ offen ist) aus `SimEvent::Notification` (neueste oben; eine wiederholte
      Meldung rückt mit „×N“ nach oben; Farbe je Schwere, Spielzeit, Klick → Kamera, Ausblenden
      nach ≈ 24 s; so viele Karten, wie nach ihrer gemessenen Höhe passen), die Liste aller
      Meldungen des Spiels (200, unabhängig vom Meldungsfilter, mit dem Spielstand gespeichert)
      mit Zähler der seit dem letzten Öffnen gezeigten, und Hinweise oben (`CommandRejected`,
      Speichern/Laden). Beruhigung: dieselbe Meldung (Text und Ort) je Kalendertag (Mitternacht bis
      Mitternacht, `GameClock::total_days`) höchstens einmal (Gefahren immer); die Schwere kommt unverändert aus der Simulation (dichter Wald am
      Försterhaus ist dort schon eine Information).
    * `minimap`: egui-Textur mit 2 × 2 Pixeln je Tile, als Raute gezeichnet; aktualisiert je Chunk
      (Dirty-Chunks der Kartenansicht, Straßenereignisse, Vergleich der Gebäuderechtecke nach
      Bauereignissen), Neuaufbau bei Gebietsänderung und bei neuem Jahreszeitenbild
      (`SeasonLook`: Schneedecke des Wetters in fünf Stufen, Herbstlaub); Sichtrahmen, Klick/Ziehen.
    * `tooltip`: Tooltip an der Karte (Feldinfo nach kurzer Ruhezeit, Platzierungsurteil mit
      Gründen und Kosten, Straßenlänge und -kosten, Abriss-Erstattung); `dialogs`: Pausenmenü und
      Abriss-Bestätigung (egui-Modal; die Esc-Taste, die das Menü öffnet, schließt es nicht);
      nach `GameState::game_over()` öffnet die Weltszene das Pausenmenü einmal als „Niederlage“
      mit `ranks::defeat_text` (`pause_texts`).
* `save`: Spielstände in `saves/` über `kingdoms_sim::save` (Format, Prüfsumme, Validierung),
  rein deutsche Fehlertexte (Rohfehler nur im Log).
  * `slots`: Dateinamen aus dem Spielernamen (`slug`: Kleinbuchstaben, Umlaute umschrieben,
    reservierte Namen `quicksave`/`autosave_*` abgelehnt), Art (`Manual`, `Quick`, `Auto(n)`),
    Metadatei `<name>.kmeta` (Einwohner, Gold, Gebäude, Spielzeit und die letzten 200 Meldungen
    der Liste „Meldungen“ – `SavedNotification`, beim Laden über
    `Session::with_restored_notifications` an `hud::feed` zurück; nur gültig, wenn Tick und
    Speicherzeit zum Header passen, ältere Metadateien ohne Meldungen bleiben lesbar), Liste neueste zuerst mit Verträglichkeit aus dem Header
    (Formatversion, Daten-Fingerabdruck; Prüfsumme und Zustand prüft erst das Laden), Löschen,
    Autospeicher-Rotation (erster freier, sonst ältester von drei Plätzen), sicheres Schreiben
    (`.tmp` + `sync_all`, alter Stand als `.bak` per Hardlink/Kopie, Umbenennen, Verzeichnis-Sync,
    Zurücklesen und Vergleichen, erst dann `.bak` löschen; sonst `.bak` zurück), Reparatur
    unterbrochener Schreibvorgänge beim Auflisten, Aufräumen alter `.tmp` beim Start, Ortszeit
    über `jiff` („Heute, 14:30“).
  * `writer`: `SaveJob` kopiert den Zustand auf dem Main-Thread (`GameState::clone`, < 1 ms bei
    kleinen Städten), Kodieren und Schreiben im Worker (`SaveWriter`, eins nach dem anderen,
    neuere Anforderung derselben Datei ersetzt eine wartende; `finish` blockiert vor dem
    Verlassen).
  * Laden ersetzt die Session (`WorldScene::replace_session`), gibt die statischen Batches der
    alten Kartenansicht frei (`WorldView::release`), baut Kartenansicht, HUD-Zustand und
    Minikarte neu, übernimmt Raster/Gebietsgrenze und richtet die Kamera auf das Hauptquartier.
* `ui::menu`: gemeinsame Menü-Bausteine (Holzknöpfe, Pergamentseiten, Hintergrund-Abdunklung,
  Titel, Ladekreisel, Tastaturnavigation) und Seiten: `new_game` (Formular, Vorschau aus
  `world::generate` im Worker mit denselben Parametern wie `GameState::new`, Kennzahlen),
  `save_browser` (Laden und Speichern, Rückfragen inline; Spielstände nach `SaveHeader::game_id`
  gruppiert – `slots::group_by_game`, Überschrift je Spiel, sobald mehrere Spiele vorliegen,
  nicht lesbare Dateien zuletzt; „Fortsetzen“ nimmt weiter den neuesten ladbaren; liegen nur
  nicht ladbare Stände vor, sagen Liste und Hauptmenü das), `settings_panel` (vier Reiter,
  Tastenbelegung mit Konfliktabfrage; feste, immer sichtbare Bildlaufleiste, sobald ein Reiter
  länger als die Seite ist), `credits`, `loading_screen`.
* `sound`: Klänge zu Simulationsereignissen (mit `Placement::from_screen` nach Bildschirmposition,
  höchstens 6 je Frame), Produktionsklänge je Gebäudetyp, Ereignisse (`sfx_event_pending`, je
  Ereignis ein Effekt am Ort und eine Beraterstimme aus `event_sounds`, `sfx_event_resolved`,
  Schäden nach Ursache aus `damage_sound`, Wiederaufbau, Seuche, Vorkommen) und Forschung
  (`sfx_research_complete`, `vo_research_complete`), Sprachausgabe (`vo_*`),
  Umgebungsebenen nach Kameraumgebung und Tageslicht (Vögel bei Tag, Grillen/Nacht bei Nacht,
  Wald und Stadt nachts leiser, Wind/Winterwind/Fluss/Meer, Regen und Sturm nach dem Wetter,
  Marktgedränge bei Tag nahe besetzter Märkte), Karrenrumpeln der nächsten fahrenden
  Karawane (alle 2,5 s), Berater (`Advisor`, einmal je Spielstunde geprüft): Nahrung für unter
  3 Tage bzw. unzufriedene Bewohner bei Mittel unter 50 (wie die Gefahrenzeilen der Übersicht) –
  gesprochen, wenn die Warnung beginnt, und täglich wieder, solange sie gilt; Werkstätten ohne
  Nahrungsmittel (`Shortage`) ebenfalls. `ui_hover` beim Überfahren der HUD-Knöpfe
  (`widgets::take_hover_sound`), `vo_citizen_yes` beim Anklicken eines Bürgers auf der Karte.
  Musikstimmung Nacht (ab `night` > 0,6 bis < 0,4, keine Umschaltung im Wechsel), sonst im
  Winter Winter, sonst Blühend (`sound::prosperous`: Festtag oder zufriedene, zahlungsfähige
  Stadt ab 50 Bewohnern, Mittel ≥ 70), sonst Friedlich. Alle IDs stehen in
  `assets/audio/manifest.ron`, und jede ID des Manifests wird im Code verwendet (beides Tests);
  jede Stimme trägt ihren Wortlaut, jede Datei liegt im Ordner ihres Kanals (Test); ohne Dateien
  oder Gerät bleibt alles stumm. `audio_report` (`KINGDOMS_AUDIO_REPORT=1` oder
  `--audio-report`) druckt vor dem Start des Fensters die Liefertabelle (`files::audit`) mit
  Hinweisen und den Sprechertexten und beendet das Programm (0 = vollständig).
* Eingabe-Reihenfolge der Weltszene: Einstellungen übernehmen, fertige Speicher- und Ladevorgänge
  abholen → Kamera (bei offenem Dialog gesperrt; Verschieben beendet
  das Folgen) → Tasten (nicht bei offenem Dialog; Esc schließt von innen nach außen:
  Werkzeugschritt bzw. letzter Wegpunkt, Folgen, Rangfeier, Ereignis-Dialog, Anfrage-Dialog,
  Seitenfenster bzw. Bauleiste, Reichsverwaltung,
  Bürgerfenster, Auswahl, sonst Pausenmenü) → Hover
  (Entwickler-Override `KINGDOMS_CURSOR`) → Werkzeug → Klänge; `ui` → HUD-Aktionen; `render` →
  Kartenabgleich, Minikarte, Zeichnen, Werkzeug-Vorschau.
* Speicherorte: `settings.ron`, `engine.ron`, `saves/`, `logs/`, `screenshots/` im
  Benutzerverzeichnis (`GamePaths::user`).
* **Diplomatie, Militär, Ränge (Spiel-Lane Stufe 6)** – nur Darstellung und Kommandos über die
  öffentliche Sim-API (`report::{diplomacy_report, kingdom_actions, pending_proposals,
  caravans_report, partner_prices, military_report, recruit_options, threats_report,
  war_status, rank_report}`, `diplomacy::{actions, caravans}`, `military::order_problem`):
  * `hud::management::diplomacy` (Reiter Diplomatie, Taste I): Reiche, Beziehung mit Anteilen,
    Krieg, Verträge, Schätzungen, Waren, Handlungen mit Kosten/Grund/Urteil (`Judgement`),
    `ActionDialog` (Geschenk mit Waren, Tribut, Frieden mit Gold; Bestätigung für Krieg und
    Kündigung; Fehlerzeile aus `actions::action_problem`, Aussicht aus `actions::judge`),
    Anfragen mit Antworten, Verlauf. `hud::proposal_dialog`: eine kompakte Karte für alle
    Anfragen, angedockt rechts über den Meldungskarten (Spalte der Karten, Bildmitte bleibt
    frei): „1 von 3“ mit Vor/Zurück, nach einer Antwort (`AnswerProposal`) die nächste
    unbeantwortete, „Später“/Esc blendet bis zur nächsten neuen Anfrage aus; öffnet sich je neue
    Seriennummer, nicht während Ereignis- oder Rangdialog (die haben Vorrang).
    `hud::heraldry`: Wappen (`icons/crest_<farbe>`, Schild als Ersatz), Zustandsfarben,
    Beziehungsbalken; `theme::KINGDOM_COLORS` = `palette::KINGDOM_COLORS` des Generators (Test).
  * `hud::management::military` (Reiter Militär, Taste Q): Karten, Bedrohungen, Kriege,
    Einheiten, Kompanien mit Befehlen (Bereitschaft, Rückzug, Auflösen, Feldzug/Belagerung über
    `CompanyOrder`; Verteidigen/Patrouille → `HudAction::PickOrder`), Soldaten ohne Kompanie
    (`CreateCompany`, `AssignSoldiers`, `DismissSoldiers` mit Rückfrage), Rekrutierung je
    Militärgebäude (`recruit_rows`, auch im Info-Fenster mit Verteidigungswerten), Feldzüge (mit
    Proviant: `provisions_left` aus `Campaign::provisions` und `provisions_eaten`),
    Kampfberichte. `hud::management::caravans` (im Handelsreiter): Karawanen, Partnerpreise,
    Sendedialog (`caravans::send_problem` live, `SendCaravan`).
  * **Befehlswerkzeug** `tools::Tool::Order { company, pick }` (`OrderPick::Defend { radius }`
    oder `Patrol`): Klick → `defend_order` (fertiges Gebäude, sonst Gebiet) bzw. Wegpunkt
    (begehbar, höchstens `max_patrol_points`; Klick auf den ersten, Eingabe oder „Fertig“ →
    `finish_patrol`), Prüfung mit `military::order_problem`, `ToolOutput::order_given` öffnet den
    Militärreiter wieder; `cancel` nimmt erst den letzten Wegpunkt zurück. Vorschau in
    `view::preview`/`view::orders` (gepunktete Route mit Rückweg und Strecke zum Cursor, Quadrat
    des Gebiets, Umriss des Gebäudes); bei offenem Militärreiter zeichnet `view::orders` die
    Routen und Gebiete aller Kompanien. `hud::war_markers`: Banner des Werkzeugs und Randmarken
    der Bedrohungen (`marker_spot`: auf dem Punkt bzw. am Rand des Innenrechtecks mit Pfeil),
    beide in der freien Kartenfläche (`free_area`: rechts der linken Leiste, links der
    Meldungskarten bzw. Seitenfenster, zwischen oberer und unterer Leiste); das Banner bricht
    um und steht oben mittig, die Marken darunter (`marker_area`); der Tooltip einer Marke nennt
    im Anmarsch die Uhrzeit der Ankunft (`threat_tooltip`, `report::deadline_clock_text`).
  * Rechte Leiste: Panel **Aufgaben** (`hud::tasks`, `rank_report`, Fortschrittsbalken am Knopf;
    jede offene Anforderung ist ein Verweis nach ihrer Art `ranks::RequirementKind`, nicht nach
    dem deutschen Text: Einwohner → Bevölkerung, Wohlstand → Wirtschaft, Gebäude → Produktion,
    Gebiet → Gebietsgrenze zeigen, fehlendes Gebäude → Bauwerkzeug), Anfragen im Ereignis-Panel; `tasks::RankDialog` feiert `SimEvent::RankAchieved` (eigener
    `EventCursor`; Ereignis- und Anfragedialog warten dahinter). Übersicht mit Kriegen,
    Bedrohungen (Ankunftszeit), Anfragen, Sold und Rang. `dialogs::defeat_screen` ersetzt das
    Pausenmenü, sobald `game_over()` gesetzt ist (`DefeatSummary`: Grund, Datum, Bilanz). Pausenmenü
    und Niederlage-Bildschirm scrollen, wenn sie nicht auf den Bildschirm passen; im Pausenmenü
    wirken F5/F9 wie die Knöpfe (`Hud::only_menu_open`).
    Linke Leiste: die Militär-Bauleiste beginnt mit „Heer verwalten“ und „Diplomatie“.
  * Karte: `view::kingdoms` (`KingdomLayer`) zeichnet die Bauwerke der Reiche
    (`Kingdom::structures`, Gebäudesprite in Drehung, 16 % Reichsfarbe, Banner an Burg und
    Wachturm) und ihre Grenzen je Chunk (Linie + getönter Randstreifen aus `LandMap::owner`,
    neu gebaut für `Session::take_land_dirty_chunks`, die `WorldView::sync` auch an die
    Minikarte weitergibt). `view::citizens::Allegiances`: Ringe, Tönung (Heere 45 %
    Reichsfarbe) und Banner (`characters/banner`) für Feinde, Karawanen und Kompanien; Kämpfer
    einer Feindgruppe auf derselben Stelle (die Simulation führt sie auf einem Weg) werden als
    Menge gezeichnet (`crowd_offset`: Sonnenblumen-Spirale um die Stelle, Rang nach Läufer-Id,
    nicht in Gebäude oder Wasser) – nur Darstellung, die Simulation bleibt unberührt. Ebenso die
    **Festgäste**: an Festtagen sind sie (wie jeder Besucher) im Gebäude ihres Festorts; die
    Darstellung stellt sie als Menge vor dessen Eingang (`festival_guests`, `festival_spot`:
    lockere Spirale, je Rang eigene Punkte auf freiem Boden, höchstens 60 je Ort, zum Gebäude
    gewandt, jede zweite Figur tritt auf der Stelle) – es sind die echten Bürger (anklickbar).
    `view::effects`: Funken und Staub, wo die Hiebe der aktuellen Kampfrunde landen
    (`strike_spots` aus `CombatRound::strikes()`: je Angreifer einmal, mitten zwischen
    Angreifer und getroffenem Kämpfer an ihren jetzigen Plätzen – ein Feind an seinem Platz in
    der Menge seiner Bande, `crowd_position` –, bei Schlägen gegen Tor, Mauer oder Gebäude
    knapp vor der getroffenen Kante; aus einer Besatzung heraus vom Rand des Gebäudes), Feuer
    und Rauch auf angegriffenen Gebäuden (`Military::damage_of`). Wer angreift, schaut zu
    seinem Ziel (`CitizenView::facing`); eine Figur, die versetzt in der Menge steht, dreht sich
    von ihrem Platz aus zum Ziel (`citizens::attack_facing`). Pfeile von Türmen
    (`Shot::shooter` leer) starten auf Turmhöhe. Minikarte: Reichsgebiete getönt, Grenzen und Bauwerke in
    Reichsfarbe, Punkte für Feinde (pulsierend), Kompanien und Karawanen.
  * Klang: `sound::danger` → Musikstimmung `War` (Krieg oder Feinde auf der Karte) bzw. `Tense`
    (Feinde im Anmarsch, Tributforderung, Sold aus) vor Nacht/Winter/Frieden; Signale für
    Bedrohung, Schlachtausgang (`battle_sounds`), Mauerbruch, Krieg, Frieden, Vertrag,
    Gesandtschaft, Geschenke, Karawanen, Rekrutierung, Aufstieg, Niederlage.
  * Entwicklerhilfen `session::dev_army` (`KINGDOMS_DEV_ARMY`), `session::dev_trade`
    (`KINGDOMS_DEV_TRADE`), `KINGDOMS_DEV_THREAT` (Krieg + `DebugSpawnThreat`) und
    `KINGDOMS_FOCUS=feind|karawane` (Kamera folgt) – alles über Kommandos.

## 5. Performance-Strategie
Chunk-Batches + Culling, instanziertes Rendering, Texturatlanten, gestaffelte Bewohner-Updates
(Simulation-LOD, §3.6 mit Messwerten), räumliche Gitterindizes, gespeichertes Weggedächtnis,
asynchrone Asset-Dekodierung
(Worker-Threads hinter dem Ladebildschirm), Weltgenerierung und Spielstand-Prüfung im Worker,
Speichern im Hintergrund,
Frame-Metriken und Profiling-Sektionen im Debug-HUD; Messläufe mit `KINGDOMS_PROFILE` (Tabelle je
Abschnitt im Log) und pixelgleiche Vorher/Nachher-Screenshots mit `KINGDOMS_FIXED_DT`. Gemessen
(Spiel-Schritt 7b, Release, lavapipe; Median je Frame): die CPU-Seite ohne das Warten auf die
GPU kostet 1,5–3 ms (Messstadt 5000 Bewohner, Entwicklerstadt nach zwei Jahren, riesige Karte
ganz herausgezoomt); die Zahlen stehen in `docs/progress.md`.

## 6. Konventionen
* Nach jeder größeren Änderung `cargo check`, nach Modulen `cargo test`; Clippy sauber halten.
* Keine `unwrap()` auf externen Daten; Fehler mit `thiserror` (Bibliotheken) / `anyhow` (Binary).
* UI-Texte deutsch; Code, Bezeichner und Kommentare englisch.
* Simulationseinheiten: Tiles (Position), Ticks (Zeit), Gold (Geld).
