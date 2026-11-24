# Fortschrittsprotokoll

Dieses Dokument hält den Arbeitsstand fest (was fertig ist, wie es geprüft wurde, was offen ist).
Neueste Einträge oben.

## 2026-11-24 · Abschlussprüfung nach Polish-Runde 10: Prüfläufe, Bilder, Audio-Lieferliste (`docs/`, `README.md`, `assets/`, ein Test in `src/sound.rs`)

**Fertig**
* **Bilder** (`docs/screenshots/`, Release-Build, 1600 × 900, Xvfb mit lavapipe): 01–03, 07, 10
  und 14 neu aufgenommen. Den Mauszeiger hat XTest aus dem Bild geschoben, damit kein Tooltip die
  Karte verdeckt.
  * 01 zeigt „Fortsetzen“ mit einem Spielstand im Format 17.
  * 07 zeigt die neue Dauerform („noch 24 h“).
  * 10 zeigt Handelsaufträge mit geöffneter Bearbeitung, Karawanen und Handelspartner in den
    Tabellen mit Spaltenbreiten nach Inhalt.
  * Neu ist `17_winter_minikarte.png`: die Entwickler-Stadt an Wintertag 2 im Jahr 2 um 10 Uhr
    nach 16 h Schneefall. Die Minikarte zeigt die volle Schneedecke (Wälder mit dunklen Kronen,
    offene Gewässer, Land außerhalb des eigenen Gebiets abgedunkelt).
  * Die Bilder 04–06, 08, 09, 11–13, 15 und 16 aus Runde 10 sind geblieben. Sie stammen aus dem
    Release-Build mit dem heutigen Stand; seither hat sich nur der Reiter Landwirtschaft geändert,
    und der ist auf keinem Bild.
  * Das Festbild behält seinen Namen `16_fest_ausrichten.png`, weil es schon verlinkt ist.
  * Alle 17 Bilder sind RGB und verlustfrei verkleinert (pixelgleich geprüft). In der
    README-Tabelle ist Bild 10 neu beschrieben und Bild 17 ergänzt.
* **Audio-Lieferliste im Projekt:** `assets/audio/delivery.json` ist aus `manifest.ron` neu
  erzeugt. Das Manifest ist seit Runde 9 unverändert, die Liste deshalb inhaltsgleich: 143 IDs,
  146 Dateien. Sie enthält je ID Kanal, Art, Dateien, Schleife, Beschreibung, empfohlene Länge,
  Varianten und bei Ansagen den Wortlaut.
  * Neuer Abschnitt „Lieferliste“ in `assets/audio/README.md` (beschreibt die Felder); Hinweise im
    README („Grafik & Audio“) und in `assets/README.md`.
  * Neuer Test `sound::tests::the_delivery_list_matches_the_manifest`: Jede ID des Manifests steht
    genau einmal in der Liste, mit allen Dateien und Sprechertexten. Gegenprobe: Mit einem
    verfälschten Dateinamen schlug der Test wie erwartet fehl, danach wurde die Datei
    wiederhergestellt.
* **Roadmap:** neue ✅-Zeile. Die ⬜-Zeilen (Rückmeldungen vom Mac, Audiodateien, finale Grafik)
  warten auf Lieferungen von außen; Phase 8 bleibt deshalb 🔶.
* **game_design §1:** Die Fortschrittskurve nennt jetzt auch die Nachmessung des Langzeitlaufs nach
  Runde 10 (siehe „Geprüft“).

**Geprüft**
* `cargo build --release`: ohne Warnungen (inkrementell 1 min 57 s).
* `cargo test --workspace`: alles grün.
  * Spiel 235, Assetgen 69, Engine 74.
  * Sim 410 (+ 9 ignorierte), Kampagne 8 (+ 2 ignorierte).
  * Determinismus 12, Fuzz 7, vernünftiger Spieler 2.
  * Doctests: Engine 0, Sim 1.
* `cargo clippy --workspace --all-targets -- -D warnings` sauber;
  `RUSTDOCFLAGS='-D warnings' cargo doc -p kingdoms_sim -p kingdoms_engine --no-deps` sauber.
* Atlanten und App-Symbol in ein Scratch-Verzeichnis neu erzeugt: alle 24 Dateien in
  `assets/atlases` und `assets/app_icon.png` sind bytegleich.
* Keine Treffer für `TODO`, `FIXME`, `todo!(`, `unimplemented!(` oder `dbg!(` in `crates/`, `src/`
  und `tools/`.
* Langzeitläufe `cargo test --release -p kingdoms_sim --test campaign -- --ignored --nocapture`:
  beide grün (9 min 43 s). Sie liefen zum ersten Mal nach den Ereignisänderungen von Runde 10.
  * Acht Jahre, vorausschauender Spieler: Seed 7 wird nach 93 Tagen Großstadt (257 Einwohner).
    Seed 23 (215 Einwohner) und Seed 31 (283 Einwohner) bleiben Stadt; ihnen fehlt Gebiet
    (1 751 bzw. 2 057 von 2 400 Feldern), Seed 23 auch die Schule.
  * Niemand verhungert. Bei Seed 31 erfrieren an Tag 143 zwei Bewohner.
  * Ehrgeiziger Spieler: Seeds 11 und 42 werden Stadt (Tag 67 bzw. 59), Seed 31 bleibt
    Marktflecken (93 von 100 Einwohnern).
* `kingdoms --audio-report`: 143 IDs, 146 Dateien erwartet, 0 gefunden, Rückgabewert 1 wie
  vorgesehen. Das Prüfskript bestätigt die Lieferliste gegen Manifest und Bericht (IDs, Dateien,
  Ordner, Kanal, Varianten, Schleifen, Längen, Wortlaut).
* Alle 34 `KINGDOMS_*`-Variablen aus `src/` und `crates/engine` stehen im README. Nur
  `KINGDOMS_BALANCE_SEED` fehlt dort; es ist ein Test-Schalter der Simulation.
* Alle 17 Bilder einzeln angesehen, dazu die Minikarte vergrößert. Die Logs der Läufe enthalten
  kein ERROR und kein panic.
* `target/` belegt 34 GB (`debug` 32 GB, `release` 1,6 GB, `doc` 65 MB). Unter dem
  Scratch-Verzeichnis liegen keine `target`-Ordner mehr.

**Offen**
* **Lieferungen von außen:**
  * Grafik: alles ist erzeugte Platzhalter-Grafik.
  * Audio: keine Dateien, nie gehört. Der Audio-Bericht misst nur den Spitzenpegel, nicht
    die Lautheit in LUFS.
  * macOS und Windows: weder gebaut noch gestartet; das Fenstersymbol ist nur unter Linux geprüft.
* **Nicht im Bild gesehen:**
  * ein Gasthaus mit Bier: In der Gebäudeschau fehlt Bier im Lager, und die Träger sind
    ausgelastet.
  * die Zeile „Einarbeitung“: Die Gründer starten voll eingearbeitet; die Zeile ist nur per Test
    geprüft.
  * der bernsteinfarbene Geist bei „kein Lager in Reichweite“.
* **Bedienung:**
  * Folgt die Kamera einem Bewohner oder Feind, hängen kartenverankerte Beschriftungen einen Frame
    nach.
  * „Später“ auf der Anfragekarte setzt eine automatische Pause nicht fort.
* **Simulation und Meldungen:**
  * Die vier Ecken eines Mauerrings lassen sich nicht bauen, weil Baustellen eine Straße am Eingang
    brauchen.
  * `Notification` hat keine Art und keine Gebäude-ID.
  * Der Förster meldet sich weiter (etwa 35-mal in zwei Jahren der Entwickler-Stadt; als
    Information, im Feed zu einer Karte zusammengefasst).
  * „Holzfällerhütte: keine Rohstoffe im Umkreis“ wiederholt sich alle 12 h je Gebäude.
* **Balance:**
  * Die Großstadt erreicht im Acht-Jahres-Lauf nur noch einer von drei Seeds; nach Sim-Lane
    Schritt 9 waren es alle drei. Der Test verlangt mindestens einen Seed und bleibt grün. Den
    Seeds 23 und 31 fehlt Gebiet. Wahrscheinliche Ursache sind die seltener gewordenen Ernte- und
    Dürre-Ereignisse aus Runde 10; untersucht ist das nicht. Die Balance gehört in die Sim-Lane.
  * Den Rang Königreich erreicht kein Seed (Eisen und Gebiet fehlen); das ist in game_design §1
    dokumentiert.
  * Bei Seed 31 erfrieren an Tag 143 zwei Bewohner (Ursache nicht untersucht).
  * Wer nie antwortet, sammelt dauerhaft Abzüge in den Beziehungen.
* **Spielstände:** Formate bis 16 werden abgelehnt; es gibt keine Umwandlung.

## 2026-11-24 · Spiel-Feinschliff Runde 10: Kalenderdaten, Tageswechsel der Meldungen, Wintervorrat, Einstellungen, Feldzustand, Bilder (`src/`, `crates/sim`, Doku)

**Fertig**
* **Fest-Statuszeile in einem Kalender** (Befund bestätigt): Die Simulation nannte den Wartegrund
  mit fortlaufender Tageszahl („… möglich (Tag 13).“), das Spiel hängte das Kopfleisten-Datum an.
  Ursache in der Simulation behoben: neues `time::day_date_string` (auch Grundlage von
  `GameClock::date_string` und `day_title` im Spiel); `festival_problem` sagt jetzt „Das nächste
  Fest ist in 10 Tagen möglich (Herbst · Tag 1 · Jahr 1).“ – in Statuszeile, Tooltip des
  gesperrten Knopfs und „Hof und Feste“ (Finanzen) gleich. Dieselbe Konvention in
  `report::deadline_clock_text`: Fristen ab dem dritten Tag „Sommer · Tag 6 · Jahr 1, 06:00 Uhr“
  statt „Tag 12, 06:00 Uhr“ (Ereignisdialog, Räuberwarnung, Anfragen, Bedrohungsmarke).
* **Drosselung der Meldungen je Kalendertag** (bestätigt): `first_today` rechnete
  `tick / TICKS_PER_DAY` (Tageswechsel um 06:00); jetzt `GameClock::at_tick(tick).total_days()`
  – Mitternacht bis Mitternacht wie die Kopfleiste. Test mit 05:00/07:00 (zusammengefasst) und
  23:00/01:00 (zwei Tage).
* **Wintervorrat** (bestätigt): Der Balken im Reiter Landwirtschaft heißt „Vorrat 4.594 · Bedarf
  im Winter 1.728“, bei Mangel „Vorrat 1.200 · Bedarf im Winter 1.728 · 528 fehlen“, ohne
  Bewohner „… · kein Bedarf im Winter“ (`winter_bar_text`, verglichen wie angezeigt).
* **Einstellungen › Spiel** (bestätigt, 1600 × 900): Die Bildlaufleiste der Einstellungsseite ist
  fest und dunkel statt schwebend (immer sichtbar, sobald ein Reiter länger als die Seite ist,
  auch „Steuerung“); die drei Meldungsfilter stehen in einer Zeile, alle sind ohne Bildlauf
  sichtbar.
* **Wünsche der Sim-Lane:** Reiter Landwirtschaft zeigt `WeatherReport::forced_text` („Wetter vom
  Ereignis „Dürre“ vorgegeben (noch 4 Tage)“) statt des festen Entwicklerbefehl-Texts und eine
  Zeile „Zustand der Felder“ (`average_condition`) mit Balken; Farbe und Tooltip nach den
  Schwellen der Ernte-Ereignisse aus den Ereignisdaten („„Schlechte Ernte“ im Herbst bei
  höchstens 85 %, „Besonders erfolgreiche Ernte“ im Herbst ab 95 %“), ohne bestellte Felder
  „– (keine bestellten Felder)“. Die Trockentage stehen schon als Wirkung („Seit 12 Tagen kein
  Regen.“). Ereignisdialog: Nennt der Text das Gebäude schon, heißt der Link nur „Gebäude
  zeigen“ (`place_link_text`), sonst weiter „Betrifft: … – zeigen“.
* **Bilder** (`docs/screenshots/`, Release-Build, 1600 × 900, RGB): 04, 05, 06, 08, 09, 11, 12,
  13 und 15 neu aufgenommen – angedockte Anfragekarte, Förster als Information, Schnee auf der
  Minikarte, Beziehungsbalken mit Skala, Räuber in Karminrot mit Beschriftung über der Gruppe,
  Brandtext mit Brandherd und „Gebäude zeigen“, Bürgerfenster (Marktfrau mit vier Töchtern und
  Freunden), nur eine Nichtangriffspakt-Anfrage, drei Spielstände im Format 17; neu
  `16_fest_ausrichten.png` (Reiter Bevölkerung). README-Tabelle angepasst.
* Doku: game_design (Festtext, Datumsform in Texten), architecture (Meldungsdrosselung,
  Ereignisdialog, Reiter Landwirtschaft, Einstellungen), README (Landwirtschaft, Bilder).

**Geprüft**
* `cargo test --workspace`: Spiel 234, Assetgen 69, Engine 74, Sim 410 (+ 9 ignorierte),
  Kampagne 8 (+ 2 ignorierte), Determinismus 12, Fuzz 7, vernünftiger Spieler 2, Doctest 1 –
  grün. Neu/angepasst: Fest-Statuszeile, Kalendertag der Meldungsdrosselung, Wintervorrat-Text,
  Feldzustand (Schwellen, Farbe, Tooltip), Link-Text im Ereignisdialog, Fristen mit
  Kalenderdatum (Spiel und Simulation), Festtext der Simulation.
* `cargo clippy --workspace --all-targets -- -D warnings` sauber;
  `RUSTDOCFLAGS='-D warnings' cargo doc -p kingdoms_sim --no-deps` sauber.
* Bilder angesehen (Xvfb, lavapipe, 1600 × 900, XTest): Einstellungen › Spiel im Pausenmenü
  (alle drei Filter, Leiste sichtbar ohne Mauszeiger) und › Steuerung (Leiste mit Griff);
  Landwirtschaft nach `KINGDOMS_DEV_EVENT=drought:0` („Wetter vom Ereignis „Dürre“ vorgegeben
  (noch 4 Tage)“, „Vorrat 7.663 · Bedarf im Winter 2.333“, „Zustand der Felder 100 %“ mit
  Tooltip); Fest aus dem Schnellspielstand gefeiert und einen Tag später „Das nächste Fest ist in
  10 Tagen möglich (Winter · Tag 3 · Jahr 2). Zuletzt gefeiert: Sommer · Tag 3 · Jahr 2.“;
  Brand-Dialog mit „Gebäude zeigen“; alle neuen Bilder einzeln angesehen.
* Lane: `sync.sh from-main`.

**Offen**
* Die übrigen Bilder (01–03, 07, 10, 14) sind unverändert; sie zeigen nichts, was sich seither
  geändert hat.
* Offen aus früheren Runden: Gasthaus mit Bier und die Zeile „Einarbeitung“ nicht im Bild;
  Kamera-Folgen verschiebt kartenverankerte Beschriftungen um einen Frame; „Später“ auf der
  Anfragekarte setzt eine automatische Pause nicht fort; Spielstände bis Format 16 werden
  abgelehnt (keine Umwandlung).

## 2026-11-24 · Sim-Feinschliff Runde 10: Ernte und Dürre nach Weltzustand, Räuberwarnung, Brandherd, Ereignistage im Spielstand (`crates/sim`, `data/`, Doku)

**Fertig**
* **Ereignistage im Spielstand** (Befund 1, bestätigt: Tag 1000 und `u64::MAX − 1` wurden
  geladen, Letzteres führte im Dev-Profil zum Überlauf in `festival_report`): `Events::validate`
  prüft gegen die Uhr – letzter Festtag, letzter Tag je Ereignis und letztes Ereignis nicht nach
  heute, ein offenes Ereignis nicht später begonnen, die Chronik nicht in der Zukunft
  entschieden, Modifikatoren und Steuerdeckel höchstens 65 535 Tage. Abklingzeit, Mindestabstand
  und nächster Festtag rechnen zusätzlich sättigend. Der Fuzz-Test baut nach jedem geladenen
  Spielstand die Berichte (darunter `festival_report`, `events_report`) und richtet ein Fest aus.
* **Räuberwarnung** (Befund 2, bestätigt): Die Wahl nennt nur fertige, arbeitende Wehrbauten, die
  schießen, nach Art und Anzahl – „…; 6 Soldaten stehen bereit, dazu 1 Bogenschützenposten“,
  „…, dazu 2 Wachtürme, 1 Wehrturm und 1 Bogenschützenposten“; ohne Wehrbau entfällt der Teil
  („…; kein Soldat steht bereit“). Neu: `military::ranged_defences` (auch Grundlage von
  `raid_defenders`), Mehrzahl `plural` in `buildings.ron` (Pflicht für schießende Gebäude, beim
  Laden geprüft), `BuildingDef::count_text`.
* **Ernte und Dürre nach Weltzustand** (Befund 3, bestätigt): neue Bedingungen `MinDryDays`,
  `MinFieldCondition`/`MaxFieldCondition` und `NoEventThisSeason`.
  * Dürre: Sommer ab Tag 7, seit mindestens 3 Tagen kein Regen, Sturm oder Schnee, heute klar
    oder bewölkt – also nie an einem Regentag und nicht, solange das Wetter selbst schon eine
    Dürre bringt (die meldet das Wetter). Text „Seit Tagen …“ statt „Seit Wochen …“.
  * Schlechte Ernte nur, wenn die bestellten Felder im Schnitt in höchstens 85 % Zustand sind
    (Stürme, Welken), besonders erfolgreiche Ernte erst ab 95 %; ohne bestellte Felder (vor der
    Aussaat, nach der Ernte) kommt keine. Beide schließen sich je Jahreszeit aus. Neuer Text der
    schlechten Ernte nennt die Ursache.
  * Trockenperiode im Spielstand: `WeatherState::dry_days` (Tage in Folge ohne Niederschlag,
    `Weather::is_precipitation`), Spielstand-Format 17. Der Wetterbericht nennt sie ab 3 Tagen
    („Seit 10 Tagen kein Regen.“, Feld `dry_days`), die Landwirtschaftsübersicht den mittleren
    Feldzustand (`average_condition`, aus `Agriculture::average_field_condition`).
  * Erzwungenes Wetter merkt sich das Ereignis (`ForcedWeather::event`): `WeatherReport::forced_text`
    sagt „Wetter vom Ereignis „Dürre“ vorgegeben (noch 4 Tage)“ bzw. „… vom Entwicklerbefehl …“
    – bisher stand im Spiel auch nach einer Ereignis-Dürre „Wetter vom Entwicklerbefehl
    vorgegeben“.
* **Brandtext** (Befund 4, bestätigt): optionaler `focus_text` mit `{building}` in den
  Ereignisdaten (beim Laden geprüft: genau ein Platzhalter, nur bei Gebäudeschaden), angewandt in
  Dialog und Meldung über `events::event_text` – „Im Gebäude „Försterhaus“ ist Feuer
  ausgebrochen! …“; ohne Brandherd „In einem Holzgebäude ist Feuer ausgebrochen! …“.
* Doku: game_design §8.2 und §12 (Tabelle, Weltzustand, Räuberwarnung, Wetter-Folge),
  architecture §3 (Module `weather`/`events`, Format 17), roadmap (✅-Zeile).

**Geprüft**
* `cargo test --workspace`: Spiel 231, Assetgen 69, Engine 74, Sim 410 (+ 9 ignorierte),
  Kampagne 8 (+ 2 ignorierte), Determinismus 12, Fuzz 7, vernünftiger Spieler 2, Doctest 1 –
  grün. Neu: Ereignistage/Ticks in der Zukunft und unbekanntes Ereignis eines erzwungenen Wetters
  werden abgelehnt, Spielstand mit Festtag 1000 bzw. `u64::MAX − 1` wird beim Laden abgelehnt und
  die Festrechnung läuft nicht mehr über, Räuberwarnung ohne/mit Wehrbauten (Mehrzahl,
  Reihenfolge), Brandtext mit und ohne Brandherd, Ernte-Ereignisse nach Feldzustand und
  Ausschluss je Herbst, Dürre nach Trockenperiode (nicht bei Regen, nicht während Wetter-Dürre),
  Zählung der Trockentage, Wetterbericht („Seit 5 Tagen kein Regen.“, Ereignis/Entwicklerbefehl
  in `forced_text`), Ladeprüfungen für `plural` und `focus_text`; Fuzz-Test mit Berichten und Fest
  nach jedem geladenen Spielstand.
* `cargo clippy --workspace --all-targets -- -D warnings` sauber;
  `RUSTDOCFLAGS='-D warnings' cargo doc -p kingdoms_sim --no-deps` sauber; `cargo build` des
  Spiels ohne Änderung in `src/`.
* Lane: `sync.sh from-main`, `crates/sim`, `data/` und `Cargo.lock` gleich (`diff -rq`, `cmp`).
* Messlauf (vorübergehender Test, danach entfernt): vernünftiger Spieler, 4 Jahre, Seeds 7, 11,
  23, 31. Mittlerer Feldzustand meist 100 %, nach Stürmen/Dürren 56–91 %; drei Dürre-Ereignisse,
  jeweils an klaren/bewölkten Tagen nach 3–14 trockenen Tagen; schlechte Ernte bei 69 % (Seed 11),
  besonders erfolgreiche Ernte bei 100 % (Seed 23), nie beide in einem Herbst.
* Bilder angesehen (Xvfb, lavapipe, 1600 × 900): Entwicklerstadt ohne Wehrbauten mit
  `KINGDOMS_DEV_EVENT=bandit_raid` – „Etwa 8 Räuber greifen gegen 14:00 Uhr unbewachte Lager an;
  kein Soldat steht bereit“ (kein Turm mehr behauptet); `KINGDOMS_DEV_ARMY=6` – „6 Soldaten
  stehen bereit, dazu 1 Bogenschützenposten“; `KINGDOMS_DEV_EVENT=fire` – Dialog und Meldung „Im
  Gebäude „Försterhaus“ ist Feuer ausgebrochen!“, Wahl „… zuerst das Gebäude „Försterhaus““;
  dieselbe Szene wie `docs/screenshots/11_ereignis.png` (Sommer Tag 3 Jahr 2, 12:00, pausiert,
  Mauszeiger per XTest beiseite) mit „Im Gebäude „Holzfällerhütte“ …“;
  Reiter Landwirtschaft nach einer Ereignis-Dürre – Wirkung „Seit 10 Tagen kein Regen.“ (die
  Kopfzeile sagt dort noch „Wetter vom Entwicklerbefehl vorgegeben“, siehe Offen). Logs ohne
  ERROR/panic.

**Offen**
* Darstellung (Wünsche an die Spiel-Lane): im Reiter Landwirtschaft `WeatherReport::forced_text`
  statt des festen „Wetter vom Entwicklerbefehl vorgegeben“ (`src/ui/hud/management/agriculture.rs`)
  und den mittleren Feldzustand `AgricultureSummary::average_condition` zeigen; optional im
  Ereignisdialog die Zeile „Betrifft: …“ kürzer fassen, wenn der Text das Gebäude schon nennt
  (der Link „zeigen“ bleibt nützlich).
* Langzeitläufe der Kampagne (`--release --ignored`) nach der Ereignisänderung nicht erneut
  gelaufen; die Ernte-/Dürre-Ereignisse kommen jetzt seltener und nur mit passendem Weltzustand.
* `docs/screenshots/11_ereignis.png` zeigt noch den alten Text „In einem Holzhaus …“; ein neues
  Bild derselben Szene (Entwicklerstadt, Sommer Tag 3 Jahr 2, 12:00, pausiert, „Im Gebäude
  „Holzfällerhütte“ ist Feuer ausgebrochen!“) liegt im Scratch-Verzeichnis dieser Runde
  (`simfix10/firedoc/fire.png`); das Ersetzen der Doku-Datei wurde hier nicht freigegeben.
* Spielstände bis Format 16 werden abgelehnt (keine Umwandlung).

## 2026-11-24 · Zusammenführung Schritt 9: Sim-Daten im Spiel – Feste, Hofhaltung, Bauplatz ohne Lager, Ankunft der Räuber (`src/`, `crates/sim`, Doku)

**Fertig**
* **Zusammenführung:** `crates/sim` und `data/` der Sim-Lane (Spielstand-Format 16) im
  Hauptarbeitsbereich; das Spiel baute ohne Anpassung. Doku: beide Einträge in progress.md,
  architecture.md §3 aus der Sim-Lane und die übrigen Abschnitte aus der Spiel-Lane,
  game_design.md aus der Sim-Lane (die Spiel-Lane hatte sie nicht geändert), assets.md aus der
  Spiel-Lane, beide ✅-Zeilen in roadmap.md.
* **Fest ausrichten** (Reiter Bevölkerung, unter der Zufriedenheit): Kasten mit Knopf „Fest
  ausrichten (544 Gold)“ → `Command::HoldFestival`, Kosten aus `FestivalRules` („100 Gold und
  4 Gold je Einwohner, gebucht unter „Ereignisse““), Wirkungen aus `report::festival_report`.
  Geht es nicht, ist der Knopf gesperrt, der Grund der Simulation steht im Tooltip und darunter
  (Abklingzeit samt letztem Festtag, fehlendes Gold, keine Kirche und kein Marktplatz,
  Bankrott). Ein angeordnetes Fest in der Kommando-Warteschlange (Pause) sperrt den Knopf: „Das
  Fest wird ausgerichtet, sobald das Spiel weiterläuft.“ Während des Fests „Heute wird gefeiert“
  und ab wann das nächste möglich ist. `SimEvent::FestivalHeld` läutet die Kirchenglocke.
* **Hof und Feste** (Reiter Finanzen, unter den Steuern): Hofhaltung des Rangs aus
  `report::court_report` („Hofhaltung als Marktflecken: keine Kosten“), der nächste teurere Rang
  („Ab dem Rang „Stadt“: 15 Gold je Tag“), Kosten eines Fests mit Hinderungsgrund und ein Knopf
  zum Reiter Bevölkerung.
* **Bauplatz, den kein Lager erreicht** (`PlacementWarning::NoStorageInReach`): der Tooltip sagt
  nicht mehr „Bauplatz geeignet“, die Warnung steht rot und fett mit Kreuz; Geist und Umriss der
  Vorschau sind bernsteinfarben statt grün (`view::preview::GhostLook`, `tools::site_unsupplied`).
* **Räuber:** Der Tooltip einer Bedrohungsmarke nennt im Anmarsch die Uhrzeit („Im Anmarsch ·
  Ankunft 17:00 Uhr · Ziel: Lager“); Ereignisdialog und Meldungen tragen die Uhrzeit schon aus
  der Simulation, die Marke zählt die 8 h herunter.
* **Meldungen:** Die Umgehung im Spiel für den Förster (`calmed_severity` mit mitgelesenem
  Produktionsstatus) entfällt – die Simulation meldet den dichten Wald selbst als Information;
  die Tagesdrosselung gleicher Meldungen bleibt.
* **Sim-Text:** „Die Feinde haben das Tor durchbrochen!“ / „… die Stadtmauer …“ (Artikel im
  Akkusativ, `military::threats::breach_text`).

**Geprüft**
* `cargo test --workspace`: Spiel 231, Assetgen 69, Engine 74, Sim 404 (+ 9 ignorierte),
  Kampagne 8 (+ 2 ignorierte), Determinismus 12, Fuzz 7, vernünftiger Spieler 2, Doctest 1 –
  grün. Neu: Festknopf und Zustandszeile (Abklingzeit, Fest läuft, Warteschlange), nächster
  teurerer Hof, Bauplatz ohne Lager (Geist grün/bernstein/rot), Ankunftszeit im Tooltip,
  Durchbruchstext.
* `cargo clippy --workspace --all-targets -- -D warnings` sauber;
  `RUSTDOCFLAGS='-D warnings' cargo doc -p kingdoms_sim --no-deps` sauber.
* Lane: `sync.sh from-main`, Kopie gleich (`crates/sim`, `data/`, `Cargo.lock`),
  `cargo test -p kingdoms_sim` im Lane-Verzeichnis grün (404/8/12/7/2/1).
* Bilder angesehen (Xvfb, lavapipe, 1600 × 900): Hauptmenü; Direktstart; Entwicklerstadt Sommer
  Jahr 1 mit Anfragekarte („Nichtangriffspakt“, „noch 15 h 27 min“); Winter Jahr 2 (Tag 3; mit
  `KINGDOMS_DEV_WEATHER=schnee:90:2` nach 15 Spielstunden Schneedecke auf Karte und Minikarte);
  Finanzen „Hof und Feste“ und Bevölkerung „Fest ausrichten“ per XTest gescrollt; Fest pausiert
  angeordnet (Knopf gesperrt, Hinweis), nach dem Fortsetzen −544 Gold, Zufriedenheit 77 → 83 %,
  „FESTTAG“ oben, Meldung mit Gesandten aus 4 Reichen, „Das nächste Fest ist ab Sommer · Tag 3 ·
  Jahr 3 möglich.“; Räuberüberfall in junger Stadt (Tag 25, 68 Einwohner,
  `KINGDOMS_DEV_EVENT=bandit_raid`): Dialog „Etwa 8 Räuber greifen gegen 17:00 Uhr …“, nach
  „Abwarten“ Marke „Räuberbande (8) · in 7 h 59 min“ mit Tooltip „Ankunft 17:00 Uhr“ und
  Meldung „Späher melden …“; Holzfällerhütte jenseits der Lagerreichweite hinter einem Wachturm
  an der Grenze (Spielstand aus einem vorübergehenden Test, danach entfernt): bernsteinfarbener
  Geist, rote Warnung. Logs ohne ERROR/panic.

**Offen**
* **Mauerecken (Sim-Regel):** Baustellen von Mauer und Tor brauchen weiter eine Straße am
  Eingang, die Ecken eines geschlossenen Rings erreichen keine (Wunsch der Spiel-Lane, nicht
  umgesetzt).
* **Meldungsart und Gebäude-Kennung** in `Notification` (Wunsch der Spiel-Lane) nicht umgesetzt;
  „Holzfällerhütte: keine Rohstoffe im Umkreis“ wiederholt sich weiter alle 12 h je Gebäude (von
  der Sim-Lane als handlungsrelevant belassen). Der Förster meldet in zwei Jahren
  Entwicklerstadt rund 35-mal (Information, in einer Karte „×35“ zusammengefasst).
* Die Erinnerung „Festgäste“ im Reiter Diplomatie nur über Tests der Simulation geprüft, nicht
  im Bild.
* Aus der Spiel-Lane weiter offen: Gasthaus mit Bier und die Zeile „Einarbeitung“ nicht im Bild,
  Kamera-Folgen hängt einen Frame nach, „Später“ auf der Anfragekarte setzt eine automatische
  Pause nicht fort.

## 2026-11-24 · Sim-Lane Schritt 9: Feinschliff – Räuber, Posteingang, Goldsenken, Gebiet, Meldungen (`crates/sim`, `data/`, Doku)

**Fertig**
* **Räuber fair getaktet** (`military.bandits`, §11): Schonfrist aus Forschung und Rang
  abgeleitet (Bürgerwehr 1,5 Tage + 100 Gold, Kaserne 14 h, Wachturm und vier Milizen – ein Dorf
  braucht rund eine Woche und 350 Gold): kein Überfall vor Tag 24 (Leicht 36, Schwer 18) und vor
  40 Einwohnern (neue Ereignisbedingung `RaidGraceOver`, ersetzt „≥ 30 Bewohner“). Größe
  3 + 1 je 12 Einwohner × Schwierigkeit (75/100/125 %), je 2 Verteidiger (Soldaten, Türme,
  Posten, Burg) ein Räuber weniger, höchstens die Hälfte. Warnung: die Bande erreicht den Rand
  8 h nach der Meldung der Späher (mindestens 2 h nach der Entscheidung, vorher 2 h nach der
  Entscheidung); Wahl und Meldung nennen die Uhrzeit („Etwa 5 Räuber greifen gegen 17:00 Uhr
  unbewachte Lager an“). „Soldaten ausschicken“ hält die Soldaten bis zur Ankunft bereit und
  jagt 8 h ab Ankunft.
* **Posteingang gedrosselt** (`diplomacy.ai`, §10): je Reich ein offener Vorschlag, höchstens
  2 insgesamt, keiner vor Tag 3, mindestens 24 h zwischen zwei neuen (in den ersten 12 Tagen
  48 h); Geschenke, Grenzzwischenfälle und Friedensangebote im Krieg bleiben frei. Frist und
  Ablauf unverändert (18 h, dann abgelehnt).
* **Goldsenken** (§7 Feste und Hofhaltung): `Command::HoldFestival` – 100 Gold + 4 je Einwohner,
  Kirche oder Markt nötig, höchstens alle 12 Tage: 1 Festtag, Zufriedenheit +8 für 6 Tage,
  Erinnerung „Festgäste“ +4 bei jedem Reich ohne Krieg (höchstens +12); Modifikatoren tragen
  ihre Quelle (`ModifierSource`, Zeile „Euer Fest“). Hofhaltung je Rang ab der Stadt (15, 60,
  120, 200 Gold/Tag), im Unterhalt gebucht. Berichte `report::court_report`/`festival_report`
  mit Kosten, Wirkungen und Hinderungsgrund. Prunkbauten nicht umgesetzt (neuer Gebäudetyp
  bräuchte Grafik und Symbol; Dom, Akademie und Burg erfüllen die Rolle).
* **Gebiet (Seed 7):** Ursache war keine Regel, die Ausdehnung verhindert, sondern ein Spieler,
  der nur in Reichweite des Rathauses baute: Baustellen werden nur aus Lagern in Trägerreichweite
  beliefert, die Grenze lag nach den ersten Türmen außerhalb. Regel bleibt, wird aber sichtbar:
  neue Bauwarnung `PlacementWarning::NoStorageInReach`. Der Langzeit-Spieler setzt jetzt
  Außenposten-Lagerhäuser an den Rand, einen Markt daneben, baut Türme bis 40 Felder und Häuser,
  Werkstätten und Holzfäller jenseits der alten Ringe, wo Lager und Markt hinreichen, betreibt
  einen Steinbruch je 80 Einwohner, hält Stein für Kirche/Schule zurück, baut Eisen (Mine,
  Köhlerei, Schmelze) und feiert Feste. Ergebnis acht Jahre: **Großstadt auf allen drei Karten**
  (Tag 83/108/133; vorher zwei von drei, Seed 7 Marktflecken), 204–351 Einwohner, Seed 7 Stadt nach
  50 Tagen und 2 833 Felder. **Königreich** nirgends: Burg (20 Eisenbarren) und Gebiet fehlen –
  gewollt, Begründung in game_design §1, Anforderungen unverändert.
* **Texte:** eine Dauerform für alle Simulationstexte (`report::hours_text`, `ticks_text`):
  „Antwort binnen 18 h (bis morgen 04:00 Uhr)“, „Sie greift in etwa 8 h an (gegen 17:00 Uhr)“,
  „jagen die Feinde für 8 h“, Feldzug „Ankunft in etwa 1 Tag 6 h“, Forschung „etwa 14 h“,
  Modifikatoren „noch 2 Tage 6 h“. „Für die Forschung … fehlt noch 1 Gold“.
* **Meldungen beruhigt:** dichter Wald am Försterhaus ist eine neutrale Information (einmal je
  Episode, höchstens einmal je Gebäude und Tag, „Der Wald im Umkreis ist dicht – gepflanzt wird
  wieder, sobald Platz frei wird.“); ein schwerer Sturm wird nur an seinem ersten Tag gemeldet.
  Geprüft und unverändert (gedrosselt und handlungsrelevant): Rohstoffmangel (12 h je Gebäude),
  Hunger, Kälte, Obdachlose (6 h), „Keine Träger“ (6 h), „Kein Weg“ (6 h je Gebäude), Bankrott
  und Kasse (täglich), Forschungsgold (einmal je Technologie).
* **Positionen für die Meldungsliste:** Kriege und Frieden der Reiche untereinander (Sitz),
  überfallene Karawanen (Sitz der Räuber), Obdachlose (Hauptquartier), Frost (erstes Feld),
  Feldzug bricht auf/fällt aus und Feldschlacht (Kartenrand), Fest (Hauptquartier).
  Ohne Position bleiben nur reichsweite Meldungen (Kasse, Rang, Forschung, Jahreszeit, Wetter).
* Spielstand-Format 16 (Modifikator-Quelle, letzter Festtag, „Festgäste“, letzter KI-Vorschlag).

**Geprüft**
* `cargo test -p kingdoms_sim`: Sim 403 (+ 9 ignorierte), Kampagne 8 (+ 2 ignorierte),
  Determinismus 12, Fuzz 7, vernünftiger Spieler 2, Doctest 1 – grün. Neu u. a.: Schonfrist je
  Schwierigkeit und Einwohnerzahl, Bandengröße nach Stadt, Schwierigkeit und Verteidigern, ein
  Dorf mit Wachturm und vier Milizen schlägt seine Bande ohne Verluste, Ankunft 8 h nach der
  Meldung bzw. 2 h nach später Entscheidung, Alarm ab Ankunft, Posteingang (je Reich einer,
  höchstens 2, Abstände, nichts vor Tag 3), Fest (Kosten, Ort, Gold, Abklingzeit, Modifikatoren,
  Gesandte), Hofhaltung im Unterhalt, Bauwarnung fern jedes Lagers, Dauerform, Förster einmal je
  Tag und neutral; Determinismus-Test mit Fest, Räubermeldung und gedrosseltem Posteingang über
  Speichern/Laden; `HoldFestival` im Fuzz-Test.
* Kampagne (Release, 3 Jahre): fünf Seeds überleben mit Krieg und Frieden; von den 15
  Testüberfällen (Tag 26, 36, 60) werden 14 ohne eigene Verluste abgewehrt (Seed 23 verliert den
  dritten: 60 Gold); ehrgeiziger Spieler Stadt nach 63 (Seed 7) und 60 Tagen (Seed 23).
  **Begründete Anpassung:** der erste Testüberfall kommt an Tag 26 statt 20 – das Spiel schickt vor
  Tag 24 keine Räuber mehr; die Zusicherungen (mindestens ein Sieg, höchstens 6 Verluste) bleiben.
  Hofhaltung erst ab der Stadt, weil schon 10 Gold/Tag im Marktflecken den ehrgeizigen Spieler auf
  Seed 23 unter 100 Einwohnern hielten.
* Langzeitlauf `--release --ignored` (Seeds 7, 23, 31): grün, siehe oben; Seed 31 verliert 3
  Bewohner an Kälte an Tag 94 (Holzmangel im Winter, Skript), niemand verhungert.
* `cargo clippy -p kingdoms_sim --all-targets -- -D warnings` sauber;
  `RUSTDOCFLAGS='-D warnings' cargo doc -p kingdoms_sim --no-deps` sauber.
* Spiel gegen diese Simulation: Kopie des Hauptarbeitsbereichs mit `crates/sim` und `data/` der
  Lane, `cargo check --workspace --all-targets` ohne Fehler und Warnungen (keine Anpassung nötig).

**Offen**
* Darstellung (Wünsche an die Spiel-Lane): Knopf „Fest ausrichten“ mit Kosten, Wirkungen und
  Hinderungsgrund aus `report::court_report`, Zeile „Hofhaltung“ im Finanzen-Reiter, die neue
  Bauwarnung deutlich (rot) im Bauplatz-Tooltip, Ankunftszeit der Räuber im Ereignisdialog/Banner
  (steht schon im Text).
* Königreich und Imperium weiter nur rechnerisch; ohne Eisen in Reichweite bleibt die Burg aus.
* Holzmangel im Winter beim Langzeit-Spieler (Seed 31) und die Sim-Grenze „Erfrieren und
  Erholung heben sich auf“ bestehen weiter.
* Spielstände bis Format 15 werden abgelehnt (keine Umwandlung).


## 2026-11-24 · Spiel-Polish-Runde 9: Anfragen, Minikarte, Schatten, Tabellen, Meldungen (`src/`, `crates/engine`, `tools/assetgen`, `assets/`, Doku)

**Fertig**
* **Anfragen der Reiche:** Statt eines Dialogs je Anfrage in der Bildmitte eine kompakte Karte,
  angedockt rechts über den Meldungskarten (Spalte der Meldungen; die Bildmitte bleibt frei): Wappen,
  Anfrage, Frist („noch 10 h – danach gilt „Verweigern““, gekürzt, wenn sie nicht passt), beide
  Antworten mit Folgen, „1 von 3“ mit Vor/Zurück, „Später“ (blendet bis zur nächsten neuen Anfrage
  aus; alle bleiben im Reiter Diplomatie und unter „Ereignisse“ beantwortbar), „Zum Reich“. Nach einer
  Antwort folgt die nächste unbeantwortete; eine abgelaufene gibt ihrer Nachbarin den Platz.
  Ereignis-Entscheidungen und die Rangfeier haben Vorrang; die automatische Pause bleibt.
* **Minikarte:** Schneedecke des Wetters in fünf Stufen (offenes Land weiß, Wälder mit dunklen
  Kronen, Wasser offen) und Herbstfarben (Laubbäume orange, Wiesen gelblich, Nadelbäume grün).
  Neu gezeichnet nur, wenn sich Stufe oder Jahreszeit ändern (`SeasonLook`).
* **Fenstersymbol** unter Linux/Windows aus `assets/app_icon.png` (auf 128 px verkleinert;
  `WindowConfig::icon`, nie gespeichert); fehlende oder kaputte Datei: Warnung im Log, kein Symbol.
  macOS nutzt weiter das `.icns` des Bundles.
* **Tabellen** Wirtschaft (Engpässe/Überschüsse, Produktionsketten) und Handel (Aufträge,
  Marktpreise, Letzte Geschäfte, Karawanen, Handelspartner): Spaltenbreiten aus dem Inhalt
  (`table::fit_widths`, `Fit`; zu wenig Platz: flexible Spalten schrumpfen bis zu ihrem Minimum,
  Platz übrig: anteilig verteilt), lange Texte (Kettenstatus, Notizen, Karawanenweg und -ladung)
  brechen um und machen die Zeile höher; wo trotzdem gekürzt wird, zeigt ein Tooltip den Text.
* **Gebäudeschatten:** Der Generator markiert gemalte Schlagschatten mit Alpha 253
  (`IsoCanvas::shadow`, überlappende Schatten dunkeln nur einmal), der Shader
  (`FLAG_SHADOW_MARK`) schwächt sie mit derselben Stärke wie die Bodenschatten (`shadow_alpha`:
  Tageslicht und Wolken). Beide Marken (Schnee 254, Schatten 253) liest der Shader exakt aus den
  vier Texeln des Filters (`textureGather`), damit Filter und Mipmaps keine Schneeränder an
  Schatten erzeugen. Atlanten neu erzeugt; Konvention in docs/assets.md.
* **Diplomatie:** Beziehungsbalken mit Beschriftung über dem Balken („Beziehung +3 · Neutral“,
  Skala „−100 … +100“) und einer Marke für den Wert – nichts überdeckt sich mehr.
* **Feindgruppen:** Räuber in karminroter Kapuze, rostbraunem Leder und knochenweißer Schärpe auf
  knochenweißem Ring (vorher dunkel auf dunkel); die Beschriftung steht über dem höchsten Kopf der
  Gruppe (Median der Kämpfer, Streuung der Menge eingerechnet), nicht mehr auf den Figuren.
* **Meldungen im Spielstand:** Die letzten 200 Meldungen stehen in der `.kmeta` des Spielstands
  (gleiche Vertrauensregel: nur bei passendem Tick und Speicherzeit; alte Metadateien ohne
  Meldungen bleiben lesbar) und kommen beim Laden, Schnellladen und Fortsetzen zurück (ohne
  Karten und ohne Ungelesen-Zähler).
* **`KINGDOMS_SELECT`** wählt auch das Gebäude, dessen Eingangs-Tile angegeben ist
  (`picking::building_at_or_entrance`).
* **Ruhigere Meldungen:** Dieselbe Meldung (Text und Ort) erscheint je Spieltag höchstens einmal
  (Gefahren immer); eine Warnung über ein Gebäude, das nur wartet (Förster im dichten Wald),
  erscheint als Information – der Status kommt aus den mitgelesenen Ereignissen, auch nach Warps.
  Zählung über zwei Jahre Entwicklerstadt: Förster vorher 138 Warnungen an 31 Tagen, jetzt
  93 Informationen (je Förster und Tag höchstens eine); Holzfäller ohne Bäume 57 → 30,
  Rohstoffmangel Köhlerei 18 → 11, Sägewerk 15 → 8.
* **Belagerung prüfbar:** `KINGDOMS_DEV_WALLS=<radius>` zieht über Kommandos eine Mauer mit Toren
  um das Rathaus (Straßenring innen, Eingänge nach innen; die Ecken bleiben offen, siehe Offen);
  Gebäudeschau mit Bier im Lager (Waren, die Gasthäuser ausschenken).

**Geprüft**
* `cargo test --workspace`: Spiel 227, Assetgen 69, Engine 74, Sim 396 (+ 9 ignorierte),
  Kampagne 8 (+ 2 ignorierte), Determinismus 11, Fuzz 7, vernünftiger Spieler 2, Doctest 1 – grün. Neu u. a.: Anfragekarte (einmal je neuer Anfrage,
  Warteschlange, Nachfolger nach Antwort und Ablauf), Spaltenbreiten, Jahreszeit der Minikarte,
  Fenstersymbol (verkleinert, fehlend, kaputt, mitgeliefert), Schattenmarke (einmal gedunkelt, nur
  wo sichtbar, Farben unverändert, Symbole ohne Marke), Meldungen speichern und laden (Feed und
  Spielstand samt fremder und alter Metadatei), Ruhe am selben Tag, Gebäude über den Eingang,
  Mauerring, neue Startoption.
* `cargo clippy --workspace --all-targets -- -D warnings` sauber (`Job::Launch` der Ladeszene
  trägt die gewachsenen Startoptionen jetzt in einer Box).
* Atlanten zweimal in Scratch-Verzeichnisse erzeugt: bytegleich; geändert gegenüber vorher nur
  `buildings.png` (Schattenmarke), `characters.png` (Räuber), `icons.png` und `app_icon.png`.
* Bilder angesehen (Xvfb, lavapipe): Anfragekarte 1600 × 900 und 1280 × 720 (Entwicklerstadt,
  Sommer Jahr 2, „1 von 2“), Reiter Wirtschaft 1280 × 720 („+1.386/Tag“ ungekürzt), Handel
  1280 × 720 per XTest gescrollt (Karawanenzeile „Auf dem Heimweg nach Sturmhöhe noch 3 h 47 min“
  ungekürzt, Partner- und Preistabellen), Diplomatie mit neuem Balken, Minikarte im Winter (weiß)
  und Herbst (orange Laubwälder), Rathaus mittags und um Mitternacht (Schlagschatten auf dem
  Pflaster nachts verschwunden), Schnee nah und fern ohne helle Ränder an Schatten, Räuberbande
  im Anmarsch, beim Angriff und auf der Flucht (Beschriftung über der Gruppe), Belagerung
  (`KINGDOMS_WORLD_SIZE=small KINGDOMS_DEV_ARMY=10 KINGDOMS_DEV_WALLS=8 KINGDOMS_WARP_TICKS=19200
  KINGDOMS_DEV_THREAT=1:30`, Tempo 4): das Heer von Sturmhöhe schlägt auf das Nordtor ein, Feuer
  und Staub am Tor, Pfeil des Bogenschützenpostens, danach Bresche und Flucht; Festgäste vor Kirche
  und Dom der Gebäudeschau (`KINGDOMS_DEV_EVENT=religious_festival:0`, 18:25, „FESTTAG“);
  `KINGDOMS_SELECT` mit Eingangs-Tile wählt Gasthaus und Großes Sägewerk. Logs ohne ERROR/panic.

**Offen**
* **Nicht im Bild gesehen:** Gasthaus mit Bier (Gebäudeschau: Bier im Lager, aber die zehn Träger
  sind mit rund 25 wartenden Transporten ausgelastet; nach 2,5 Tagen noch „0 / 12“) und die Zeile
  „Einarbeitung“ (Gebäudeschau und Ausbau-Setup zeigten nur fertig eingearbeitete Belegschaften).
* **Mauern (Sim):** Baustellen brauchen eine Straße am Eingang; die Ecken eines geschlossenen
  Mauerrings erreichen keine, ein Spieler kann den Ring also nicht schließen (sim_request).
  „Die Feinde haben Tor durchbrochen!“ fehlt der Artikel (Sim-Text).
* **Meldungen (Sim):** Der Förster meldet „kein freier Platz“ jeden Tag neu als Warnung (die
  Problemzeit beginnt nach jedem Feierabend neu); das Spiel stuft sie nur herab. Meldungen tragen
  keine Art/Gebäude-Kennung, das Spiel erkennt sie über Ort und Gebäudestatus (sim_request).
* Kamera-Folgen (Bürger, `KINGDOMS_FOCUS=feind`) setzt die Kamera erst nach der Oberfläche: an
  die Karte gebundene Beschriftungen hängen einen Frame nach (bei 60 FPS unsichtbar, bei 10 FPS
  und Tempo 4 deutlich).

## 2026-11-24 · Schritt 9: Abschlussprüfung und Lieferstücke (`docs/`, `README.md`)

**Fertig**
* **Bilder:** 15 Screenshots des Release-Builds (1600 × 900) in `docs/screenshots/`: Hauptmenü,
  Neues Spiel, Spielstart, Stadt im Sommer, verschneite Winternacht, Reichsverwaltung Wirtschaft,
  Forschung, Diplomatie, Kampf gegen eine Räuberbande, Handel mit Karawanen, Ereignis „Brand“,
  Bürgerfenster, Ausbau zum Großen Sägewerk, Einstellungen, Spielstände. Alle zeigen echte
  Spielzustände (Entwickler-Variablen, echte Eingaben per XTest), verlustfrei verkleinert
  (RGB, pixelgleich geprüft). README mit neuem Abschnitt „Bilder“ (Hinweis: erzeugte
  Platzhalter-Grafik) und `docs/screenshots/` in der Projektstruktur.
* **Audio-Lieferliste:** JSON aus `assets/audio/manifest.ron` (eigener RON-Leser) mit allen
  143 IDs (15 Musikstücke, 11 Umgebungsschleifen, 58 Effekte, 11 Oberflächenklänge, 48 Ansagen;
  146 Dateien): Kanal, Art, erwartete Dateien, Schleife (nur Umgebung, Musik blendet über),
  Beschreibung, genauer Wortlaut der Ansagen, empfohlene Länge nach `assets/audio/README.md`,
  Anzahl Varianten. Für den Auftraggeber außerhalb des Projekts abgelegt.
* **Roadmap:** ✅-Zeile für diese Prüfung; die offenen Punkte (Mac-Rückmeldungen, Audiodateien,
  finale Grafiken) sind als ⬜ mit Begründung eingetragen, Phase 8 bleibt deshalb 🔶.
* Aufgeräumt: alte Scratch-`target`-Ordner früherer Durchläufe (zusammen etwa 6,3 GB) gelöscht.

**Geprüft**
* `cargo build --release`: ohne Warnungen (4 min 5 s).
* `cargo test --workspace`: Spiel 219, Assetgen 68, Engine 73, Sim 396 (+ 9 ignorierte),
  Kampagne 8 (+ 2 ignorierte), Determinismus 11, Fuzz 7, vernünftiger Spieler 2, Doctests
  Engine 0 und Sim 1 – alles grün.
* `cargo clippy --workspace --all-targets -- -D warnings` sauber;
  `RUSTDOCFLAGS='-D warnings' cargo doc -p kingdoms_sim -p kingdoms_engine --no-deps` sauber.
* Atlanten und App-Symbol neu in ein Scratch-Verzeichnis erzeugt: alle 24 Dateien in
  `assets/atlases` und `assets/app_icon.png` bytegleich.
* Keine Treffer für `TODO`, `FIXME`, `todo!(`, `unimplemented!(`, `dbg!(` in `crates/`, `src/`,
  `tools/`.
* Audio-Lieferliste per Skript geprüft: jede Manifest-ID genau einmal, Dateien, Ordner, Kanal,
  Varianten, Schleifen, Längenbereiche und Wortlaut stimmen; alle Dateien und Sprechertexte
  stehen auch im Bericht `kingdoms --audio-report` (143 IDs, 146 Dateien erwartet, 0 gefunden,
  Rückgabewert 1 wie vorgesehen).
* Jeder Screenshot angesehen; Logs aller Läufe ohne ERROR/panic (nur die erwarteten
  winit-/Audio-Hinweise unter Xvfb). Alle Umgebungsvariablen des Codes stehen in der README-Tabelle.
* `target/` des Projekts belegt 32 GB (davon `debug` 31 GB, `release` 1,6 GB).

**Offen**
* **Grafik:** alles ist erzeugte Platzhalter-Grafik; finale Grafiken müssen die Konventionen in
  `docs/assets.md` einhalten (auch die Schneemarke Alpha 254). Gebäudeschatten sind in die Sprites
  gemalt und werden nachts nicht schwächer; Dachschnee ist ein gleichmäßiger Belag; die Minikarte
  zeigt die Schneedecke des Winters nicht; kein Fenstersymbol unter Linux/Windows.
* **Audio:** keine Audiodateien geliefert, nie gehört (kein Gerät); Überblendung, Absenken bei
  Stimmen, Schleifen und Stimmung „Blühend“ nur per Test und Code geprüft; Lautheit nur als
  Spitzenpegel, nicht als LUFS gemessen.
* **macOS:** weder gebaut noch gestartet; macOS-eigener Code (Menü, Ausblenden, Cmd-Tasten nach
  Tastaturbelegung, Trackpad) nicht kompiliert, `sips`/`iconutil`/`codesign` in `make_app.sh`
  nicht ausgeführt; GPU-Zeiten nur mit lavapipe (nicht aussagekräftig).
* **Nicht im Bild gesehen:** Belagerung und Schläge gegen Tor/Mauer, Gasthaus mit Bier,
  Festgäste vor einer Kirche, Zeile „Einarbeitung“ im Info-Fenster (nur Tests).
* **Balance:** Königreich und Imperium nur rechnerisch; Seed 7 bleibt im Acht-Jahres-Lauf beim
  Marktflecken (Gebiet 1 474/1 600); Räuber im ersten Winter treffen Städte ohne Soldaten; im
  späten Spiel fehlt eine Goldsenke; ohne Brennholz heben sich Erfrieren und Erholung auf.
* **Texte und Bedienung:** Gebäudenamen ohne Genus/Dativ („Ausbau: Großes Sägewerk“);
  Simulationsmeldungen sagen noch „Antwort binnen 18 Stunden“ statt „h/min“; Anfragen der
  KI-Reiche öffnen sich oft und verdecken die Bildmitte (in der Entwicklerstadt drei auf einmal);
  `KINGDOMS_SELECT` mit dem geloggten Eingangsfeld wählt das Feld, nicht das Gebäude.
* **Spielstände:** ältere Formate (bis 14) werden abgelehnt, es gibt keine Umwandlung; die
  Meldungsliste wird nicht gespeichert.

## 2026-11-24 · Spiel-Schritt 8: Prüfbefunde, Dachschnee, Darstellung der Sim-Schritt-8-Daten (`src/`, `crates/engine`, `tools/assetgen`, `assets/`, Doku)

**Fertig**
* **Einstellungen beim Schließen des Fensters:** Geänderte Einstellungen werden auch geschrieben,
  wenn das Fenster bei offenem Einstellungsfenster geschlossen wird (Hauptmenü und Spiel; im Spiel
  vor der Rückfrage „Spiel beenden?“, gleich welche Antwort) und beim Programmende
  (`GameShared::flush_settings`).
* **Bauen in der Pause:** Wartende Platzierungen, Versetzungen und Straßen erscheinen als bläuliche
  Geister samt geplanter Zufahrt (`preview::draw_pending`); die Bauwerkzeuge behandeln deren Felder
  als belegt (`QueuedClaims`), ein zweiter Klick auf denselben Platz wird sofort rot abgelehnt
  statt nach dem Fortsetzen als „Der Platz ist bereits bebaut.“.
* **Dachschnee:** Im Winter liegt Schnee auf allen Dächern, Kegeldächern und flachen Oberseiten
  (Zinnen, Kisten, Fundamente), Wände bleiben frei. Der Generator markiert diese Pixel mit Alpha 254
  (`IsoCanvas::with_snow_mask`, nur Flächen, deren Normale genug nach oben zeigt; danach
  darübergemalte Details wie Schilder, Waren und Traufkanten bleiben frei); warme Flächen bleiben
  schneefrei (`IsoCanvas::snow_free`: Meiler, Brenn- und Backöfen, Essen, Rennofen, Hochofen,
  Ringofen, Schornsteine, Brunnenschacht). Neuer Sprite-Flag `FLAG_SNOW_ON_ROOF` legt die
  Schneedecke der Umgebung auf markierte Pixel (Gebäude, Mauern, Tore, Nachbarreiche); Symbole und
  App-Symbol entfernen die Marke. Konvention in docs/assets.md.
* **Texte und Zahlen:** Rezept ohne Erzeugnis zeigt seinen Namen („Setzling pflanzen (2 h)“);
  „Wohlgesonnen“ zählt nur freundschaftliche und verbündete Reiche; Bürgerliste „arbeitslos“ statt
  „Erwachsen“; „Ihr“ in der Anrede überall groß (auch in den Stärketexten der Simulation);
  „Einkauf 2,28 Gold · Verkauf –“; Minuszeichen „−“ zentral in `format::int`/`decimal`; „1 Datei“
  bei nicht lesbaren Spielständen; Einzahl mit Verb („1 Einheit in Auftrag geben“, „bis zu
  1 Soldat“, „1 Arbeitsgang“, „1 holt ab“, „1 Sammler trägt Ertrag heim“, „1 Soldat daheim“);
  Preisspalten mit zwei Nachkommastellen („3,50“); Ausbau-Grund als ein Satz („Nicht genug Stein
  (0 von 30) – und ein weiterer Grund.“).
* **Bürgerfenster** endet über der unteren Leiste; **Arbeitsklänge** für Köhlerei, Ziegelei,
  Töpferei (`sfx_kiln`), Gerberei, Kunsthandwerker (`sfx_workshop`) und Imkerei (`sfx_bees`, mit
  Großen Stufen; 128 Klang-IDs); **macOS** Cmd+Q/H/M nach dem logischen Zeichen (AZERTY);
  `KINGDOMS_FOCUS`/`KINGDOMS_SELECT` wirken auch mit `KINGDOMS_QUICK=load`; `RenderList`-Puffer
  liegen im Renderer (`Renderer::begin_list`/`recycle_list`) statt in einem `thread_local!`; leeres
  `config/` entfernt, `tools/macos` in den Projektbäumen.
* **Sim-Schritt-8-Daten:** Reiter Statistik mit Zeitraum „Jahreszeiten (gesamt)“ für jede Anzeige
  (`statistics().seasons()`, Achse „J2 Fr“, Tooltip mit Rang) und den neuen Anzeigen „Wohlstand“
  (Ersparnisse aller Haushalte und je Haushalt, je Tag und je Jahreszeit) und „Gebiet und Bauten“;
  Zeile „Einarbeitung“ im Info-Fenster, solange neue Fachkräfte lernen; Einarbeitung und
  „Krankgeschrieben – sucht nach der Genesung neue Arbeit“ im Bürgerfenster; Warnung vor
  Versorgungslücken über „Ausbauen“ („Während des Ausbaus fehlt 18 Häusern die Versorgung
  „Markt“.“); „Kein freier Platz zum Pflanzen“ als neutrale Wartezeit; Handelshinweis, dass Boni die
  Marge höchstens bis auf den Preis des Partners schließen.

**Geprüft**
* `cargo test --workspace`: Spiel 219, Assetgen 68, Engine 73, Sim 396 (+ 9 ignorierte),
  Kampagne 8 (+ 2 ignorierte), Determinismus 11, Fuzz 7, vernünftiger Spieler 2, Doctest 1 – grün.
  Neu u. a.: Schneemaske (Dach ja, Wand und Boden nein, warme Flächen und Übermaltes nein, Farben
  unverändert), Langzeitreihe der Statistik (Achse, Titel mit Rang, Bilanz, je Haushalt, Nahrung,
  Ausdünnung), Zeitraum je Anzeige, Einarbeitungszeile, Krankenstand, Ausbau-Satz, Handelshinweis.
* `cargo clippy --workspace --all-targets -- -D warnings` sauber; `RUSTDOCFLAGS='-D warnings' cargo doc
  -p kingdoms_sim --no-deps` sauber.
* Atlanten zweimal erzeugt: byte-gleich; gegenüber vorher ändern sich nur `buildings.png` und
  `roads.png` (Schneemarke), `icons.png` bleibt gleich. Kontaktbogen aller Gebäude mit simuliertem
  Schnee angesehen.
* Screenshots (Xvfb, lavapipe) angesehen: Winter mittags in der Entwicklerstadt (Dächer weiß,
  Wände braun, Traufkanten sichtbar), Hauptmenü und Herbst unverändert; Statistik „Jahreszeiten“,
  „Wohlstand“ (Tage und Jahreszeiten) und „Gebiet und Bauten“; Marktplatz mit Versorgungswarnung;
  Bürgerfenster mit Einarbeitung (über der unteren Leiste); Försterhaus „Kein freier Platz zum
  Pflanzen“ neutral; wartender Hausgeist in der Pause. XTest: Musik-Regler geändert, Fenster
  geschlossen – im Hauptmenü und im Spiel (Rückfrage, „Ohne Speichern beenden“) steht
  `music: 0.1` in `engine.ron`.

**Offen**
* Die Zeile „Einarbeitung“ im Info-Fenster ist nur durch Tests belegt (die Entwicklerstadt zeigte
  keinen Betrieb mit neuen Fachkräften).
* Dachschnee ist ein gleichmäßiger Belag nach Schneedecke; Schneelast an Traufen, abrutschender
  Schnee oder tauende Dächer über geheizten Häusern gibt es nicht.
* Echte Grafiken müssen die Schneemarke (Alpha 254) selbst setzen; ohne sie bleiben Dächer frei.
* macOS-Kürzel weiter nur gegen den winit-Quelltext geschrieben, nicht auf einem Mac gebaut.

## 2026-11-24 · Sim-Lane Schritt 8: Prüfbefunde, Ausbauketten, Einarbeitung, Langzeitstatistik und Acht-Jahres-Lauf (`crates/sim`, `data/`, `tools/assetgen`, `assets/`, Doku)

**Fertig**
* **Rathaus → Burg aus dem eigenen Lager:** Ein Lager im Ausbau hat keine Träger; es beliefert
  jetzt seine eigene Baustelle direkt aus dem eigenen Bestand (`logistics::supply_own_upgrades`
  vor jeder Planungsrunde, nur nicht versprochene Waren), ist weder Quelle noch Ziel neuer
  Transporte (wartende Aufträge gehen an andere Lager) und meldet 0 Träger. Vorher blieb der
  Ausbau ewig stehen, wenn Stein, Bretter und Eisen nur im Rathaus lagen.
* **Gebiet beim Ausbau:** Start und Abbruch eines Ausbaus mit größerer Grundfläche rechnen das
  Gebiet sofort neu (Kreis um die Mitte), der Abschluss auch dann, wenn nur der Ausgangstyp Gebiet
  beansprucht – ein geladener Spielstand ist wieder gleich dem Original.
* **Fliehende Banden ohne Ausweg** (Insel, Brücke abgerissen) zerstreuen sich nach
  `armies.withdraw_stuck_hours` wie eingemauerte Heere („Die Räuber finden keinen Weg aus Eurem
  Land und zerstreuen sich.“) – kein Daueralarm mehr. Nebenbei: abgezogene Kämpfer verlassen
  ihre Gruppe im selben Tick (Prüfung nach jedem Tick konsistent; vom Fuzz-Test gefunden).
* **Präparierte Spielstände:** Geldbörsen über 10¹² Gold und unmögliche Soldaten-Stunden werden
  beim Laden abgelehnt; Summen in Wohlstand, Prognose, Bericht und Proviant sind gesättigt.
* **Texte:** „(1 Person)“, „1 Soldat kam nicht rechtzeitig und bleibt daheim“, „Gefallen:
  1 eigener Soldat, kein feindlicher Soldat.“ (auch in der Feldschlacht), „Es steht kein Soldat
  bereit (nötig: 5 Soldaten).“ / „Es steht nur 1 von 5 …“, „mindestens 1 Soldat“, „1 freier
  Wohnplatz“, „1 Feld vom Fluss“; Ereignistexte mit Verb nach Zahl („1 Soldat jagt“, „Höchstens
  ein Gebäude am Fluss wird überschwemmt“, „Etwa 1 Bewohner erkrankt“, „1 Flüchtling zieht zu“).
* **Frost:** Obst verdirbt nur zu Winterbeginn; Frosttage im Frühling/Herbst töten weiter nur
  unreife Saat (mit den Klimadaten selten: kaltes Startgebiet mit Schnee oder Sturm).
* **Kranke im eigenen Krankenhaus** gelten als Patienten, nicht als anwesende Arbeiter.
* **Krankenstand:** Wer 24 h am Stück krank ist, gibt die Stelle ab und nimmt erst gesund wieder
  Arbeit an (`sick_leave_after_hours`) – Kranke blockieren keine Holzfällerhütte durch den Winter.
* **Handelspreise:** Ein Bonus verringert die Marge höchstens bis auf den Preis des Partners
  (`economy::trade::trade_percent`, Karawanen und Weltmarkt) – Einkaufen kostet nie weniger, als der
  Verkauf derselben Ware bringt.
* **Qualifikation:** Einarbeitung in Fach- und Gelehrtenarbeit (`population.training`: Neulinge 80 %,
  Gebildete 90 %, nach 3 Arbeitstagen voll; Erfahrung je Berufskategorie gespeichert; die Siedler
  des Spielstarts sind eingearbeitet) in Belegschaft, Tagesleistung und Forschung; Berichte
  `ProductionReport::training_percent`, `CitizenDetail::training`.
* **Försterhaus im dichten Wald:** eigener Status „Kein freier Platz zum Pflanzen“ (gilt als untätig)
  und nur eine Meldung je Episode („… – der Wald ist dicht.“).
* **Langzeitstatistik:** eine Stichprobe je Jahreszeit für das ganze Spiel (`Statistics::seasons()`,
  höchstens 400, dann ausgedünnt) und Ersparnisse im Tagesverlauf der Bevölkerung.
* **20 neue Ausbaustufen** mit Daten, Forschung, Grafik, Symbol und Nachtlicht: Holzfällerlager,
  Großer Steinbruch, Hochofen (mit Arbeitsschleife), Große Ziegelei, Töpferei, Weberei, Schneiderei,
  Gerberei, Tischlerei, Brauerei, Große Fischerei, Große Imkerei, Großer Markt, Dom, Akademie,
  Spital, Gasthof, Garnison, Schützenturm, Gestüt (32 von 82 Typen mit Ausbau). Forschungsboni und
  Ausbildung gelten auch für Ausbaustufen (`BuildingDef::lineage`); bewusste Endstufen in §5
  begründet. `upgrade_service_gaps` warnt, wenn Häuser während eines Ausbaus einen Dienst
  verlören (der einzige Markt: Hunger). Spiel: Arbeitsklänge der neuen Werkstätten, Gebäudeschau
  mit fünftem Ring.
* **Langzeitlauf:** neuer Spieler `Style::Expansive` (Holz und Nahrung vor dem Wachstum, Lehm für
  Bauernhäuser, Schule, Lagerhaus, zweiter Markt, Kontor, Verkauf von Überschüssen, Ausbauten,
  Burg). Acht Jahre: 208–322 Einwohner, Großstadt nach 118 bzw. 169 Tagen auf zwei von drei Karten;
  der Stillstand bei 90–100 Einwohnern lag an der Bauordnung, Gold ist danach kein Engpass
  (Kurve in game_design §1). Spielstand-Format 15.

**Geprüft**
* `cargo test --workspace`: Spiel 206, Assetgen 66, Engine 73, Sim 396 (+ 9 ignorierte),
  Kampagne 8 (+ 2 ignorierte), Determinismus 11, Fuzz 7, vernünftiger Spieler 2, Doctest 1 – grün.
  Neu u. a.: Burg-Ausbau aus dem Rathaus mit Speichern/Laden direkt nach dem Start (schlägt ohne
  Eigenbelieferung bzw. ohne Gebietsneuberechnung fehl), Gebiet bei Start/Abbruch, Insel-Bande,
  präparierte Börsen und Soldaten-Stunden, Texte, Frost im Frühling, Krankenhaus-Patient,
  Krankenstand, Einarbeitung, Preisgrenze mit bestem Bonus, dichter Wald, Langzeitreihe,
  Versorgungslücke, Ausbauketten.
* `cargo test --release -p kingdoms_sim --test campaign expansive -- --ignored`: Seeds 7/23/31
  ohne Verhungern; 23 und 31 Großstadt. Ehrgeiziger Spieler: Seeds 7 und 23 Stadt nach 69/68 Tagen.
* `cargo clippy --workspace --all-targets -- -D warnings` sauber;
  `RUSTDOCFLAGS='-D warnings' cargo doc -p kingdoms_sim --no-deps` sauber.
* Grafik angesehen: Kontaktbögen alt/neu aller 20 Stufen; im Spiel (Xvfb + lavapipe, 1600 × 900,
  `KINGDOMS_DEV_SHOWCASE=1`, Seed 331, Zoom 1,1) Dom, Großer Markt, Akademie, Spital, Großer
  Steinbruch, Hochofen, Garnison, Schützenturm, Gestüt und Große Ziegelei. Atlanten zweimal
  erzeugt, bytegleich. Kein ERROR/panic im Log.

**Offen**
* Darstellung (Wünsche an die Spiel-Lane): Zeitraum „Jahreszeiten (gesamt)“ im Reiter Statistik
  aus `Statistics::seasons()`, Ersparnis-Verlauf aus `PopulationDay::savings`; Einarbeitung und
  Krankenstand im Bürgerfenster, Zeile „Einarbeitung“ in der Produktivitäts-Aufschlüsselung
  (`training_percent`), Warnung `upgrade_service_gaps` im Ausbau-Abschnitt; `NoPlantingSpace` als
  neutrale Wartezeit (heute rot wie ein Problem) – Farben in `info_panel`/`management/production`.
* Seed 7 bleibt im Langzeitlauf ohne Stadt-Gebiet (1 474/1 600): die Türme finden an der Grenze
  keinen versorgten Platz; der Burgausbau kam auf keiner Karte zustande (Eisenbarren nur über
  Kaufaufträge). Königreich/Imperium weiter nur rechnerisch.
* Erfrieren und Erholung heben sich weiter auf (Gesundheit bleibt bei fehlendem Brennholz stehen);
  mit dem Krankenstand blockiert das keine Stellen mehr.
* Gebäudeschau: Seed 331 stellt jetzt alles außer der Sandgrube.

## 2026-11-23 · Merge Schritt 7: Sim-Lane 7b + Spiel-Lane 7a/7b, Darstellung der Sim-Schritt-7b-Daten (`crates/sim`, `data/`, `src/`, Doku)

**Fertig**
* **Zusammenführung:** Die Spiel-Lane hatte `crates/sim` und `data/` nicht angefasst (letzte
  Änderung dort vor dem Start der Sim-Lane). Sicherung von `crates/sim`, `data/`, `Cargo.lock`
  und `docs/` im Scratch-Verzeichnis, dann `sync.sh to-main`. Doku von Hand: `game_design.md`
  aus der Sim-Lane (die Spiel-Lane hatte sie nicht geändert), `architecture.md` §3 aus der
  Sim-Lane, alle anderen Abschnitte aus der Spiel-Lane, Fortschritt und Roadmap kombiniert.
  Das Spiel baute ohne Anpassung an der Sim-API.
* **Kampfbild mit Hieben:** Funken und Staub kommen jetzt aus `CombatRound::strikes()` statt aus
  der Nähe von Feind und Soldat (`effects::strike_spots`): je Angreifer einmal, mitten zwischen
  Angreifer und getroffenem Kämpfer an ihren jetzigen Plätzen – ein Räuber an seinem Platz in
  der Menge seiner Bande (`citizens::crowd_position`) –, bei Schlägen gegen Tor, Mauer oder
  Gebäude knapp vor der getroffenen Kante, aus einer Besatzung heraus vom Rand des Gebäudes.
  Angreifer schauen zu ihrem Ziel (`CitizenView::facing` der Simulation); eine Figur, die
  versetzt in der Menge steht, dreht sich von ihrem Platz aus zum Ziel
  (`citizens::attack_facing`). Turmpfeile erkennt die Darstellung an `Shot::shooter`.
* **Gemessen gegen möglich:** Das Info-Fenster zeigt unter der Wirtschaftlichkeit einen Balken
  „Gestern N % des Möglichen“, den Satz der Simulation („Gestern 8 Arbeitsgänge (8 Einheiten
  Holz), heute bisher 0 – möglich sind 16,5 je Tag.“) und das Mittel der gespeicherten Tage
  („Mittel der letzten 9 Tage: 8,6 Holz je Tag“). Feldfarmen und Obstgärten erklären ihre Zahlen
  im Tooltip als Ernten im Mittel der gemessenen Tage („Abgeerntete Felder“, „Wert der Ernte“;
  vor dem ersten Tag „noch nicht gemessen“) statt „mit der zugewiesenen Belegschaft“.
  Herstellkosten mit Cent („0,03 Gold je Einheit“ statt „0“). Der Reiter **Produktion** hat
  die Spalte „Gestern“ („5 von 6,5“ nach Anteil gefärbt, bei Feldern die Ernte des Tages, der
  Satz der Simulation als Tooltip); die Produktionsketten im Reiter **Wirtschaft** zählen
  Feldfarmen und Obstgärten ab ihrem ersten gemessenen Tag mit und nennen im Tooltip die
  Grundlage.
* **Proviant:** Der Reiter **Militär** zeigt je Feldzug „Proviant 34 von 60“ (`Campaign::provisions`
  abzüglich `provisions_eaten`), gelb unter einem Fünftel.
* **Texte:** Die Fußzeile des Ereignisdialogs nennt die Frist mit Tag („Frist bis morgen
  06:00 Uhr“, `report::deadline_clock_text`). In der Simulation (Wunsch der Spiel-Lane): die
  Restzeit der Ereigniswirkungen lautet unter einem Tag „noch 7 h 30 min“ wie jede Dauer des
  Spiels (vorher „noch 8 Stunden“); `WealthSummary`, `WealthClassRow` und `TavernSummary` sind
  aus `report` erreichbar. Die neuen Meldungen der Sim-Lane (Heer löst sich auf, Proviant zurück
  ins Lager) kommen als `Notification` und erscheinen ohne Änderung im Meldungsstrom; kein
  Spielcode vergleicht geänderte Sim-Texte.

**Geprüft**
* `cargo test --workspace`: Spiel 206, Assetgen 66, Engine 73, Sim 383 (+ 9 ignorierte),
  Kampagne 8 (+ 1), Determinismus 10, Fuzz 7, vernünftiger Spieler 2, Doctest 1 – grün. Neu:
  Funkenorte und Blickrichtung in einem echten Überfall (Entwicklerheer, Bande von 30, jeder
  Hieb mit Funken innerhalb von 2 Feldern um Angreifer und Ziel, Räuber in der Menge schauen zum
  Soldaten), Funken- und Kantengeometrie, Spalte „Gestern“, Anteil und Mittel des Gemessenen,
  Grundlagentexte, Kettentooltip, Proviant, Frist mit „morgen“, Cent-Preise, „noch 7 h 30 min“.
* `cargo clippy --workspace --all-targets -- -D warnings` sauber;
  `RUSTDOCFLAGS='-D warnings' cargo doc -p kingdoms_sim --no-deps` sauber.
* Bilder angesehen (Xvfb + lavapipe, 1600 × 900): Hauptmenü; Direktstart; Kampf (kleine Karte,
  Seed 7, `KINGDOMS_DEV_ARMY=10`, `KINGDOMS_WARP_TICKS=9600`, `KINGDOMS_DEV_THREAT=banditen:30`,
  Tempo 4, `KINGDOMS_FIXED_DT=0.1`, Bilder 46–64): der Soldat schlägt zur Bande hin zu,
  Funken zwischen ihm und den Räubern, Staub beim Abzug; Entwicklerstadt Sommer Jahr 1
  (`KINGDOMS_WARP_TICKS=43200`): Reiter Produktion mit „Gestern“ und Tooltip, Gemüsefarm und
  Holzfällerhütte im Info-Fenster (per XTest heruntergescrollt) mit Ernte-Wirtschaftlichkeit bzw.
  „Gestern 48 % des Möglichen“ und Mittel; Winter Jahr 2 (`209600`): Reiter Wirtschaft mit
  Feldfarmen in den Ketten, Obstgarten im Winter (−2,9 Gold/Tag, keine Ernte gemessen);
  Gebäudeschau Seed 331 mit dem Reiter Produktion (Großes Sägewerk). In keinem Log WARN,
  ERROR oder panic.
* `sync.sh from-main`, danach in der Lane-Kopie `cargo test -p kingdoms_sim` grün (gleiche Zahlen).

**Offen**
* Schläge gegen Tor und Mauer sind nur per Unit-Test und Code geprüft; im Bildlauf kämpften
  Räuber gegen Soldaten, eine Belagerung wurde nicht angesehen. Proviantspalte und „Frist bis
  morgen …“ nur per Test (kein Feldzug, kein Ereignis über Mitternacht im Bild).
* „Möglich“ ist die Tagesleistung der heutigen Belegschaft rund um die Uhr (Rechnung der
  Simulation); am ersten Spieltag ist „gestern“ ein angebrochener Tag.
* Entwicklerstadt im Spiel (mit Zufallsereignissen, anders als der Zwei-Jahres-Test ohne
  Ereignisse): am zweiten Wintertag des zweiten Jahres arbeitet keine der 11 Holzfällerhütten
  („Keine Rohstoffe im Umkreis“), Holz 1, zu Winterbeginn fror eine Familie. Nicht geändert
  (Entwicklerhilfe); die Sim-Lane sah im Test ohne Ereignisse keinen Bedarf.
* Weiterhin offen aus den Lanes: Wohlstandsverlauf je Tag (braucht Verlauf und Spielstandfeld in
  der Simulation), Geschlecht/Dativ der Gebäudenamen („Ausbau zum Großen Sägewerk“), Hilfsfunktion
  für Festgäste je Ort, Werkzeug je Arbeitsgang statt je Gebäudetyp.

## 2026-11-23 · Spiel-Schritt 7b: Grafik-Vollständigkeit, Darstellungsleistung, macOS (`src/`, `crates/engine`, `tools/assetgen`, `tools/macos`, `assets/`, Doku)

**Fertig**
* **Sammler mit eigenem Werkzeug:** neue Schleifen `<man|woman>_<dir>_<pickaxe|shovel|rod>`
  (gleiche Figur, 32 × 24, Anker wie die Arbeitsschleifen): Spitzhacke mit Steinsplittern,
  Spaten mit Erdklumpen und Auswurf, Angel mit Auswurf, Schwimmer und Biss (einmal auswerfen,
  dann halten; von hinten liegt der Schwimmer weiter weg). Das Spiel wählt die Schleife nach
  dem Sammeln des Arbeitsplatzes (`GatherTool::of`, je Gebäudetyp einmal): Holzfäller Axt,
  Steinbruch (und Minen) Spitzhacke, Lehm-/Sandgrube und Förster Spaten, Fischerei Angel.
* **Gebäudeschau** `KINGDOMS_DEV_SHOWCASE=1`: Szenario `dev_showcase` im Speicher (wie
  `dev_crowd`) mit Burg und je einem fertigen, besetzten Gebäude jeder Art auf Zellenringen, allen
  Technologien, Zutaten aller Rezepte, ohne Rivalen und Zufallsereignisse; das Log nennt jedes
  Gebäude mit Eingangs-Tile. Seed 7: alles außer Sandgrube und drei Minen (kein Vorkommen auf dem
  Land der Burg), Seed 331: jedes Gebäude.
* **Schleifen auf der Karte geprüft** und verbessert: Mühlen- und Großmühlenflügel drehen sich,
  Essen von Schmiede, Meisterschmiede, Eisenschmelze flackern. Neu: Schleife der
  Waffenschmiede (Esse im Hof mit Funken); Feuerfarben weiter gespreizt (Aufflackern,
  Glühen, Glimmen statt dreier fast gleicher Orangetöne); Schmiede und Meisterschmiede haben ein
  zweites Feuermaul zur offenen Seite, damit das Feuer unter der Traufe sichtbar ist (bei der
  Meisterschmiede war es nur ein Pixel).
* **Einheitlichkeit geprüft** (Kontaktbögen aller Gebäude und Figuren, Übersicht der Schau bei
  Zoom 0,6): gleiche Lichtrichtung, Konturen, Bodenplatten und Maßstäbe; aus der Ferne lesbar.
  Atlanten deterministisch neu erzeugt; App-Symbol `assets/app_icon.png` (1024 px, Burg auf
  blauem, goldgerahmtem Quadrat nach Apples Raster) aus dem Generator.
* **Messwerkzeuge:** `KINGDOMS_PROFILE=<frames>[@<start>]` (Mittel, Median, p95, Maximum je
  Abschnitt ins Log, mit Sprite- und Draw-Call-Zahlen), `KINGDOMS_FIXED_DT=<s>` (jeder Frame
  gleich lang → pixelgleiche Läufe), `KINGDOMS_AUTOSHOT_FRAME=30,37` (mehrere Bilder). Neue
  Abschnitte: jede Kartenebene (`draw: …`), Minikarte, Renderer-Schritte (`render: acquire`,
  `prepare`, `encode`, `submit`).
* **Optimierungen:** Terrain, Straßen, Felder und Grenze werden für einen Dirty-Chunk nur neu
  gebaut, wenn sich Geländeart oder Höhe geändert haben (`GroundSignatures`) – auf großen
  bewaldeten Karten meldete das Baumwachstum fast jeden Tick Chunks und ließ vier Ebenen neu
  bauen. Renderer ohne Speicheranforderungen je Frame: stabiles Zählsortieren der opaken
  Sprites nach Textur, Tiefenschlüssel transparenter Sprites einmal je Sprite berechnet
  (gleiche Reihenfolge wie vorher, Test), wiederverwendete Puffer (`FrameScratch`) und ein Vorrat
  für die Listen der `RenderList`. Gemessen (Release, Xvfb + lavapipe, 150 Frames, Median in ms,
  vorher → nachher; das Warten auf die Software-GPU, 85–190 ms, ist nicht aussagekräftig und
  nicht enthalten):

  | Szenario | Kartenabgleich + Minikarte | Render-Liste | Renderer vorbereiten | CPU je Frame ohne GPU |
  |---|---|---|---|---|
  | 5000 Bewohner, große Karte, Zoom 0,5 | 0,003 (p95 0,44) → 0,004 (p95 0,35) | 0,34 → 0,27 | 0,42 → 0,30 | 3,1 → 2,7 |
  | dieselbe Stadt, Zoom 1,0 | 0,002 (p95 0,47) → 0,003 (p95 0,31) | 0,30 → 0,21 | 0,21 → 0,17 | 2,6 → 2,0 |
  | Entwicklerstadt nach zwei Jahren | 0,002 → 0,001 | 0,07 → 0,07 | 0,18 → 0,16 | 1,4 → 1,6 (Rauschen) |
  | riesige Karte, Zoom 0,2 | 0,42 (p95 0,96) → 0,24 (p95 0,68) | 0,72 → 0,52 | 1,47 → 0,97 | 3,8 → 2,9 |

  Offene Reichsverwaltung (Entwicklerstadt): UI 1,0–2,8 ms je Frame (Bevölkerung am teuersten);
  die Tabellendaten waren schon zwischengespeichert, der Rest ist egui-Layout.
* **macOS (Apple Silicon), per Code-Prüfung:** Inhalte zuerst aus `Contents/Resources` des
  App-Bundles (auch wenn die App in einem Quellbaum liegt); ohne das Standard-Menü von winit,
  dessen „Beenden“ ohne Rückfrage beendete – Cmd+Q fragt jetzt wie das Fenster-X, Cmd+H und Cmd+M
  wie gewohnt; Cmd+S/L/P/D und ⌫ zusätzlich zu F5/F9/F12/F3/Entf (die F-Tasten brauchen fn), unter
  Cmd feuern Tasten ohne Cmd nicht (Cmd+S schiebt die Karte nicht mehr nach unten),
  Tastennamen „Ctrl“, „Option“, „Cmd“; Trackpad: Wischen verschiebt die Karte (Einstellung,
  Standard auf dem Mac), Zusammenziehen zoomt, Pixel-Scrollen in logischen Pixeln (auf Retina
  vorher doppelt so schnell); verdecktes Fenster (macOS behält die Größe) läuft mit 10 Frames/s
  statt Volllast; `engine.ron` wird vor dem Umbenennen auf die Platte geschrieben. Geprüft und in
  Ordnung: Benutzerverzeichnis `~/Library/Application Support/Kingdoms`, `sync_all` (auf macOS
  `F_FULLFSYNC`), Retina (Welt in Gerätepixeln, egui mit `pixels_per_point`, Screenshots in voller
  Auflösung), Metal über `Backends::PRIMARY`, Fifo/Immediate, sRGB-Surface mit linearer egui-View,
  CoreAudio über cpal (ohne Gerät stumm).
* **`tools/macos/make_app.sh`:** baut (`--target`, `--universal`, `--no-build`, `--out`) und legt
  `Kingdoms.app` an: Binary, `assets/` und `data/` in `Contents/Resources`, `Info.plist` (Bundle-ID,
  Version aus `Cargo.toml`, `NSHighResolutionCapable`, ab macOS 11, Kategorie Strategiespiele),
  `Kingdoms.icns` aus dem App-Symbol mit `sips` und `iconutil`, Ad-hoc-Signatur. README-Abschnitt
  „Auf dem Mac spielen (Apple Silicon)“.

**Geprüft**
* `cargo test --workspace`: Spiel 200, Assetgen 66, Engine 73, Sim 375 (+ 8 ignorierte),
  Kampagne 5, Determinismus 10, Fuzz 7, vernünftiger Spieler 2, Doctest 1 – grün. Neu u. a.:
  Werkzeug je Gebäude, Sammler-Schleifen (eigene Bilder je Werkzeug, Anker, Breite), App-Symbol,
  Gebäudeschau (Zellen, Betten, fertige besetzte Werkstätten), Bildreihenfolge des Renderers gleich
  dem früheren stabilen Sortieren, Pufferwiederverwendung, Messerfassung, Skript-Frames,
  feste Frame-Zeit, Bodensignaturen, Pixel-Scrollen und Zusammenziehen, Cmd-Regeln und
  Mac-Standardbelegung ohne Überschneidungen, Mac-Kürzel, Bundle-Suche, Benutzerverzeichnis.
* `cargo clippy --workspace --all-targets -- -D warnings` sauber.
* Atlanten zweimal erzeugt: byte-gleich (`diff -r`).
* Vorher/Nachher: alle vier Messszenarien mit `KINGDOMS_FIXED_DT=0.05` bei Frame 40 – 0 von
  1 440 000 Pixeln verschieden.
* Screenshots angesehen (Xvfb, lavapipe, 1600 × 900): Gebäudeschau Seed 7 und 331 (Übersicht
  Zoom 0,6), Mühle und Große Mühle, Schmiede, Meisterschmiede, Waffenschmiede, Eisenschmelze je in
  2–3 Frames nebeneinander (Flügel gedreht, Feuer und Funken wechseln), Holzfäller mit Axt (drei
  Bilder), Steinbrecherin mit Spitzhacke, Fischer mit Angel und Schwimmer, Kontaktbögen aller
  Gebäude, Figuren und Schleifen, App-Symbol, Einstellungen → Spiel mit der Trackpad-Option.
* `make_app.sh`: `bash -n`, Probelauf unter Linux (`--no-build`; Aufbau, `Info.plist` mit
  Python `plistlib` gelesen, Warnungen für fehlendes `sips`/`codesign`), Optionsfehler; das
  Binary aus `Kingdoms.app/Contents/MacOS` von `/` gestartet fand `Contents/Resources` und zeigte
  das Hauptmenü.

**Offen**
* Auf einem Mac weder gebaut noch gestartet (kein macOS-Ziel hier). Code nur für macOS
  (`with_default_menu(false)`, `hide_application`) ist hier nicht kompiliert; Trackpad-Wischen
  und Zusammenziehen sind nur per Unit-Test geprüft (X11 liefert beides nicht).
* Die GPU-Zeit ist mit lavapipe nicht messbar; auf echter Hardware nicht gemessen.
* Gebäudeschau: welche Minen und Gruben stehen, hängt vom Seed ab; an den ersten Tagen melden
  einige Werkstätten um 7 Uhr Rohstoffmangel, bis die Träger geliefert haben (Kunsthandwerker
  und Goldschmied auch am zweiten Tag – Verteilung der Simulation).
* Der Schwimmer der Angel liegt vor den Füßen in Blickrichtung; schaut der Fischer nicht zum
  Wasser (Blickrichtung kommt aus der Simulation), liegt er auf dem Ufer.
* Bergleute arbeiten im Gebäude (Vorkommen direkt am Stollen) und zeigen die Spitzhacke daher nie.
* Kein Fenstersymbol unter Linux/Windows (das App-Symbol wird nur für das Mac-Bundle verwendet).

## 2026-11-23 · Sim-Lane Schritt 7b: letzte Simulationspunkte, Grenzfälle und Balance (`crates/sim`, `data/`, Doku)

**Fertig**
* **Kampfbild mit Hieben** (§11): `CombatRound::strikes()` hält jeden Nahkampfhieb mit Angreifer
  und Ziel (`StrikeTarget::Walker` oder `Building { building, tile }` – Schläge gegen Tore,
  Mauern und Gebäude zählen jetzt auch als Schlag), `target_of(walker)`, `Shot::shooter`. Wer in
  der aktuellen Runde angreift, schaut in `CitizenView::facing` zu seinem Ziel. Weiterhin reine
  Darstellungsdaten (nicht gespeichert, nicht im Vergleich, von den Regeln nie gelesen).
* **Gemessener Ertrag** (§7 Wirtschaftlichkeit): jedes Produktionsgebäude zählt Arbeitsgänge und
  Waren je Tag (`ProductionState::output_log`, heute und 24 abgeschlossene Tage, gespeichert und
  geprüft; Feldfarmen zählen jedes abgeerntete Feld). `report::building_economics` nennt
  „gemessen“ gegen „möglich“ (`MeasuredOutput`, z. B. „Gestern 5 Arbeitsgänge (10 Einheiten
  Bretter), heute bisher 2 – möglich sind 6,0 je Tag.“) und liefert jetzt auch für Feldfarmen
  und Obstgärten Zahlen: Ernte je Tag im Mittel der gespeicherten Tage, Wert, Saldo und
  Herstellkosten (`EconomicsBasis::Harvests`) statt „–“.
* **Proviant** (§11): Ein Feldzug isst seinen Proviant stündlich je Soldat auf; was beim
  Heimkehren übrig ist (Rückzug, Frieden, Gefallene), kommt ins Lager zurück („Vom Proviant kommen
  12 Einheiten Nahrung zurück ins Lager.“). Die tägliche Kriegsration wird um Mitternacht für den
  vergangenen Tag gegessen – nichts bleibt übrig (in §11 so beschrieben).
* **Hauptquartier abgerissen, während die Kompanie fort ist:** Die unsichtbar wartenden Soldaten
  ziehen ins verbleibende Hauptquartier um (vorher standen sie danach im Spielstand in einem
  fehlenden Gebäude bzw. ohne Gebäude).
* **Eingemauertes Heer nach dem Frieden:** wartet `armies.withdraw_stuck_hours` (4) Stunden auf
  einen Weg und löst sich dann mit Meldung auf („Das Heer von … findet keinen Weg aus Euren
  Mauern und löst sich auf: …“); Soldaten und Beute kehren heim.
* **Arbeitszuteilung schneller, Ergebnis gleich:** weiterhin vollständige Neuberechnung, aber
  ohne die Hauptquartier-Suche je Bewohner, mit Index statt Map, ohne Suche bei keiner freien
  Stelle und mit den Arbeitslosen nach Haus gruppiert (Auswahl der k Nächsten statt k-mal des
  Minimums). Ein Differenztest gegen die alte Implementierung (Kopie als
  `workforce_reference.rs`, nur in Tests) über 24 Zufallsstädte × 30 Änderungen ist
  Byte-gleich. Release-Messung in der Messstadt (5001 Bewohner): nichts geändert 178 → 78 µs,
  ein Betrieb wechselt 233 → 171 µs, zwanzig wechseln 601 → 233 µs (max 3,8 → 0,3 ms), alle
  suchen neu 9,6 → 1,3 ms.
* **Wachstum der Seeds 7/11 (Ursache in den Regeln):** Arbeiteten alle Erwachsenen, bekam keine
  Baustelle je einen Bauarbeiter (Bau steht in der Kategorie-Reihenfolge nach Nahrung, Rohstoffen
  und Verarbeitung) – das Haus für die Zuwanderer wurde nie fertig, die Stadt blieb bei 41–43
  Bewohnern stehen. Neue **Grundbesetzung**: Die erste Baustelle mit Material (Gebäude-Priorität,
  dann ID) bekommt vor allen Prioritäten einen Bauarbeiter (§6 Arbeit). Kampagne jetzt Seed 7 →
  80, 11 → 95, 23 → 84, 31 → 99, 42 → 101 Bewohner (vorher 47, 44, 84, 89, 89).
* **Ränge** (§1): Das Gebiet war die harte Hürde (das Rathaus beansprucht ≈ 1 264 Felder, die
  Stadt verlangte 2 400 – ein Dutzend Türme). Neu: Stadt 1 600, Großstadt 2 400, Königreich
  3 500, Imperium 5 000 Felder (`rules.ron` und §1). Neuer **ehrgeiziger Spieler** in
  `tests/campaign.rs` (zwei Häuser zugleich bei Platz und Vorrat, Bau-Priorität hoch, solange
  Baustellen ohne Bauarbeiter warten, Kirchenordnung und Kirche – dafür 15 % Steuer –,
  Wachtürme an der Grenze in Reichweite der Träger, Verkauf von Überschüssen an anfragende
  Reiche): Stadt auf Seed 7 (Tag 69) und 23 (Tag 67) als Tests, in der Übersicht
  (`--ignored`) auch Seed 31 (Tag 69); Seeds 11/42 bleiben mit 87/91 Bewohnern Marktflecken.
  Neuer Test: auf jeder kleinen Kampagnenkarte gibt es über 12 000 Felder freies Land (mehr als
  doppelt so viel wie das Imperium braucht).
* **Räuber:** Die Kompanie des Kampagnen-Spielers verteidigt jetzt ein Gebiet über Hauptquartier
  und allen Lagern und stellt sich an das Ziel einer anrückenden Bande; alle Überfälle nach dem
  Bau der Kaserne werden abgewehrt. Die verbliebenen Niederlagen sind die Überfälle des ersten
  Winters (Tag 20–22), bevor es Soldaten gibt. Türme setzt das Skript nur noch, wo Baumaterial
  hinkommt (vorher blieben Turm-Baustellen bei Speichern ohne Träger ewig liegen).
* **Brennholz:** Entwicklerstadt 2 (Spiel-Test gegen diese Simulation): nach zwei Jahren 88
  Bewohner (vorher 38–48), Holz im zweiten Jahr nie leer (Minimum 6 zu Frühlingsbeginn, im Herbst
  236–307), niemand erfroren. Der vernünftige Spieler kommt mit Wärme 100 durch den Winter (Holz
  am ersten Frühlingstag aufgebraucht). Keine Regel- oder Planänderung nötig.
* **Fristen und Deutsch:** `PendingEventReport::deadline_text` nur noch mit Uhrzeit und
  Standardwahl („Entscheidung bis morgen 06:00 Uhr, sonst gilt „Kein Interesse“.“; Restzeit in
  `minutes_left`), dieselbe Uhrzeit (`report::deadline_clock_text`: „18:00 Uhr“, „morgen …“,
  „übermorgen …“, „Tag 5, …“) in der Ereignis-Meldung und bei KI-Anfragen. Einzahl/Mehrzahl und
  Fälle: „Es fehlt 1 Gold“, „Es fehlt 1 Einheit Waffen“, „1 Feld“/„im Umkreis von 1 Feld“, „hat
  nur noch 1 freien Ausbildungsplatz“, „Es steht nur 1 von 2 Erwachsenen bereit“, „1 Holz ging
  verloren“, „für etwa 1 Tag“; Gebäudenamen mit Artikel („Das Gebäude „Kaserne“ ist noch nicht
  bereit.“, „… entsteht nur durch Ausbau“, „… muss auf einen Mauerabschnitt“), „Milizen werden
  nicht hier ausgebildet, sondern im Gebäude „Kaserne““.
* **Spielstand-Format 14** (Ertragsbuch, Proviant, Wartezähler eingemauerter Heere; Format 13
  wird abgelehnt); Kommentar der Formatgeschichte um 12–14 ergänzt.

**Geprüft**
* `cargo test -p kingdoms_sim`: Sim 383 (+ 9 ignorierte Messungen), Kampagne 8 (+ 1 ignorierte
  Übersicht), Determinismus 10, Fuzz 7, vernünftiger Spieler 2, Doctest 1 – grün. Neu u. a.:
  Hiebe gegen das Tor mit Blickrichtung, Hiebpaare und Ziele in der Kampfrunde, Ertragsbuch
  (Zählung, Tagesabschluss, Prüfung), gemessen/möglich beim Sägewerk (Summe = `cycles_completed`),
  Ernte-Wirtschaftlichkeit der Gemüsefarm, Proviant zurück nach Rückzug (mit
  Spielstand-Rundreise), Soldaten ziehen ins zweite Hauptquartier um (ohne Umzug schlägt der Test
  fehl – geprüft), eingemauertes Heer löst sich auf, Differenztest der Arbeitszuteilung,
  deutsche Fristen/Mengen/Gründe, Ertragsbuch im Determinismus-Test (gefüllt und nach dem Laden
  gleich).
* `cargo clippy -p kingdoms_sim --all-targets -- -D warnings` sauber;
  `RUSTDOCFLAGS='-D warnings' cargo doc -p kingdoms_sim --no-deps` sauber.
* Release: `allocation_performance` (Werte oben, neu als ignorierte Messung in
  `scale_tests.rs`); Kampagne über drei Jahre ohne Niederlage, Zufriedenheit 77–83, Gefallene
  gegen Räuber höchstens 4 je Seed, Rang Marktflecken auf allen fünf Seeds (der vernünftige
  Spieler baut keine Kirche); ehrgeiziger Spieler wie oben; vernünftiger Spieler nach einem Jahr
  44 Bewohner, Zufriedenheit 76, naiver Spieler 14 Bewohner, Sättigung 100.
* Spiel gegen diese Simulation: Kopie des Haupt-Workspace mit `crates/sim` und `data/` der Lane,
  `cargo check --workspace --all-targets` ohne Fehler und Warnungen, `cargo test -p kingdoms`
  198 Tests grün (darunter `town_grows_through_two_years`, siehe Brennholz).

**Offen**
* Darstellung im Spiel (nächste Spiel-Etappe): Hiebe für Blickrichtung und Funken
  (`CombatRound::strikes`, die Blickrichtung kommt schon über `CitizenView::facing`),
  „gemessen/möglich“ und Ernte-Zahlen der Feldfarmen im Info-Fenster (die Feldfarm zeigt jetzt
  Wirtschaftlichkeit mit `basis: Harvests`; der Tooltip „mit der zugewiesenen Belegschaft“ passt
  dort nicht), Proviant eines Feldzugs.
* Der ehrgeizige Spieler erreicht die Stadt nur auf drei von fünf Seeds; Gold bleibt der Engpass
  (Unterhalt ≈ Steuern bei 80–100 Bewohnern ohne Warenhandel). Großstadt bis Imperium sind nur
  rechnerisch (freies Land) und nicht in einem simulierten langen Spiel geprüft.
* Die Überfälle des ersten Winters treffen eine Stadt ohne Soldaten (das Skript forscht die
  Bürgerwehr erst ab 30 Bewohnern).

## 2026-11-23 · Spiel-Schritt 7a: Sim-Schritt-7-Daten, Audio-Lieferung, UI-Feinschliff (`src/`, `crates/engine`, `assets/audio`, Doku)

**Fertig**
* **Ausbauen für jedes Gebäude** mit `BuildingDef::upgrade` im Info-Fenster (`upgrade_section`):
  Ziel mit Beschreibung, bei Häusern die Voraussetzungen, Kosten, Bauzeit, was sich ändert
  (`upgrade_changes`: Arbeiter, Puffer, Lagerplatz, Trägerreichweite, Besatzung, Fernkampf,
  Trefferpunkte, Dienste, Unterhalt, Wohnraum samt Forschungsbonus), Knopf „Ausbauen“
  (`UpgradeBuilding`) mit allen `upgrade_blockers` als Grund und dem ersten darunter; die
  Ausbau-Sperre nur bei Häusern. Baustellentitel „Ausbau: Großes Sägewerk“.
* **Bürgerfenster:** Freunde als Links (Freund/Freundin/Freunde/Freundinnen; ebenso
  Ehemann/Ehefrau, Sohn/Tochter/Kinder nach Geschlecht), Ersparnisse des Haushalts mit
  Wohlstand, „krank“ an der Gesundheit; Tätigkeiten Krank, Im Krankenhaus, In Sicherheit, Feiert
  und die Wege zu Schutz bzw. Krankenhaus farbig mit Erklärung.
* **Haus-Fenster:** je Haushalt Mitglieder, Ersparnisse, Wohlstand (Einnahmen/Ausgaben von gestern
  und Unzufriedenheitstage im Tooltip) und die Summe von gestern. **Gasthaus:** Abschnitt
  „Ausschank“ aus `report::shop_report` (Bier 0/12, „Kein Bier vorrätig“).
* **Reiter Bevölkerung:** „Wohlstand der Haushalte“ (Anteile Arm/Auskömmlich/Wohlhabend als
  Balken, Ersparnisse gesamt/Ø/Median, gestern verdient/ausgegeben, Faktor „Wohlstand“, Grenzen
  aus `WealthRules`) und „Gasthäuser“ (geöffnet, mit/ohne Bier).
* **Temperatur:** Landwirtschaft & Wetter zeigt „−8 °C · Häuser heizen · Frost“ im Kopf, die
  obere Leiste die Tagestemperatur neben der Jahreszeit (bei Frost frostblau), der Tooltip Wetter,
  Heizen und Frost.
* **Handel:** `trade_road_text` im Zustand des Handels; die Weltmarktpreise der Liste rechnen den
  Handelsstraßen-Bonus jetzt wie die Aufträge ein; jede Karawane zeigt den Straßenanteil ihres
  Wegs („Handelsstraße +8 %“, `RouteTally::bonus_percent`).
* **Festtag:** in der Übersicht mit Restdauer, als „Festtag“ in der oberen Leiste; Festgäste, die
  die Simulation im Festort führt, stehen als Menge vor dessen Eingang (`festival_spot`: je Figur
  ein eigener Punkt auf freiem Boden, zum Gebäude gewandt, jede zweite tanzt; anklickbar).
* **Spielstände nach Spielkennung:** `slots::group_by_game` (Spiel mit dem neuesten Stand zuerst,
  je Spiel neueste zuerst, nicht lesbare zuletzt), Überschriften bei mehreren Spielen;
  „Fortsetzen“ unverändert. Nur nicht ladbare Stände: „Es gibt 2 Spielstände, aber keiner kann
  mit dieser Version geladen werden.“ bzw. im Hauptmenü „2 Spielstände vorhanden, aber keiner
  ladbar“. Pausenmenü eines nie gespeicherten Spiels: „Noch nicht gespeichert.“
* **Neue Ausbaustufen überall:** Werkstattklänge für Großes Sägewerk, Meisterschmiede, Große
  Mühle, Großbäckerei (Test: Ausbaustufen klingen wie ihre Grundstufe), Banner auch am Wehrturm
  der KI-Reiche; Info-Fenster, Produktion, Statistik, Minikarte, Lichter, Rauchpunkte und
  Arbeitsschleifen waren bereits datengetrieben (im Code und in den Atlanten nachgesehen).
* **Audio-Lieferung:** die Engine findet jede Datei über ihren Stamm mit `.ogg`, `.mp3`, `.wav`
  oder `.flac` (`audio::files`, einmal beim Start; Summe im Log, Einzelheiten mit
  `KINGDOMS_DEBUG=1`); Musik blendet 2,5 s vor Stückende ins nächste Stück über und bei
  Stimmungswechseln echt über (vorher erst Ausblenden, dann Einblenden); Stimmen senken Musik
  und Umgebung ab; die Stimmung „Blühend“ (Festtag oder zufriedene Stadt ab 50 Bewohnern) wird
  jetzt gewählt – vorher blieben drei Stücke ungespielt; fehlende Musik wird nicht mehr in jedem
  Frame neu gesucht. Lieferbericht `KINGDOMS_AUDIO_REPORT=1`/`--audio-report` (Kanal, erwartete
  und gefundene Datei, Länge, Format, Spitze, Hinweise zu Lücken in Schleifen, MP3-Schleifen,
  Übersteuerung und Länge, Sprechertexte; Rückgabewert). Manifest: Stimmen mit Feld `text`
  (genauer Wortlaut je Datei) und englischer Vortragsangabe; `assets/audio/README.md` als
  deutsche Lieferanleitung mit allen Dateinamen und Sprechertexten; Ordner `ui/` angelegt.
* **Feinschliff:** eine deutsche Dauerform überall („45 min“, „1 h 35 min“, „14 h“, „3 Tage“ –
  `format::duration_minutes`/`duration_hours`, wie die Texte der Simulation; Ereignis- und
  Anfrage-Fristen, Forschung, Abstieg, Arbeitswege, „Punkte/h“); Militärbanner und
  Bedrohungsmarken bleiben in der freien Kartenfläche links der Meldungskarten (`free_area`,
  `marker_area`, Banner bricht um); „Lager reicht ohne Erzeugung 1 Tag“ statt „1 Tage“.

**Geprüft**
* `cargo test --workspace`: Spiel 194, Assetgen 64, Engine 64, Sim 375 (+ 8 ignorierte
  Messungen), Kampagne 5, Determinismus 10, Fuzz 7, vernünftiger Spieler 2, Doctest 1 – grün. Neu
  u. a.: Ausbau-Änderungen aller Ketten, Haushaltstexte, Verwandtenbezeichnungen,
  Tätigkeitshinweise, Dauerformen (gleich `report::minutes_text`), Tage mit Komma, Gruppierung
  der Spielstände samt „Fortsetzen“, nicht ladbare Stände, Speicherstand-Text, freie
  Kartenfläche für Banner und Marken, Festgäste-Spirale, Stimmung „Blühend“, Klänge der
  Ausbaustufen, Manifest (Wortlaut jeder Stimme, Ordner je Kanal); Engine: Stamm-Vergleich,
  Dateisuche nach Endung und Schreibweise, Dekodieren einer erzeugten WAV (Länge, Spitze, Stille),
  Liefertabelle, Absenken, Auflösung im `Audio`.
* `cargo clippy --workspace --all-targets -- -D warnings` sauber.
* `KINGDOMS_AUDIO_REPORT=1`: 140 IDs, 143 Dateien, Rückgabewert 1 ohne Lieferung; mit zwei
  erzeugten WAV-Dateien „music/Menu Theme.wav“ und „ambience/rain.wav“ (mit 0,4 s Stille) als
  gefunden erkannt, mit Länge, Format, Spitze und den Hinweisen „kurz für ein Musikstück“,
  „übersteuert“, „Stille am Anfang 0,4 s – Schleife mit Lücke“.
* Screenshots angesehen (Xvfb, lavapipe; 1600 × 900 und 1280 × 720): Haus-Fenster mit Haushalten
  und Ausbau „Bauernhaus“ (gescrollt, Hinderungsgrund Lehm), Bürgerfenster mit Freund und
  Ersparnissen, Vorratsspeicher mit „Ausbau: Lagerhaus“ (1280), Reiter Bevölkerung mit
  Wohlstand und Gasthäusern (1280), Winter mit −8 °C in Leiste und Wetterkarte (1280),
  Handel mit Straßentext und Karawanen, Festtag in Leiste und Übersicht und die Festmenge am
  Markt (Entwicklerstadt, `KINGDOMS_DEV_EVENT=religious_festival:0`), Festtag-Leiste bei 1280,
  Gasthaus „Kein Bier vorrätig“ (per XTest gebaut), Ladeliste mit zwei Spielen und einer
  kaputten Datei, Hauptmenü und Ladeliste nur mit kaputten Ständen, Pausenmenü eines neuen
  Spiels, Patrouillenbanner mit Bedrohungsmarke neben den Meldungskarten (1280), Produktion mit
  Großen Sägewerken.

**Offen**
* Kein Verlauf des Wohlstands: die Simulation zeichnet je Tag keine Ersparnisse auf (Anfrage an
  die Sim-Lane); `WealthSummary`, `WealthClassRow` und `TavernSummary` sind nicht aus `report`
  exportiert (das Spiel liest sie nur als Felder).
* Restzeiten der Ereignis-Wirkungen kommen als „noch 22 Stunden“ aus der Simulation, alle anderen
  Dauern als „h/min“ (Anfrage an die Sim-Lane).
* Audio nie gehört (kein Gerät, keine Dateien): Überblendung, Absenken und Schleifen nur per Test
  und Code geprüft. MP3-Schleifen können je nach Encoder eine Lücke haben (Bericht weist darauf
  hin). Lautheit wird als Spitze, nicht als LUFS gemessen.
* Festmenge nur am Markt gesehen (die Entwicklerstadt baut keine Kirche); ein Gasthaus mit Bier
  im Ausschank nicht gesehen (keine Brauerei im Testaufbau).
* „Ausbau zum Großen Sägewerk“ bräuchte Genus/Deklination in den Daten; neutral „Ausbau: …“.

## 2026-11-23 · Spiel-Schritt 7: Lücken der Vollständigkeitsprüfung (`src/`, `crates/engine`, `tools/assetgen`, `assets/`, kleine Ergänzungen in `crates/sim`, Doku)

**Fertig**
* **Wind** (Master §16, game_design §8.2): `WeatherLook` liest jetzt den Wind der Simulation
  (`wind_vector_permille` auf den Bildschirm projiziert, `wind_strength`; `ScreenWind`): Stärke
  bestimmt Schwanken, Neigung und Drift, die Richtung, wohin sich Bäume neigen, wohin Böen drücken
  (`Environment::wind_dir`, neuer Uniform-Vektor) und wohin Rauch, Laub, Regen und Schnee treiben –
  derselbe Wind, der Brände weiterträgt. Regen fällt schräg mit dem Wind (gespiegelter Streifen bei
  Wind nach rechts). **Felder und Obstgärten** wiegen sich: `FLAG_WIND` auf Boden-Sprites schert im
  Fragment-Shader nur die Pixel über der Tile-Raute nach ihrer Höhe (die Raute bleibt stehen),
  Phase je Tile. Der Wetterreiter zeigt „Wind: aus West, mäßig“.
* **Winter in Obstgärten** (§8.3): neues Sprite `fields/orchard_bare` (Bäume der Stufe 2 ohne Laub
  und Früchte, angeschneit), die Feldebene nutzt `FieldView::seasonal_sprite`; dazu
  `buildings/orchard_winter` (Obstgarten-Gebäude mit kahlen Bäumen und leeren Körben), das das Spiel
  im Winter zeigt.
* **Schornsteinrauch nach der Heizung** (§16): Häuser rauchen nur, wenn ihr Ofen brennt
  (`Population::heating(house, needs_heating)`: Bewohner, kalter Tag, Brennstoff); ein Haus ohne
  Holz bleibt kalt und rauchlos, andere Gebäude rauchen an kalten Tagen statt fest im Winter. Das
  Info-Fenster eines Hauses nennt „Heizung: Der Ofen brennt / Kein Brennstoff – die Bewohner
  frieren / Heizen nicht nötig“ mit Tagestemperatur im Tooltip.
* **Weidetiere nach der Herde** der Simulation (`Farm::herd`) statt nach der Weidefläche; der
  falsche Doku-Kommentar ist korrigiert.
* **Animationen** (Master §3.2, §16): neue Atlas-Schleifen `<rolle>_<richtung>_work_<0..3>`
  (Bauarbeiter Hammer, Bauern Hacke, Männer/Frauen Axt) und `_attack_<0..3>` (Speerstoß, Bogen
  spannen, Schwerthieb, Räuber-Axt, Katapultwurf), 32 × 24 mit gleicher Figur und Anker;
  Gebäudeschleifen `<id>_work_<0..3>` für Mühle und Große Mühle (Flügel drehen sich), Schmiede,
  Meisterschmiede und Schmelze (flackerndes Feuer, die Esse der Schmiede steht dafür vorn unter dem
  Schleppdach). Das Spiel spielt alles über `AssetManager::animation`/`animation_data` (auch die
  Laufbilder; neu `AssetManager::try_animation` ohne Warnung) in Spielzeit, je Läufer/Gebäude
  versetzt. Wer arbeitet oder kämpft, sagt die Simulation: neu `CitizenView::action`
  (`WalkerAction::{None, Work, Attack}`).
* **Bodenschatten** (§3.2): `overlay/blob_shadow` unter Läufern, Weidetieren, Bäumen und Büschen als
  transparente Boden-Decals (`SHADOW_BIAS`), leicht nach rechts versetzt, Deckkraft nach Tageslicht
  und Wetterlicht (`shadow_alpha`).
* **Geschosse** (§11/§16): die Simulation hält die letzte Kampfrunde als reine Darstellungsdaten
  (`Military::last_round()` → `CombatRound` mit `Shot { from, to, kind, player }` und den
  Angreifern; nicht gespeichert, nicht im Zustandsvergleich, von der Simulation nie gelesen). Das
  Spiel zeichnet Pfeile (`effects/arrow_<0..15>`, Bild nach Flugrichtung) und Steine
  (`effects/stone`) auf einem Bogen vom Schützen oder der Turmspitze zum Ziel (`view/projectiles`).
* **Klänge:** alle 125 Manifest-IDs werden benutzt (neuer Test `every_manifest_id_is_used`):
  `amb_market` (Marktgedränge bei Tag nahe besetzter Märkte), `sfx_cart` (nächster fahrender
  Karawanenkarren), `ui_hover` (HUD-Knöpfe), `vo_citizen_yes` (Bürger auf der Karte angeklickt),
  `vo_food_shortage` (Werkstatt ohne Nahrungsmittel; Nahrung unter 3 Tagen) und `vo_low_happiness`
  (unzufriedene Bewohner, Mittel unter 50) – der Berater spricht beim Eintreten und täglich wieder.
* **Forschung** ohne eigene Spielregel im UI: Vorschau aus `research::queue_plan` der Simulation,
  Einreihen als ein `QueueResearchWithPrerequisites` (alles oder nichts); die UI-Kopie ist entfernt.
* **Reichsverwaltung:** zwei Reiterzeilen mit Gruppen „Wirtschaft“ (Lager, Produktion, Wirtschaft,
  Finanzen, Handel), „Volk“ (Arbeit, Bevölkerung, Landwirtschaft), „Reich“ (Forschung, Diplomatie,
  Militär, Statistik) – jeder Reiter mit Namen, auch bei 1280 × 720.
* **Handelsbilanz und Produktionskosten** (§6.3): neu `report::trade_balance` (Aus-/Einfuhr je Tag,
  Mittel der letzten 7 Tage, Satz „Handelsüberschuss/-defizit“) und `report::building_economics`
  (Arbeitsgänge je Tag mit der zugewiesenen Belegschaft, Wert der Erzeugnisse, Rohstoffe und
  Werkzeugverschleiß, Löhne, Unterhalt, Saldo, Herstellkosten je Einheit gegen den Marktpreis;
  Löhne und Unterhaltsnachlass teilen sich die Funktionen mit `finance::daily_rates`). Anzeige als
  Karte im Reiter Handel (mit Verlauf), Kennzahl und Spalte „Gold/Tag“ je Produktionskette im
  Reiter Wirtschaft und „Wirtschaftlichkeit“ im Info-Fenster jedes Betriebs.
* **Doku und Struktur:** architecture.md-Baum (Tests in `crates/sim/tests`, Unit-Tests je Crate),
  `tests/README.md`, leere Grafikordner unter `assets/` entfernt und `assets/README.md` mit der
  Atlas-Aufteilung, zwei veraltete Roadmap-Zeilen (Forschungswirkungen, Militär-Kennzeichen)
  berichtigt; assets.md, README, architecture.md ergänzt.
* Nebenbei: „reicht noch etwa 1 Tage“ im Reiter Wirtschaft → „1 Tag“.

**Geprüft**
* `cargo test --workspace`: Spiel 179, Assetgen 64, Engine 57, Sim 375 (+ 8 ignorierte Messungen),
  Kampagne 5, Determinismus 10, Fuzz 7, vernünftiger Spieler 2, Doctest 1 – grün. Neu u. a.:
  Kampfrunde mit Turmpfeilen und angezeigten Angreifern (Datensatz nicht im Zustandsvergleich),
  Handelsbilanz, Wirtschaftlichkeit (unabhängig von der Uhrzeit, still gelegt nur Kosten), Wind
  aus der Simulation (Richtung, Stärke, Projektion), Rauch nach Heizung, Herdengröße, kahle
  Obstbäume samt Sprite, Schattenlage und -tiefe, Pfeilrichtung und Flugbahn, Arbeits-/Kampf- und
  Gebäudeschleifen im Generator, Winterbild, Berater-Wiederholung, Windtext, Manifest vollständig
  benutzt, Reitergruppen vollständig. Atlanten neu erzeugt: geändert nur buildings, characters,
  effects, fields und icons (Symbole aus den Gebäudebildern); ein zweiter Lauf ist bytegleich.
* `cargo clippy --workspace --all-targets -- -D warnings` sauber;
  `RUSTDOCFLAGS='-D warnings' cargo doc -p kingdoms_sim --no-deps` (und Engine) sauber.
* Screenshots angesehen (Xvfb, lavapipe): Sturm am Obstgarten (Regen schräg mit dem Wind, Wind im
  Wetterreiter), zwei Bilder bei klarem Wetter mit Pixelvergleich (nur die Gemüsepflanzen bewegen
  sich, Boden und Gras nicht), Winter Jahr 2 mittags (kahle Obstbäume auf den Feldern, Obstgarten
  im Winterbild, Rauch nur aus beheizten Häusern, „Heizung: Der Ofen brennt“ im Haus-Fenster),
  Bodenschatten mit/ohne im Vergleich, Bauarbeiter mit erhobenem Hammer, Armbrustschützen beim
  Spannen und fliegende Pfeile gegen eine Räuberbande (`KINGDOMS_DEV_ARMY` + `DEV_THREAT`, kleine
  Karte), Reiter Wirtschaft und Handel (auch 1280 × 720 mit `engine.ron`), Wirtschaftlichkeit im
  Info-Fenster einer Holzfällerhütte; Mühlen- und Schmiedeschleifen als Atlas-Ausschnitt.

**Offen**
* Gebäudeschleifen von Mühle und Schmiede wurden nur im Atlas angesehen, nicht laufend auf der
  Karte (die Entwicklerstadt baut keine Mühle oder Schmiede). Weidetiere nach der Herde und
  `amb_market`/`sfx_cart`/Beraterstimmen sind nur per Test und Code geprüft (keine Audiodateien,
  kein Gerät im Container).
* Kampfschleifen zeigen die Blickrichtung des letzten Schritts; ein Nahkämpfer schlägt nicht
  unbedingt zum Gegner hin. Feinde zeigen die Schleife nur, wenn sie in der Runde zuschlagen
  (Gebäude beschädigen zählt nicht als Schlag).
* Frauen und Männer als Sammler schwingen dieselbe Axt (Holzfäller, Steinbruch, Lehm, Fischer).
* `building_economics` rechnet mit der zugewiesenen Belegschaft und Rezeptdauer rund um die Uhr
  – fehlende Rohstoffe oder Bäume in Reichweite senken den echten Ertrag; Felder und Weiden haben
  keine Rezepte und zeigen „–“.
* Die Darstellung der Sim-Schritt-7-Daten (Ausbauen für alle Gebäude, Wohlstand, Freunde,
  Gasthaus-Bestand, Temperatur im Jahreszeit-Tooltip, Handelsstraßen-Text, Spielstände nach
  Kennung) war nicht Teil dieser Lückenliste und bleibt offen (Roadmap).

## 2026-11-23 · Sim-Schritt 7: Lücken der Vollständigkeitsprüfung (`crates/sim`, `data/`, `tools/assetgen`, Anpassungen in `src/`, Doku)

**Fertig**
* **Ausbau aller Gebäudearten** (Master §5.1 „Umbau, Gebäude-Upgrades“, §5.3 „Upgrade-Stufen“):
  `upgrade: (to, cost, hours)` steht jetzt am Gebäudetyp (`BuildingDef::upgrade`, ersetzt
  `HousingDef::upgrade_to`; Kosten/Stunden ohne Angabe = Baukosten/Bauzeit des Ziels).
  `UpgradeBuilding` und `upgrade_blockers` gelten für jedes Gebäude (Forschung, Höchstzahl wie bei
  der Burg, Platz, Gold, Material; Häuser zusätzlich ihre Wohnstufe; neue Gründe `NoUpgrade`,
  `MaxCountReached`). Das Gebäude behält Id, Lagerbestand, Puffer, Rezept, Aufträge und gezählte
  Arbeitsgänge; während des Ausbaus halten Bauarbeiter die Stellen (`Building::trade_staff`):
  Werkstätten „Wird ausgebaut“, Dienste, Träger, Ausbilder und Schulforschung pausieren. Die
  Datenprüfung lässt nur verträgliche Ziele zu (Rolle, Rezepte zuerst gleich, Lager bleibt Lager,
  keine Farmen/Mauern/Tore, keine kleinere Grundfläche, keine Zyklen); ein geladener Ausbau muss
  zum Folgetyp passen. Ketten: Vorratsspeicher → Lagerhaus (50 G), Rathaus → Burg (1500 G,
  Festungsbau), Wachturm → **Wehrturm** (neu, Stadtmauern), Sägewerk/Schmiede/Mühle/Bäckerei →
  **Großes Sägewerk, Meisterschmiede, Große Mühle, Großbäckerei** (neu, Zunftwesen: mehr Arbeiter,
  Puffer 16, 50–65 % mehr Leistung). Neue Sprites, Symbole und Nachtlichter in `tools/assetgen`.
  „Umbau“ verstehen wir als diesen Ausbau; Versetzen und Rezeptwechsel gab es schon.
* **Handelsstraßen wirken** (§5.2): Karawanen zählen auf ihrem Weg über die Karte die Tiles und
  den Handelsbonus der Straße darunter (`Caravan::route`); fremde Händler handeln am Kontor, eigene
  Karawanen beim Partner mit diesem Anteil als zusätzlichem Preisvorteil (nur Handelsstraße:
  +15 %). Ohne Königreiche zählt der kürzeste Straßenweg vom Kontor zum Kartenrand
  (`trade_route_percent`); der Handelsbericht nennt den Bonus.
* **Gasthaus mit Bierausschank** (§5.4): Gebäude mit `sells` halten 12 Einheiten Vorrat
  (`shop_stock`), Träger füllen ihn auf; Gäste kaufen beim abendlichen Besuch Bier für den
  Haushaltsvorrat (zählt auch für das Stadthaus). Unterhaltung 100 nur mit Bier, sonst 60 und halber
  Gemeinschaftsbonus. `report::shop_report` („Kein Bier vorrätig“), Zähler im Bevölkerungsbericht.
* **Wohlstand** (§7.1): Geldbörse je Haushalt (`Purse`): Einkommen je arbeitendem Mitglied nach
  Wohnstufe (`income_per_worker`) plus Lohn bzw. Sold, Alte 50 %, Arbeitslose 25 %; Kopfsteuer und
  Marktgebühr kommen aus der Ersparnis (die Staatskasse bucht genau das Gezahlte), jede Einheit aus
  Lager, Markt oder Gasthaus kostet den Marktpreis (Nahrung/Brennstoff auch ohne Geld, Waren nur mit
  Geld). Arm (−15 Zufriedenheit, Faktor „Wohlstand“) bzw. wohlhabend (+4); Anteile wandern bei
  Heirat und Haushaltsteilung mit. Berichte je Haushalt, Bewohner und als Verteilung.
* **Tagesablauf nach Ereignissen** (§7.3): Festtage (neue Wirkung `Festival(days)` im Ereignis
  „Religiöses Fest“: nachmittags Feier bei der Kirche/dem Markt statt Arbeit), Kranke (Gesundheit
  < 40) bleiben daheim oder gehen ins Krankenhaus (Erholung doppelt so schnell), Zivilisten im
  Alarmradius von Feinden fliehen nach Hause bzw. ins Hauptquartier. Neue Tätigkeiten „Feiert“,
  „Krank zu Hause“, „Im Krankenhaus“, „In Sicherheit“.
* **Freundschaften** (§7.1 Beziehungen): bis zu 3 gegenseitige Freunde, die sich stündlich am
  selben Ort (Arbeit, Schule, Gasthaus/Kirche, Haus) per Hash finden; +10 Gemeinschaft je Freund;
  enden mit Tod/Abwanderung; `CitizenDetail::friends`.
* **Temperatur** (§8): Tagestemperatur in °C aus Jahreszeit, Wetter, Tagesabweichung (Hash) und
  Klima des Startgebiets; unter 5 °C wird geheizt (Wärme, Brennstoff, Erfrieren – nicht mehr fest an
  den Winter gebunden), ab 0 °C Frost auf unreifer Saat. Wetterbericht mit „Temperatur: −3 °C“.
* **Deutsch:** `report::since_days_text` („seit heute“, „seit 1 Tag“, „seit 3 Tagen“) im
  Diplomatie- und Militärbericht; Gebäudenamen in Sätzen mit Artikel („Das Gebäude „Sägewerk“ ist
  abgebrannt …“, „Rohstoffmangel: Dem Gebäude „Sägewerk“ fehlt Holz.“, „Kein Weg zur Arbeit: Das
  Gebäude „…“ ist …“, „Ausbau begonnen: Aus „…“ wird „…“.“).
* **Spielkennung (uuid):** jedes Spiel hat eine deterministische UUID v4 aus einem eigenen
  ChaCha-Strom der Einstellungen (`GameState::game_id`, `SaveHeader::game_id`, beim Laden geprüft);
  der Haupt-Zufallsgenerator bleibt unberührt. Credits-Zeile angepasst.
* **Spielstand-Format 13** (Format 12 wird abgelehnt) mit Prüfungen für Geldbörsen, Freunde,
  Ausbauziel, Karawanen-Weg und Spielkennung.
* `src/` nur angepasst: Info-Fenster liest `BuildingDef::upgrade`, neuer Status „Wird ausgebaut“
  neutral gefärbt, `ModifierKind::Festival` in der Übersicht, Credits; Entwicklerschalter
  `KINGDOMS_DEV_UPGRADE=<Stunden>` (baut nach dem Warp alles Ausbaubare aus).

**Geprüft**
* `cargo test --workspace`: Spiel 167, Assetgen 59, Engine 57, Sim 372 (+ 8 ignorierte Messungen),
  Kampagne 5, Determinismus 10, Fuzz 7, vernünftiger Spieler 2, Doctest 1 – grün. Neue Tests u. a.:
  Ausbau von Sägewerk (Puffer, Auftrag, Ruhe während des Ausbaus), Vorratsspeicher → Lagerhaus mit
  Bestand, Rathaus → Burg mit Höchstzahl und Forschung, Daten-Fehler unverträglicher Ziele,
  Handelsstraßen-Anteil (Weltmarkt und Karawanen-Zählung), Bierausschank (Träger, Kauf, Preis, ohne
  Geld), Unterhaltung mit/ohne Bier, Einkommen/Steuer/Gebühr/Armut, Käufe zum Marktpreis, Freunde
  (gegenseitig, Tod, Prüfung), Festtag/Krank/Schutz im Tagesablauf, Temperatur, „seit …“,
  Spielkennung im Spielstand.
* `cargo clippy --workspace --all-targets -- -D warnings` sauber;
  `RUSTDOCFLAGS='-D warnings' cargo doc -p kingdoms_sim --no-deps` sauber.
* Balance (Release, `--nocapture`): vernünftiger Spieler nach 24 Tagen 51 Bewohner, Zufriedenheit
  77 (Faktor Wohlstand +1,7: fast alle Haushalte sparen), niedrigster Schatz 88; Kampagne über drei
  Jahre ohne Niederlage, je mindestens ein Krieg: Seed 7 → 43 Bewohner, Seed 11 → 44 (beide Dorf),
  Seed 23 → 84, Seed 31 → 89, Seed 42 → 88 (Marktflecken), Zufriedenheit 79–83. Seeds 7 und 11
  wuchsen vorher auf 89/87: Ihr Wachstum hing an einer Flüchtlingswelle nach Tag 40, die mit der
  veränderten Ereignisfolge ausbleibt; davor stagnierte auch der alte Lauf bei 36–38 (Baustellen
  ohne freie Bauarbeiter, weil das Skript alle Erwachsenen beschäftigt). Die Asserts sind
  unverändert und grün.
* Screenshots angesehen (Xvfb, lavapipe): neue Sprites im Atlas (alle fünf neben ihren
  Grundstufen), `KINGDOMS_DEV_SETTLEMENT=1 … KINGDOMS_DEV_UPGRADE=2` (Sägewerk „Wird ausgebaut ·
  wartet auf Baumaterial“) und `=24` (zwei Große Sägewerke auf der Karte, 3 von 3 Arbeitern, Puffer
  16, Meldung „Ausgebaut: Großes Sägewerk“, „Rohstoffmangel: Dem Gebäude „Großes Sägewerk“ fehlt
  Holz.“), Entwicklerstadt 2 nach 48 000 Ticks (38/42 Bewohner, 161 Gold, 708 Nahrung; vorher
  41/48, 145, 557).

**Offen**
* Darstellung fehlt noch (nächste Spiel-Etappe): „Ausbauen“ für Nicht-Häuser im Info-Fenster
  (bisher nur per Kommando/`KINGDOMS_DEV_UPGRADE`), Wohlstand/Freunde/Krank im Bürger-, Haus- und
  Bevölkerungsfenster, Gasthaus-Bestand, Temperatur im Wetterreiter und Jahreszeit-Tooltip,
  Handelsstraßen-Text im Handelsreiter, Spielstände nach Kennung gruppieren. Der Titel einer
  Ausbau-Baustelle im Info-Fenster lautet „Ausbau zu Großes Sägewerk“ (Spieltext, ungebeugt).
* Gleiche Einstellungen (Seed, Größe, Szenario, Schwierigkeit, Schalter) ergeben dieselbe
  Spielkennung – deterministisch gewollt; zwei getrennt gestartete Spiele mit gleichem Seed werden
  in einer Gruppierung zusammenfallen.
* Wohlstand ist abstrakt: Käufe gehen an „die Händler“ (kein Gold in die Staatskasse); die
  Einkommen sind so gewählt, dass beschäftigte Haushalte sparen.
* Grammatisches Geschlecht der Gebäude ist nicht in den Daten; Sätze verwenden „das Gebäude „…““.

## 2026-11-23 · Review-Befunde: Bedrohungen, Frieden, Kaufangebote, Mehrzahl, HUD-Skalierung, Spielstände (`crates/sim`, `crates/engine`, `src/`, `data/`, Doku)

**Fertig**
* **Frieden ohne erfundenen Sieg:** Ein Heer im Anmarsch kehrt bei Frieden still um (Größe 0, kein
  Bericht). Ein Heer auf der Karte zieht ab (neue Stufe `WarbandStage::Withdrawing`): es kämpft
  nicht mehr, wird nicht mehr angegriffen, geht durch Tore, zählt nicht als Bedrohung (kein Alarm)
  und bekommt nur dann einen Kampfbericht, wenn es gekämpft, beschädigt oder geplündert hat
  (`Warband::fought`; Ausgang Unentschieden bzw. Niederlage bei behaltener Beute, keine
  Erinnerung „Verlorene Schlachten“).
* **Feinde ohne Weg bleiben nicht mehr ewig:** Einstieg und Ziel nach Erreichbarkeit
  (Zusammenhangskomponenten; Tore gelten als passierbar), Ziel ist der Eingang oder das nächste
  freie Feld am Gebäude, `set_destination` jede Runde (gescheiterte Suchen werden wiederholt).
  Ohne Weg und ohne Mauer: nächstes erreichbares Ziel (ein Eroberungsheer plündert dann das
  reichste erreichbare Lager) oder Abzug ohne Beute („… findet keinen Weg zu seinem Ziel und zieht
  ab.“). Fliehende brechen Tore/Mauern auf ihrem Weg, nehmen ein anderes Randfeld, wenn ihres
  unerreichbar ist, und verlassen die Karte auch, wenn sie schon auf dem Randfeld stehen.
* **Tor vor Soldaten:** Feinde verfolgen nur Soldaten, die sie erreichen (Nahkampf) oder in
  Reichweite haben (Schützen); steht ein Soldat hinter einem Tor, brechen sie das Tor auf.
* **Umschlossenes Hauptquartier:** Einstieg am Randfeld mit Weg zum Hauptquartier, sonst mit Weg
  zu einer Mauer (Bresche), sonst an einem begehbaren Randfeld – Räuber, Heere und das
  Raub-Ereignis kommen wieder. Ein Feldzug ohne Weg zum Kartenrand wird abgelehnt
  („Vom Hauptquartier führt kein Weg zum Kartenrand.“, auch in `campaign_problem`), statt die
  Order ohne Feldzug zu setzen.
* **Manipulierte Spielstände:** `Military::validate` prüft „Gebiet verteidigen“ (Mittelpunkt auf
  der Karte, Umkreis `DEFEND_RADIUS` 1–12); vorher Überlauf/Panic beim ersten Tick.
* **KI-Kaufangebote** zahlen aus dem Schatz beim Annehmen: nur so viele Einheiten, wie das Reich
  noch bezahlen kann („Mehr kann … nicht mehr bezahlen.“); reicht es für keine, nennt
  `proposal_problem` den Grund. Der Schatz wird nie negativ.
* **Mehrzahl der Einheiten:** `units.ron` hat `plural` (Milizen, Bogenschützen,
  Armbrustschützen, Reiter, Belagerungsgeräte …); `unit_count_text`/`fighter_count_text` in
  Rekrutierungsmeldungen, Kompanie- und Bedrohungszusammensetzung („6 Armbrustschützen in
  Ausbildung“).
* **HUD bei großer UI-Skalierung:** Die Engine senkt die UI-Skalierung, solange das Fenster sonst
  weniger als die vom Spiel gesetzte Mindestgröße zeigt (`EngineContext::set_ui_min_size`,
  1280 × 720 Punkte; nie erhöht). 150 % auf 1280 × 720 wirken als 100 % (die Einstellungen sagen
  es), ein 1024 × 600-Fenster zeigt das ganze HUD mit 80 %. Pausenmenü und
  Niederlage-Bildschirm scrollen, wenn sie nicht passen.
* **Feindgruppen als Menge:** Kämpfer einer Gruppe auf derselben Stelle werden auf einer
  Sonnenblumen-Spirale verteilt gezeichnet (nur Darstellung, nicht in Gebäude/Wasser); ein Heer
  von 120 ist als Heer zu sehen.
* **Aufgaben:** Anforderungen tragen ihre Art (`ranks::RequirementKind`); jede offene führt
  irgendwohin (Wohlstand → Wirtschaft, Gebiet → Gebietsgrenze, fehlendes Gebäude → Bauwerkzeug).
  Die Zeilen waren auch für Einwohner/Gebäude nicht klickbar (wählbare Beschriftungen nahmen den
  Klick) – behoben.
* **Pausenmenü:** F5/F9 wirken dort wie die Knöpfe.
* **Spielstände:** Überschreib-Frage „Den Spielstand „kopie“, gespeichert heute um 15:04,
  überschreiben?“; Enter im Namensfeld bestätigte die Frage bisher im selben Frame mit (es wurde
  ohne Rückfrage überschrieben) – behoben. Ein abgeschnittener Spielstand (Länge ≠ Kopfangabe)
  gilt beim Auflisten als beschädigt (`read_header_from_file`), „Fortsetzen“ wählt ihn nicht;
  Spielstände mit gleichem Namen zeigen ihren Dateinamen.
* **Neues Spiel:** Die Landschaftsvorschau nutzt dieselben Generator-Einstellungen wie das Spiel
  (`state::world_generator_settings`: Siedlungsorte je Kartengröße statt Szenario-Obergrenze).
* **Ereignis-Dialog:** bleibt über später geöffneten Seitenfenstern, Info-Fenster und Meldungen
  (`hud::docked_area_on_top`, auch der Anfrage-Dialog); die Fußzeile nennt nur noch die Uhrzeit
  der Frist („Frist bis 18:00 Uhr“), die Restzeit steht einmal im Balken.
* Nebenbei: Übersicht „reicht etwa 1 Tag“ statt „1 Tage“.

**Geprüft**
* `cargo test --workspace`: Spiel 167, Assetgen 59, Engine 57, Sim 357 (+ 8 ignorierte
  Messungen), Kampagne 5, Determinismus 10, Fuzz 7, vernünftiger Spieler 2, Doctest 1 – grün.
  Neue Tests u. a.: Frieden vor/nach Ankunft ohne Bericht und Erinnerung, Räuber an verbautem
  Eingang, Abzug ohne erreichbares Ziel (Insel), erneute Wegsuche nach `NoPath`, Tor vor
  Soldaten mit Rückzugsbefehl, ummauertes Rathaus (Bresche, Feldzug abgelehnt), Validierung
  „Gebiet verteidigen“, anteilige Kaufangebote, Mehrzahl, abgeschnittene Spielstände,
  Vorschau-Einstellungen, Anforderungs-Verweise, UI-Skalierung, Fristtext, Menge.
* Die Review-Szenarien (Harness der Review, vorübergehend eingebunden und wieder entfernt):
  R1/R2 ohne Bericht und ohne „Verlorene Schlachten“, R3/R3b/R10 Bedrohung binnen Tag 0 beendet
  (0 von 80 Soldaten-Nachtstunden im Dienst), Tor-Szenario: erster Torangriff nach 350 Ticks,
  Tor nach 1090 Ticks gebrochen, ummauertes Rathaus: Räuber und Heer kommen, Feldzug abgelehnt,
  manipulierte Spielstände beim Laden abgelehnt.
* `cargo clippy --workspace --all-targets -- -D warnings` sauber;
  `RUSTDOCFLAGS='-D warnings' cargo doc -p kingdoms_sim --no-deps` (und Engine) sauber.
* Screenshots angesehen (Xvfb, lavapipe): 150 % auf 1280 × 720 (HUD, Pausenmenü, Hinweis in den
  Einstellungen), 1024 × 600 (HUD und Pausenmenü bei 80 %), Brand-Dialog über Aufgaben- und
  Übersichtsfenster, Heer von 120 als Menge, Ladeliste mit abgeschnittenem Spielstand und
  gleichnamigen Ständen, Überschreib-Frage, Klick auf „Wohlstand“ öffnet „Wirtschaft“;
  F5 im Pausenmenü speichert (Log „save of tick 24001“); Standard-Autoshot Seed 7.

**Offen**
* Ein nach dem Frieden abziehendes Heer, das ganz von Mauern eingeschlossen wird, wartet auf
  einen Weg (es bricht keine Mauer mehr); ein von Nicht-Mauer-Gebäuden vollständig umbautes
  Hauptquartier kann nicht erobert werden (das Heer plündert dann das reichste erreichbare
  Lager).
* Bei sehr kleinen Fenstern (unter 1280 × 720) wird die Oberfläche entsprechend kleiner
  (z. B. 63 % bei 800 × 480); die Mindestfenstergröße der Engine bleibt 800 × 480.
* Die Menge ist reine Darstellung: Kampfeffekte und Treffer liegen weiter an den Positionen der
  Simulation; stirbt ein Kämpfer, rücken die übrigen Figuren der Gruppe um einen Platz nach.

## 2026-11-23 · Sim-Lane Schritt 6: Skalierung, Simulation-LOD, Robustheit, Kampagnen-Balance (`crates/sim`, `data/`, Doku)

**Fertig**
* **Weggedächtnis statt Pfad-Cache** (Backlog „Pfad-Warteschlange bei 2000 Bewohnern“): gefundene
  Wege bleiben über Rasteränderungen erhalten, solange sie kürzeste Wege sind (vergessen wird nur,
  was ein teurer/gesperrt gewordenes Tile im Umriss hat oder was ein billiger gewordenes Tile
  schlagen könnte – untere Schranke über das geänderte Rechteck), am längsten unbenutzte zuerst.
  Das Gedächtnis wird mit dem Fingerabdruck des Wegkosten-Rasters gespeichert und beim Laden Schritt
  für Schritt geprüft; ein gemerkter Weg kostet daher nur 25 Knoten des Budgets
  (`walking.path_cache_hit_cost`) statt seiner ganzen Suche, und ein geladenes Spiel antwortet
  trotzdem exakt wie das Original. Das Raster wird am Ende jedes Ticks nachgeführt. Ergebnis
  `walking_performance` (2045 Bewohner, Release): Wartezeit Ø 39,0 → 12,0 Ticks, höchstens
  166 → 98 Ticks.
* **Simulation-LOD** (§18): automatischer Hausausbau je Haus in seinem Stunden-Slot statt aller
  Häuser zur vollen Stunde (vorher ≈ 2 ms Spitze je Stunde bei 1000 Gebäuden); Besorgungsgänge eines
  Ticks füllen ihre Vorräte gemeinsam am Ende des Bewegungsschritts auf (ein Dienst-/Lager-Index je
  Tick statt je Ankunft). Strategie und Messwerte: architecture.md §3.6.
* **Messstadt** `scale_tests::scale_performance` (Release, ignoriert): Karte „Groß“ (256²,
  Seed 7) mit 5 KI-Reichen, geplanter Stadt aus 11 × 11 Blöcken (999 Gebäude), 5000 Bewohnern in
  1667 Haushalten, 130 Soldaten in 4 Kompanien, Handelsaufträgen mit Karawanen, Krieg mit einem
  aggressiven Reich und einem Heer von 120 Mann; 3 Tage: Tick Ø 399 µs, Median 313 µs, p99 1,48 ms,
  max 4,76 ms (kein Tick über 8 ms); bis 3095 Läufer gleichzeitig, Pfad-Wartezeit Ø 1,4 Ticks;
  RSS 29 MiB; Spielstand 1,60 MiB, Kodieren 4,7 ms, Laden inkl. Prüfung 16,9 ms; das geladene Spiel
  läuft eine Stunde identisch weiter. Beim Aufbau fielen zwei Fehler der Messstadt selbst auf
  (Häuser ohne Markt und Lager in Reichweite verhungern; Lager ohne Straßenanschluss zählen nicht
  zum Reichslager) – die Simulation verhielt sich dabei regelkonform.
* **Wünsche der Spiel-Lane umgesetzt:** Anwesende je Gebäude (`Population::occupants`, O(1)),
  Baustellen-Status (`buildings::site_work` → `SiteWork` mit deutschem Text,
  `ConstructionSite::progressed_at`), Heizen je Haus (`Population::heating` → `Heating`), Wind im
  Wetterzustand (Richtung in 16 Stufen, Stärke, Einheitsvektor; trägt Brände weiter), Herden der
  Weidehöfe als Zustand (wachsen täglich, treiben die Produktion), kahle Obstbäume im Winter
  (`FieldView::is_bare`/`seasonal_sprite` → `fields/orchard_bare`, Chunks werden gemeldet),
  Orte bei `StockLost` und `CitizensFellIll` (Gebäude/Häuser und Position; die Ereignis-Meldung
  zeigt dorthin), `research::queue_plan` und `Command::QueueResearchWithPrerequisites` (alles oder
  nichts), „Das Spiel ist verloren.“ in Bauplatz- und Straßenprüfung
  (`PlacementError::GameOver`, `RoadProblem::GameOver`).
* **Felder:** Feldfarmen beanspruchen nur erreichbare Tiles; ein Feld, das 24 Stunden unerreichbar
  ist, wird freigegeben (vorher blieb es dauerhaft brach beansprucht).
* **Lager über Kapazität – Regel geprüft und beibehalten:** Waren gehen nie verloren; ein Lager
  liegt höchstens um die Last bereits unterwegs befindlicher Träger über der Kapazität, nimmt dann
  keine Aufträge an und zeigt den Bestand ehrlich (game_design.md §7).
* Nicht umgesetzt: Kennzeichen „wirkt erst mit dem Militär“ je Technologie (seit 5b wirken alle
  militärischen Wirkungen, das Spiel hat das Schild entfernt); neutraler Fristablauf für
  KI-Vorschläge (die Spiel-Lane hat den Posteingang gebaut).
* **Robustheit** (`tests/fuzz.rs`): Befehls-Fuzzing auf 6 Seeds über alle 44 Kommandoarten
  (gültige und ungültige Ids, Positionen außerhalb der Karte, unbekannte Daten-Ids, absurde
  Mengen; je Lauf ≈ 1600 Kommandos, davon ≈ 80 % abgelehnt, 14–43 Gebäude gebaut, Kriege und
  Eroberungen), Prüfung jede Spielstunde, Speichern/Laden zweimal je Tag mit identischer
  Fortsetzung; manipulierte Spielstände (abgeschnitten, Bits gekippt mit und ohne reparierte
  Prüfsumme, Müll, leer): nie ein Panic, jeder Fehler ist ein `SaveError` (die Spiel-Lane zeigt
  ihn deutsch, `src/save/mod.rs`), harmlose Änderungen laden und laufen weiter.
* **Kampagnen-Balance** (`tests/campaign.rs`): der vernünftige Spieler spielt drei Jahre auf fünf
  Seeds (7, 11, 23, 31, 42) mit Ereignissen, KI-Reichen, drei Räuberbanden und – falls kein Reich
  angreift – einem selbst erklärten Krieg gegen das schwächste Reich (Tag 42) mit Friedensangeboten
  alle drei Tage. Seed 42 sparte nie genug für die Kaserne (Steuern 39 Gold/Tag bei 39 Bewohnern):
  der Spieler erhöht die Steuern jetzt auf 15 %, solange er für die Kaserne spart, und legt in der
  Zeit keine Gemüsefarmen unter 80 Gold Rest an (Skript, keine Datenänderung). Kennzahlen:

  Seed | Bewohner | Rang | Schatz (tiefster) | Zufriedenheit | Soldaten | Kriege / Frieden | Tote
  ---|---|---|---|---|---|---|---
  7 | 12 → 89 | Marktflecken | 420 (40) | 74 | 8 | 1 / 1 | 2 im Kampf
  11 | 12 → 87 | Marktflecken | 470 (40) | 77 | 8 | 1 / 1 | 7 im Kampf
  23 | 12 → 83 | Marktflecken | 634 (84) | 78 | 8 | 1 / 1 | 5 im Kampf
  31 | 12 → 99 | Marktflecken | 907 (232) | 78 | 8 | 1 / 1 | 6 im Kampf, 1 Krankheit
  42 | 12 → 99 | Marktflecken | 319 (0) | 79 | 8 | 2 / 2 | 17 im Kampf, 1 Krankheit

  Niemand verhungert oder erfriert; je Seed mindestens eine Bande geschlagen (die dritte Bande an
  Tag 60 erreicht meist ein unbewachtes Lager). Der naive Spieler (`reasonable_player.rs`)
  besteht unverändert.
* Speicherformat 12 (Weggedächtnis, Wind, Herden, Erreichbarkeits-Stunden, „zuletzt gearbeitet“ der
  Baustellen) mit Validierung; neue Regeln in `rules.ron` (`walking.path_cache_hit_cost`,
  `agriculture.pasture_tiles_per_animal`, `herd_start_percent`, `herd_growth_percent_per_day`).
* Doku: game_design.md §5.1, §6, §7, §8.2, §8.3, §9, §12, §1; architecture.md §3.2–§3.6;
  roadmap.md Phase 8.

**Geprüft**
* `cargo test -p kingdoms_sim` grün: Bibliothek 350 + 8 ignoriert (neu: Weggedächtnis folgt
  Rasteränderungen, Weggedächtnis gespeichert und beim Laden geprüft, Anwesende, Baustellen-Status,
  Heizen, Wind, Brand mit Wind, Forschung samt Voraussetzungen, Niederlage in der Vorschau, Ort von
  Seuchen; angepasst: Hausausbau im Slot, Herden in den Weide-Tests, unerreichbares Feld wird
  freigegeben), Kampagne 5 (drei Jahre, Seeds 7/11/23/31/42), Determinismus 10 (Weggedächtnis,
  Wind und Herden nach dem Laden gleich), Fuzz 7, Bilanz 2, Doctest 1.
  `cargo clippy -p kingdoms_sim --all-targets -- -D warnings` sauber,
  `RUSTDOCFLAGS='-D warnings' cargo doc -p kingdoms_sim --no-deps` sauber.
* Release-Messungen einzeln (`--test-threads=1`): `scale_performance` (Werte oben, das geladene
  Spiel läuft identisch weiter), `walking_performance` (Ø 12,0 / max 98 Ticks Wartezeit, Tick Ø
  182 µs). Ein Lauf parallel zu einem Build zeigte Spitzen bis 10,6 ms – die Maxima hängen von der
  Rechnerlast ab, Mittelwert und p99 kaum.
* Spiel gegen diese Simulation: Kopie des Haupt-Workspace mit `crates/sim` und `data/` der Lane,
  `cargo check --workspace --all-targets` ohne Fehler und Warnungen (keine Anpassung in `src/`
  nötig); `cargo test -p kingdoms` dort grün (161 Tests).

**Offen**
* Die Arbeitszuteilung (Schritt 3) läuft bei jeder Änderung komplett und ist bei 5000 Bewohnern
  mit 2–4,5 ms die größte verbliebene Einzelspitze; ein inkrementelles Verfahren wäre der nächste
  Schritt.
* `fields/orchard_bare` fehlt im `fields`-Atlas (Spiel-Lane/Assetgen); bis dahin `sprite()` statt
  `seasonal_sprite()` verwenden. Die Spiel-Lane kann ihre eigene Einreih-Logik
  (`research.rs::queue_plan`) durch `Command::QueueResearchWithPrerequisites` ersetzen, Rauch über
  `Population::heating`, Lichter über `Population::occupants`, Staub über `site_work`, Wind über
  `WeatherState::wind_*` und Weidetiere über `Farm::herd` steuern.
* Die Messstadt ist ein synthetischer Plan (Blöcke mit vollen Lagern), keine gewachsene
  Spielstadt; Felder und Werkstätten sind bewusst gleichmäßig verteilt.
* Die dritte Räuberbande der Kampagne plündert meist erfolgreich (die Kompanie verteidigt nur das
  Gebiet um das Rathaus); kein Assert verlangt mehr als einen Sieg.

## 2026-11-23 · Spiel-Lane Stufe 6: Diplomatie, Militär und Herrscherrang in Oberfläche und Karte (`src/`, `tools/assetgen`, `assets/`, Doku)

**Fertig**
* **Reiter Diplomatie** (Taste I, Militär-Leiste „Diplomatie“): Reiche mit Wappen, Zustand und
  Beziehung; je Reich Herrscher, Wesen, Beziehungsbalken mit allen Anteilen, Krieg mit
  Kriegsmüdigkeit, Verträge mit Restlaufzeit, Einwohner, Stärke, Wohlstand, Gebiet,
  Karawanenrisiko, gesuchte/angebotene Waren mit euren Preisen, „Kamera hin“, Verlauf.
  Handlungen (Verträge, Kündigung, Geschenk, Tribut, Krieg, vier Friedensangebote) mit den Kosten
  der Simulation, dem erwarteten Urteil des Reiches samt Gründen oder dem Grund, warum es nicht
  geht; Dialoge für Geschenk (Gold und Waren), Tribut und Frieden mit Gold, Rückfrage vor Krieg
  und Kündigung. Anfragen der Reiche erscheinen oben im Reiter, im Ereignis-Panel (blauer Zähler)
  und als **Entscheidungskarte** mit Wappen, Frist und beiden Antworten.
* **Reiter Militär** (Taste Q, Militär-Leiste „Heer verwalten“): Soldaten, Bereite, Rekruten,
  Feldzug, Verwundete, Sold, Stärke, Alarm; Bedrohungen mit Zusammensetzung, Ankunft, Ziel und
  „Kamera hin“; Kriege; Einheiten; Kompanien mit Werten, Mitgliedern und Befehlen (Bereitschaft,
  Verteidigen …, Patrouille …, Rückzug, Auflösen, Feldzug/Belagerung gegen Reiche im Krieg);
  Soldaten ohne Kompanie (neue Kompanie, zuweisen, entlassen mit Rückfrage); Rekrutierung je
  Militärgebäude mit Kosten, Werten und Gründen samt Rekruten in Ausbildung (auch im
  Info-Fenster, dort mit Besatzung, Schüssen und Trefferpunkten); Feldzüge; Kampfberichte.
* **Befehle auf der Karte:** Befehlswerkzeug mit Banner, Vorschau (Gebäude- bzw. Gebietsumriss,
  gepunktete Patrouille mit Rückweg und Strecke zum Cursor), Tooltip mit dem Urteil von
  `order_problem`; Eingabe/„Fertig“/Klick auf den ersten Punkt gibt die Patrouille, Rechtsklick
  nimmt den letzten Punkt zurück; danach öffnet sich der Reiter wieder und zeigt alle Routen.
* **Handel:** Karawanen unterwegs, Handelspartner mit Preisen je Ware, „Karawane aussenden …“
  (Waren, Gold, Wünsche; `send_problem` live).
* **Karte:** KI-Siedlungen aus ihrer Bauwerksliste (leicht in Reichsfarbe getönt, Banner an Burg
  und Wachtürmen), Reichsgrenzen als farbige Linie über einem getönten Randstreifen, Tooltip
  „Gebiet: Königreich …“; Heere in Reichsfarbe getönt mit Ring und Banner, Räuber mit eigenen
  Figuren auf roten Ringen, eigene Soldaten auf goldenen Ringen mit Kompaniebanner, Karren mit
  Reichsbanner; Funken und Staub im Nahkampf, Feuer und Rauch auf angegriffenen Gebäuden;
  Randmarken für Feinde (Art, Größe, Ankunft, Pfeil, Klick = Kamera). Minikarte mit
  Reichsgebieten, Grenzen und Bauwerken in Farbe sowie Punkten für Feinde, Kompanien, Karawanen.
* **Aufgaben** (rechte Leiste, Panel und Fortschrittsbalken am Knopf): Rang, Ansehen, Wohlstand,
  Anforderungen des nächsten Rangs mit Balken (Klick öffnet den passenden Reiter), erreichte
  Ränge. **Aufstiegsfeier** bei `RankAchieved` (Entscheidungsdialoge warten dahinter).
  **Niederlage-Bildschirm** statt Pausenmenü: Grund, Datum, Bilanz, „Spielstand laden“,
  „Hauptmenü“, „Reich ansehen“, „Beenden“. Übersicht mit Kriegen, Feinden samt Ankunft,
  Anfragen, Sold und Rang.
* **Klang:** Musik „Krieg“ bei Krieg oder Feinden auf der Karte, „Spannung“ bei Feinden im
  Anmarsch, Tributforderung oder ausbleibendem Sold; Signale und Beraterstimmen für Bedrohung,
  Schlachtausgang, Mauerbruch, Krieg (von uns/gegen uns), Frieden, Vertrag, Gesandtschaft,
  Tribut, Geschenke, Karawanen, Rekrutierung, Aufstieg, Niederlage; 20 neue Manifest-Ids mit
  deutschen Beschreibungen. Automatisches Anhalten bei Ereignissen gilt auch für Anfragen.
* **Grafik (assetgen):** keine gemalten Menschen mehr in Gebäuden (Rathaus, Markt, Farmen,
  Militär, Werkstätten; Requisiten statt Figuren, `props::animal` nur für Tiere), neue Läufer
  `siege` (Katapult) und `raider`, `banner`, `res_armor`, `ui_rank`, Wappen `crest_0..6` in den
  Reichsfarben (Spiel und Generator nutzen dieselbe Palette, Test).
* Entwicklerhilfen: `KINGDOMS_DEV_ARMY`, `KINGDOMS_DEV_THREAT`, `KINGDOMS_DEV_TRADE`,
  `KINGDOMS_FOCUS=feind|karawane`, `KINGDOMS_WINDOW=diplomatie|militaer`,
  `KINGDOMS_PANEL=aufgaben` (alles über Kommandos; README).

**Geprüft**
* `cargo test --workspace` grün: Spiel 161, Engine 56, Assetgen 59, Sim 340 + 7 ignoriert,
  Feldzug 3, Determinismus 10, Bilanz 2, Doctest 1 (neu u. a.: Dialoge senden die bearbeitete
  Handlung, Anfragekarte öffnet einmal je Anfrage, Patrouillen-Punkte, Randmarken, Grenzen aus
  der Landkarte, Allianzfarben der Läufer, Entwickler-Heer, Karawanen-Entwurf, Gefahr → Musik,
  Niederlage-Bilanz, Kronen-/Wappen-Atlas, gleiche Reichsfarben in Spiel und Generator).
  `cargo clippy --workspace --all-targets -- -D warnings` sauber.
* Screenshots (Xvfb + lavapipe) angesehen, 1600 × 900: Reiter Diplomatie (vor und im Krieg),
  Geschenk-Dialog, Reiter Militär mit Kompanie, Patrouillen-Vorschau mit Banner, Heer im Anmarsch
  (Randmarke, Übersicht, Minikarte), Heer marschiert, Kampf mit Funken/Staub, Flucht, KI-Stadt mit
  Grenze, Weltübersicht mit allen Reichen, Karawane am Handelskontor, Handelsreiter mit
  Karawanen, Anfrage-Karte, Aufstiegsfeier, Aufgaben-Panel, Militär-Leiste und
  Kasernen-Info-Fenster, Niederlage-Bildschirm; 1280 × 720: Diplomatie und Militär im Krieg,
  Kaserne, Niederlage. Generator-Bögen der neuen Sprites und Gebäude vorher/nachher geprüft.
* Echte Eingabe (XTest): Diplomatie – Handelsvertrag vorgeschlagen (angenommen, Preise sinken),
  Geschenk über den Dialog gesendet (Gold 500 → 450, Dank-Meldung); Militär – „Patrouille …“,
  Wegpunkte auf der Karte, Eingabe → „Patrouilliert (2 Wegpunkte)“, Reiter wieder offen.
  Keine Panics, keine ERROR-Zeilen.

**Offen**
* Sim-Texte: „Krieg seit 0 Tage“ (`KingdomRow.war.text`) bzw. „seit 0 Tagen“ (`war_status`)
  – Grammatik und „seit heute“ gehören in die Simulation (Übersicht formuliert selbst).
* Feldzüge sind abseits der Karte abstrakt; die Karte zeigt nur Sammeln und Rückkehr. Pfeile der
  Schützen werden nicht gezeichnet (nur Funken/Staub im Nahkampf).
* Die Reiter haben bei 12 Tabs nur Symbole (außer dem gewählten); die Beschriftung steht im
  Tooltip.
* Ohne Spielstand mit gemischter Militärlage wurden die Belagerung und der Feldzug nur im Reiter
  (Bedingungstext), nicht auf der Karte geprüft.

## 2026-11-23 · Merge Schritt 5 – Sim-Lane 5a/5b + Spiel-Lane 5a/5b (`crates/sim`, `data/`, `src/`, Doku)

**Fertig**
* `crates/sim` und `data/` aus der Sim-Lane übernommen (keine neuen Abhängigkeiten; die Versionen
  der Sim-Abhängigkeiten in `Cargo.lock` stimmen mit der Lane überein). Doku zusammengeführt:
  `game_design.md` aus der Sim-Lane, `architecture.md` §3 aus der Sim-Lane und §1/§2/§4–§6 aus der
  Spiel-Lane, `roadmap.md` mit den Zeilen beider Lanes, alle vier Einträge hier unten.
* Anpassungen in `src/` an die neue Sim-API (ohne neue Oberfläche):
  * Kassenbuch-Kategorie „Diplomatie“ (eigene Farbe, ohne Tagesrate); Läuferrollen Karren,
    Bogenschütze, Ritter, Belagerung, Räuber – Belagerung und Räuber haben noch keine Bilder und
    laufen bis dahin als Soldat bzw. Mann (Hinweis im Log); Ziel „Feld x, y“ im Bürgerfenster;
    Gebäudeschaden durch Angriff klingt nach Kampf.
  * Niederlage: Sobald die Simulation das Spiel für verloren erklärt, öffnet sich einmal das
    Pausenmenü als „Niederlage“ mit dem Grund (auch nach dem Laden eines verlorenen Spielstands);
    „Reich ansehen“ schließt es, Esc öffnet es wieder. Befehle lehnt die Simulation dann mit „Das
    Spiel ist verloren.“ ab.
  * Handel: Mit KI-Königreichen laufen die Aufträge über deren Karawanen. Statt der
    Weltmarktpreise zeigt der Reiter den günstigsten Einkauf bzw. besten Verkauf bei Reichen mit
    Handelszugang („–“, wenn keines die Ware anbietet bzw. sucht), der Kontor-Text sagt nicht mehr
    „stündlich“.
  * Forschung: Das Schild „wirkt erst mit dem Militär“ ist entfallen – die militärischen
    Wirkungen gelten jetzt.
  * Minikarte zeichnet von Feinden zerstörte Mauern und Tore sofort neu; Übersicht: „etwa 1 Tag“
    statt „1 Tage“, keine Warnung „Forschung steht still“, wenn alles erforscht ist.
  * Sim-Text: „Räuber überschreiten die Grenze! Ihr Ziel: …“ / „Das Heer von … überschreitet die
    Grenze! Sein Ziel: …“ (vorher falsche Verbform).
* Diplomatische Vorschläge der KI laufen ohne Antwort nach 18 Stunden als abgelehnt aus und
  blockieren nichts; Vorschläge, Kriegsmeldungen, Karawanen, Bedrohungen und Schlachten erscheinen
  als Meldungen im Feed.

**Geprüft**
* `cargo test --workspace` grün: Spiel 147, Engine 56, Assetgen 57, Sim 340 + 7 ignoriert,
  Feldzug 3, Determinismus 10, Bilanz 2, Doctest 1 (neu: Niederlage im Pausenmenü,
  Partnerpreise der Reiche, Kontor-Texte, Übersicht bei voller Forschung).
  `cargo clippy --workspace --all-targets -- -D warnings` sauber.
* Screenshots (Xvfb + lavapipe) angesehen: Hauptmenü; Direktstart Seed 7; Entwicklerstadt nach
  48 000 Ticks (Meldungen mit KI-Vorschlägen und „(keine Antwort)“, Handel mit Partnerpreisen,
  Forschung ohne Schild), im Winter (100 800 Ticks, Übersicht) und nach zwei Jahren (230 400 Ticks,
  101 Bewohner, 2 083 Gold); Niederlage live (Heer erobert das Rathaus bei 4×) und nach dem Laden.
  Echte Eingabe (XTest): „Reich ansehen“, Straße ziehen → Hinweis „Das Spiel ist verloren.“, Esc
  öffnet die Niederlage wieder. Keine Panics, keine ERROR-Zeilen.
* In der Lane-Kopie nach `sync.sh from-main`: `cargo test -p kingdoms_sim` grün.

**Offen**
* Diplomatie- und Militärfenster fehlen (nächste Etappe): KI-Vorschläge lehnt das Spiel bis dahin
  stets durch Fristablauf ab (kostet etwas Beziehung, ein verweigerter Tribut kann Krieg bringen),
  obwohl die Meldung „Antwort binnen 18 Stunden“ sagt; Rekrutierung, Kompanien und Feldzüge sind
  nur über Befehle erreichbar.
* KI-Städte und Grenzen sind auf Karte und Minikarte unsichtbar (die Bauwerke blockieren aber
  Läufer); Feinde werden nicht hervorgehoben; Bilder für Belagerung, Räuber und `res_armor`
  fehlen (das Symbol-Laden meldet `res_armor` als fehlend).
* Das Werkzeug zeigt nach der Niederlage weiterhin „Kann gebaut werden“; erst die Ablehnung sagt,
  dass nichts mehr geht.

## 2026-11-22 · Sim-Lane Schritt 5b: Militär und Krieg, Herrscherränge, Niederlage (`crates/sim`, `data/`, Doku)

**Fertig**
* **Soldaten sind Bewohner** (neues Modul `military`, Daten `units.ron` erweitert, `rules.ron` →
  `military`): `Command::Recruit` an Kaserne, Bogenschützenposten oder Stallungen (Freischaltung
  `UnlockUnit`, 6 Ausbildungsplätze je Gebäude, Pferde je Viehzucht für 4 Reiter) kostet Gold
  (Kassenbuch „Militär“) und Ausrüstung aus den Lagern; Arbeitslose zuerst, dann Arbeiter verlassen
  ihre Stelle (die Arbeitskraft sinkt), bleiben im Haushalt und wohnen zu Hause; Ausbildung nur im
  Gebäude (Ausbilder beschleunigen), Belagerungsgerät mit Bediener und drei Mann Mannschaft. Sold
  stündlich, im Krieg Rationen, Moral (Sold, Hunger, Zufriedenheit, Kampfergebnisse,
  Kriegsmüdigkeit, Ausrüstung), Ausrüstung mit Verschleiß und Neuausrüstung, Heilung, Erfahrung,
  Entlassung (`DismissSoldiers`, Alter). Neue Ressource `armor` (Rüstungen, Waffenschmiede);
  Miliz mit Holzspießen (3 wood), Kaserne 150 G.
* **Kompanien und Befehle** (`CreateCompany`, `AssignSoldiers`, `DisbandCompany`,
  `CompanyOrder`): Bereitschaft am Posten, Verteidigen (Garnison bis zur Kapazität, übrige rundum;
  Gebiet), Patrouille (Wegpunkte im Kreis), Feldzug, Belagerung, Rückzug. Dienst in den
  Arbeitsstunden, bei Alarm Tag und Nacht; Eingreifen im Alarmradius, Verfolgung, Schützen halten
  Abstand, Verwundete und Mutlose ziehen sich zurück. Soldaten laufen als Läufer mit den Rollen
  `soldier`, `archer`, `knight`, `siege` (neu: `CitizenRole::{Archer, Knight, Siege, Raider}`,
  `CitizenView::hostile`).
* **Bedrohungen:** Das Ereignis Räuberüberfall schickt eine echte Bande (2 h angekündigt), die zu
  unbewachten Lagern mit Waren läuft, 24 Runden plündert (Waren, Gold), mit 35 % Brand legt und
  beladen langsamer flieht; Gefallene lassen die Beute fallen. Reiche im Krieg schicken statt der
  abstrakten Überfälle **Heere** (Hälfte der Soldaten, höchstens 40, Heeresmischung) mit
  Ankunftszeit: aggressive auf das Hauptquartier (Sturm, Eroberung nach 2 h ohne Verteidiger in
  8 Feldern = Niederlage), andere plündern das reichste Lager. Tore halten Feinde auf (sie brechen
  sie auf), ohne Weg schlagen sie Breschen in Mauern.
* **Kampf** alle 10 Ticks deterministisch: Schützen zuerst (Soldaten, Wachtürme, Posten, Burg;
  Garnison +2 Reichweite), dann Nahkampf; Schaden Angriff × 30 / (Verteidigung + 10) ± 20 %;
  `UnitBonus`, Erfahrung, Ausrüstung, Moral, Klassenboni (Speer gegen Reiter, Reiter gegen
  Schützen), Wald, Höhe, Mauer, Garnison. Gefallene sterben als Bewohner (Todesursache „Kampf“),
  Gebäude werden zu Ruinen (Ursache „Angriff“), Mauern/Tore verschwinden, Schäden heilen ohne
  Feinde. Kampfberichte (Verlauf 32) mit Verlusten, Beute, Ausgang.
* **Feldzüge und Belagerungen:** Sammeln am Kartenrand, Proviant, Reise wie Karawanen, abstrakte
  Feldschlacht gegen das Feldheer (Brechen nach Verlusten und Moral), Kriegsbeute, Kriegsmüdigkeit;
  Belagerung mit Belagerungsgerät gegen Mauern und Besatzung, Erfolg erzwingt Frieden mit
  Gebietsabtretung und Entschädigung; Heimkehr; Rückzug und Frieden beenden Feldzüge.
* **Kriegsfolgen** in der Diplomatie: Stärke des Spielers aus echten Soldaten nach Einheit,
  geschlagene Heere machen das Reich müde (+30) und zornig (Erinnerung „Verlorene Schlachten“),
  Verluste senken Soldaten und Einwohner der Reiche, Heere kehren beim Frieden um.
* **Herrscherränge** (`rules.ron` → `ranks`, Modul `ranks`): Siedlung → … → Imperium nach
  Einwohnern, Wohlstand, Gebäuden, Gebiet und Pflichtgebäude, Aufstieg mit Meldung, **Ansehen** als
  Beziehungsteil bei allen Reichen (+2 … +12). **Niederlage** (Hauptquartier erobert, Bevölkerung
  0): `GameState::game_over`, `SimEvent::GameOver`, danach keine Kommandos, Uhr steht;
  `GameSettings::defeat_enabled` für Systemtests.
* **Forschung:** `UnlockUnit`, `UnitBonus`, `WallStrengthBonus` wirken (Texte ohne „wirkt ab der
  Militär-Phase“); Abschlussmeldung nennt neue Einheiten.
* **Ereignisse mit echten Soldaten:** Räuberüberfall (Abwarten → Bande; Lösegeld; Soldaten
  ausschicken → Bande + 8 h Alarm), Unruhen (Wachen → 8 h Aufmarsch am Hauptquartier);
  `MinSoldiers` zählt einsatzbereite Soldaten. Neue Effekte `Raiders`, `Alarm`, `Muster`.
* **Bevölkerung:** Besorgungen macht ein Nicht-Soldat, wenn es einen gibt; Haushalte nur aus
  Kindern (Kriegswaisen) werden stündlich von den Nachbarn versorgt; Soldaten zählen beim Lohn und
  nicht als arbeitslos.
* **Berichte** (deutsch): `military_report` (Einheiten, Kompanien, Soldaten, Sold, Stärke, Alarm),
  `recruit_options` (Kosten, Werte, mögliche Anzahl oder Grund), `threats_report` (Art, Größe,
  Zusammensetzung, Ankunft, Ziel), `battle_reports`, `war_status`, `campaigns_report`,
  `rank_report` (Panel „Aufgaben“ mit Anforderungen und Fortschritt, Niederlage).
* Entwicklerbefehl `DebugSpawnThreat`; Spielstandformat 11 mit Validierung; `rederive` kürzt
  Gesundheit bei gesenkten Trefferpunkten.

**Geprüft**
* `cargo test -p kingdoms_sim` grün: Lib 340 + 7 ignoriert (Messungen), Feldzug-Bilanz 3,
  Determinismus 10, Bilanz 2, Doctest 1; `cargo clippy -p kingdoms_sim --all-targets -- -D
  warnings` sauber; `RUSTDOCFLAGS='-D warnings' cargo doc -p kingdoms_sim --no-deps` sauber.
* Neue Militär-Tests (Lib): Rekrutierung (Arbeitskraft, Gold, Holz, Ausbildung, Sold 1 G/Tag,
  Stärke), Hinderungsgründe (Forschung, falsches Gebäude, Plätze, Gold, Erwachsene, Pferde), Räuber
  plündern unbewachtes Lager (≤ 15 Waren und 10 Gold je Räuber, Niederlage) und fliehen, Soldaten
  und Turm schlagen Räuber (Gefallene als echte Tote mit Statistik, Erfahrung), Heer erobert
  unverteidigtes Hauptquartier (Niederlage, Kommandos abgelehnt, Uhr steht), Verteidiger schlagen
  ein Heer (Kriegsmüdigkeit, Erinnerung), Tor hält Feinde auf bis zum Bruch, Kompanien
  (Garnison 1 im Wachturm, übrige rundum, Patrouille läuft beide Wegpunkte ab, Umgruppieren,
  Auflösen, Entlassen), Feldzug mit Feldschlacht und Belagerung erzwingt Frieden, Forschungsboni,
  Rangaufstieg mit Ansehen, Entvölkerung, Ereignis-Aufmarsch, Bericht, Validierung. Daten-Tests
  für Einheitenklassen, Reichweiten, Ränge und Heeresmischung.
* Determinismus: `battles_continue_identically_after_loading` (Kaserne, Turm, Rekruten,
  Kompanie, Räuber und Kriegsheer; gespeichert nach 300 Ticks Kampf, gleiche Ereignisse und Bytes,
  gleich einem frischen Lauf).
* **Bilanz zwei Jahre** (`tests/campaign.rs`, Seeds 7, 11, 23, Small, Ereignisse an, KI-Reiche):
  der vernünftige Spieler (gemeinsamer Code in `tests/common`) forscht ab 30 Einwohnern die
  Bürgerwehr, baut Steinbruch, Sägewerk, Kaserne, Türme an unbewachten Lagern, hält eine Kompanie
  von bis zu 8 Milizen im Gebiet um das Rathaus, schickt bei Räuberüberfällen die Soldaten, hält
  Frieden (Pakte, Friedensangebote); der Test schickt an Tag 20 und 36 zusätzlich Banden. Ergebnis:
  keine Niederlage, 12 → 67/73/50 Einwohner, 8/7/4 Soldaten, jede Seed mindestens ein Überfall
  abgewehrt (4/4/3 Räuber gefallen, 0–1 eigene Verluste), niemand verhungert oder erfroren,
  Schatz nie im Minus (Räuber leeren ihn bei zwei Seeds kurz bis 0), Zufriedenheit 77–79; kein
  Krieg in zwei Jahren (Pakte). Die übrigen Überfälle (vor dem Aufbau der Wache oder außerhalb des
  bewachten Gebiets) kosten je 55–150 Waren und Gold. Die Schonfrist der KI (kein Krieg vor Tag 36) bleibt; der
  naive Spieler (10 Tage) und der vernünftige Spieler (ein Jahr) sind unverändert grün (gleiche
  Ereignisse wie vorher).
* Abstimmung aus der Bilanz: Räuber Angriff 3, Verteidigung 1, 60 TP, Bande 3 + 1 je 12 Bewohner
  (höchstens 12), 24 Runden Plündern, beladen 80 % Tempo; Miliz Angriff 4, Verteidigung 3, Moral
  50, 3 wood statt Werkzeug (Werkzeuge fehlen der frühen Wirtschaft); Kaserne 150 G, 25 stone,
  15 planks, Unterhalt 4. Gefundene Fehler dabei: Soldaten im Dienst konnten die Besorgungen
  ihres Haushalts blockieren (Hunger) – Dienst jetzt in den Arbeitsstunden, Besorgungen durch
  Nicht-Soldaten; Kriegswaisen verhungerten – Nachbarn versorgen sie.
* Performance (Release, `battle_performance`): 1946 Bewohner, bis 200 Kämpfer (100 eigene
  Schwertkämpfer und Bogenschützen gegen ein Heer von 100): Militär Ø 10,6 µs/Tick (max. 230 µs
  in Kampfrunden), ganzer Tick Ø 35 µs (max. 460 µs).
* Spiel-Binary gegen diesen Stand (Kopie des Hauptarbeitsbereichs mit Lane-`crates/sim` und
  `data/`): `cargo check -p kingdoms --all-targets` scheiterte nur an den erwarteten
  nicht erschöpfenden `match`es (5a: `LedgerCategory::Diplomacy` ×2, `CitizenRole::Cart`; neu:
  `DamageCause::Attack`, `Destination::Tile`, neue Rollen); mit den Korrekturen nur in der Kopie
  `cargo check` sauber und `cargo test -p kingdoms` grün (145).
* Keine Sichtprüfung: diese Lane hat keine Darstellung.

**Offen**
* Darstellung (Spiel-Lane): Militärfenster, Rekrutierung im Info-Fenster der Militärgebäude,
  Bedrohungen mit Ankunftszeit, Kampfberichte, Kriege und Feldzüge, Aufgaben-Panel mit Rang,
  Niederlage-Bildschirm, Feinde markiert (`CitizenView::hostile`), Sprites `siege_*` und
  `raider_*` fehlen im `characters`-Atlas, Klänge für die neuen Ereignisse.
* Feldzüge sind abstrakt (keine sichtbaren Heere außerhalb der Karte, KI-Sitze auf der Karte
  werden nicht betreten); KI-Reiche führen gegeneinander weiter abstrakte Kriege.
* Kein Verwundetenstatus über die Gesundheit hinaus; Belagerungsgerät-Mannschaft kämpft nicht;
  Moral wirkt auf Angriff und Rückzug, nicht auf Desertion.
* In zwei Jahren Bilanz kam es zu keinem KI-Krieg gegen den Spieler (Pakte); Verteidigung gegen
  Heere ist durch Unit-Tests belegt, nicht durch den Bilanztest.

## 2026-11-22 · Sim-Lane Schritt 5a: Königreiche, Diplomatie und Handelskarawanen (`crates/sim`, `data/`, Doku)

**Fertig**
* **KI-Königreiche** (neues Modul `diplomacy`, Daten `data/kingdoms.ron`, `rules.ron` →
  `diplomacy`): 3–5 Reiche nach Kartengröße (Klein 3, Mittel 4, Groß/Riesig 5; das Szenario
  begrenzt mit `rival_kingdoms`, Standard jetzt 5, 0 = keine Reiche) an den Siedlungsstellen des
  Weltgenerators weit vom Spieler (auf engen Karten werden die Abstände schrittweise gelockert; die
  Goldwerte der kleinen Karten bleiben gleich). Je Reich Name und Herrscher (zehn Vorlagen),
  Persönlichkeit (händlerisch/expansiv/friedlich/aggressiv), Farbe, Einwohner, Gold, Soldaten,
  Vorräte und ein Gebiet (Radius 9 + Einwohner/30, höchstens 22) in einer gespeicherten
  Land-Karte (`LandMap`: Besitzer und Bauwerk je Tile). Der Spieler kann dort nicht bauen
  (`PlacementError::ForeignKingdom`, „Dieses Land gehört einem anderen Königreich.“), sein Gebiet
  endet an fremden Grenzen.
* **Sichtbare Siedlungen:** Ortsplan aus Gebäudetypen (Burg am Sitz, Häuser, Markt, Kirche,
  Werkstätten innen, Farmen/Holzfäller/Bergwerke außen nahe ihrem Land, Wachtürme an der Grenze);
  bei Spielbeginn ein fertiger Ort, danach bis zu ein Bauwerk pro Tag. Bauwerke blockieren Läufer
  (Wegkosten-Raster mit Land-Karte), halten ein Tile Abstand, räumen Bäume. Gebiete wachsen mit den
  Einwohnern (nach 60 Tagen z. B. 158 → 216 Einwohner, Radius 14 → 16, 23 → 27 Bauwerke).
* **Abstrakte Wirtschaft:** Produktion aus Acker, Weide, Wald, Wasser und Vorkommen je Tile,
  Handwerk je 100 Einwohner nur mit passendem Land, Fernhandelswaren händlerischer Reiche,
  Winterabschlag, Verbrauch je Einwohner mit Nahrungsersatz, Lagergrenze; daraus Bedarf und
  Angebot, Preise nach §7, Wachstum nach Nahrung, Steuern und Sold.
* **Beziehungen** zum Spieler als Summe benannter Teile (Persönlichkeit, Verträge, Krieg,
  gemeinsamer Feind, Grenze) plus verblassende Erinnerungen mit deutschen Gründen; Zustände
  Feindlich/Rivalisierend/Neutral/Freundschaftlich/Verbündet (nur per Bündnis) mit Meldung beim
  Wechsel; Beziehungen und Kriege der Reiche untereinander (Gebietsabtretung, Entschädigung).
* **Diplomatie** (`Command::Diplomacy`): Handelsvertrag, Nichtangriffspakt, Bündnis vorschlagen
  oder kündigen (Vertragsbruch, Wortbruch bei den anderen), Geschenk, Tributforderung,
  Kriegserklärung, Friedensangebot (ohne Bedingungen, Gold, Entschädigung, Gebietsabtretung);
  sofortiges deterministisches Urteil mit Gründen, Aussicht im Bericht. **KI-Initiativen** mit
  Posteingang (`Command::AnswerProposal`, 18 h, danach abgelehnt): Vertragsvorschläge, Geschenke,
  Tribut, Kauf-/Verkaufsangebote, Grenzzwischenfälle, Kriegserklärung (Schonfrist bis Tag 36),
  Friedensangebote. Krieg: kein Handel, Überfälle auf unbewachte Lager alle 3 Tage (Beute an den
  Feind), Kriegsmüdigkeit (`HappinessFactor::War`), Verbündete stehen bei.
* **Handelskarawanen:** Händler und Karren (neue Rolle `CitizenRole::Cart`) laufen vom Kartenrand
  in Richtung ihres Reichs zum Kontor, handeln nach den Aufträgen, bleiben 2 h und gehen heim;
  eigene Karawanen (`Command::SendCaravan`) verkaufen beim Partner und kaufen Wunschwaren.
  Preise: Preis des Reichs ∓ 15 % Marge, Forschungsbonus, Beziehung, Vertragsbonus. Überfälle auf
  gefährlichen Routen geben Ladung und Börse an die Räuber. **Regel zum Weltmarkt:** mit
  Königreichen laufen alle Aufträge über Karawanen, nur ohne Königreiche handelt das Kontor
  stündlich mit dem abstrakten Weltmarkt.
* **Ereignis** `war_refugees` (Bedingung `WarNearby`), Kassenbuch-Kategorie „Diplomatie“.
* **Entwicklerbefehle** `DebugGrant`, `DebugSpawnHouseholds`, `DebugSetRelation`.
* **Texte:** Staatskasse „… für etwa 1 Tag“, „… nicht einmal mehr für 1 Tag“, leer: „Die
  Staatskasse ist leer.“; Forschung: „Alle Technologien sind erforscht.“
* **Berichte** (`report::diplomacy_report`, `kingdom_actions`, `pending_proposals`,
  `caravans_report`, `partner_prices`), Renderer-API (`Diplomacy::structures_in`, `land()`,
  `GameState::take_land_dirty_chunks`), Spielstandformat 10 mit Validierung.

**Geprüft**
* Neue Tests (Modul `diplomacy`): Lage und Land der Reiche, Bauverbot, Wachstum über 30 Tage,
  deterministische Urteile mit Gründen, Vertragslebenszyklus (Ablauf, Bruch, Pakt verhindert
  Krieg), Geschenke, fremde Karawanen mit Warenerhaltung jedes Ticks (Waren außer um
  Mitternacht, Gold außer zur vollen Stunde) und Speichern/Laden mitten im Lauf, eigene
  Karawanen, Krieg sperrt Handel, Überfälle, Raub auf gefährlicher Route, Initiativen über 40 Tage
  mit Schonfrist, Antworten auf Vorschläge, Entwicklerbefehle, Validierung; Land-Karte,
  Erinnerungen, Zustände, Ortsplan; Texte der Staatskasse und Forschung. Determinismus:
  `diplomacy_and_caravans_continue_identically_after_loading` (Kontor, Aufträge, Geschenk,
  Vertrag, eigene Karawane, gespeichert während eine Karawane läuft).
* Bilanztests (vernünftiger und naiver Spieler, ein Jahr) unverändert grün; der vernünftige
  Spieler behandelt Kriegsflüchtlinge wie Flüchtlinge. In den 25 Tagen gibt es durch die
  Schonfrist keinen Krieg gegen ihn.
* Performance (Release, 4 Tage, Kontor mit Aufträgen, alle Reiche freundlich): Mittel 4 Reiche
  Diplomatie Ø 0,33 µs/Tick (max. 208 µs um Mitternacht), Riesig 5 Reiche Ø 0,45 µs/Tick (max.
  294 µs), bis 5 Karawanen und 10 Karawanen-Läufer gleichzeitig.
* `cargo test -p kingdoms_sim` grün: Lib 324 + 6 ignoriert (Messungen), Determinismus 9, Bilanz 2,
  Doctest 1; `cargo clippy -p kingdoms_sim --all-targets -- -D warnings` sauber,
  `RUSTDOCFLAGS='-D warnings' cargo doc -p kingdoms_sim --no-deps` sauber.
* Spiel-Binary gegen diesen Stand (Kopie des Hauptarbeitsbereichs): `cargo check -p kingdoms
  --all-targets` braucht drei Ergänzungen in `src/` (Farbe und Tagesrate für
  `LedgerCategory::Diplomacy` in `ui/hud/management/finance.rs`, `CitizenRole::Cart` in
  `ROLES`/`role_index` von `view/citizens.rs`); damit kompiliert es und `cargo test -p kingdoms`
  ist grün (129). Keine Sichtprüfung im Spiel (Sim-Lane ohne Darstellung).

**Offen**
* Darstellung im Spiel (Game-Lane): Diplomatiefenster, KI-Orte (`structures_in`), Grenzen und
  Minikarte (`LandMap`), Karren-Sprite; drei kleine Anpassungen in `src/` nötig
  (`LedgerCategory::Diplomacy`, `CitizenRole::Cart`).
* Militär (Schritt 5b): Soldaten der Reiche sind noch eine Zahl; Überfälle prüfen nur die
  Sicherheitsabdeckung der Lager.

## 2026-11-22 · HUD nach Master-Spezifikation, Forschung, Ereignis-Dialog, Wirtschaft (`src/`, `tools/assetgen`, `assets/`, Doku)

**Fertig**
* **HUD-Anordnung nach §13** (alles passt auf 1280 × 720, verschoben statt verdoppelt):
  oben Bevölkerung, Gold, Nahrung, Holz, Stein, Eisen (Erz im Tooltip), bei Platz Bretter,
  Werkzeuge, Ziegel, Lehm, dann Zufriedenheit, Jahreszeit, Datum, rechts die zehn Knöpfe der
  Reichsverwaltung und das Menü; links eine senkrechte Leiste Bauen (Wohnen + Öffentlich),
  Straßen (mit Mauern/Toren), Landwirtschaft, Produktion, Militär – jeweils mit Bauleiste
  daneben (`build_menu`, ersetzt die untere Bauleiste; `B` öffnet die letzte Kategorie) – und
  Forschung; rechts Ereignisse, Meldungen, Übersicht (`right_bar`); unten Auswählen, Straße,
  Abreißen, Versetzen, Raster, Gebietsgrenze, Pause/1×/2×/4× und die Tageszeit als Sonnen-/
  Mondbogen (`bottom_bar`). Info-Fenster neben der rechten Leiste, Seitenfenster und
  Meldungskarten links davon, jeweils über Minikarte bzw. unterer Leiste endend. Kein Platz
  „Aufgaben“ (kommt mit den Herrscherrängen).
* **Forschung** (neuer Reiter, Taste U, Knopf links): Baum je Bereich mit
  Voraussetzungs-Kurven (`layout_tree`: Spalte = längste Kette, Ketten bleiben auf einer Zeile),
  Knoten mit Zustand (erforscht, in Arbeit, vorgemerkt mit Platz und Restzeit, verfügbar,
  gesperrt), Kosten, Fortschritt, Schild bei Wirkungen erst ab der Militär-Phase; Auswahl mit
  Beschreibung, Wirkungstexten der Simulation, Kosten, Dauer, Voraussetzungen, „Ermöglicht“;
  Einreihen per Doppelklick oder Knopf, gesperrte mit fehlenden Voraussetzungen davor
  (`queue_plan`); Forschungsliste mit nach vorn/hinten/entfernen (`MoveResearch`,
  `CancelResearch`), Quellen (Hauptquartier, Schulen), Punkte je Tag; wartende Kommandos im
  Pausenzustand sichtbar; Ablehnungen der Simulation als deutscher Hinweis.
* **Ereignis-Dialog:** Karte mit Titel, Schwere, Text, betroffenem Gebäude („zeigen“),
  Fristbalken („noch 7 Std. 30 Min. – danach gilt …“), je Wahl vorhergesagte Wirkungen oder der
  Grund, warum sie nicht geht; Klick → `ResolveEvent`. Öffnet sich bei jedem neuen Ereignis,
  „Später entscheiden“/Esc blendet aus, `E` oder der pulsierende Knopf „Ereignisse“ holt ihn
  zurück. Nach automatischem Anhalten für ein Ereignis läuft das Spiel nach der Entscheidung im
  vorherigen Tempo weiter. Fenster „Ereignisse“: offene Entscheidung, Wirkungen (Modifikatoren
  mit Restdauer, Summen, Steuerdeckel), Verlauf mit Wahl und Ergebnis.
* **Übersicht:** Bevölkerung (Veränderung heute), Zufriedenheit mit größter Last, Schatz und
  Tagesbilanz, Nahrung mit Reichweite, Forschung, Gefahren (offene Entscheidung, Bankrott/knapper
  Schatz, Nahrung < 3 Tage, Ruinen, negative Ereigniswirkungen, Steuerdeckel, Obdachlose,
  Unzufriedene, stillstehende Forschung), letzte Ereignisse – jede Zeile führt zum passenden
  Reiter, Gebäude oder Dialog. **Meldungen:** alle Benachrichtigungen der Sitzung (150), Filter,
  Zähler ungelesener, Klick → Ort.
* **Wirtschaft** (neuer Reiter, Taste O): Kennzahlen (Schatz, Tagesbilanz, Warenwert,
  Preisindex, Engpässe, Betriebe in Arbeit), Angebot und Nachfrage je Warengruppe, Engpässe und
  Überschüsse (Klick → Verlauf), Produktionsketten je Gebäudetyp mit fehlenden Eingangswaren
  (Klick → erster Betrieb mit Problem). Reiterleiste mit zehn Reitern: 15 pt, sonst 13 pt,
  sonst nur Symbole.
* **Ruinen:** `rubble_<B>x<T>` war bereits angebunden (geprüft); Brand-/Überschwemmungs-/
  Sturmschäden erscheinen mit Ort in Meldungen und Übersicht.
* **Audio:** Ereignis beginnt (`sfx_event_pending` + Effekt am Ort + Beraterstimme je
  Ereignis), entschieden (`sfx_event_resolved`), Schäden nach Ursache, Wiederaufbau, Seuche,
  Vorkommen, Forschung abgeschlossen; neue Manifest-Einträge mit deutschen Beschreibungen
  (`sfx_event_pending`, `sfx_event_resolved`, `sfx_flood`, `sfx_storm_damage`, `sfx_plague`,
  `sfx_raid`, `sfx_deposit_found`, `vo_event_decision`, `vo_flood`, `vo_storm_damage`,
  `vo_drought`, `vo_refugees`, `vo_festival`, `vo_unrest`, `vo_deposit_found`, `vo_rebuilt`).
* **Grafik:** neue Symbole `ui_inspect`, `ui_grid`, `ui_territory`, `ui_economy`,
  `ui_medicine`, `ui_administration` (assetgen, nur `icons.png`/`.ron` geändert).
* **Entwickler:** `KINGDOMS_DEV_EVENT=<ereignis>[:<wahl>]`, `KINGDOMS_DEV_RESEARCH=<tech>,…`,
  `KINGDOMS_PANEL=…`, `KINGDOMS_WINDOW` kennt `wirtschaft` und `forschung`; neue Tasten
  E (Ereignis), O (Wirtschaft), U (Forschung).

**Geprüft**
* Neue Tests: Baum-Layout (alle Verbindungen nach rechts, keine Überlappung, eine Bahn je
  Bereich, gerade Ketten, kleiner Baum, Zyklus endet), `queue_plan` (Bewässerung, Weinbau,
  schon eingereihte), Kurz-Dauern, Fristtexte, Dialog öffnet sich je Ereignis einmal und
  schließt ohne Ereignis, Standardwahl nach 12 Stunden, Fortsetzen nur nach der automatischen
  Ereignispause, Engpass-/Überschuss-Regeln, Summen und Preisindex je Warengruppe, Übersicht
  eines neuen Reichs, Tageszeitbogen, Meldungsliste und Zähler, Ereignis-Klänge am Ort und
  alle Klang-IDs im Manifest, `ui_*`-Symbole im Atlas, Entwicklerstart mit Forschung und
  Ereignis, neue Startvariablen.
* `cargo test --workspace` grün: Spiel 145, Engine 56, Assetgen 57, Sim 302 + 5 ignoriert,
  Determinismus 8, Bilanz 2, Doctest 1. `cargo clippy --workspace --all-targets -- -D warnings`
  sauber.
* Screenshots angesehen: neues HUD 1600 × 900 und 1280 × 720 (mit Info-Fenster, Bauleiste
  Produktion, Übersicht, Meldungen), Forschung 1600 × 900 und 1280 × 720 (mit Liste von sechs
  Technologien), Ereignis-Dialog Brand (1600 × 900) und Händlerbesuch mit nicht möglichen
  Wahlen (1280 × 720), Fenster „Ereignisse“ mit Verlauf, Ruinen nach dem Brand auf der Karte,
  Wirtschaft, Straßen-Bauleiste, Pausenmenü, Hauptmenü, Standardbefehl (`KINGDOMS_SEED=7`,
  Frame 30, startet direkt ins Spiel).
* Echte Eingabe (XTest, 1600 × 900): Leiste links Produktion → Bauleiste, Bauen → Wohnen und
  Öffentlich, Esc schließt sie; Forschung per Knopf und per U; Doppelklick auf Getreideanbau
  im Pausenzustand („Änderung wartet“), Leertaste → Forschung beginnt (60 Gold); Bewässerung →
  „Mit 2 Voraussetzungen einreihen“ (drei Technologien eingereiht); ▲ auf Bewässerung →
  Hinweis der Simulation „„Bewässerung“ kann nicht vor „Fruchtwechsel“ erforscht werden.“;
  ✕ auf Getreideanbau entfernt es samt abhängigen; Brand-Dialog → „Später entscheiden“ (Knopf
  pulsiert) → E holt ihn zurück → Klick auf „Feuerwehrleute bezahlen“ → −60 Gold, Försterhaus
  brennt, Verlauf im Fenster „Ereignisse“.

**Offen**
* Seuche und Räuberüberfall melden keinen Ort (die Simulation liefert keine Position); die
  Meldungen sind daher nicht anklickbar.
* Automatisches Anhalten mit Fortsetzen nach der Entscheidung nur per Unit-Test geprüft (ein
  zufälliges Ereignis lässt sich nach dem Start nicht gezielt auslösen).
* Bei 1280 × 720 mit Info-Fenster und Seitenfenster liegen die Meldungskarten über der
  Kartenmitte (sie verblassen nach ≈ 24 s).
* Wünsche an die Simulation: Position(en) bei `StockLost`/`CitizensFellIll` bzw. den
  Meldungen dazu; eine Kennzeichnung „wirkt ab der Militär-Phase“ je Wirkung (statt der
  Varianten-Liste im Spiel); ein Kommando, das eine Technologie samt Voraussetzungen einreiht.

## 2026-11-22 · Hauptmenü, Spielstände, Einstellungen, asynchrones Laden (`src/`, `crates/engine`, Doku)

**Fertig**
* **Start und Ladebildschirm:** Atlanten werden auf bis zu vier Worker-Threads dekodiert und je
  Frame mit 12 ms Budget hochgeladen (`AssetManager::start_loading_atlases`/
  `continue_loading_atlases`, Index-Reihenfolge bleibt → gleiche Sprite-IDs); Boot-Szene mit
  Ladebildschirm (Holzhintergrund, Titel, Stufen mit Haken/Kreisel/Wartekreis, Fortschritt,
  wechselnde Tastenhinweise nach aktueller Belegung). Neues Spiel, Entwicklerstart und
  Spielstände werden im Worker vorbereitet (`scenes::loading`), danach die Kartenansicht auf dem
  Main-Thread; Fehler deutsch mit „Zurück zum Hauptmenü“. Engine: `EngineContext::set_loading` –
  Ladeframes zählen nicht für `KINGDOMS_AUTOSHOT_FRAME`.
* **Hauptmenü** (Standardstart): lebende Entwicklerstadt (Seed 7, Klein, nach zwei Tagen, klares
  Wetter, eigene Session im Worker) als Hintergrund, Kamera driftet langsam, Einblendung über
  einem Holzhintergrund; Titel, Fortsetzen (neuester ladbarer Spielstand mit Name und Zeit),
  Neues Spiel, Spiel laden, Einstellungen, Credits, Beenden; Maus und Tastatur.
  `KINGDOMS_SEED` & Co. starten weiter direkt ein Spiel, `KINGDOMS_START=menu|game` erzwingt.
* **Neues Spiel:** Seed (Zahl oder Text → FNV-Hash, „Zufällig“ ohne Sim-Zufall), Kartengröße mit
  Feldzahl, Schwierigkeit mit ihrer Wirkung (Ereignishäufigkeit aus den Regeln), Zufallsereignisse,
  Szenario (Auswahl ab zwei Szenarien, sonst Name, Beschreibung, Siedler, Gold); Vorschau der
  Landschaft als Raute mit Startplatz aus `world::generate` im Worker (0,35 s nach der letzten
  Änderung, dieselben Parameter wie `GameState::new`), Wasser-, Wald- und Gebirgsanteil,
  Süßwasser und Vorkommen am Start.
* **Spielstände:** benannte Spielstände (Dateiname aus dem Namen, Umlaute umschrieben,
  reservierte Namen abgelehnt), Speichern-Dialog mit Überschreib-Rückfrage, Laden-Liste (neueste
  zuerst; Name, Ortszeit „Heute, 14:30“, Spieldatum, Einwohner, Gold, Gebäude, Szenario, Größe,
  Schwierigkeit, Seed, Spielzeit, Format, Version, Dateigröße; nicht ladbare mit deutschem Grund:
  anderes Format, andere Spieldaten, kein Spielstand), Löschen mit Rückfrage, Tastatur
  (↑/↓, Eingabe, Entf, Esc). Metadaten in `<name>.kmeta` (nur gültig, wenn Tick und Zeit zum
  Header passen). Sicheres Schreiben: `.tmp` + `sync_all`, alter Stand als `.bak`, Umbenennen,
  Verzeichnis-Sync, Zurücklesen; Reparatur unterbrochener Schreibvorgänge. Speichern im
  Hintergrund (`SaveWriter`: Zustand auf dem Main-Thread kopiert, < 1 ms; Kodieren/Schreiben im
  Worker), auch F5. Autospeichern alle N Minuten laufender Spielzeit oder N Spieltage in drei
  rotierende Plätze. Laden im Spiel im Hintergrund (angehalten, Meldung), Fehler als Hinweis.
* **Pausenmenü:** Fortsetzen, Speichern …, Laden …, Einstellungen, Hauptmenü, Beenden (Tastatur),
  Schnellspeichern/-laden, Stand „Nicht gespeichert: 3 Spielstunden“; Hauptmenü, Beenden und
  Fenster-X fragen bei ungespeichertem Fortschritt („Speichern …“ speichert und verlässt danach).
* **Einstellungen** (Hauptmenü und Pausenmenü, sofort wirksam, Speichern beim Schließen):
  Audio (Gesamt + fünf Kanäle, Stumm, Hinweis auf fehlendes Gerät/fehlende Dateien), Grafik
  (Vollbild, VSync, Bildratenbegrenzung, MSAA live – Engine `Renderer::set_msaa_samples`,
  UI-Größe beim Loslassen, Partikeldichte, Tag und Nacht), Spiel (Autospeichern, automatisch
  anhalten bei Ereignis-Entscheidung oder Gefahrenmeldung, Randscrollen, Kamerageschwindigkeit,
  Meldungsfilter), Steuerung (alle Aktionen mit deutschen Namen, je zwei Tasten auch mit
  Strg/Alt/Shift, Konfliktabfrage „F ist bereits „Finanzen“ zugeordnet …“, Rechtsklick entfernt,
  „Standard“ je Aktion, Überschneidungen rot). `settings.ron` (Spiel, Darstellung, Tasten) und
  `engine.ron` (Fenster, Grafik, Lautstärken), Standardwerte je Reiter. Engine:
  `Input::raw_keys_pressed` für die Tastenabfrage.
* **Credits:** Bibliotheken mit Version, Aufgabe und Lizenz laut Cargo-Metadaten, Schriften mit
  Lizenzdateien, Platzhalter-Grafik (assetgen), Audio-Stand (keine Dateien).
* Neue Abhängigkeiten des Spiels: `jiff` (Ortszeit, nur `tz-system`/`tzdb-zoneinfo`, keine neuen
  Pakete im Lockfile) und `thiserror`.

**Geprüft**
* Neue Tests: Spielstand-Namen/Dateistämme, Slot-Arten, sicheres Schreiben, Reparatur nach
  Absturz, Datums-/Spielzeittexte, Liste mit Metadaten und unverträglichen Spielständen
  (fremde Daten, Müll), Autospeicher-Rotation (5 Speicherungen → 1, 2, 3, 1, 2),
  Hintergrund-Schreiber, `a_loaded_game_continues_exactly` (Speichern → Laden → 400 Ticks
  byte-gleich), Einstellungen (Datei-Rundreise, Teil- und Fehldateien), Konflikte und Tastennamen,
  Neubelegung mit Konflikt + Zurücksetzen, Startmodus aus Variablen, Vorschau = Spielwelt,
  Seeds, Meldungsfilter, automatisches Anhalten, ungespeicherte Zeit mit „ist/sind“, Engine
  `atlas_loading_keeps_the_index_order`.
* `cargo test --workspace` grün: Spiel 129, Engine 56, Assetgen 57, Sim 302 + 5 ignoriert,
  Determinismus 8, Bilanz 2, Doctest 1. `cargo clippy --workspace --all-targets -- -D warnings`
  sauber, `RUSTDOCFLAGS='-D warnings' cargo doc -p kingdoms_engine --no-deps` sauber.
* Screenshots angesehen (1600 × 900 und 1280 × 720 mit passender Fenstergröße): Hauptmenü mit
  Stadt und vor dem Einblenden, Neues Spiel mit Vorschau (Mittel und Riesig), Ladebildschirm mit
  Stufen (Messstadt 2000), Ladefehler (beschädigter Spielstand), Einstellungen (alle vier Reiter,
  Tastenabfrage, Konfliktfrage, nach Übernahme), Credits, Laden-Liste mit fünf Spielständen
  (alte Version, fremde Daten, beschädigt, Schnellspielstand, benannt), Speichern-Dialog,
  Pausenmenü, Rückfrage beim Verlassen, Spiel nach dem Laden.
* Echte Eingabe (XTest) von Anfang bis Ende: Menü → Neues Spiel → Riesig → Start → Esc →
  Speichern (Eingabe) → Laden (Eingabe, im Hintergrund) → Hauptmenü → „Ohne Speichern verlassen“
  → Menü; Fortsetzen aus dem Menü; MSAA aus und UI-Größe 135 % im Spiel (sofort sichtbar, in
  `engine.ron` gespeichert); F als Taste für „Ansicht nach oben“ (Finanzen danach leer, in
  `settings.ron`). Autospeichern (1 Minute, 2×): `autosave_1` bei Tick 2396, `autosave_2` bei
  4798, Zustand kopiert in 0,5 ms, geschrieben in 9–29 ms. Riesige Welt in 288 ms erzeugt,
  Kartenansicht 35 ms; Atlanten 340–380 ms.
* Standardbefehl (`KINGDOMS_SEED=7`, Frame 30) startet weiter direkt ins Spiel (06:17, Rathaus).

**Offen**
* Die Boot-Ladeanzeige ist bei 0,35 s Atlaszeit kaum zu sehen und wurde nicht einzeln
  fotografiert (dieselbe Komponente ist beim Entwicklerstart geprüft).
* Vollbild unter Xvfb ohne Fenstermanager nicht prüfbar; Windows zeigt Ortszeiten ohne
  Zeitzonen-Datenbank (`jiff` ohne Bundle) in UTC.
* Wünsche an die Simulation: Einwohner, Schatz, Gebäudezahl, Reichsname/Rang und eine vom
  Aufrufer übergebene Spielzeit im `SaveSummary` (dann entfällt `.kmeta`);
  `GameSettings::generator_settings` für die Vorschau; Fortschritts-Rückruf der Weltgenerierung;
  Ereignis-Dialog bleibt offen (das automatische Anhalten verweist auf die Meldungen).

## 2026-11-22 · Review-Befunde: Brücken, Arbeitsplätze, Träger, Pfad-Warteschlange, Auswahl, Texte (`crates/sim`, `crates/engine`, `src/`, `data/`, Doku)

**Fertig**
* **Brücke unter Läufern** (`roads::remove_roads`): `RemoveRoad` lehnt eine Brücke ab, solange
  ein Bewohner oder Reisender darauf steht oder sie gerade betritt bzw. verlässt
  (`RejectReason::BridgeInUse`, „Auf der Brücke bei … ist gerade jemand unterwegs – sie kann erst
  abgerissen werden, wenn sie frei ist.“). Arbeitsplätze im Freien (Bauarbeiter, Sammler) liegen
  nie auf Brücken. Vorher stand der Läufer danach dauerhaft auf Wasser und jeder spätere
  Spielstand ließ sich nicht mehr laden.
* **Erreichbare Arbeitsplätze:** Bauarbeiter- und Feldplätze nur noch auf Tiles, die vom Eingang
  aus zu Fuß erreichbar sind (gleiche Zusammenhangskomponente); Farmen wählen nur erreichbare
  Feld-Tiles als Aufgabe und geben eine Aufgabe auf, zu der kein Weg mehr führt (eingeschlossene
  Felder bleiben brach). Bauern zählen nur als anwesend, wenn sie höchstens 2 Tiles von der
  Aufgabe entfernt stehen (kein „Säen aus der Ferne“); der n-te Bauer geht auf das n-nächste
  erreichbare Feld in diesem Umkreis. Die Komponenten werden in den Schritten 4 und 7 aktuell
  gehalten (`navigation::prepare`).
* **Träger blockieren ihr Lager nicht mehr:** Andere Zugänge (`storage::deposit`: Rodungsholz,
  Käufe, Rückerstattungen, Ereignisse) nutzen den von Trägern reservierten Platz erst, wenn kein
  Lager mehr freien Platz hat. Findet ein Träger sein Ziellager voll und kein anderes Lager mit
  Platz, stapelt er die Last dort über die Kapazität (`Inventory::add_beyond_capacity`, der
  Überschuss wird zuerst verbraucht); ohne erreichbares Lager mit Platz geht er in sein eigenes
  Lager. Warten mit Ware gibt es nur noch, wenn keines davon erreichbar ist.
* **Tätigkeitstext:** „Lädt ab (…)“ nur noch, wenn das Ziel die Ware nimmt; ein Träger vor einer
  vollen Werkstatt oder Baustelle zeigt „Wartet mit 8 Mehl auf Lagerplatz (Bäckerei)“.
* **Pfad-Warteschlange:** Das Knotenbudget je Tick wächst mit der Warteschlange
  (`rules.ron`: `path_budget_per_queued: 100`, `path_budget_max_per_tick: 25000`, Grundbudget
  10 000); deterministisch, weil die Warteschlange gespeichert ist.
* **Versetzen ohne Teleport:** Wer in einem versetzten oder vergrößerten Gebäude ist, tritt an
  der alten Tür heraus und läuft zur neuen (vorher sprang er im selben Tick an die neue Tür).
* **Auswahl durch Lücken:** Die Engine hält je Atlas eine Alpha-Maske (1 Bit je Pixel,
  `AssetManager::sprite_opaque_at`). Gebäude werden nur noch dort getroffen, wo ihr Sprite
  deckend ist (nicht in der Luft neben Giebeln oder zwischen Burgtürmen); ein Läufer gilt nur als
  verdeckt, wenn an der Stelle ein deckendes Gebäudepixel vor ihm liegt.
* **Pause ohne Sprung:** `Time::alpha` bleibt in der Pause auf seinem Wert (vorher 1,0), Läufer,
  Folgekamera und Weidetiere springen beim Anhalten und Fortsetzen nicht mehr.
* **Texte:** Einzahl/Mehrzahl über `format::count`/`years`: „1 Transport wartet …“, „Abstieg in
  1 Stunde“, „Bis zum Winter: 1 Tag“, „1 Jahr“ (Bürgerfenster, Tooltip), Lager-Tab-Tooltip;
  das Info-Fenster eines Lagers nennt wartende Transporte einmal, mit Alter („seit 15 h 36 min“,
  neue Sim-Funktion `report::minutes_text`, auch in „Wartet auf Träger: …“); Staatskassen-Warnung
  „für etwa einen Tag“.
* **Fensterlicht nach Sicht:** Nachtlichter prüfen nur Gebäude im Bild (vor `visual()`), die
  Anwesenden werden einmal je Tick in ein flaches Feld gezählt und nur, wenn etwas im Bild ist.
* **Produktion-Tab:** kein Zeilen-Tooltip mehr, der nur den Zustand wiederholt und den Namen der
  nächsten Zeile verdeckt; die Zustandszelle zeigt abgeschnittenen Text ganz und nur bei
  Zusatzinformation (Arbeitsweg bei Abwesenheit, „Kein Weg“, wartende Transporte) einen Tooltip.
* Entwicklerstadt: die Schafzucht kommt erst mit dem Sägewerk (die Startbretter bleiben für
  Obstgarten und Weizenfarm); mit den geänderten Bauernplätzen baute das Skript sonst die
  Schafzucht vor der Weizenfarm und hatte danach keine Bretter mehr. Ergebnis: 98 Bewohner nach
  zwei Jahren (vorher 89).

**Geprüft**
* Neue Regressionstests: `a_bridge_with_a_walker_on_it_is_not_removed` (inkl. Speichern/Laden),
  `people_inside_a_moved_building_walk_to_it` (jeden Tick ohne Sprung; `run_checked` erlaubt
  keine Sprünge im Gebäude mehr), `farmers_skip_field_tiles_they_cannot_reach` (1 und 3 Bauern),
  `a_carrier_without_room_anywhere_stacks_the_goods_in_its_target` (ersetzt den alten Wartetest),
  `a_full_storage_never_keeps_its_carrier_from_other_jobs` (Szenario des Reviews: Baustelle bekommt
  ihr Holz), `deposits_leave_room_claimed_by_carriers_to_them`,
  `a_carrier_at_a_full_workshop_says_it_waits_for_room`, `a_long_path_queue_gets_a_larger_budget_up_to_the_cap`,
  `durations_read_in_hours_and_minutes`, Engine `alpha_masks_mark_opaque_pixels`,
  `alpha_is_frozen_while_paused`, Spiel `counts_use_the_singular_for_one`,
  `row_tooltips_only_add_what_the_cell_does_not_say`.
* `walking_performance` (Release, 2036 Bewohner, 2 Tage): Wartezeit in der Pfad-Warteschlange
  Ø 114,5 → 39,0 Ticks, höchstens 412 → 166 Ticks (Test verlangt ≤ 1 Spielstunde); längster Tick
  3,6 → 3,9 ms (Rechner geteilt).
* `cargo test --workspace` grün: Spiel 106, Engine 55, Assetgen 57, Sim 302 + 5 ignoriert,
  Determinismus 8, Bilanz 2, Doctest 1. `cargo clippy --workspace --all-targets -- -D warnings`
  sauber, `RUSTDOCFLAGS='-D warnings' cargo doc -p kingdoms_sim --no-deps` sauber.
* Screenshots mit echter Eingabe (XTest) angesehen: Messstadt 2000, Zoom 2,5, Pause – Läufer
  hinter der Burg und hinter Stadthausdächern zeigen ihren Bürger-Tooltip mit Ring, ein Klick
  öffnet das Bürgerfenster; der Burgkörper selbst zeigt weiter „Burg“. Messstadt nachts (04:39,
  Zoom 0,5, Fensterlichter an): F3 „map draw“ 0,13 ms (max 0,17; im Review 0,26/0,31 mit
  Lichtern). Produktion-Tab der Entwicklerstadt: Namenszelle ohne Tooltip, Zustandszelle
  „Feierabend“ mit „Arbeitsweg Ø 41 min (8 % der Schicht)“. Start-Screenshot (Seed 7) ohne
  Auffälligkeiten; im Feed „Die Staatskasse reicht nur noch für etwa einen Tag.“

**Offen**
* Pfad-Warteschlange bei 2000 Bewohnern weiterhin spürbar (Ø 39 Ticks ≈ 12 Spielminuten, höchstens
  166 Ticks ≈ 50 min zur Pendelzeit): Cache-Treffer kosten weiter ihre volle Suche, damit das
  Timing nach dem Laden gleich bleibt. Weitere Hebel: Cache im Spielstand, schärfere Heuristik.
* Belebte Brücken: Der Abriss wird abgelehnt, solange jemand darauf ist; bei viel Verkehr muss der
  Spieler es erneut versuchen.
* Unerreichbare beanspruchte Feld-Tiles bleiben beansprucht (brach), bis wieder ein Weg hinführt.
* Lager können um höchstens die Last bereits unterwegs befindlicher Träger über ihrer Kapazität
  liegen, wenn nirgends Platz ist (Anzeige z. B. „308 / 300“).
* Die Zeile „… Transporte warten auf einen Träger dieses Lagers (seit …)“ und das Nichtspringen
  beim Pausieren sind nur per Test geprüft, nicht im Screenshot (zu den geprüften Zeiten wartete
  kein Transport; ein Pausen-Zeitpunkt lässt sich im Screenshot nicht abpassen).
* Das Fensterlicht zählt die Anwesenden weiterhin über alle Bewohner (einmal je Tick, flaches
  Feld); ein Index in der Simulation wäre der nächste Schritt.

## 2026-11-22 · Merge Schritt 4 – Sim-Lane Schritt 4 + Spiel-Lane Etappe 4 (`crates/sim`, `data/`, `src/`, Doku)

**Fertig**
* `crates/sim` und `data/` aus der Sim-Lane übernommen (keine neuen Abhängigkeiten, `Cargo.lock`
  unverändert); Doku zusammengeführt: `game_design.md` aus der Sim-Lane, `architecture.md` §3 aus
  der Sim-Lane und §2/§4 aus der Spiel-Lane, `roadmap.md` Phase 7 aus der Sim-Lane, beide neuen
  Einträge hier oben.
* Anpassungen in `src/` an die neue Sim-API:
  * Finanzen: Kassenbuch-Kategorien „Forschung“ und „Ereignisse“ mit eigenen Diagrammfarben
    (Magenta, Oliv), ohne laufende Tagesrate.
  * Info-Fenster: Baustellen-Titel „Wiederaufbau“ für `SitePurpose::Repair`; die
    Produktivitäts-Aufschlüsselung nennt den Ereignis-Modifikator („Ereignisse“, aus
    `ProductionReport::event_modifier_percent`). Produktionsliste: „Beschädigt · x %“.
  * Karte: beschädigte Gebäude zeigen `rubble_<B>x<T>`, solange der Wiederaufbau auf Stufe 0
    steht, danach die Baustellenstufen. Die Minikarte färbt Gebäude bei Fertigstellung,
    Beschädigung und Wiederaufbau sofort um (vorher blieb ein fertiges Gebäude bis zum nächsten
    Neuzeichnen des Chunks in Baustellenfarbe).
  * Ereignisse brauchen keine Oberfläche, um weiterzulaufen: Beginn und Ausgang erscheinen als
    Meldungen im Feed, ohne Entscheidung gilt nach der Frist die erste Wahl (neuer Test
    `undecided_events_expire_to_their_default_choice`).
  * Test `town_grows_through_two_years` (Entwicklerstadt) läuft ohne Zufallsereignisse
    (`GameSettings::with_events(false)`): das Skript entscheidet keine Ereignisse, und die
    Standardwahlen (Dürren, Plünderung) verzögerten die Weizenfarm; die Ereignisse prüfen die
    Bilanzläufe der Simulation.

**Geprüft**
* `cargo test --workspace` grün: Spiel 104, Engine 53, Assetgen 57, Sim 294 + 5 ignoriert,
  Determinismus 8, Bilanz 2, Doctest 1. `cargo clippy --workspace --all-targets -- -D warnings`
  sauber.
* Screenshots (Xvfb + lavapipe, Seed 7) angesehen: Start; `KINGDOMS_DEV_SETTLEMENT=1` mit 9 600
  Warp-Ticks (Gold, Waren, Siedlung ändern sich plausibel); Entwicklerstadt mit 35 500 Ticks
  (offene „Dürre“ im Feed, Spiel läuft weiter); Entwicklerstadt im Winter (94 400 Ticks, Schnee,
  beleuchtete Fenster); Ruinen nach einem ausgelösten Brand mit Info-Fenster „Beschädigt ·
  wartet auf Baumaterial“ / „Wiederaufbau“; Finanzen mit den neuen Kassenbuchzeilen. Laut
  Spielstand der Winterstadt hatte das Jahr eine Dürre und einen Händlerbesuch, beide nach
  Fristende mit der ersten Wahl entschieden. Keine Panics, keine WARN/ERROR außer XSETTINGS/XRandR
  und fehlendem Audiogerät.

**Offen**
* Forschungsfenster und Ereignis-Dialog fehlen noch (nächste Etappe); bis dahin steht im Feed
  „Entscheidung bis … Uhr“, obwohl der Spieler nicht wählen kann.
* Sim-Texte: „Die Staatskasse reicht nur noch für etwa 1 Tage.“ (Singular) und mit allen
  erforschten Technologien „Keine Forschung ausgewählt – die Forschungspunkte verfallen.“

## 2026-11-22 · Sim-Lane, Schritt 4 – Forschung mit Fortschritt und Ereignisse mit Entscheidungen (`crates/sim`, `data/`, Doku)

**Fertig**
* **Forschung** (`research`, §9): Forschungspunkte aus dem fertigen Hauptquartier (1 Punkt/h,
  unabhängig von der Besetzung) und aus Schulen (neues Datenfeld `research_points_per_hour`,
  Schule 2,0 bei voller Besetzung; anwesende Gelehrte, Gesundheit/Zufriedenheit, Schichtfaktor 3,
  im Bankrott halbiert); Gelehrtentum +25 %. Forschungsliste (höchstens 8) mit den Kommandos
  `QueueResearch`, `CancelResearch`, `MoveResearch`: Voraussetzungen erforscht oder weiter vorn,
  Gold beim Beginn an der Spitze (sonst Warten mit einer Warnung), Fortschritt ganzzahlig je Tick,
  Abbrechen behält Fortschritt und Bezahlung und nimmt abhängige Einträge mit, Umsortieren hält
  Voraussetzungen vorn. Abschluss wendet die Wirkungen sofort an (Lager, Gebiet, Zuteilungen) und
  meldet „Forschung abgeschlossen: … Neu verfügbar: …“. Tagesstatistik (48 Tage), Kassenbuch
  „Forschung“. Bisher wirkungslose Effekte sind jetzt echt: `ServiceRadiusBonus` (alle Dienste
  des Gebäudetyps), `ResearchSpeedBonus`, `FireRiskReduction`, `ToolWearReduction`; die
  militärischen Effekte sind als „wirkt ab der Militär-Phase“ gekennzeichnet.
  `DebugUnlockAllResearch` leert zusätzlich die Liste.
* **Bericht Forschungsbaum** `report::research_report`: je Technologie Bereich, Zustand
  (Erforscht/In Arbeit/Vorgemerkt/Verfügbar/Gesperrt mit fehlenden Voraussetzungen), Kosten,
  Punkte, Fortschritt, Restzeit (nach erwarteter Tagesleistung), deutsche Wirkungstexte; Liste mit
  voraussichtlichem Abschluss, Status- und Leistungstext, Tagesstatistik.
* **Ereignisse** (`events`, §12): tägliche Auslosung um 9:00 ab Tag 7 (35 % × Schwierigkeit,
  höchstens alle 2 Tage, eigener Zufallsgenerator im Spielstand), Bedingungen, Abklingzeiten,
  Gewichte mit Risiko (Brand: Anteil ungeschützter Holzgebäude × Dachforschung; Seuche:
  Resistenzforschung); offenes Ereignis mit 12 Stunden Frist, danach die erste Wahl;
  `ResolveEvent { serial, choice }` prüft die Voraussetzungen der Wahl mit deutschem Grund.
  Konsequenzen: Gold/Waren (Kassenbuch „Ereignisse“), befristete Modifikatoren für
  Zufriedenheit (Faktor „Ereignisse“), Produktion (Produktivität aller Betriebe und der
  Feldarbeit, `ProductionReport::event_modifier_percent`) und Feldertrag (Ernte), Wetter über den
  bestehenden Wetter-Weg (Dürre, Regen bei Überschwemmung), Flüchtlinge zu Fuß vom Kartenrand
  (Familien passend zu freien Plätzen), Krankheit (40 Gesundheitspunkte, mit Krankenhaus 10),
  Plünderung nur unbewachter Lager (Anknüpfung fürs Militär: `events::raid_protected`), neue
  Vorkommen (Kachel-Änderung mit Dirty-Chunks), Steuerdeckel (`SetTaxRate` abgelehnt),
  Gebäudeschaden und zerstörte Saat. Neues 13. Ereignis „Sturmschäden“ (offen aus Phase 6) bei
  Sturm ab 50 %. Entwicklerbefehl `DebugTriggerEvent`.
* **Schadensmodell:** Brand, Überschwemmung und Sturm machen ein Gebäude zur Ruine – einer
  Wiederaufbau-Baustelle (`SitePurpose::Repair`, `BuildingStatus::Damaged`, „Beschädigt“) mit
  50 % Bauzeit und 50 % Material, ohne Gold; Bauarbeiter und Träger übernehmen sie, bis Stufe 1
  liefert `Building::sprite` `buildings/rubble_<B>x<T>` (Grafik vorhanden). Ohne Funktion bis
  zum Wiederaufbau; Warenverlust Brand 100 %, Überschwemmung 50 %, Sturm 0 %; Abriss möglich,
  Abbrechen nicht; Brand springt nur auf ungeschützte Holzgebäude bis 4 Felder über.
* **Berichte Ereignisse:** `report::pending_event` (Titel, Text, Frist, Wahlmöglichkeiten mit
  Verfügbarkeit/Grund und vorhergesagten Wirkungen auf Deutsch), `report::events_report`
  (Modifikatoren mit Restdauer, Summen, Steuerdeckel, Verlauf der letzten 32).
* **Einstellungen:** `GameSettings::events_enabled` (Standard an, `with_events(false)`); nur die
  Unit-Test-Vorlage `flat_state` schaltet die Auslosung ab. Beide Balance-Spieler laufen mit
  Ereignissen.
* Spielstandformat 9 (min. 9); `validate` prüft Forschung und Ereignisse, `rederive` repariert
  die Reihenfolge der Forschungsliste und kürzt den Ereignisverlauf nach Datenänderungen.
  Pipeline-Schritt 9 = Forschung, dann Ereignisse. Doku: game_design §5.4, §6, §7, §8.2, §9,
  §12; architecture §3; roadmap Phase 7.

**Geprüft**
* `cargo test -p kingdoms_sim` grün; neue Tests: Forschung 12 (Liste, Voraussetzungen, Gold,
  Fortschritt auf den Tick genau, Warten auf Gold, Abbrechen/Fortsetzen, Umsortieren, Schulen,
  Statistik, Wirkungen, Validierung/Reparatur), Ereignisse 21 + 1 ignorierte Leistungsmessung (jedes Ereignis mit und ohne seine
  Bedingungen, jede Wahl jedes Ereignisses mit ihren Wirkungen, Abklingzeit, tägliche
  Auslosung mit Starttag/Abstand/Schalter, Frist mit Standardwahl, Kommandoprüfungen,
  Brandausbreitung und Wiederaufbau, Ruinen, Seuche mit Krankenhaus, Plünderung bewacht/unbewacht,
  Flüchtlinge, Festmodifikator in der Aufschlüsselung, Steuerdeckel, Vorkommen, Sturm, Berichte,
  Validierung); Determinismus 8 (neu: Forschung + offenes Ereignis + Modifikator über Speichern
  und Laden, gleiche Bytes von vorn).
* Balance mit Ereignissen: vernünftiger Spieler (Seed 7) entscheidet mit kleiner Tabelle und
  250 Gold Reserve; im Jahr Dürre (Tag 8), Händler, Brand, alle Prüfungen unverändert grün; naiver
  Spieler (Standardwahlen) grün. Abgestimmt: Ereignisse erst ab Tag 7 (statt 4) und erzwungenes
  Wetter mit 60 % (statt 80 %) – mit den ersten Werten trafen Überschwemmungen am 4. Tag die
  junge Siedlung und starke Dürren die zweite Aussaat (Seeds 1, 11, 42 scheiterten). Optionale
  Seeds mit Ereignissen danach: 1, 3, 7, 11 grün; 42 scheitert an der zweiten Gemüseernte (Dürre am
  8. Tag während der Sommeraussaat); 2 und 5 scheiterten schon ohne Ereignisse (Brennholz).
* `cargo clippy -p kingdoms_sim --all-targets -- -D warnings` sauber;
  `RUSTDOCFLAGS='-D warnings' cargo doc -p kingdoms_sim --no-deps` sauber.
* Spiel gegen die neue Simulation in einer Kopie im Scratch-Verzeichnis: `cargo check --workspace
  --all-targets` braucht nur drei neue Match-Arme in `src/` (siehe Offen); damit
  `cargo clippy --workspace --all-targets -- -D warnings` sauber und `cargo test --workspace` grün
  bis auf `town_grows_through_two_years` (102 von 103 im Spiel): mit Ereignissen (zwei Dürren,
  Räuber im Winter, alle Standardwahlen) steht die Weizenfarm am Tag 49 noch im Bau („vegetable,
  wheat and fruit farms have fields“). Mit `GameSettings::new(…).with_events(false)` im Test grün.
* Leistung (Release, `progression_performance`: 902 Gebäude, ~1400 Bewohner, Ereignisse an,
  Forschungsliste): Forschungsschritt Ø 6,5 µs je Tick (höchstens 0,4 ms beim Abschluss einer
  Technologie), Ereignisschritt Ø 0,14 µs; eine vollständige Auslosung mit allen Bedingungen
  0,3 ms (einmal am Tag, nur wenn der Würfel ein Ereignis zulässt); Forschungs- und
  Ereignisbericht zusammen 39 µs; ganzer Tick mit einer Entscheidung 0,13–0,7 ms (Flüchtlinge am
  teuersten).

**Offen**
* Spiel-Lane: drei Match-Arme anpassen – `management/finance.rs` (`category_color`,
  `daily_rate`: `LedgerCategory::Research`, `LedgerCategory::Events`) und `info_panel.rs`
  (`SitePurpose::Repair` → „Wiederaufbau“). Der Entwickler-Stadt-Test braucht entweder
  `.with_events(false)` (geskriptete Wirtschaftsprüfung) oder Entscheidungen im Skript. Darstellung fehlt noch: Forschungsfenster,
  Ereignis-Dialog (am besten mit Pause), Ruinen (`rubble_*` bis Stufe 1), Modifikatoren in
  Produktivitäts- und Zufriedenheitsaufschlüsselung, Kassenbuch-Kategorien.
* Militärische Forschungswirkungen (`UnlockUnit`, `UnitBonus`, `WallStrengthBonus`) und die
  Soldaten-Wahlmöglichkeiten warten auf Phase 7 Militär; „Soldaten“ sind bis dahin Bewohner mit
  Arbeitsplatz in fertigen Militärgebäuden.
* Ereignis-Gewichte und Stärken sind erste Werte. Die Wahl „Felder bewässern“ bei der Dürre
  gibt nur +25 % Feldertrag für 4 Tage und hilft der gerade gesäten Sommerfrucht kaum (Seed 42);
  eine echte Bewässerungswirkung (Bodenfeuchte) wäre die bessere Gegenmaßnahme.

## 2026-11-22 · Spiel-Lane, Etappe 4 – Bewohner auf der Karte, Bürgerfenster, Tag und Nacht (`src/`, Engine, Assetgen, Doku)

**Fertig**
* **Läufer auf der Karte** (`src/view/citizens.rs`): Bewohner, Träger, Bauarbeiter, Bauern, Händler,
  Kinder und abwandernde Reisende aus `GameState::citizens_in` für den Sichtbereich (räumlicher Index
  der Simulation, danach Bildschirm-Culling); zwischen `prev_pos` und `pos` mit `Time::alpha`
  interpoliert (Pause: steht), Laufanimation `characters/<rolle>_<richtung>_<0..3>` nach dem
  zurückgelegten Weg (8 Bilder/s bei Grundtempo), im Stand Bild 1; Höhe der Tiles mit weichem
  Übergang an Stufen (0,35 Tiles), auf Brücken auf dem Deck; Tiefe am Fußpunkt (Häuser und Bäume
  davor verdecken, davor verdeckt der Läufer), auf Brücken vor dem Geländer. Getragene Ware als
  Warensymbol in der Hand (Vorderansicht vor, Rückansicht hinter dem Körper). LOD unter Zoom 0,45:
  Standbild ohne Ware. Weidetiere bleiben reine Darstellung.
* **Anklicken und Bürgerfenster** (`src/ui/hud/citizen_window.rs`): ein Läufer unter der Maus hat im
  Prüfwerkzeug Vorrang vor Gebäude und Boden, wenn kein Gebäude vor ihm steht (Tiefenvergleich),
  nie vor der Oberfläche; Tooltip mit Name, Alter, Tätigkeit, Ware; Ring unter den Füßen.
  Das Fenster zeigt `report::citizen_detail`: Name, Geschlecht, Alter, Lebensabschnitt, Tätigkeit
  (deutscher Text), Ziel, Aufenthaltsort, getragene Ware, Haushalt, Zuhause, Ehepartner, Eltern,
  Kinder (Links), Arbeit, letzter Arbeitsweg, Arbeitskraft, Bildung, Gesundheit, Zufriedenheit
  mit Faktoren, Bedürfnisse. Knöpfe Folgen (Kamera auf der interpolierten Position; Verschieben,
  Esc, Schließen oder Kamerasprung beenden es, Zoomen nicht), Zeigen, Zuhause, Arbeitsplatz.
  Esc-Reihenfolge: Werkzeugschritt, Folgen, Reichsverwaltung, Bürgerfenster, Auswahl, Menü.
  Bewohnerzeilen im Info-Fenster der Häuser und eine neue Bürgerliste im Reiter Bevölkerung
  (Filter Alle/Unzufrieden/Ohne Arbeit/Obdachlos/Ohne Weg/Unterwegs, Namenssuche) öffnen es.
* **Tag und Nacht** (`src/view/daylight.rs`): Licht nach Tageszeit (Nacht → Morgendämmerung →
  Morgenlicht → Tag → Abendsonne → Abenddämmerung → Nacht, weich), Tageslänge je Jahreszeit
  (Frühling 5:30–20:00 … Winter 7:45–16:30), Wechsel um Mitternacht ohne Sprung; multipliziert das
  Wetterlicht, nur die Welt (Oberfläche und emissive Sprites nicht), Partikel werden mitgetönt.
* **Beleuchtete Fenster** (`src/view/lights.rs`, neuer Atlas `lights`): Assetgen erfasst die
  Fensterscheiben beim Zeichnen (`IsoCanvas::window`/`pane`) und schreibt je Gebäude mit Fenstern
  (29) ein zugeschnittenes Nachtlicht mit Schein und Gebäudeanker; die übrigen Atlanten sind
  byte-gleich. Im Spiel emissiv mit der Tiefe der Wände darunter (neu in der Engine:
  `SpriteDepth::sub_span`), wenn Menschen drinnen sind oder produziert wird; Häuser gehen zwischen
  22 Uhr und Mitternacht schlafen (jedes zehnte bleibt wach).
* **Klang:** Umgebungsebenen nach dem Tageslicht (Vögel/Grillen, nachts leiser Wald und Stadt) und
  neu nach dem Wetter (`amb_rain`, `amb_storm`), Musikstimmung Nacht mit Hysterese, sonst Winter
  bzw. Friedlich.
* **Neue Simulationszustände in der Oberfläche:** „Kein Weg zur Arbeit“ (rot, auch als Problem in
  der Reichsverwaltung), Arbeitsweg (`ProductionReport::commute`, Einzelheiten im Tooltip),
  „Wartet auf Träger …“ (`carrier_wait`), Wegprobleme (`path_problem`) bei Betrieben, Baustellen,
  Lagern und Häusern, Bauarbeiter an der Baustelle, Träger unterwegs/frei und wartende Transporte
  der Lager, Logistikzeile im Reiter Lager & Waren, Erklärung der Rund-um-die-Uhr-Rezeptdauern
  (Schichtfaktor). Die Meldungen der Simulation zum Laufen („Kein Weg …“, „Keine Träger …“,
  „Kein Lagerplatz …“) kommen mit deutschem Text, Schwere und Ort und brauchten keine Änderung.
* **Entwicklerhilfen:** `KINGDOMS_SPEED=pause|1|2|4` (Starttempo); `KINGDOMS_DEV_CROWD=<n>` –
  Messstadt mit `n` Siedlern als Szenario im Speicher (`Defs::from_files`), dichte Stadt um eine
  Burg. Debug-Fenster: Zeitmessung „citizens“, Abschnitt „Bewohner & Tageszeit“.
* Doku: README (Steuerung, Optionen, Beispiele), `architecture.md` §2/§4, `assets.md` (`lights`,
  Läufer), Roadmap (Phase 5 ✅, Tag-Nacht ✅).

**Geprüft**
* `cargo test --workspace` grün (Spiel 103, Engine 53, Assetgen 57, Simulation 262 + 4 ignoriert,
  Determinismus 7, Balance 2, Doctest 1); `cargo clippy --workspace --all-targets -- -D warnings`
  sauber. Neue Tests u. a. Interpolation, Richtungen, Laufbild, Höhenübergang, Picking,
  Tageslichtkurve (Stetigkeit je Minute, Monotonie morgens/abends, Jahreszeiten), Fensterlicht,
  `sub_span`, Nachtlicht über Glasscheiben, Klang (Tag/Nacht/Wetter, Musik-Hysterese),
  Bürgerliste, Bürgerfenster-Texte, Läufer-Klick im Werkzeug, Messstadt.
* Unter Xvfb + lavapipe angesehen (Seed 7): Straßenszene Zoom 2 (Messstadt 18:04, Laufbilder,
  Verdeckung an Häusern und Büschen; Entwickler-Stadt mit Träger und Holz in der Hand); Hover und
  Klick auf einen Läufer vor einem Haus (Tooltip, Ring, Bürgerfenster) mit echten Mausereignissen
  (XTest); Folgen (Kamera bleibt am Träger, auch hinter einem Baum und an der Rathaustür), Esc
  beendet das Folgen, Pfeiltaste verschiebt; Info-Fenster Haus (Bewohner-Links) und Holzfäller
  (Arbeitsweg Ø 26 min), Bürgerliste; 05:30, 06:30, 12:00, 20:00, 20:45, 23:00 im Frühling und
  Winternacht 22:00 mit Schnee, Schneefall und leuchtenden Fenstern.
* Bildzeit mit 2000 Bewohnern (`KINGDOMS_DEV_CROWD=2000`, 18:11, Besorgungsgang): Zoom 0,5 –
  808 Läufer gezeichnet (761 unterwegs), „citizens“ Ø 0,07 ms (max 0,22), Renderliste 0,73 ms,
  Simulation 2,06 ms je Frame (4 Ticks); Zoom 0,3 (LOD) – 882 Läufer, Ø 0,06 ms (max 0,18), Rendern
  181 ms gegenüber 170 ms um 03:00 ohne Läufer (lavapipe, Software-Rasterung, ≈ 7 FPS); Zoom 2 –
  555 abgefragt, 235 gezeichnet, Ø 0,12 ms. Keine Panics, keine WARN/ERROR außer XSETTINGS/XRandR,
  fehlendem Audiogerät und den Startplatz-Warnungen der Messstadt.

**Offen**
* Mehr als ≈ 900 gleichzeitig sichtbare Läufer wurden nicht gemessen: die Messstadt hat 2000
  Bewohner, aber nur ≈ 140 Arbeitsplätze; auf Seed 7 (mittlere Karte) fanden 26 Startgebäude
  keinen Platz (2000 Bewohner, 1800 Wohnplätze). Messungen nur mit Software-Vulkan.
* In manchen Gebäude-Sprites (Rathaus, Marktplatz, Farmen, Militär) sind Figuren gemalt; sie sehen
  wie Bewohner aus, sind aber Dekoration (nicht anklickbar). Entfernen hieße bestehende Grafik zu
  ändern – Vorschlag für eine Grafik-Überarbeitung.
* Ein eigenes Standbild gibt es im Atlas nicht (Bild 1 dient als Stand). Klänge nur über Tests
  geprüft (kein Audiogerät, keine Audiodateien).
* Meldungstext der Simulation: „Die Staatskasse reicht nur noch für etwa 1 Tage.“ (Einzahl fehlt).

## 2026-11-22 · Merge Schritt 3 – Sim-Lane Schritte 3a/3b + Spiel-Lane Etappen 3a/3b (`crates/sim`, `data/`, `src/`, Doku)

**Fertig**
* `crates/sim` und `data/` aus der Sim-Lane übernommen (`sync.sh to-main`); die Spiel-Lane hatte
  beides nicht verändert. Keine neuen Abhängigkeiten, `Cargo.lock` unverändert.
* Doku zusammengeführt: `game_design.md` aus der Sim-Lane, `architecture.md` §3 aus der Sim-Lane
  und die übrigen Abschnitte aus der Spiel-Lane, `assets.md` aus der Spiel-Lane, `roadmap.md`
  kombiniert, die vier Lane-Einträge unten in `progress.md` (neueste zuerst).
* Anpassungen in `src/` an die neue Simulation (der Code kompilierte ohne Änderung):
  * Die neuen Produktionszustände „Arbeiter auf dem Weg“, „Mittagspause“ und „Feierabend“
    erscheinen im Info-Fenster und in der Reichsverwaltung (Produktion, Filter „Nur Betriebe mit
    Problemen“) neutral statt rot als Problem.
  * Info-Fenster, Zeile „Belegschaft“: `staffing_permille` zählt jetzt nur Arbeiter an ihrem
    Arbeitsplatz; die Zeile nennt deshalb anwesende und zugeteilte Arbeiter, z. B.
    „100 % (2 vor Ort, 2 von 2 besetzt)“.
  * Baustaub nur, solange Bauarbeiter an der Baustelle stehen (`population::present_workers`),
    nicht mehr auch nachts bei bloß zugeteilten Bauarbeitern.
  * Entwickler-Stadt (`KINGDOMS_DEV_SETTLEMENT=2`): Holzfäller und Förster dürfen die
    Brennholzreserve für den Winter angreifen (sie bringen das Brennholz). Vorher standen im
    Herbst Erwachsene frei, während die Reserve neue Holzfäller sperrte, und das Holz war am
    Winterende aufgebraucht. Mit laufenden Arbeitern und Trägern kommen Weizenfarm, Steinbruch,
    Sägewerk, Köhlerei und Schafzucht erst im zweiten Jahr; der Test heißt deshalb
    `town_grows_through_two_years` und läuft 49 Tage (ca. 7 s) mit unveränderten Prüfungen.
    README und Moduldoku angepasst.

**Geprüft**
* `cargo build` ohne Warnungen; `cargo test --workspace` grün (Spiel 83, Engine 52, Assetgen 56,
  Simulation 262 + 4 ignoriert, Determinismus 7, Balance 2, Doctest 1);
  `cargo clippy --workspace --all-targets -- -D warnings` sauber.
* Entwickler-Stadt im Test (kleine Karte, Seed 7): 12 → 51 Bewohner im ersten Jahr, 89 nach zwei
  Jahren, keine Todesfälle, keine abgelehnten Kommandos, alle Zielgebäude außer dem Lagerhaus.
* Unter Xvfb + lavapipe (Seed 7) angesehen: Startzustand; Test-Siedlung (`=1`) nach 9600 Ticks
  (Produktion: „Feierabend“ neutral, Sägewerke „Rohstoffmangel“ rot), nach 10 600 Ticks
  (Holzfäller „In Arbeit“, „Belegschaft 100 % (2 vor Ort, 2 von 2 besetzt)“) und nach 88 800 Ticks
  (Wintertag 1, Schnee, Rauch); Entwickler-Stadt (`=2`, mittlere Karte) nach 174 000 Ticks
  (Herbst Jahr 2, 64 Bewohner, „Mittagspause“ neutral), 223 200 Ticks (Winter Jahr 2) und
  232 800 Ticks (Frühling Jahr 3, 69 Bewohner, keine Todesfälle); Schnellspeichern und -laden im
  Format 8 nach 60 000 Ticks. Keine Panics, keine WARN/ERROR außer XSETTINGS/XRandR und fehlendem
  Audiogerät.

**Offen**
* Läufer (Bewohner, Träger mit Ware, Bauarbeiter, Bauern) zeichnet das Spiel noch nicht; die
  Sim-API (`citizens_in`, `CitizenView`) steht bereit. Ebenfalls noch nicht angezeigt: Arbeitsweg
  (`commute`), Wartezeit auf Träger (`carrier_wait`), Wegprobleme der Häuser (`path_problem`),
  getragene Waren und die neuen Aktivitätstexte.
* Rezeptdauern im Info-Fenster sind Rund-um-die-Uhr-Stunden (Schichtfaktor 3) ohne Erklärung.
* Entwickler-Stadt auf der mittleren Karte: ab Herbst Jahr 1 meist 0 Holz; im zweiten Jahr
  stockt sie bei etwa 69 Bewohnern („Brennstoffmangel“, Förster ohne freie Fläche, der fünfte
  Holzfäller wartet auf Holz, 9–10 Erwachsene arbeitslos), bisher ohne Todesfälle.
* Spielstände älterer Formate (≤ 7) werden abgewiesen (Format 8).

## 2026-11-21 · Sim-Lane, Schritt 3b – Arbeit geschieht dort, wo die Arbeiter sind: Träger, Bauarbeiter, Bauern und Sammler laufen (`crates/sim`, `data/`, Doku)

**Fertig**
* **Träger als Läufer** (`logistics::carriers`, ersetzt die abstrakten Laufzeit-Transfers): Planer
  und Auftragswarteschlange bleiben (Prioritäten, Ansprüche, Grundbesetzung der Lager). Ein Träger,
  der in seiner Arbeitszeit außerhalb der Mittagspause im Lager ist, übernimmt den wichtigsten
  wartenden Auftrag seines Lagers, geht zur Quelle, lädt 10 Ticks – erst dann verlassen die Waren
  die Quelle –, trägt sie zum Ziel, lädt 10 Ticks ab – erst dann kommen sie an – und geht zurück.
  Auftragsstufen `Waiting` / `Fetching` / `Carrying` mit Träger; die Waren unterwegs liegen beim
  Bewohner (`Citizen::carry` → `CarryTrip`), sonst nirgends. Gleiche wartende Aufträge werden bis
  zur Traglast zusammengelegt, Rückfracht bis 6 Tiles vom Abladeort wird gleich mitgenommen, ein
  Lager nimmt höchstens 4 wartende Aufträge je Träger an (sonst nächstes Lager), ein begonnener
  Gang endet auch nach Feierabend. Baustellen werden genauso beliefert.
* **Keine Ware geht verloren oder doppelt:** Ziel abgerissen, voll, nimmt nicht mehr an oder
  unerreichbar → nächstes erreichbares Lager mit Platz (Auftrag wird `Return`), ohne eines wartet
  der Träger und versucht es stündlich; wer seine Stelle verliert, liefert erst ab; Tod oder
  Abwanderung → Übergabe an Ziel, eigenes Lager oder nächstes Lager mit Platz (nur was nirgends
  Platz hat, geht verloren und wird gemeldet); Quelle weg/unerreichbar vor dem Abholen → Auftrag
  entfällt. `validate` prüft Träger ↔ Auftrag ↔ Ladung (Ware und Menge).
* **Traglast** `carrier_capacity` 8 → 12 (`rules.ron`, §7): mit Laufwegen und nur noch Schichtbetrieb
  der Träger reichten 8 Einheiten je Gang nicht mehr für die Ernte in den Scheunen.
* **Sammler gehen zum Ziel** (§5.2): Holzfäller, Förster, Steinbruch, Lehm- und Sandgrube, Fischerei
  wählen nur zu Fuß erreichbare Ziele und arbeiten auf dem Arbeitsplatz daneben (Ziel-Tile oder
  erstes begehbares Nachbar-Tile, Fischer am Ufer); der Arbeitsgang läuft nur mit Arbeitern dort.
  Der Ertrag wird als `TransportKind::Haul` vom ersten Arbeiter zur Hütte getragen (Platz im
  Ausgangslager reserviert), die anderen gehen gleich zum nächsten Ziel; verschwindet das Ziel,
  zieht der Arbeitsgang mit Fortschritt um. Minen, Viehzucht, Hühnerhof, Imkerei arbeiten drinnen.
  Holzfäller-Arbeitsgang 2 h → 1,5 h (§5.2), damit Wege und Heimtragen die Holzmenge nicht
  einbrechen lassen.
* **Bauarbeiter** stehen auf den begehbaren Tiles rund um die Baustelle (sichtbar, vom Eingang aus
  verteilt) und bauen nur dort; **Bauern** säen und ernten nur auf Feld-Tiles ihrer Farm (sie
  folgen der Aufgabe, bleiben bis 2 Tiles Abstand stehen; ohne Aufgabe warten sie im Hof); der
  längere Sommer-/kürzere Winterarbeitstag der Bauern galt schon seit 3a. Neue
  `Destination::Spot`, `population::work_area` / `WorkArea`, `on_duty_workers`.
* **Besorgungsgang statt stündlichem Auffüllen** (§6, §6.1): der Haushaltsvorrat wird beim Eintreffen
  des Läufers am Markt aufgefüllt (ohne Markt im Lager, dessen Trägerradius das Haus erreicht;
  Obdachlose weiter stündlich am Hauptquartier); ist der Vorrat ohne Nahrung, geht der Läufer
  sofort, sobald er nicht arbeitet und es dort Nahrung gibt. Vorrat 1 → 1,5 Tage (`pantry_days`),
  Brennstoff schon am letzten Herbsttag, „Nahrungsmangel“ nur, wenn auch Markt/Lager nichts haben.
* **Arbeitsweg** zählt nur noch von zu Hause bis zur ersten Ankunft am Arbeitsort (Wege zwischen
  Arbeitsplätzen, Abholungen, Lieferungen nicht).
* **Berichte/API:** `CitizenView::carried` gefüllt (Träger mit Ware, Sammler mit Ertrag), Rolle
  `carrier` für jeden, der für ein Lager trägt; `LogisticsReport` mit holen ab / liefern / zurück /
  frei / Einheiten unterwegs / Heimtragen / Ø Transportdauer / Anzahl; `carrier_wait` („Wartet auf
  Träger: 2 Transporte (12 Einheiten) seit 35 min“, auch in `ProductionReport`);
  `StorageReport::{idle_carriers, waiting_jobs}`; `CitizenDetail::carried`; Tätigkeitstexte „Holt
  Holz (Holzfällerhütte)“, „Bringt 8 Holz (Lagerhaus)“, „Lädt ab (…)“, „Wartet mit 8 Holz auf
  Lagerplatz“; `Logistics::{goods_in_transit, average_trip_ticks, trips_completed}`.
* **Leistung:** Bestandsansicht einmal je Logistikschritt statt je Abholung (vorher Spitze 23 ms,
  wenn nach der Mittagspause 160 Träger gleichzeitig luden), Zuteilung nur für Lager mit freiem
  Träger, verwaiste Aufträge nur im Planungstakt verworfen.
* Spielstand-Format 8; Doku: `game_design.md` §5, §5.2, §5.3, §6, §6.1, §7; `architecture.md`
  §3.2–3.5; Roadmap Phase 4/5.

**Geprüft**
* `cargo test -p kingdoms_sim` grün (Simulation 262 + 4 ignoriert, Determinismus 7, Balance 2,
  Doctest 1); `cargo clippy -p kingdoms_sim --all-targets -- -D warnings` sauber;
  `RUSTDOCFLAGS='-D warnings' cargo doc -p kingdoms_sim --no-deps` ohne Warnungen.
* Neue Tests: Träger geht zur Quelle, Ware erst beim Abholen weg und erst beim Abladen da, Ladung in
  `CitizenView`/`CitizenDetail`; Abriss des Ziels (Erhaltung in jedem Tick geprüft), Tod des Trägers
  (Übergabe), Stellenverlust (liefert ab), kein Lagerplatz (wartet, nichts verloren, danach
  geliefert), unerreichbares Ziel (anderes Lager), Zusammenlegen, Rückfracht, Berichte,
  `validate` erkennt falsche Ladung / fehlende Ware / Ware ohne tragenden Auftrag; Holzfäller am
  Baum, Stamm erst bei Ankunft in der Hütte, Förster pflanzt, wo er steht, Fischer am Ufer;
  Bauarbeiter sichtbar rund um die Baustelle, nachts kein Fortschritt; Bauern auf den Feldern;
  Besorgungsgang füllt den Vorrat (nicht stündlich). Determinismus:
  `carriers_with_goods_continue_identically_after_loading` (gespeichert, während ein Träger mit Ware
  zwischen zwei Tiles läuft; Ereignisse, Läufer und Bytes nach einem Tag identisch).
* Angepasste Tests (Zeitpunkte, weil Träger erst ab 08:00 arbeiten; Schwellen mit Begründung im
  Test): Brot-Kette Bäckerei ≥ 17 → ≥ 14 Arbeitsgänge in 2 Tagen (jede Einheit zwei Trägergänge),
  Holzkette Holzfäller ≥ 30 → ≥ 21 in 3 Tagen (Wege und Heimtragen, erste Gänge ohne Werkzeug),
  Säge/Tischler jetzt im Verhältnis zur Holzmenge. Determinismus-Testdorf: Hühnerhöfe etwas
  weiter außen, wo ihr Eingang nicht zugebaut ist (vorher ohne Straßenzugang, Zuwanderung hing
  am Zufall).
* Balance „vernünftiger Spieler“ Seed 7 (unveränderte Bedingungen): 12 → 53 Bewohner, keine Toten,
  Hunger 100, zwei Gemüseernten (664 + 502), Ø Arbeitsweg 40 min, 4 Träger, Ø Transport ≈ 60 min.
  Das Skript baut Vorratsspeicher für volle Scheunen jetzt auch ohne freien Erwachsenen (Lager
  bekommen ihren ersten Träger vor allen anderen). Seeds 1, 3, 11, 42 bestehen ebenfalls (29–58
  Bewohner; Seed 11 schlug schon in 3a fehl); Seeds 2 und 5 nicht (siehe Offen). Naiver Spieler grün.
* Leistung (Release, allein): neuer `carrier_performance` (256², Wohn- und Werkstattblöcke im
  Wechsel, Lagerhaus je Werkstattblock, ≈ 2050 Bewohner, 249 Träger, 2 Tage): Tick Ø 360 µs,
  max. 3,0 ms (kein Tick über 4 ms), Logistikschritt Ø 48 µs, max. 0,8 ms; Spitze 181 Träger
  unterwegs, 1021 Einheiten unterwegs, 2105 Transporte, Ø 73 min. `walking_performance`: Tick
  Ø 173 µs, max. 2,0 ms; `population_performance`: Tick Ø 318 µs.
* Spiel-Binary (Kopie des Hauptstands mit dieser Simulation, nur im Arbeitsverzeichnis):
  `cargo check -p kingdoms --all-targets` ohne Fehler und Warnungen; `cargo test -p kingdoms`
  82 von 83 grün – `town_grows_through_a_year` scheitert (siehe Offen).

**Offen**
* Entwickler-Stadt (`src/session/dev_settlement.rs`, Spiel-Lane): wächst ohne Tote und ohne
  abgelehnte Kommandos von 12 auf 51 Bewohner und baut Weizenfarm, Obstgarten und zwei
  Vorratsspeicher, aber keinen Steinbruch mehr (Assertion „quarry built“; dahinter würden auch
  Köhlerei/Sägewerk fehlen): im Sommer fehlen freie Erwachsene (`slots + workers + 2 <= adults`),
  ab Herbst sperrt die Brennholzreserve das Bauen (Holz 46–108 bei 51 Bewohnern). Die Spiel-Lane
  muss Reihenfolge/Schwellen des Skripts an die langsamere Logistik anpassen.
* Balance-Seeds 2 und 5 (optionale Prüfung): Brennholz im Winter reicht nicht (Wärme 21–23): das
  Skript baut Holzfäller nur „je 10 Bewohner und mit freien Erwachsenen“, die Stadt bleibt bei
  24–34 Bewohnern mit 1–2 Holzfällern.
* Märkte haben noch keinen eigenen Vorrat: die Stände schöpfen beim Besorgungsgang abstrakt aus
  den nächsten Lagern (Belieferung der Märkte durch Träger wäre der nächste Schritt). Ernten
  kommen ohne Laufen in die Scheune, Viehzüchter arbeiten im Hof.
* Träger arbeiten nur für ihr eigenes Lager; im Leistungstest stehen in einem Lager freie Träger,
  während die Aufträge eines anderen warten (Aushilfe zwischen Lagern wäre eine Erweiterung).
* Spiel: getragene Ware zeichnen (`CitizenView::carried`), neue Berichtsfelder anzeigen
  (`carrier_wait`, Träger frei/unterwegs, Ø Transport), Tätigkeitstexte; Bauarbeiter und Bauern
  sind jetzt draußen sichtbar.

## 2026-11-21 · Sim-Lane, Schritt 3a – Bewohner laufen: Navigation und Tagesablauf (`crates/sim`, `data/`, Doku)

**Fertig**
* Neues Modul `navigation`: Wegkosten-Raster je Tile in ‰ eines Pfad-Tiles – Straßen/Brücken
  1000 / Tempo aus `roads.ron` (Pflaster 714), Gelände aus `rules.ron` → `walking` (Gras 1250,
  Sand 1600, Wald 1750, Fels 2250, Schnee 2500, Sumpf 3750); Wasser nur über Brücken (nur längs),
  Felsbrocken, Grundflächen und Mauern blockieren, Tore lassen durch, Nachbar-Tiles höchstens eine
  Höhenstufe auseinander (+250 ‰), Klippen blockieren. Abgeleitet (nicht gespeichert),
  inkrementell über neue Änderungs-Rechtecke von Straßen und Gebäuden
  (`ChangeTracker::take_rects`, unabhängig von den Renderer-Chunks; Terrain ändert sich zur
  Laufzeit nicht, Felsbrocken verschwinden nur mit Bauten), Zusammenhangskomponenten für
  sofortiges „kein Weg“.
* A* ganzzahlig auf 4 Richtungen mit vollständiger Gleichstandsregel (f, h, Tile-Index; N, O, S,
  W), in Scheiben über mehrere Ticks fortsetzbar; Cache je ungeordnetem Tile-Paar und Revision
  (Rückweg = umgekehrter Hinweg, FIFO 8192). Budget 10 000 Knoten je Tick: Anfragen warten in einer
  gespeicherten Warteschlange, eine längere Suche läuft im nächsten Tick weiter. Jede Antwort
  kostet die Knoten ihrer Suche, auch aus dem Cache – so ist der Ablauf nach dem Laden (leerer
  Cache) identisch.
* Bewohner haben Position und Vor-Tick-Position in 1/256 Tile (nur Ganzzahlen), Blickrichtung,
  Gebäude (im Haus unsichtbar), Ziel, Weg mit Fortschritt; Tempo = 0,2 Tiles/Tick ÷ Wegkosten ×
  Kind 90 % / Alt 75 % × Gesundheit × Schnee/Regen/Sturm, ganzzahlig mit Rest. Keine Teleports:
  Tür → Eingang → Weg → Eingang → Tür; wird unterwegs etwas auf den Weg gebaut, wird neu gesucht,
  wer selbst überbaut wird, geht auf kürzestem Weg aus der Grundfläche hinaus (gefunden mit der
  Entwickler-Stadt des Spiels: dort saß der einzige Hühnerhof-Arbeiter sonst für immer fest).
  Ohne Weg bleibt der Bewohner, wo er ist, versucht es nach 2 h erneut; Meldung „Kein Weg zur
  Arbeit/nach Hause: … ist … nicht erreichbar.“ höchstens alle 6 h je Gebäude; Haus- und
  Produktionsbericht nennen das Problem. Zuwanderer kommen zu Fuß vom Kartenrand (dem
  Hauptquartier nächster erreichbarer Rand-Tile, Straßen am Rand bevorzugt), Abwanderer laufen als
  Reisende hinaus, Neugeborene erscheinen im Haus, Tote verschwinden an Ort und Stelle; Bewohner
  aus abgerissenen Gebäuden treten an der alten Tür heraus.
* Tagesablauf (`population::routine`, Stunden in `rules.ron` → `walking.routine`): 08:00
  Arbeitsweg, Arbeit ab Ankunft, 12–13 Mittagspause am Arbeitsplatz, bis 17:00 (Bauern Sommer
  18:00, Winter 16:00), 18:00 Markt (erster Erwachsener des Haushalts), 19–21 jeden zweiten Abend
  Gasthaus oder Kirche, Kinder 08–13 Schule, nachts zu Hause; `Activity` um Mittagspause,
  Besorgungen, Unterwegs, Kein Weg erweitert.
* **Anwesenheit:** Produktion, Feldarbeit und Bau rechnen nur mit anwesenden Arbeitern
  (`is_at_work`, `present_workers`, `staffing_permille` jetzt anwesend). Rezept- und Bauzeiten
  gelten für Betrieb rund um die Uhr; anwesende Arbeiter arbeiten mit dem Schichtfaktor 3 (24 h /
  8 h Schicht) – eine volle Schicht ohne Weg ergibt die bisherige Tagesleistung, jeder Arbeitsweg
  kostet Leistung. Neue Status „Arbeiter auf dem Weg“, „Mittagspause“, „Feierabend“ (Probleme wie
  „Ausgangslager voll“ haben Vorrang). Märkte, Dienste und die noch abstrakten Träger wirken weiter
  über die Zuteilung (Träger als Läufer: Schritt 3b).
* Berichte: `ProductionReport::{present, commute}` („Arbeitsweg Ø 12 min (3 % der Schicht)“),
  `commute_report`, `path_problem`, `walking_summary`; `CitizenDetail` mit Position, Gebäude, Ziel,
  Zielname, `activity_text` („Auf dem Weg zur Arbeit (Sägewerk)“) und letztem Arbeitsweg;
  `HouseReport::path_problem`.
* Renderer-API: räumlicher Index je Chunk (inkrementell beim Tile-Wechsel, Betreten, Verlassen),
  `citizens_in(state, defs, rect)` / `GameState::citizens_in` → `CitizenView { id: WalkerId, pos,
  prev_pos, facing, role, moving, visible, carried }` (Rollen des `characters`-Atlas, `carried`
  bis 3b `None`), `citizen_view`, `walker_view`, `Direction::sprite_dir`, `CitizenRole::sprite_key`.
* Balance: Startvorrat Brot 40 → 60 (`scenarios/default.ron`, §13). Gebaut und gearbeitet wird
  nur tagsüber; abends gesetzte Häuser und Höfe werden erst am nächsten Morgen fertig. Mit 40 Brot
  lief der Vorrat der Entwickler-Stadt vor der ersten Ernte leer (keine Zuwanderung, Hunger bis 64).
* Spielstand-Format 7 (Positionen, Wege, Warteschlange mit Suchfortschritt, Reisende,
  Meldungs-Sperrzeiten); `validate` prüft Positionen (Karte, ≤ 1 Tile je Tick, begehbar oder
  Grundfläche), Wege, Gebäude-Bezüge und Warteschlange; `rederive` baut Raster und Index neu.
  Neuer Pipeline-Schritt 7b „Bewegung“.
* Doku: `game_design.md` §2 (Laufgeschwindigkeit), §3 (Begehbarkeit), §5.2 (Schichtbetrieb), §6
  (Tagesablauf, Anwesenheit, Bewegung, Zu-/Abwanderung zu Fuß); `architecture.md` §3.2–3.5.

**Geprüft**
* `cargo test -p kingdoms_sim` grün (Simulation 249 + 3 ignoriert, Determinismus 6, Balance 2,
  Doctest 1); `cargo clippy -p kingdoms_sim --all-targets -- -D warnings` sauber;
  `RUSTDOCFLAGS='-D warnings' cargo doc -p kingdoms_sim --no-deps` ohne Warnungen.
* Neue Tests: kürzeste Wege (Straße vs. Gras, Lücke in einer Felswand), Klippen/Rampen,
  Brücken nur längs, Mauern/Tore, deterministische Gleichstände (fester Weg, unabhängig von Cache
  und Vorgeschichte), Cache mit gleicher Abrechnung, inkrementelles Raster = Neuaufbau, Knotenlimit,
  Suchen in Scheiben (auch nach Unterbrechung) und umgekehrter Rückweg, Index-Schritte = Positions-
  Schritte; Arbeiter läuft zur Arbeit und zurück (keine Sprünge, nur begehbare Tiles, Arbeitsweg
  gemessen, Mittagspause, Feierabend), langer Arbeitsweg kostet Leistung, unerreichbarer
  Arbeitsplatz bis zum Brückenbau (eine Meldung), verbauter Eingang, Umweg um einen neuen Bau,
  überbauter Läufer geht aus der Grundfläche hinaus,
  Budget verteilt 24 Anfragen auf ~20 Ticks (nie mehr als das Budget je Tick), Zuwanderer vom
  Rand, Abwanderer verschwinden am Rand, Schule/Markt/Gasthaus/Kirche, Index = Vollsuche,
  `validate` erkennt kaputte Positionen, Speichern/Laden mitten im Weg (auch über das echte
  Spielstand-Format in `tests/determinism.rs`). Bestehende Tests an die Anwesenheit angepasst
  (Arbeiter laufen erst hin; Bauzeiten mit Schichtfaktor).
* Balance „vernünftiger Spieler“ (25 Tage): Seed 7: 12 → 62 Bewohner (vorher 67), keine Toten,
  Hunger durchgehend 100, Ø Arbeitsweg 38 min (8 % der Schicht); Seeds 1, 2, 42 bestehen ebenfalls
  (62–65 Bewohner, Ø Arbeitsweg 32–36 min). Seed 11 besteht mit 40 Brot, mit 60 Brot nicht: nur eine
  Gemüseernte, weil die Scheunen voll bleiben (bekannter offener Punkt „Volle Scheunen“ des
  Testskripts, Lager für Gemüse voll bei 350). Neue Bedingung: Ø Arbeitsweg 3–60 min und niemand
  ohne Weg; Seed per `KINGDOMS_BALANCE_SEED` wählbar. Naiver Spieler grün.
* Leistung (Release, `walking_performance`, 256² mit Pfad-Raster, 480 Häuser im Westen,
  Arbeitsplätze im Osten, ≈ 2050 Bewohner, 1158 Beschäftigte, 2 Tage, allein laufend): ganzer
  Tick Ø 161–177 µs, max. 2,4–2,9 ms (kein Tick über 4 ms); Bewegungsschritt Ø 96–100 µs,
  max. 2,0–2,8 ms; Spitze 1124 gleichzeitig Laufende, Warteschlange bis 1093 Anfragen (morgens),
  8266 Anfragen, Cache-Treffer 49,6 %, größte Suche 31 922 Knoten (über mehrere Ticks). Parallel
  zu den beiden anderen Leistungstests einmal 4,3 ms. Räumliche Abfrage nur über die Chunks des
  Rechtecks. `population_performance`: Tick Ø 302 µs.
* Spiel-Binary (Kopie des Hauptstands mit dieser Simulation, nur in meinem Arbeitsverzeichnis):
  `cargo check -p kingdoms --all-targets` ohne Fehler und Warnungen; `cargo test -p kingdoms`
  82 von 83 grün – `session::dev_settlement::town_tests::town_grows_through_a_year` scheitert (siehe
  Offen).

**Offen**
* Spiel-Test `town_grows_through_a_year` (Entwickler-Stadt, Spiel-Lane): mit dieser Simulation
  wächst die Stadt in 25 Tagen von 12 auf 63 Bewohner ohne Tote, baut Weizenfarm, Steinbruch,
  Obstgarten, Sägewerk und Köhlerei, scheitert aber an „enough woodcutters for 63 citizens“
  (4 Holzfäller × 14 < 63): das Skript ist durch Erwachsene (44 für 42 Plätze) und die
  Brennholzreserve begrenzt. Die Schwellen des Skripts (Holzfäller je Bewohner, Plätze für
  Erwachsene) muss die Spiel-Lane an Schichtbetrieb und Arbeitswege anpassen.
* Spiel: Läufer zeichnen (Interpolation `prev_pos` → `pos`, Laufanimation `<role>_<dir>`),
  neue Status-Texte (Farbe; „Feierabend“ erscheint derzeit rot), Arbeitsweg/Anwesende im
  Info-Fenster, Bewohner-Details mit Position/Ziel. Rezeptdauern im UI sind Betriebsstunden
  (Schichtfaktor 3).
* Schritt 3b: Träger mit Waren (`carried`), Bauarbeiter zu Baustellen und Bauern zu den Feldern
  als Läufer; bis dahin bleiben Träger abstrakt und unabhängig von der Anwesenheit.
* Morgens in sehr großen Städten mit weiten Wegen wartet ein Teil der Pendler auf seinen Weg
  (Budget); die Wartezeit zählt zum Arbeitsweg. Cache-Treffer kosten wegen der Determinismus-
  Garantie dasselbe Budget wie die Suche.

## 2026-11-21 · Spiel-Lane, Etappe 3b – Lebendiges Land: Felder, Wetter, Partikel (`src/`, Engine, Assetgen, Doku)

**Fertig**
* **Feld-Ebene** (`view/fields`): alle beanspruchten Feld- und Weide-Tiles aus
  `agriculture().fields_in` mit `FieldView::sprite` als statische Batches je Chunk, neu gebaut
  nur für `take_field_dirty_chunks` und Terrain-Chunks. Weiden und Brache liegen flach auf dem
  Boden; Feldfrüchte und Obstbäume sind Bodenbewuchs (neu in der Engine: `FLAG_DEPTH_MAX`,
  `SpriteDepth::ground_cover`) – Pflanzen über dem Tile überdecken Bäume und Gebäude dahinter,
  alles davor überdeckt sie.
* **Weidetiere** (`view/animals`, reine Darstellung): Kühe bei der Viehzucht, Schafe bei der
  Schafzucht, Zahl nach Weide-Tiles (höchstens 8), wandern auf der eigenen Weide und grasen –
  reine Funktion von Farm, Tiles und Spielzeit (stehen bei Pause still).
* **Straßenebene** reagiert nur noch auf `take_structure_dirty_chunks` (abgeholt, wenn sich
  `structure_revision` geändert hat) und Terrain-Chunks; der Vergleich aller Straßen-Tiles ist
  entfernt.
* **Engine-Partikelsystem** (`rendering/particles.rs`): Pool mit harter Obergrenze, Stile
  (Sprites/Animation, Lebensdauer, Geschwindigkeit mit Streuung, Schwerkraft/Auftrieb, Wind,
  Pendeln, Größe und Deckkraft über die Lebenszeit), Emitter im Immediate-Mode mit eigenem
  deterministischem Zufall je Schlüssel, Weltpartikel mit Iso-Tiefe ihrer Quelle und Culling,
  Bildschirmpartikel für Wetter; transparenter Pass. **Umgebung** im Sprite-Shader
  (`RenderList::environment`): Windstärke, Zeitbasis und Neigung, Schneedecke und Nässe auf
  Boden-Sprites (`FLAG_GROUND`, Wege mit `FLAG_TRODDEN` nur dünn verschneit), Schnee auf grünen
  Pixeln (`FLAG_SNOW_ON_GREEN`: Rasen und Hecken der Grundstücke, Obstbäume, Pflanzen).
* **Wetter und Effekte** (`view/effects`), alles aus dem Zustand und weich nachgeführt: Licht je
  Wetter und Stärke (Sturm dunkel, Schnee kühl, Dürre warm), Regen und Sturmregen, Schneefall,
  Schneedecke und Nässe aus `WeatherState`, Bäume schwanken im Sturm stärker und neigen sich;
  Rauch aus Werkstätten, solange sie arbeiten, und im Winter aus bewohnten Häusern und anderen
  beheizten Gebäuden; Staub über Baustellen mit Bauarbeitern; fallendes Laub im Herbst.
  Rauchpunkte je Sprite in `assets/atlases/emitters.ron`, vom Generator geschrieben
  (Schornsteine automatisch, Rauchlöcher von Holz- und Bauernhaus, Meiler, Ofen, Brennofen).
* **Assetgen**: `leaf_<0..3>` im `effects`-Atlas (128 × 256), `emitters.ron`; alle anderen
  Atlanten bytegleich.
* **Entwickler**: `KINGDOMS_DEV_WEATHER=<wetter>[:<stärke>[:<tage>]]` (nach dem Warp,
  `DebugSetWeather`), `KINGDOMS_DEV_UNLOCK=1` (`DebugUnlockAllResearch`).
  `KINGDOMS_DEV_SETTLEMENT=2` ist jetzt eine lebensfähige kleine Stadt (`DevPlan::Town`): baut
  stündlich das nächste sinnvolle Gebäude (Häuser, Brunnen, Holzfäller + Förster, Hühnerhöfe,
  Markt, Gemüse, Obstgarten, Weizen, Steinbruch, Köhlerei mit täglichem Arbeitsgang, Sägewerk
  mit begrenztem Auftrag, Schafzucht, Speicher), nur mit vorhandenen Erwachsenen und
  Brennholzreserve. Debug-Fenster: Abschnitt „Wetter & Partikel“, Zeiten „effects“ und
  „map draw“, Feld-Sprites, Weidetiere, neu gebaute Chunks.
* Doku: README, `architecture.md` §2/§4, `assets.md` (Emitter-Punkte, Laub), `roadmap.md`
  (Phase 6 ✅, Partikel ✅).

**Geprüft**
* `cargo test --workspace` grün (Spiel 83, Assetgen 56, Engine 52, Simulation 225 + 2
  ignoriert, Determinismus 5, Balance 2, Doctest 1); `cargo clippy --workspace --all-targets --
  -D warnings` sauber. Neue Tests u. a.: Partikel (Rate, Lebensdauer, Obergrenze,
  Determinismus je Schlüssel, Bewegung mit Wind, Culling, Vergessen von Emittern, Kurven,
  Tiefe/Bildschirmgröße), WGSL-Validierung des Sprite-Shaders mit naga, Bodenbewuchs-Tiefe
  (Engine und Feldebene), Weidetiere (bleiben auf der Weide, deterministisch, Herdengröße),
  Wetteroptik und Nachführung, Rauchregeln, ausgelieferte `emitters.ron`, Rauchpunkte auf
  Sprite-Pixeln, `DevWeather`-Parser, Entwicklerschalter beim Start, Entwickler-Stadt über ein
  Jahr (wächst, niemand verhungert oder erfriert, Gemüse-, Weizen- und Obstfelder, keine
  abgelehnten Kommandos).
* Unter Xvfb + lavapipe angesehen (Seed 7, Karte medium, `KINGDOMS_DEV_SETTLEMENT=2`):
  Gemüsefelder mit Keimlingen und Weide mit Schafen (Sommer, Jahr 1), Obstgarten mit Früchten
  (Herbst), Brache der Weizenfarm mit Farmfenster (Jahr 2), Winter mit Schneedecke, Schneefall
  und Rauch aus Häusern und Rathaus, erster Schnee mit fleckiger Decke, Sturm (Licht 0,52,
  1767 Partikel), Regen, Köhlerei „In Arbeit“ mit Rauch im Winter (Jahr 2), Staub über
  Baustellen (`=1`), fallendes Laub im Laubwald, Straßen der Test-Siedlung erscheinen während
  des Spiels, Schnellspeichern/-laden (statische Batches 144 vor und nach dem Laden).
  Zoom 0,5 im Winter (148 Bewohner, 2354 Partikel, 29 Rauchquellen): 9 FPS, Frame 112 ms –
  fast ganz Software-Rendering (lavapipe); `effects` 0,07 ms (max 0,11), `map draw` 0,12 ms,
  `build render list` 0,26 ms.

**Offen**
* Wünsche an die Simulation: Winterstufe der Obstbäume (kahle Bäume; jetzt verschneit dargestellt),
  Heizen je Haus (Rauch hört bisher nicht auf, wenn einem Haus das Brennholz fehlt), Windrichtung
  und -stärke im Wetterzustand, Herdengröße als Zustand (die Tiere zeigen nur „hat Weide“),
  „Bauarbeiter haben in diesem Tick gearbeitet“ je Baustelle (Staub nutzt Belegung und
  Baufortschritt-Möglichkeit).
* Die Entwickler-Stadt baut Sägewerk, Schafzucht und Köhlerei meist erst im zweiten Jahr
  (Erwachsene und Stein sind im ersten Jahr knapp); Weizen wird erst im zweiten Frühling gesät.
  Gegen Winterende wird das Brennholz knapp (im Test erfriert niemand).
* Regen und Schnee sind Bildschirmpartikel und wandern beim Verschieben der Kamera nicht mit;
  `FLAG_SNOW_ON_GREEN` färbt auch grüne Nicht-Pflanzen (z. B. grüne Markisen) bei Schnee weiß.
* Die Minikarte zeigt keine Felder.

## 2026-11-21 · Spiel-Lane, Etappe 3a – Reichsverwaltung und Farmfenster (`src/`, Doku)

**Fertig**
* **Reichsverwaltung** (`ui/hud/management`): ein verschiebbares Fenster mit acht Reitern statt
  acht einzelner Fenster (nie Fenster übereinander, Tasten wechseln nur den Reiter). Öffnen über
  acht Knöpfe in der Leiste (neben dem Datum) oder die Tasten **L** Lager, **P** Produktion,
  **F** Finanzen, **K** Handel (Kontor), **J** Arbeitskräfte (Jobs), **V** Bevölkerung (Volk),
  **N** Landwirtschaft & Wetter (Natur), **C** Statistik (Chronik); die Taste des offenen Reiters,
  der Schließen-Knopf und Esc (nach dem Werkzeugschritt, vor der Auswahl) schließen. Höchstens
  1040 × 700, passt mit Scrollen auf 1280 × 720; links der Mitte, damit das Info-Fenster frei
  bleibt. Daten je Reiter gecacht (höchstens alle 0,25 s neu, nur wenn die Simulation lief);
  im Pausenzustand nennt die Fußzeile wartende Änderungen.
  * Lager & Waren: alle Waren nach Kategorie (Filter, sortierbare Spalten, „nur genutzte“) mit
    Bestand, Lagerplatz, für Baustellen gebunden, heute erzeugt/verbraucht/Haushalte, Trend,
    Preis und Preistrend; Lagerhäuser mit Füllstand, vollen Waren, Trägern, Erreichbarkeit;
    Klick auf eine Ware öffnet ihren Verlauf in der Statistik.
  * Produktion: alle Betriebe nach Typ (zuklappbar) mit Zustand und Grund, Belegschaft,
    Produktivität, Arbeitsgang/Auftrag, Priorität; Filter „nur Probleme“.
  * Finanzen: Schatz, Bilanzen, Kassenbuch je Kategorie (heute, gestern, laufend), Verlauf als
    gestapelte Balken mit Bilanzlinie, Steuersatz (`SetTaxRate`) mit Wirkung auf die
    Zufriedenheit und laufenden Steuereinnahmen.
  * Handel: Zustand des Kontors mit Grund, Aufträge anlegen/bearbeiten/löschen
    (`SetTradeOrder`/`RemoveTradeOrder`, Vorschau mit `trade::validate_order`, Ablehnungen der
    Simulation als Hinweis), Ein-/Verkaufspreise aller Waren, letzte Geschäfte.
  * Arbeitskräfte: Bedarf/besetzt/unbesetzt je Bereich mit Priorität
    (`SetWorkforcePriority`), alle Arbeitsplätze mit eigener Priorität
    (`SetProductionPriority`).
  * Bevölkerung: Lebensabschnitte × Geschlecht, Wohnraum, Bewegung heute/gestern mit Verläufen,
    Bedürfnisse, Zufriedenheit mit Faktoren, Häuserliste (Stufe, Bewohner, unerfüllte
    Bedürfnisse, Ausbau-Hindernis) – Klick wählt das Haus.
  * Landwirtschaft & Wetter: Wetter mit gemaltem Symbol, Stärke, Dauer, Schneedecke,
    Bodennässe, Wirkungen, Jahreszeit mit Resttagen; Nahrungsprognose bis Winterende
    (`days_of_stock` als Option); Feldfrüchte, Farmen (Klick wählt aus).
  * Statistik: Bevölkerung, Schatzkammer, Nahrungsvorrat, Ware (Bestand, Erzeugung, Verbrauch,
    Handel, Preis), Zufriedenheit, Beschäftigung – über bis zu 48 Tage oder 72 Stunden.
* **Diagramm-Widget** `ui/chart.rs`: Linien und gestapelte Balken, schöne Achsenteilung (ganze
  Schritte für Stückzahlen und Gold), deutsche Zahlen, Legende, Hover mit allen Werten.
* **Farmfenster** (`ui/hud/farm_panel.rs`) im Info-Fenster von Feldfarmen, Obstgärten und
  Weidehöfen: Zustand mit Erklärung (wächst, außerhalb der Saison mit Aussaatzeit, kein
  Ackerland), Fruchtwahl mit Bild der reifen Stufe (`SetCrop`, nur angebotene Früchte),
  bestellt/reif/brach, Wachstum, erwartete Ernte, Reifezeit, Fruchtbarkeit gegenüber dem
  natürlichen Wert, Erschöpfung, Feuchte, Zustand, Scheune, letzte Ernte, Einzelliste der
  Felder mit Stufenbildern aus dem `fields`-Atlas.
* Entwickler: `KINGDOMS_WINDOW=<reiter>` öffnet die Reichsverwaltung beim Start;
  `KINGDOMS_DEV_SETTLEMENT=2` legt eine kleine, wachsende Wirtschafts-Siedlung an (Farmen,
  Sägewerk, Handelskontor, Markt, Handelsaufträge; vorab `DebugUnlockAllResearch`).
* Doku: README (Steuerung, Entwickler-Optionen), `architecture.md` §4, `roadmap.md`
  (Phase 4 ✅, Bevölkerungs- und Farmfenster ✅).

**Geprüft**
* `cargo test --workspace` grün (Spiel 70, Assetgen 54, Engine 41, Simulation 225 + 2
  ignoriert, Determinismus 5, Balance 2, Doctest 1); `cargo clippy --workspace --all-targets --
  -D warnings` sauber. Neue Tests u. a.: Achsenteilung und Trefferprüfung des Diagramms,
  deutsche Achsentexte, Reiter-Schlüssel und Umschalten, Cache-Regel, Kalenderbeschriftungen,
  Filter/Sortierung der Waren, Trendtexte, Statusfarben, Auftragsentwurf ↔ Auftrag mit der
  Validierung der Simulation, Handelszustand ohne Kontor, Schatz-Rückrechnung (gegen die
  Stundenwerte der Simulation um Mitternacht geprüft), alle Statistik-Auswahlen mit gleich
  langen Reihen, Todesursachen-Text, Farm-Erklärungen und Feldstatistik einer echten Farm.
* Unter Xvfb + lavapipe angesehen (Seed 7, `KINGDOMS_DEV_SETTLEMENT=2`, 48 000 und 96 000
  Warp-Ticks): alle acht Reiter bei 1600 × 900, Lager, Handel, Produktion, Landwirtschaft,
  Finanzen und Bevölkerung bei 1280 × 720 (Fenster passt, Inhalt scrollt), Farmfenster von
  Gemüse- und Weizenfarm, Test-Siedlung `=1` mit Baustelle in der Produktionsliste.
* Echte Eingaben per XTest: L öffnet, Ziehen verschiebt das Fenster, Mausrad über dem Fenster
  zoomt die Karte nicht, P wechselt den Reiter, Esc schließt erst das Fenster, dann die
  Auswahl; „+“ setzt den Steuersatz auf 11 % (Einnahmen und Zufriedenheitswirkung folgen); ein
  leerer Handelsauftrag wird von der Simulation abgelehnt und als Hinweis „Ein Handelsauftrag
  braucht eine Kauf- oder Verkaufsgrenze.“ gezeigt; Klick auf eine Produktionszeile wählt die
  Weizenfarm und schwenkt die Kamera; „Gerste“ im Farmfenster sendet `SetCrop`; „Hoch“ setzt
  die Priorität „Nahrung“; Klick auf „Gemüse“ im Lager öffnet die Warenstatistik. Keine
  Panics, keine WARN/ERROR außer fehlendem Audiogerät.

**Offen**
* Wünsche an die Simulation: erwarteter Erntetermin je Feld/Farm (das Farmfenster zeigt bisher
  nur Wachstum und Reifezeit der Frucht), Vorschau „Steuereinnahmen und Zufriedenheit bei
  Satz X“, Schatz am Tagesende im Kassenbuch (wird jetzt zurückgerechnet), Handelsprotokoll im
  Zustand (die letzten Geschäfte stammen aus dem Ereignis-Log seit Start/Laden), Debug-Befehl
  für Gold/Waren (die Wirtschafts-Siedlung stockt, sobald Holz fehlt).
* Die Wirtschafts-Siedlung ist eine Entwicklerhilfe ohne Balance-Anspruch: das Sägewerk
  verbraucht das Holz, Markt und weitere Häuser kommen erst spät; in den ersten Tagen
  meldet die Simulation Nahrungsmangel.
* Feld-Ebene auf der Karte und Wetter-/Schnee-Optik fehlen weiterhin (nächste Etappe).

## 2026-11-21 · Review-Befunde behoben (`crates/sim`, `data/`, `src/`, Engine, Doku)

**Fertig**
* **Träger zuerst (kritisch):** Die Arbeitszuteilung besetzt vor allen Prioritäten je Lager einen
  Träger und je Markt einen Arbeiter (Grundbesetzung; stillgelegte Lager nicht). Produktion ohne
  Arbeit (keine Aufträge, keine Rohstoffe im Umkreis, Felder wachsen oder ruhen) kommt zuletzt an
  die Reihe; ihr Status bleibt auch ohne Arbeiter erhalten (Sammler prüfen ihr Ziel, Feldfarmen
  ihre Aufgabe), ein Wechsel löst eine Neuzuteilung aus. Neue Warnung „Keine Träger: … hat keine
  Arbeiter, Transporte bleiben liegen.“ (alle 6 h, Aufträge warten seit ≥ 1 h). Erledigt damit
  den offenen Punkt der Test-Siedlung (Rathaus ohne Träger).
* **Ruhestand:** Wer alt wird, verliert die Arbeit sofort (auch in `rederive`); `validate` lehnt
  Arbeit ohne Arbeitsfähigkeit ab. Ein Schnellspeicherstand aus diesem Tick lädt wieder.
* **Haushalte:** Ein obdachloser Haushalt, größer als das größte baubare Haus, teilt sich auf
  (Paar mit den jüngsten Kindern bleibt, Erwachsene und älteste Kinder ziehen aus) – niemand
  bleibt dauerhaft obdachlos, die Zuwanderung wird nicht für immer gesperrt. Zuwanderer lassen je
  Haus einen Platz für Geburten und Hochzeiten frei (`immigration_reserved_places: 1`, neu in
  `rules.ron`); Bericht `immigrant_places`, Tooltip „Frei für Zuwanderer“.
* **Hochzeiten** suchen den Partner über Ledige nach Alter statt über alle Paare.
* **Spielstände:** Tageszähler sättigend; `validate` prüft Geburtstage (≤ heute, letzte Geburt
  nach der eigenen), Zähler ≤ `MAX_DAY_COUNT` (1 Mio.), doppelte Haushaltsmitglieder und
  Selbstpartnerschaft. Format bleibt 6 (neue Kommando-Variante am Ende angehängt).
* **Berichte/Texte:** `food_forecast` ohne „-0“ und ohne ∞ (`days_of_stock: Option<f32>`);
  Rodungswarnung „(1 Baum, 2 Büsche)“ mit Singular/Plural, ohne Nullwerte;
  `CannotPauseHousing` → „Wohnhäuser können nicht stillgelegt werden.“
* **Regeln nur in der Simulation:** neue Abfragen `buildings::demolish_refund`
  (`DemolishRefund`, Fehler „letztes Hauptquartier“), `pause_allowed`, `move_allowed`,
  `production::staffing_permille`, `agriculture::is_arable`; das Spiel nutzt sie für
  Erstattung, Abriss-Dialog, Stilllegen, Versetzen und Belegschaft, die Bedürfnis-Schwelle kommt
  aus `need_met_threshold`. Neues Kommando `PlaceBuildingWithAccess`: die Zufahrt wird im selben
  Tick neu geplant und nur gebaut, wenn das Gebäude angenommen wurde.
* **Spiel:** Schnellladen gibt die statischen Batches der alten Kartenansicht frei
  (`WorldView::release`, Zähler `Renderer::static_batch_count`), behält Raster/Gebietsgrenze und
  richtet die Kamera auf das Hauptquartier; Kamera bei offenem Dialog gesperrt (Tasten, Rad,
  Rand, Ziehen); Meldungsliste: neueste oben, Wiederholungen rücken nach oben, so viele Karten,
  wie nach gemessener Höhe über die Bauleiste passen; Stilllegen-Knopf nur, wenn die Simulation
  es erlaubt; Zufahrts-Vorschau wird neu geplant, sobald sich das Urteil der Prüfung ändert;
  Fruchtbarkeit als Rohwert mit Urteil („170 · Ackerboden“, „45 · zu karg für Felder (ab 60)“);
  Speicherfehler rein deutsch (Rohfehler nur im Log); Tooltip-Ruhezeit je Gebäude statt je Feld;
  kein irreführendes „(Entf)“ am Abriss-Knopf; „Zufahrt: 1 Feld Trampelpfad wird mitgebaut.“
* Doku: `game_design.md` §5.6, §6.1 (Arbeit, Zuwanderung, zu große Haushalte), §8.3;
  `architecture.md` §3.3, §4.

**Geprüft**
* `cargo test --workspace` grün (Spiel 46, Assetgen 54, Engine 41, Simulation 225 + 2 ignoriert,
  Determinismus 5, Balance 2, Doctest 1); `cargo clippy --workspace --all-targets -- -D warnings`
  sauber; `RUSTDOCFLAGS='-D warnings' cargo doc -p kingdoms_sim --no-deps` ohne Warnungen.
* Neue Tests u. a.: Grundbesetzung vor allen Prioritäten, untätige Produktion zuletzt (ohne
  Flackern, Baum in Reichweite → wieder besetzt), Lager ohne Träger warnt, Ruhestand +
  Speichern/Laden im selben Tick, manipulierte Spielstände abgelehnt, Haushalt teilt sich nach
  Abriss eines Bauernhauses, Zuwanderer lassen Platz frei, Hochzeitspaare wie zuvor
  (nächstes Alter, kleinste ID), Zufahrt nur mit angenommenem Gebäude, Erstattungsvorschau =
  tatsächliche Erstattung, Pause-Abfrage = Kommando (auch leeres Haus), Meldungsreihenfolge und
  Kartenzahl, Fruchtbarkeitstext, deutscher IO-Fehler.
* Balance-Test „naiver Spieler“ (feste Anfänger-Bauliste, mehr Arbeitsplätze als Erwachsene,
  10 Tage, Seed 7): mit der alten Zuteilung bricht er sofort ab (Rathaus ohne Träger, geprüft durch
  vorübergehendes Abschalten), jetzt keine Toten, Hunger durchgehend 100. Vernünftiger Spieler
  Seed 7: 12 → 67 Bewohner (vorher 63), keine Toten; das Skript baut Häuser jetzt nach
  `immigrant_places`. Mit der Review-Kopie über ein Jahr: Geburten 5–8 und Hochzeiten 2–4 je Seed
  (vorher 0–2 bzw. 0–2); Seeds 1, 2, 7, 11, 42, 1234 bestehen alle Balance-Bedingungen.
* Hochzeitsdurchlauf (Testprofil): 3000 Ledige 0,68 ms (Review: 281 ms), 2000 → 0,42 ms.
* Unter Xvfb + lavapipe angesehen: Start (Seed 7, F3); Test-Siedlung nach 48 000 und 88 800 Ticks
  (12 Bewohner leben, Rathaus „1 von 2“ Arbeitern, vorher 0 Bewohner); 1280×720 mit vielen
  Meldungen (5 Karten, Unterkante ≈ 540 px über der Bauleiste ≈ 555 px; neueste oben, „×11“
  oben); Pausenmenü + 3× D + 2× Minus → Karte unverändert, nach dem Schließen bewegt D die
  Kamera; 8× F9: statische Batches 108 → 108, VmRSS 288 084 → 288 336 kB und dann konstant
  (Review: +4 MB je Laden), Raster an/Grenze aus bleiben; Seed-7-Stand in Seed-3-Spiel geladen →
  Kamera am Rathaus; Ordner statt Spielstand-Datei → „Speichern fehlgeschlagen: Anstelle der
  Spielstand-Datei liegt dort ein Ordner.“; Bauvorschau „(1 Baum)“, „1 Feld … wird mitgebaut“;
  Haus-Info ohne Stilllegen-Knopf, Abriss-Tooltip ohne Taste; Rathaus-Abriss-Tooltip mit dem
  Ablehnungsgrund der Simulation; Feldinfo „170 · Ackerboden“; Bevölkerungs-Tooltip
  „Frei für Zuwanderer“. Keine Panics, keine WARN/ERROR außer fehlendem Audiogerät und dem
  absichtlichen Speicherfehler.

**Offen**
* Tooltip-Flackern über mehrfeldrigen Gebäuden nur im Code behoben; per XTest nicht
  nachstellbar. Ein *leeres* fertiges Haus wurde nicht interaktiv angeklickt (die Regel ist per
  Unit-Test abgedeckt, der Knopf hängt nur an `pause_allowed`).
* Review-Kopie Seed 3: „zweite Ernte“ scheitert, weil das Testskript keinen Bauplatz für einen
  Vorratsspeicher findet und die Scheune voll bleibt (offener Punkt „Volle Scheunen“); im
  ausgelieferten Test (Seed 7) nicht betroffen.
* Test-Siedlung hat außer einem Hühnerhof keine Nahrung und keinen Markt: im Winter
  Nahrungsmangel, Holz wird komplett zu Brettern verarbeitet (Brennstoffmangel) – Folge der
  Bauliste, nicht der Zuteilung.
* Jeder Wechsel einer Produktion zwischen „untätig“ und „beschäftigt“ löst eine volle
  Arbeitszuteilung aus; bei sehr großen Reichen mit vielen Sammlern nicht gemessen.

## 2026-11-21 · Merge Schritt 2 – Sim-Lane Phase 6 + Spiel-Lane Etappe 2 (`crates/sim`, `data/`, `src/`, Doku)

**Fertig**
* `crates/sim` und `data/` aus der Sim-Lane übernommen (`sync.sh to-main`); die Spiel-Lane hatte
  beides nicht verändert. Keine neuen Abhängigkeiten, `Cargo.lock` unverändert.
* Doku zusammengeführt: `game_design.md` aus der Sim-Lane, `architecture.md` §1–3 aus der Sim-Lane
  und §4 aus der Spiel-Lane, `roadmap.md` kombiniert, beide Einträge unten in `progress.md`.
* Anpassungen in `src/` an die neue Sim-API:
  * `Vegetation::Stump` in Debug-Fenster („Baumstumpf (n h)“), Feld-Tooltip („Baumstumpf“) und
    Minikarte (wie freier Boden). Die Vegetationsebene zeichnet Stümpfe mit `nature/stump`;
    sonst stünde in der Bauvorschau „zu roden“ auf scheinbar leeren Feldern.
  * Info-Fenster: die neuen Status `Growing` (grün) und `OutOfSeason` (neutral) erscheinen nicht
    mehr als Fehler (rot). Die Produktivitätsfaktoren enthalten jetzt „Jahreszeit/Wetter“ aus
    `production::season_weather_permille`, sobald der Faktor ≠ 100 % ist (z. B. Winter 85 %).

**Geprüft**
* `cargo build` ohne Warnungen; `cargo test --workspace` grün (Spiel 41, Engine 41, Assetgen 54,
  Simulation 216 + 2 ignoriert, Determinismus 5, Balance 1, Doctest 1);
  `cargo clippy --workspace --all-targets -- -D warnings` sauber.
* Unter Xvfb + lavapipe (Seed 7) angesehen: Startzustand; Test-Siedlung nach 9600 Warp-Ticks
  (Tag 3, Stümpfe um den Holzfäller, Info-Fenster Holzfäller und Rathaus); nach 88 800 Ticks
  (Wintertag 1: verschneite Bäume, Sägewerk mit „Jahreszeit/Wetter 85 %“). Keine Panics, keine
  WARN/ERROR außer fehlendem Audiogerät.

**Offen**
* Test-Siedlung: alle 8 Erwachsenen arbeiten in der Produktion, das Rathaus bleibt ohne Arbeiter
  („Träger 0 von 0“). Abholungen und Belieferungen bleiben deshalb liegen (Holzfäller und
  Hühnerhof „Ausgangslager voll“, Sägewerk „+8 unterwegs“); zu Winterbeginn hat die Siedlung
  0 Bewohner. Die liegengebliebenen Transporte meldete die Spiel-Lane schon vor dem Merge; zu
  klären sind Arbeitszuteilung bzw. Träger (Sim-Lane) oder eine kleinere Test-Siedlung.
* Felder, Wetter, Schnee, Farm- und Wetterfenster stellt das Spiel noch nicht dar (nächste Etappe).

## 2026-11-21 · Sim-Lane, Phase 6 – Landwirtschaft, Jahreszeiten und Wetter (`crates/sim`, `data/`, Doku)

**Fertig**
* Neues Modul `agriculture`: Feldfarmen (`wheat_farm`, `vegetable_farm`, `orchard`) und Weidehöfe
  (`cattle_farm`, `sheep_farm`) beanspruchen Tiles **exklusiv** (Belegung Tile → Farm im Zustand,
  nächstgelegene zuerst, bis zur Höchstzahl; Gebiet, Boden mit Fruchtbarkeit ≥ 60, keine Gebäude,
  Straßen, Vorkommen, Bäume, Eingänge; Büsche/Stümpfe werden für Felder gerodet). Gebäude oder
  Straßen auf Feldern, Abriss, Versetzen und Gebietsverlust geben Tiles frei – offener Punkt
  „Farmen zählen dieselben Tiles“ erledigt, auch die Bauplatz-Prüfung zählt nur freies Land.
* Feldfrüchte aus `data/crops.ron` (neu: Säen-/Erntezeit je Feld, Erschöpfung, Fruchtwachstum):
  Aussaat in der Saison (nur wenn die Frucht mit 10 % Reserve vor dem Winter reift), stündliches
  Wachstum nach Jahreszeit × Wetter × Bodenfeuchte × Bodenqualität, Stufen 0–4, Ernte als
  Farmarbeit mit Produktivität der Arbeiter in die Scheune (Ausgangslager 800), Frost tötet
  unreife Einjährige, Erschöpfung/Brache/Erholung (Fruchtwechsel +50 %), Obstgärten mehrjährig
  (Blüte, Fruchtwachstum, Herbsternte), Sturmschäden und Verdorren, Forschung Feldertrag und
  Bewässerung. Feldfarmen produzieren damit wirklich; Weiden treiben Viehhöfe, Obstgarten-Felder
  die Imkerei. Neue Status `Growing` („Felder wachsen“) und `OutOfSeason`.
* Neues Modul `weather`: Tageswetter um Mitternacht aus den Jahreszeit-Gewichten (35 % bleibt das
  Wetter, Stärke 20–100 %, eigener gespeicherter `ChaCha8Rng`), Wirkungen bei Wachstum, Feuchte,
  Arbeit im Freien (neues Datenfeld `outdoor`), Heizbedarf im Winter, Krankheitsrisiko,
  Dürre-Durst und Feldschaden; Schneedecke und Bodennässe für die Optik; deutsche Meldungen
  (schwerer Sturm, Dürre, Regen nach Dürre, erster Schnee, strenger Frost, Frostschäden, Ernte).
* Jahreszeiten: Winter-Arbeitstempo −15 % jetzt auch in der Produktion (Hook
  `season_weather_permille` gefüllt), Fischerei/Weide/Imkerei nach Jahreszeit
  (`season_gathering`), Baustellen × Winter × Wetter, Winterbrennstoff × Heizbedarf des Wetters.
* Für die Darstellung: `agriculture().field_at`/`fields_in` mit Sprite-Namen des `fields`-Atlas,
  `take_field_dirty_chunks`, `weather()` (Wetter, Stärke, Schnee, Nässe). Wünsche der Spiel-Lane:
  `take_structure_dirty_chunks` + `structure_revision` (Straßen, Mauern, Grundflächen/Eingänge),
  Baumstümpfe (`Vegetation::Stump`, verrotten nach 48 h, Förster pflanzen darauf),
  Entwickler-Kommandos `DebugUnlockAllResearch` und `DebugSetWeather`, `building_elevation`.
* Neues Kommando `SetCrop` (Weizen/Gerste); Ereignisse `WeatherChanged`, `CropChanged`,
  `FieldsChanged`, `HarvestCompleted`, `CropsFrozen`, `DebugResearchUnlocked`.
* Berichte: `report::farm_report`, `agriculture_summary`, `weather_report` (deutsche
  Wirkungstexte), `food_forecast` (reichen die Vorräte über den Winter?); Feldfarmen zeigen im
  Produktionsfenster den Fortschritt der laufenden Feldarbeit.
* Spielstandformat **Version 6** (Wetter, Farmen, Felder, Stümpfe; Version 5 wird nicht migriert);
  `validate` prüft Farm ↔ Belegung ↔ Gebäude, keine Felder unter Gebäuden/Straßen; `rederive`
  kürzt Farmen über einer gesenkten Feld-Höchstzahl.
* Balancing (`rules.ron`, `crops.ron`, Doku §3, §4, §5.2, §5.3, §6.1, §8 neu geschrieben, §9):
  Erträge so, dass eine Gemüsefarm (3 Arbeiter) mit zwei Ernten ≈ 1100 Gemüse im Jahr liefert;
  Gemüse 6,5 statt 8 Tage (sonst gelingt die zweite Ernte nicht vor dem Frost).

**Geprüft**
* `cargo test -p kingdoms_sim`: 216 Unit-Tests (+2 ignorierte Leistungsmessungen), 5
  Determinismus-Tests (neu: Bauerndorf mit Gemüse, Obstgarten, Weide über 19 Tage bis in den
  Winter, Speichern/Laden nach 8 Tagen → gleiche Ereignisse je Tick und gleiche Bytes, zusätzlich
  zweiter Lauf von vorn gleich), Balance-Test, Doctest grün. Neue Tests u. a. exklusive Felder,
  Bauplatz zählt nur freies Land, Straße/Abriss gibt frei, Aussaat → Stufen → Ernte mit
  Statistik, späte Aussaat unterbleibt, Frost, volle Scheune, Obstgarten, Brache/Erholung, Sturm
  und Dürre, Wachstumsfaktoren, Weiden/Viehhöfe, Bienen, Stümpfe, Wetterziehung und Wirkungen,
  Debug-Kommandos, Struktur-Dirty-Chunks, Berichte, Validierung/rederive.
* Balance-Test `tests/reasonable_player.rs` jetzt ein ganzes Jahr plus Frühlingsanfang (nur
  Kommandos, Seed 7): Bevölkerung 12 → 63, keine Toten, Gemüseernten 664 (Sommer) und 501 (Herbst),
  kein Feld erfroren, Prognose zu Winterbeginn „reicht“ (Vorrat 1066 + Scheunen 1264 Sättigung),
  Hunger und Wärme durchgehend 100, 185 Holz zu Winterbeginn, Schatz nie unter 40 Gold,
  Tagesbilanz +44,7 Gold, Zufriedenheit 74. Mit vorübergehend geänderten Seeds 1, 42 und 1234
  ebenfalls grün (Bevölkerung 57–72, keine Hunger- oder Kältetoten, je zwei Ernten).
* Leistung (`--release`, 384²-Karte, 300 Farmen, 5600 beanspruchte Tiles, echtes Wetter):
  Landwirtschaftsschritt Ø 10,6–12,2 µs je Tick (3 Läufe), ganzer Tick Ø 24–29 µs, erster
  Wintertick (Frost über alle Felder) ≈ 56–64 µs; einzelne Ausreißer bis 1,6 ms ohne Bezug zu
  Stunden- oder Tagesgrenzen (geteilte CPU). Bevölkerungsmessung unverändert (Tick Ø 235 µs,
  Bevölkerungsschritt Ø 38,5 µs).
* `cargo clippy -p kingdoms_sim --all-targets -- -D warnings` sauber,
  `RUSTDOCFLAGS='-D warnings' cargo doc -p kingdoms_sim --no-deps` ohne Warnungen.
* Spiel gegen den neuen Sim-Stand in einer Kopie unter dem Scratch-Verzeichnis geprüft
  (`cargo check -p kingdoms --all-targets`, Haupt-Workspace unverändert): einzige Brüche sind 4
  erschöpfende `match` auf `Vegetation` (neue Variante `Stump`) in `src/ui/debug.rs`,
  `src/ui/hud/format.rs`, `src/ui/hud/minimap.rs`, `src/view/vegetation.rs`; mit je einem Arm
  fehlerfrei und ohne Warnungen.

**Offen**
* Darstellung (Spiel-Lane): Feld-Ebene aus `fields_in`/`take_field_dirty_chunks`, Baumstümpfe,
  Schnee/Nässe/Regen, Farm-/Wetterfenster und Nahrungsprognose; Straßen-Ebene kann auf
  `take_structure_dirty_chunks` umstellen.
* Gebäudeschäden durch Sturm und Wetter-Ereignisse (`Weather`, `FieldYield`) kommen mit dem
  Ereignissystem; `DebugSetWeather` nutzt schon dieselbe Erzwingung.
* Tagesablauf der Bauern (länger im Sommer) erst mit den Laufwegen; Feldarbeit ist bis dahin
  abstrakt (kein Bauer läuft aufs Feld).
* Ohne Werkzeuge arbeiten Holzfäller mit 60 %; im Balance-Test begrenzt Holz (Häuser + 0,6
  Heizwert je Bewohner und Wintertag) das Wachstum ab dem Herbst – ein spürbarer, aber
  gewollter Engpass; Kohle/Köhlerei brauchen Forschung (Phase 7).
* Volle Scheunen: Gemüse wartet in der Scheune, wenn die Lager voll sind; Haushalte nehmen nur aus
  Lagern – Vorratsspeicher bauen (der Balance-Test tut es, wenn die Scheunen sich füllen).

## 2026-11-21 · Spiel-Lane, Etappe 2/2 – Spielerinteraktion und HUD (`src/`, Audio-Manifest, Doku)

**Fertig**
* **Werkzeuge** (`src/tools`, Zustandsautomat ohne Rendering): Inspizieren (Auswahl, Feld-Tooltip),
  Bauen mit Geist-Vorschau (grün/rot, Feldmarkierungen frei/Rodung/blockiert mit Kreuz, Eingang,
  vorgeschlagene kostenlose Zufahrt, die mit dem Gebäude gebaut wird), Drehen (R, Shift+Mausrad),
  Klick platziert (Shift: weiterbauen), Rechtsklick/Esc bricht schrittweise ab; Straßen ziehen mit
  Live-Route aus `plan_road` (nur bei neuem Endfeld neu geplant), Länge/Kosten/Probleme, Loslassen
  baut; Abriss von Gebäuden (Rückfrage bei Hauptquartier, bewohnten, gefüllten oder teuren
  Gebäuden; Erstattungsvorschau) und Straßen (Klick oder Ziehen); Versetzen (aufnehmen, Vorschau
  mit `check_move`, platzieren). Alles läuft über die Kommando-Warteschlange; abgelehnte Kommandos
  werden zu einem deutschen Hinweis plus `ui_error`.
* **HUD** (`src/ui/hud`, Theme und Widgets aus Etappe 1, Symbole aus den Atlanten `icons` und
  `roads` per `register_egui_texture`): Leiste oben aus dunklem Holz (Schatz mit Tagesbilanz-Trend
  und Aufschlüsselung, Bevölkerung/Wohnraum, Zufriedenheit mit Faktoren, Nahrung und Kernwaren mit
  Bestand/Erzeugung/Verbrauch, Datum mit Jahreszeit und Uhrzeit, Tempo Pause/1×/2×/4×, Menü);
  Baumenü unten mit sechs Reitern inkl. „Straßen & Mauern“, Tooltips aus den echten Daten,
  gesperrte Typen deaktiviert mit Grund (B blendet um); Info-Fenster rechts für Gebäude und Felder
  (Status mit Grund, Bau/Ausbau mit Material und Bauarbeitern, Arbeiter und Priorität,
  Produktion mit Produktivitätsfaktoren, Rezeptwahl, Puffern und Aufträgen, Häuser mit Bewohnern,
  Bedürfnissen, Ausbau-Voraussetzungen, Ausbau und Sperre, Lagerinhalt, Gebiet/Dienste,
  Stilllegen/Versetzen/Abreißen); Meldungen links (Farbe je Schwere, Spielzeit, Klick springt
  zum Ort, Ausblenden, Anzahl nach Bildschirmhöhe) und Hinweise oben; Minikarte unten rechts
  (Pergamentrahmen, inkrementell je Chunk, Sichtrahmen, Klick/Ziehen); Pausenmenü (Fortsetzen,
  Schnellspeichern, Schnellladen, Beenden) und Abriss-Dialog als Modal.
* **Schnellspeichern/-laden** (F5/F9, Menü) nach `saves/quicksave.ksav` über die Speicher-API der
  Simulation; deutsche Meldungen bei Erfolg und Fehler, das Spiel läuft bei Ladefehlern weiter.
* **Audio-Anbindung** (`src/sound.rs`): UI-Klänge, Weltklänge zu Ereignissen mit Stereo-Position
  (`play_placed`), Produktionsklänge je Gebäudetyp, Sprachausgabe `vo_*`, Umgebungsebenen nach
  Kameraumgebung und Tageszeit, Musikstimmung Friedlich (Winter im Winter). Neue Manifest-IDs mit
  deutschen Beschreibungen: `sfx_downgrade`, `sfx_road_remove`, `sfx_tree_plant`,
  `sfx_death_bell`, `sfx_wedding`, `vo_bankrupt`, `vo_emigrants`, `vo_deposit_depleted`,
  `vo_storage_full`. Ohne Audiodateien bleibt alles stumm.
* Tasten: B, Entf/X, R, M, Esc, F5/F9; Hinweise in den Tooltips. Zeiger über der Oberfläche wirkt
  nie auf die Karte (Drücken wird gefiltert, Ziehen endet über der UI ohne Wirkung, Shift+Mausrad
  dreht statt zu zoomen).
* Entwickler-Hilfen für Screenshots: `KINGDOMS_TOOL`, `KINGDOMS_CURSOR`, `KINGDOMS_MENU`,
  `KINGDOMS_QUICK=save|load` (README). Die Test-Siedlung baut zusätzlich einen Hühnerhof.
* Doku: README (Steuerung, Optionen), `architecture.md` §4 (UI-Aufbau), Roadmap.

**Geprüft**
* `cargo test --workspace` grün: Spiel 41 Tests (neu u. a. Bauwerkzeug über echte Kommandos bis
  `BuildingPlaced`, Shift/ungültige Plätze, Drehung, Straßen-Ziehen und Abbruch über der UI,
  Abbruch-Reihenfolge, Abriss mit Rückfrage und Straßen-Ziehen, Versetzen inkl. Hauptquartier-
  Ablehnung, Erstattung, Schnellspeichern/-laden mit beschädigter Datei, alle Sound-IDs im
  Manifest, Ereignis-Klänge, Umgebungsebenen, Zahlenformat, Minikarten-Projektion,
  Tastenbelegung ohne Doppelungen); Engine 41, Assetgen 54, Simulation 190 + 4 + 1 + 1.
* `cargo clippy --workspace --all-targets -- -D warnings` sauber.
* Sichtprüfung unter Xvfb + lavapipe (Screenshots angesehen): HUD bei 1600 × 900 und 1280 × 720
  (kompakte Reiter, Meldungen, Minikarte), Bauvorschau gültig (Sägewerk mit Zufahrts-Vorschlag)
  und ungültig (uneben, Kreuze, Gründe), Straßen-Ziehen (Schotterweg um Felsbrocken, Kosten),
  Abriss-Vorschau mit Erstattung, Versetzen (Hühnerhof), Info-Fenster Sägewerk, Hühnerhof,
  Baustelle und Holzhaus (Bewohner, Bedürfnisse, Ausbau-Voraussetzungen), Pausenmenü,
  Hinweise für Speichern, Laden und fehlgeschlagenes Laden. UI-Zeit im Debug-Build ≈ 3,5 ms je
  Frame.

**Offen**
* Echte Mausklicks, Tooltips der Leiste/des Baumenüs und die Abriss-Bestätigung konnten unter Xvfb
  nicht ausgelöst werden (kein xdotool); die Logik dahinter ist unit-getestet, die Darstellung der
  Vorschauen über `KINGDOMS_TOOL`/`KINGDOMS_CURSOR` geprüft.
* Produktivitätsfaktoren: Jahreszeit/Wetter fehlen in der Aufschlüsselung (der Faktor ist in der
  Simulation privat); die Abriss-Erstattung wird im Spiel aus öffentlichen Regeln nachgerechnet.
* Verwaltungsfenster (Lager, Finanzen/Steuern, Handel, Arbeitskräfte-Prioritäten, Bevölkerung),
  Hauptmenü und Spielstand-Liste fehlen noch; Audiodateien fehlen (IDs sind bereit).
* Der Hühnerhof der Test-Siedlung meldet „Ausgangslager voll“ (Fleisch wird nicht abgeholt) – zu
  prüfen in der Sim-Lane (Lagerannahme/Träger).

## 2026-11-21 · Sim-Lane, Phase 5 Teil 1 – Bevölkerung als einzelne Bewohner (`crates/sim`, `data/`, Doku)

**Fertig**
* Neues Modul `population`: Bewohner in einer `Arena` mit `CitizenId` (deutscher Vor- und
  Familienname aus neuem `data/names.ron`, Geschlecht, Geburtstag – 1 Lebensjahr je Jahreszeit –,
  Lebensphase Kind < 14 / Erwachsen / Alt ≥ 61, Haushalt, Partner, Eltern, Arbeit
  (`Employment`), 9 Bedürfnisse, Gesundheit, Zufriedenheit, Bildung, Tätigkeit nach Tagesablauf
  §6). Haushalte (Familien) mit Vorrat, Versorgungszustand und Familiennamen; Wohnungen
  (`Home`) je fertigem Haus. Vorbereitet für Laufwege: Wohnung, Arbeitsplatz und Tätigkeit sind
  Felder, eine Position kommt im nächsten Schritt dazu.
* Versorgung (§6/§7): Haushalte füllen stündlich einen Tagesvorrat auf – mit Markt Nahrung,
  Winter-Brennstoff und die Waren ihrer Wohnstufe (plus nächster Stufe, falls nicht gesperrt) aus
  den nächstgelegenen Lagern, ohne Markt nur Nahrung/Brennstoff aus einem Lager in
  Trägerreichweite; gebundenes Baumaterial bleibt unangetastet. Verbrauch läuft in die
  Ressourcenstatistik (`DayFlows::household`, Teil von `consumed`, wirkt auf Preise). Neues
  Ressourcenfeld `household_use`.
* Bedürfnisse, Gesundheit, Zufriedenheit mit Faktorzerlegung (`HappinessFactor`, deutsche
  Labels), Produktivität der Arbeiter/Bauarbeiter aus Gesundheit × Zufriedenheit (Produktion und
  Bau rechnen mit der Summe der Effizienzen statt `min(besetzt, Plätze)`).
* Demografie: Alter, Bildung (Schule), Tod durch Alter/Krankheit/Hunger/Kälte (Statistik nach
  Ursache), Geburten, Hochzeiten, Zuwanderung am Haupt-Hauptquartier (Tageslimit),
  Abwanderung unzufriedener Haushalte; Tageszähler + 48-Tage-Historie (`PopulationStats`), Abschluss
  im Statistikschritt.
* `workforce` teilt echte Bewohner zu (stabil, nächstwohnende Arbeitslose zuerst, gebildete Jobs
  nur für Gebildete, Prioritäten wie bisher); Kinder und Alte arbeiten nicht; Stilllegen/Abriss
  gibt frei. `Workforce` zählt nur noch (Bevölkerung, Erwerbsfähige, Beschäftigte).
* Geld: Steuern je Haushalt (Mitglieder × Stufensatz × Steuersatz im Verwaltungsradius),
  Marktgebühr je versorgtem Bewohner, Löhne je beschäftigtem Bewohner – Milli-Gold wie bisher.
* Häuser: Ausbau prüft die echten Bewohner (Bedürfnis-Mittel ≥ 60, Waren im Vorrat jedes
  Haushalts, Versorgung; neuer Blocker `NoResidents`); **Abstieg** nach 48 h unerfüllter eigener
  Stufe (Grundfläche schrumpft um den Eingang, Überzählige werden obdachlos und ziehen um) –
  offener Punkt aus dem letzten Eintrag erledigt. Abriss/Umzug eines Hauses: Haushalte ziehen
  in passende freie Häuser.
* Szenariostart: 12 Siedler als 4 Paare + Kinder, so verteilt, dass die Starthäuser voll werden
  (10 wohnen, 2 obdachlos).
* Report-API (`report::population_summary`, `needs_summary`, `happiness_report`,
  `house_report`, `residents`, `citizen_detail`; Haushaltsverbrauch in `ResourceRow`/
  `ResourceDay`). Neue Ereignisse `CitizenBorn`, `CitizenDied`, `CitizensMarried`,
  `HouseholdArrived`, `HouseholdEmigrated`, `HouseholdMoved`, `HouseDowngraded`; deutsche,
  je Thema gedrosselte Meldungen (Nahrungs-/Brennstoffmangel, Obdachlose, Zu-/Abwanderung,
  Geburt, Tod, Abstieg). Keine neuen Kommandos (§6 sieht außer Berufsprioritäten keine vor).
* `validate` prüft alle Querverweise Bewohner ↔ Haushalt ↔ Haus ↔ Arbeitsplatz/`staffed`;
  `rederive` gleicht geänderte Altersgrenzen und Hauskapazitäten ab. Spielstandformat
  **Version 5** (Version 4 wird nicht migriert).
* Balancing (`rules.ron` → `population`, Doku §4/§5.1/§6.1/§7/§13): Nahrungsbedarf 4 Sättigung
  pro Erwachsenem und Tag (statt ≈ 1 je Wachstunde – damit ernährt ein Hühnerhof ≈ 9, eine
  Fischerei ≈ 10 Erwachsene; eine Bäckerei mit Feldern später ≈ 36), Gewichte so, dass ein
  einfaches Dorf ≈ 72 Zufriedenheit erreicht (Zuwanderung ab > 70) und Kirche/Gasthaus/Krankenhaus
  sie anheben. Steuern beim Standardsatz decken Löhne und Unterhalt (Dorf mit 33 Bewohnern im
  Balance-Test: +28 Gold/Tag laufende Bilanz).
* Zusammenführung mit der Spiel-Lane (`src/`): die Weltansicht gleicht Gebäude und Straßen auch
  bei `HouseDowngraded` neu ab; das Debug-Fenster zeigt die Bevölkerung als alle Bewohner mit
  Erwerbsfähigen und Beschäftigten.

**Geprüft**
* `cargo test -p kingdoms_sim`: 190 Unit-Tests (+1 ignorierte Leistungsmessung) + 4
  Determinismus-Tests (inkl. neuem Test „Bevölkerung läuft nach Laden identisch weiter“, 4 Tage)
  + 1 Balance-Test + 1 Doctest grün. Neue Tests u. a. Startfamilien, Essen aus dem Lager mit
  Statistik, Durst/Brunnen, Verhungern, Markt bringt Stufenwaren, Winterbrennstoff, Geburten,
  Alterstod, Zuwanderung mit Tageslimit, Abwanderung, Hochzeit, Volljährigkeit mit Bildung und
  Arbeit, Abstieg (auch 3×3 → 2×2), Abriss von Haus/Arbeitsplatz, Effizienz, Zufriedenheitsfaktoren,
  Reports, `rederive` mit umbalancierten Daten, Validierungsfehler.
* Balance-Test `tests/reasonable_player.rs` (nur Kommandos, 10 Tage, Seed 7): Bevölkerung 12 → 33,
  niemand verhungert, Nahrung produziert ≥ verbraucht (letzte 3 Tage), Schatz nie unter 239 Gold,
  laufende Bilanz positiv, Zufriedenheit 73; zusätzlich manuell mit Seeds 1, 42, 1234 grün.
* Leistung (`cargo test --release -p kingdoms_sim population_performance -- --ignored`,
  ≈ 2000 Bewohner, 500 Haushalte, 400 Häuser, 2 Spieltage): Bevölkerungsschritt Ø 36–42 µs je
  Tick, ganzer Tick Ø 207–252 µs (5 Läufe); einzelne Ausreißer bis ≈ 1 ms ohne Bezug zu
  Stunden-/Tagesgrenzen (geteilte CPU).
* `cargo clippy -p kingdoms_sim --all-targets -- -D warnings` sauber,
  `RUSTDOCFLAGS='-D warnings' cargo doc -p kingdoms_sim --no-deps` ohne Warnungen.
* Nach der Zusammenführung im Hauptprojekt: `cargo test --workspace` (Spiel 22, Engine 41,
  Assetgen 54, Simulation 190 + 4 + 1 + 1) und `cargo clippy --workspace --all-targets -- -D warnings`
  grün; Spiel unter Xvfb (Screenshots angesehen): Start Seed 7 (12 Bewohner, 8 erwerbsfähig),
  Entwickler-Siedlung nach 9600 Warp-Ticks (Tag 3, 8 beschäftigt) und 48 000 Ticks ohne Panik.

**Offen**
* Laufwege, A*, Tagesablauf mit Positionen und Träger als Läufer (nächster Schritt); bis dahin
  ist `Activity` nur der Tagesablauf nach Uhrzeit.
* Ersparnis/Wohlstand und Freunde aus §6 sind noch nicht modelliert; Ereignis-Modifikatoren der
  Zufriedenheit (Feste, Unruhen) kommen mit dem Ereignissystem.
* Feldfarmen liefern erst mit der Landwirtschaft; bis dahin tragen Hühnerhof und Fischerei die
  Ernährung. Im Winter braucht jeder Bewohner 0,6 Holz-Heizwert am Tag – ohne Förster und genug
  Holzfäller wird es kalt (im 30-Tage-Probelauf gab es Brennstoffmangel, aber keine Kältetoten).
* Ohne Nahrungserzeuger verhungert die Startsiedlung nach ≈ 7 Tagen (Probelauf Seed 7 ohne
  Kommandos); das gilt auch für die Entwickler-Siedlung (`KINGDOMS_DEV_SETTLEMENT`), die keine
  Nahrung baut – lange Warp-Läufe zeigen dann ein leeres Dorf.

## 2026-11-21 · Spiel-Lane, Etappe 1/2 – Die lebendige Welt (`src/`, Doku)

**Fertig**
* **Sitzung** (`src/session`): `Defs` werden beim Start einmal aus `data/` geladen (`Arc<Defs>` in
  `GameShared`; ungültige Daten brechen mit der vollständigen Problemliste ab). `Session` hält den
  `GameState` (Seed `KINGDOMS_SEED`, Standard 7; Größe `KINGDOMS_WORLD_SIZE`, Standard `medium`),
  eine Kommando-Warteschlange (`queue_command`, angewendet im nächsten Tick), genau einen
  `GameState::tick` je Engine-Tick (Tempo Pause/1×/2×/4× unverändert über Leertaste/1/2/3), ein
  begrenztes Ereignis-Log (Ringpuffer 2048, Leser mit eigenem Cursor, verpasste Ereignisse werden
  gemeldet) und gemessene Tickzeiten. Abgelehnte Kommandos landen mit deutschem Grund im Log.
* Entwickler-Hilfen (README): `KINGDOMS_WARP_TICKS=<n>` (Ticks direkt nach dem Start),
  `KINGDOMS_DEV_SETTLEMENT=1` (Test-Siedlung ausschließlich über echte `PlaceBuilding`-/`BuildRoad`-
  Kommandos im Rahmen des Startbudgets: Sägewerke 2×3 nach Süden/Westen, Holzfäller nach Norden,
  Förster, ein Holzfäller auf einer Geländekante, Zufahrten aus Schotter und Pfad),
  `KINGDOMS_FOCUS`, `KINGDOMS_ZOOM`, `KINGDOMS_SELECT`.
* **Darstellung des Simulationszustands** (`src/view`): Terrain aus der `WorldMap` des Zustands mit
  Dirty-Chunks (alle tile-abhängigen Ebenen werden neu gebaut); Vegetation/Vorkommen als statische
  Chunk-Batches (Art × Jahreszeit × Stufe nach `nature::tree_stage`, also passend zur Fällgrenze;
  Wind mit Phase je Baum, Versatz/Spiegelung/Tönung je Tile; Neuaufbau beim Jahreszeitwechsel);
  Straßen je Typ mit 16er-Masken aus `GameState::road_mask` und Brücken (statische Batches;
  Änderungserkennung: nach Straßen-/Gebäudeereignissen Abgleich der aufgelösten Sprites aller
  Straßen-Tiles, nur geänderte Chunks werden neu gebaut); Gebäude jedes Frame mit Culling:
  fertige Gebäude, Baustellen `construction_<B>x<T>_<0..2>`, Versetzen = Baustelle am neuen Ort,
  **Hausausbau** = Gerüst der Zielgröße auf der reservierten Grundfläche und das weiter bewohnte
  Haus auf seiner alten Grundfläche davor, Mauern `wall_<maske>`, Tore `gate_<x|y>` nach Mauerlinie,
  dezente Tönung für stillgelegte und unbesetzte Gebäude. Gebäude stehen auf der höchsten
  Höhenstufe ihrer Grundfläche.
* **Sprite-Ausrichtung** aus der Grundflächen-Semantik der Simulation abgeleitet (`view::buildings`,
  Tests): ein Sprite für `B×T` deckt ungespiegelt `(x: T, y: B)` ab, gespiegelt `(x: B, y: T)` (Anker
  wird mitgespiegelt). Nicht-quadratische Typen: Drehung 0/2 gespiegelt, 1/3 nicht – die Grundfläche
  passt immer. Quadratische Typen: gespiegelt bei Eingang Nord/Ost, damit die Tür auf der sichtbaren
  Wand neben dem Eingang liegt (Süd/Ost exakt, West → Südwand, Nord → Ostwand).
* Gebietsgrenze des Königreichs (dünne Bernstein-Linie, `T`, Standard an), Raster (`G`),
  **Hover und Auswahl**: Picking über Belegung und Sprite-Silhouette mit Tiefenvergleich (hohe
  Sprites sind auch über dem Dach anklickbar, der Boden davor verdeckt sie), Umriss der Grundfläche
  am Boden plus schwacher Umriss über allem, Aufhellen des Gebäudes; Linksklick wählt
  (nur wenn der Druck auf der Karte begann), Esc hebt auf. `Selection` liegt für das Info-Fenster
  in der Weltszene bereit. Kamera startet am Rathaus (H/Pos1 springt dorthin).
* Debug-Fenster (F3): Abschnitt Simulation (Datum, Uhrzeit, Tick und Tempo, Tickzeit zuletzt/Ø/max,
  Bevölkerung, Gold, Gebäude/Baustellen, Straßenfelder, Gebiet, wartende Befehle, Ereignisse),
  Timing inkl. „map sync“, Auswahl-Details (Typ, Status, Grundfläche, Drehung, Eingang,
  Straßenanschluss, Arbeitskräfte, Baufortschritt, Produktion, Inhalt), Ebenen-Statistik,
  letzte Ereignisse.
* Doku: README (Steuerung, Entwickler-Optionen), `architecture.md` §4, Roadmap Phase 2–4.

**Geprüft**
* `cargo test --workspace` grün (Spiel 22 Tests, u. a. Sprite-Ausrichtung 2×3/3×4/2×2 in allen
  Drehungen, gespiegelter Anker = vordere Ecke = Tiefen-Split, Silhouetten-Picking, Torachse,
  Ereignis-Log mit Überlauf, Startoptionen, Kommando-Warteschlange, Test-Siedlung über Kommandos;
  Engine 41, Assetgen 54, Simulation 170 + 3 + 1); `cargo clippy --workspace --all-targets -- -D warnings`
  sauber.
* Sichtprüfung (Xvfb + lavapipe, Screenshots angesehen): Start Seed 7 bei Zoom 1, 0,45, 0,6, 1,6, 2,
  2,5 und 4; Test-Siedlung nach 600/2400/4800 Ticks (Baustellen aller Stufen, fertige Sägewerke
  gespiegelt/ungespiegelt, Platten deckungsgleich mit der Raute, Zufahrten mit korrekten
  Verbindungen, Schotter und Pfad); Seeds 1, 3, 42 (Laub-/Nadelwald, Vorkommen, Förster pflanzt
  Setzlinge, die erscheinen); Gebäude auf Geländekante (Seeds 7, 42: Klippe davor korrekt sichtbar);
  Brücken (Seeds 5, 6: Brücke hinter dem Rathaus korrekt verdeckt); Winter nach 87 000 Ticks
  (kahle Bäume, Schneekiefern, große Rodung des Holzfällers); Bäume vor/hinter Gebäuden, Straßen vor
  Gebäudefronten, Hover (Cursor in Bildmitte) und Auswahl per `KINGDOMS_SELECT`.
* Zeiten (Debug-Build, Software-Rasterizer): Tick 0,003–0,08 ms, „map sync“ Ø 0,01 ms (max 0,6 ms),
  Renderliste 0,05 ms; 4800 Warp-Ticks ≈ 10 ms, 87 000 ≈ 0,3 s.

**Offen**
* Nicht auf dem Bildschirm geprüft, weil per Kommando noch nicht erreichbar: Kirche (3×4, nur per
  Unit-Test), Mauern/Tore (Forschung nötig; noch kein Forschungskommando), Hausausbau (Bedürfnisse
  ab Phase 5), Versetzen und Pausieren (noch keine Werkzeuge – Etappe 2).
* Asset-Konvention: die Gebäude-Sprites tragen die Tür auf der `T`-Seite, die Simulation legt den
  Eingang auf die `B`-Seite (Breite). Bei nicht-quadratischen Gebäuden zeigt die gezeichnete Tür
  daher auf die Nachbarwand des echten Eingangs; Abhilfe im Asset-Generator (Tür auf die `B`-Wand).
* Beim Herauszoomen (< ≈ 0,6) zeigen sich dunkle Linien an den Terrain-Tilekanten (vorbestehend:
  Mipmaps mischen die Seitenwand des Terrain-Sprites in die Unterkanten der Oberfläche).
* Baumstümpfe gibt es in der Simulation noch nicht; Straßenänderungen werden mangels
  Revisionszähler über Ereignisse + Abgleich erkannt.

## 2026-11-21 · Review-Korrekturen Simulation (`crates/sim`, `data/`, Doku)

**Fertig** (je Befund aus zwei unabhängigen Reviews; alle bestätigt und behoben)
* Vorkommen: Sammler erzeugen nichts mehr aus dem Nichts – verschwindet das Ziel-Vorkommen
  während des Arbeitsgangs und gibt es kein anderes, liefert der Gang 0 Einheiten. Laufende
  Arbeitsgänge **reservieren** ihre Höchstausbeute je Tile; ein neuer Gang startet nur auf einem
  Tile mit unreservierten Einheiten (auch für geteilte Fischgründe).
* Abriss eines Hauses im Ausbau lagert die 50-%-Erstattung jetzt tatsächlich ein (plus
  ungenutzter Ausbau-Anteil).
* `Rotation::grown_origin` verankert am **Eingangs-Tile**: Ausbauten (auch Residenz 3×3 → Adelssitz
  4×4) behalten den Eingang, die vorhandene Straße beliefert die Ausbau-Baustelle weiter.
* **Tore tragen Straßen**: Straßen dürfen auf Tor-Tiles liegen (Planer, `BuildRoad`, Validierung);
  so verbindet ein Tor beide Seiten einer Mauerlinie. Mauersegmente blockieren weiterhin.
* Tor auf eine **Mauer-Baustelle**: die Baustelle wird wie beim Abbrechen erstattet (Gold und
  geliefertes Material, Ereignis `ConstructionCancelled`) statt verloren zu gehen.
* Eingang über eine Klippe: `EntranceStatus::Blocked`, wenn das Eingangs-Tile > 1 Höhenstufe von
  der Tür entfernt ist; `has_road_access`, `building_component`, `buildings_connected` und
  `road_mask` prüfen das ebenfalls (`Building::entrance_reachable`, neue `world`-Parameter).
* Spielstände: überlaufsichere Längenprüfung (`checked_add`, `bytes.get`), `WorldMap`
  deserialisiert über `try_from` (Größe 1..=`MAX_MAP_EDGE` = 384, Tile-Anzahl, Höhen,
  Start/Siedlungsplätze im Bereich), `Buildings`/`RoadNetwork`/`Territory` prüfen ihre Größe
  **vor** jeder Allokation (`OccupancyError::InvalidGridSize`, `RoadGridError` jetzt Enum,
  `TerritoryError`). Manipulierte Dateien liefern `SaveError` statt Panic/Abbruch.
* Balancing-Änderungen: `GameState::rederive` (nach `validate` in `save::decode`) setzt Lager- und
  Pufferkapazitäten aller fertigen Gebäude, Gebiet und Lagersicht aus den aktuellen Daten neu
  (bei unveränderten Daten ein No-op → Laden bleibt bytegenau). `Defs::fingerprint` umfasst
  zusätzlich die Datenform (Gebäudegrößen, Produktion/Rezeptanzahl, Lager, Wohnen, Brücken);
  Formänderungen ergeben `DataMismatch` statt `InvalidState`.
* Weltgenerator nutzt für `sin/cos/exp/pow` die reine Rust-Crate `libm` (neue Abhängigkeit
  `libm = "0.2.16"`) → gleiche Welt für denselben Seed auf allen Plattformen; Golden-Checksummen
  für Seeds 1 und 42 (Debug und Release geprüft).
* **Automatischer Hausausbau** (§5.1): stündlich im Bau-Schritt starten fertige, nicht
  stillgelegte Häuser ohne Ausbausperre den Ausbau, sobald nichts blockiert (Info-Meldung);
  neues Kommando `SetUpgradeLocked { building, locked }`, Ereignis `UpgradeLockChanged`,
  `Building::is_upgrade_locked`. Manueller Ausbau bleibt trotz Sperre möglich.
* Pipeline-Schritt 10 `statistics`: stündliche Stichprobe (`HourSample`: Schatz, Lagerbestand,
  Bevölkerung, Beschäftigte; rollierend 72 h, `GameState::statistics()`), Tagesabschluss
  (Kassenbuch, Preise, Ressourcenstatistik, Handelslimits) läuft jetzt als letzter Schritt des
  Ticks. `GameClock::total_hours`. `docs/architecture.md` §3.2–3.4 aktualisiert.
* `docs/game_design.md` §2: Lauftempo gepflastert 0,28 (= 0,2 × 1,4 laut §5.6/`roads.ron`).
* Spielstandformat **Version 4** (Ausbausperre, Stundenhistorie, Form-Fingerprint); Version 3 wird
  nicht migriert (nur interne Entwicklungsstände).

**Geprüft**
* `cargo test -p kingdoms_sim`: 170 Unit-Tests + 3 Integrationstests + 1 Doctest grün; neue
  Regressionstests u. a. geteilte Vorkommen/Fischgründe, verschwundenes Vorkommen, Abriss im
  Ausbau, Adelssitz-Ausbau mit Belieferung über die alte Straße, Straße durch Tor, Tor auf
  Mauer-Baustelle, Eingang über Klippe, präparierte Längen/Weltkarte/Gittergrößen, Laden mit
  umbalancierten Daten, automatischer Ausbau mit Sperre, Stundenhistorie, Golden-Checksummen.
* `cargo clippy -p kingdoms_sim --all-targets -- -D warnings` sauber, `cargo doc -p kingdoms_sim
  --no-deps` ohne Warnungen, `cargo check -p kingdoms` grün, `cargo test --workspace` grün,
  `cargo clippy --workspace --all-targets` ohne Warnungen.

**Offen**
* Hausabstieg bei länger unerfüllten Bedürfnissen (§5.1) braucht echte Haushalte (Phase 5).
* Ein Tor kann nur auf eine vorhandene Mauer gesetzt werden; eine Mauer quer über eine
  bestehende Straße erfordert weiterhin: Straßenstück entfernen → Mauer → Tor → Straße.
* `rederive` setzt Arbeitskräfte nicht neu zu; geänderte Arbeiterzahlen wirken ab der nächsten
  Neuzuteilung (Produktivität rechnet bereits mit `min(besetzt, Plätze)`).

## 2026-11-21 · Lane A, Schritt 3/3 – Wirtschaft & Produktion (Phase 4, `crates/sim`)

**Fertig**
* `economy::Inventory` (`BTreeMap<ResourceId, u32>` + Kapazität je Ressource, Überschuss bei
  gesenkter Kapazität). Lagergebäude (Rathaus 200, Lagerhaus 400, Vorratsspeicher 150 nur Nahrung,
  Handelskontor 100, Burg 300; `StorageBonus` der Forschung) halten den Bestand in `Building::inventory`.
  **Königreichslager** = Sicht über alle betriebsbereiten Lager, die mit dem Haupt-Hauptquartier per
  Straße verbunden sind (ohne HQ: alle). Zusätzlich `committed` (an Baustellen geschuldetes Material);
  `available = Bestand − committed` gilt für Platzierung, Straßen und Ausbau.
* Staatsschatz intern in Milli-Gold (stündliche Buchungen exakt), Tageskassenbuch nach Kategorie
  (Steuern, Marktgebühren, Exporte, Löhne, Unterhalt, Importe, Bau, Militär) + rollierende Historie
  48 Tage; Ressourcenstatistik (produziert/verbraucht/importiert/exportiert, Tagesend-Bestand und -Preis).
* `production`: Rezepte aus den Daten, Eingangs-/Ausgangspuffer (Standard 8, Werkzeugpuffer 1),
  Arbeitsgänge über Spielzeit × Produktivität (Besetzung × Werkzeuge 100/60 % × Forschungsbonus ×
  Umgebung (Weide-Abdeckung, Bienen-Bonus) × Jahreszeit/Wetter-Hook 1,0 × Bankrott 50 %), 1 Werkzeug je
  20 Arbeitsgänge, Bruchteile (0,25 tools, 0,5 honey) als Guthaben, optionale Eingänge mit Bonus.
  Status mit Grund (`Working`, `MissingInput(r)`, `OutputFull`, `NoWorkers`, `Paused`,
  `NoResourcesInRange`, `Idle`, `UnderConstruction`), Warnung nach 1 h ohne unterwegs befindliche
  Lieferung (max. 2× täglich je Gebäude). Rezeptwahl, Priorität, Warteschlange („N Einheiten“ vor
  „dauerhaft“, max. 16). Sammler: Holzfäller fällt ausgewachsene Bäume (≥ 192) im Radius, Förster
  pflanzt Setzlinge, Steinbruch/Minen/Lehm-/Sandgrube bauen Vorkommen ab (leer → entfernt), Fischerei
  nutzt Fischgründe (bleiben, regenerieren). Tiles werden für Renderer als „dirty“ markiert.
* `nature`: Baumwachstum (3/h × Jahreszeit, Winter 0; Dirty nur bei Stufenwechsel) und
  Fisch-Regeneration (1/Tag, max. 600), zeilenweise über die Stunde verteilt.
* `logistics`: Auftragswarteschlange (Abholung Ausgang → Lager, Rücktransport von Überschuss,
  Belieferung Lager → Eingang, Baumaterial Lager → Baustelle) mit Planer alle 20 Ticks und
  **abstrakten Trägern** (jeder Arbeiter eines Lagers = 1 Träger; Reisezeit aus Straßen-Dijkstra mit
  Straßentempo + Ladezeit). Nur Straßenverbindung + Trägerradius des Lagers erlauben Austausch.
  Trennung Planer/Warteschlange ↔ Ausführer dokumentiert (Phase 5 ersetzt nur den Ausführer, nutzt
  `pick_up`/`deliver`).
* Bau neu: Gold bei Platzierung, Material liefern Träger; Baustellen fordern Bauarbeiter nur an,
  solange Material für Fortschritt da ist (verhindert Blockade der Träger). Rodungsholz, Erstattungen,
  Abrissinhalte gehen ins nächste Lager (Überlauf geht verloren, Ereignis + Warnung).
* `workforce`: Bevölkerung (Szenario-Siedler) = Arbeitskräfte; Zuteilung nach Kategorie-Priorität →
  Gebäude-Priorität → Kategorie-Reihenfolge → ID; Hausbelegung (`housing_occupancy`), Übersicht
  (Beschäftigte, Arbeitslose, offene Stellen, je Kategorie, Wohnraum, Obdachlose).
* Preise: `p = base × clamp((Nachfrage+1)/(Angebot+1), 0,25…4)^0,5`, täglich 25 % geglättet, Historie.
* Geld (stündlich gebucht): Steuern (Bewohner × Stufensatz × Steuersatz × TaxBonus, nur im
  Verwaltungsradius von Rathaus/Burg), Marktgebühren (0,5 Gold je von besetztem Markt versorgtem
  Bewohner), Löhne je besetztem Arbeiter nach Berufsstufe, Unterhalt (UpkeepReduction). Bankrott bei
  negativem Schatz: Ereignisse/Warnungen, Produktion 50 %, keine Platzierung/Ausbauten/kostenpflichtigen
  Straßen; Warnung bei absehbarer Leere (< 3 Tage).
* Handel: Aufträge je Ressource (verkaufe über X, kaufe bis Y, Tageslimit), stündlich über ein
  besetztes, angebundenes Handelskontor zum Marktpreis ±20 % (TradePriceBonus); Gewürze/Seide nur so.
* Kommandos `SetRecipe`, `SetProductionPriority`, `QueueProduction`, `ClearQueue`, `SetTradeOrder`,
  `RemoveTradeOrder`; `SetJobPriority` heißt jetzt `SetWorkforcePriority`. Ereignisse u. a.
  `Shortage` (mit Position), `ProductionStatusChanged`, `ProductionCompleted`, `Transport*`,
  `TradeExecuted`, `DailyBalance`, `TreasuryLow`, `BankruptcyStarted/Ended`, `ResourcesLost`.
* `report`: Abfragen für UI-Fenster (Wirtschaft, Ressourcenhistorie, Produktion je Gebäude, Lager,
  Finanzen, Arbeitskräfte, Handel, Logistik).
* `rules.ron` um Logistik-/Preis-/Handels-/Natur-Werte erweitert. Spielstandformat Version 3
  (Version 2 ohne Wirtschaft wird nicht migriert); `validate` prüft Produktionszustände,
  Transportaufträge, Wirtschaftsdaten und dass die Lager-Sicht den Lagerinhalten entspricht.

**Geprüft**
* `cargo test -p kingdoms_sim`: 153 Unit-Tests + 3 Integrationstests + 1 Doctest grün, u. a.
  Ketten Weizen → Mehl → Brot (2 Tage) und Holzfäller → Sägewerk → Tischlerei (3 Tage) mit
  Mengenerhaltung, Rohstoffmangel, volles Ausgangslager, fehlende Straße, Trägerradius, Reisezeit,
  Preise bei Über-/Unterangebot, Tagesbuchungen, Bankrott, Handel, Determinismus über 6000 Ticks
  (inkl. Tagesabschluss) und Fortsetzung nach Laden.
* `cargo clippy -p kingdoms_sim --all-targets -- -D warnings` sauber; `cargo check -p kingdoms` grün;
  `cargo doc -p kingdoms_sim --no-deps` ohne Warnungen.

**Offen**
* Feldfarmen (Weizen, Gemüse, Obst) sind bis zur Landwirtschaft (Phase 6) `Idle`; ihre Arbeiter
  werden trotzdem angefordert. Weizen gibt es bis dahin nur aus dem Lager/Handel.
* Keine Bevölkerungsentwicklung und kein Nahrungsverbrauch der Haushalte (Phase 5); Marktgebühren
  sind bis dahin pro versorgtem Bewohner abstrahiert.
* Zahlungen für Straßen, Erstattungen, Rodungsholz und Handel wechseln das Lager ohne Träger.
* Kein Lager-zu-Lager-Ausgleich; Lagerkapazitäten nach neuer Forschung muss Phase 7 per
  `storage::init_storage` neu setzen.

## 2026-11-21 · Lane A, Schritt 2/3 – Bausystem, Straßen, Gebiet (Phase 3, `crates/sim`)

**Fertig**
* `grid`: `Direction` (Bitmaske N=1 −y, E=2 +x, S=4 +y, W=8 −x), `Axis`, `TileRect` (Chebyshev-Abstände),
  `Rotation` 0–3 (0 = Eingang Süd, 1 = West, 2 = Nord, 3 = Ost; ungerade Drehung tauscht B/T;
  Eingang = Feld vor der Mitte der Vorderkante; `grown_origin` für größere Hausstufen).
* `buildings`: `Building` in `Arena` (Typ, Footprint, Drehung, Eingang, Phase `Construction(New|Move)` /
  `Active` / `Upgrading`, Pause-Flag, `staffed`, `Inventory`), Belegungsgitter (Tile → Gebäude, beim Laden
  neu aufgebaut, Überlappungen in Spielständen werden abgewiesen), `wall_mask`, Sprite-Helfer
  (`construction_<B>x<T>_<0..2>`).
* Platzierung: `check_placement`/`check_move` → `PlacementCheck` (ok, typisierte `PlacementError`s mit
  deutschem Text, Warnungen, Kosten, Rodung, Bauzeit, Status je Tile, Eingangsstatus). Regeln: Kartenrand,
  Höhenstufen, Wasser, Sumpf, Gebäude, Straße, Gebiet, Forschung, `buildable`, `MaxCount`, Vorkommen-Radius,
  Wassernähe (Fischerei), Tor nur auf Mauer, Farmen brauchen ≥ 50 % freies Acker-/Weideland, Gold/Material.
  Warnungen: kein Straßenanschluss, Eingang versperrt, Rodung (+10 % Bauzeit je Tile, 2 Holz je Baum),
  Mauer wird ersetzt, versperrt fremden Eingang.
* Bau: Kosten bei Platzierung (Gold sofort, Material aus dem Königreichslager an die Baustelle), Arbeit in
  Arbeitspunkten (1 Bauarbeiter × 1 Tick = 100; Winter 85 %, Forschungs-Bautempo), Baustellen bekommen
  ½ Fläche Bauarbeiter (1–4). Abbrechen (ungenutzter Anteil voll zurück), Abriss (50 % Material, Inhalt
  zurück, letztes Hauptquartier geschützt), Ausbau von Häusern (Anforderungen aus den Daten, Kosten und
  Bauzeit der Zielstufe, größere Stufen wachsen nach hinten), Versetzen (gleiche ID, Inhalt bleibt, 30 %
  Bauzeit), Pausieren. Pipeline-Schritt 3 `run_construction`.
* `workforce`: abstrakte Zuteilung der Siedler auf Arbeitsplätze/Baustellen nach Priorität → Kategorie →
  Gebäude-ID (Schnittstelle für Phase 5 dokumentiert: nur `Building::staffed` wird gelesen).
* `roads`: Straßengitter (Typ + Brückenachse), Masken inkl. Anschluss an Gebäudeeingänge, Zusammenhangs-
  komponenten (Erreichbarkeit, `buildings_connected`), `has_road_access`. Planer `plan_road` (A* über
  (Tile, Richtung) mit Abbiegestrafe, meidet Gebäude/Felsbrocken/fremdes Gebiet/tiefes Wasser, |ΔHöhe| ≤ 1,
  gerade Brücken über flaches Wasser bis 6 Tiles mit Brückenkosten je Wasser-Tile), bei Misserfolg
  L-Linie mit markierten Problemfeldern; `validate_road_tiles`, `plan_access_road` (Vorschlag zum
  nächsten Weg). Schnellere Typen ersetzen langsamere, langsamere lassen vorhandene Straßen stehen.
* `territory`: Bitmap; Rathaus 20, Burg 26, Wachturm 8 (neu in `buildings.ron`), nur fertige Gebäude.
* `economy` (Staatsschatz + Königreichslager, Ausbaupunkt für Phase 4), `research` (erforschte Techs,
  Freischaltungen, Bautempo-Bonus; Ausbaupunkt für Phase 7).
* Szenariostart: Rathaus nahe dem Startpunkt (Spiralsuche, erst ohne Rodung), Häuser mit `rotation`
  (neues optionales Feld in `scenarios/*.ron`), Trampelpfad von jedem Eingang zum Rathauseingang,
  Vorrat/Gold/Siedler laut `default.ron`.
* Kommandos `PlaceBuilding`, `DemolishBuilding`, `CancelConstruction`, `UpgradeBuilding`, `MoveBuilding`,
  `SetBuildingPaused`, `BuildRoad`, `RemoveRoad`; Ereignisse für Platzierung, Baubeginn, Fertigstellung,
  Abbruch, Abriss, Ausbau (Start/Ende/Abbruch), Umzug, Pause, Straßen, Gebiet; Ablehnungen mit deutschem Grund.
* Spielstandformat Version 2 (Version 1 ohne Bausystem wird nicht migriert); `validate` prüft Gebäude,
  Straßen, Gebiet, Lager, Forschung, Arbeitskräfte.

**Geprüft**
* `cargo test -p kingdoms_sim`: 116 Unit-Tests + 3 Integrationstests (Determinismus mit Bau-/Straßen-
  Kommandos, Fortsetzung nach Laden mitten im Bau) + 1 Doctest grün.
* `cargo clippy -p kingdoms_sim --all-targets -- -D warnings` sauber; `cargo check -p kingdoms` grün.

**Offen**
* Hausausbau-Bedürfnisse/Waren werden bis Phase 5 auf Königreichsebene geprüft (Lager, Dienst-Reichweite).
* Baumaterial wird bei Platzierung vollständig geliefert; Träger-Logistik (Phase 5/6) kann die Lieferung
  über `ConstructionSite::materials_delivered`/`work_limit` zeitlich strecken.
* Kein Schutt nach Abriss; Straßen werden sofort gebaut und beim Entfernen nicht erstattet.
* Lager ohne Kapazitätsgrenze (Phase 4: Lagergebäude, Kapazitäten).

## 2026-11-21 · Lane A, Schritt 1/3 – Simulationsfundament (`crates/sim`)

**Fertig**
* `ids`: generationelle `Arena<T>` mit typisierten `Id<T>` (Free-List LIFO, Generationen, Iteration in
  Index-Reihenfolge, serde, Integritätsprüfung für geladene Daten).
* `time`: Kalender (200 Ticks/h, 4800 Ticks/Tag, 6 Tage/Jahreszeit, Start Frühling Tag 1 Jahr 1 06:00),
  `Season`, `GameClock` mit Grenz-Prädikaten, Tagesanteil, deutscher Anzeige; Pipeline-Schritt Kalender.
* `defs`: datengetriebene Definitionen mit typisierten IDs; Loader löst String-Referenzen auf und
  validiert alle Querverweise (sammelt alle Probleme mit Datei + ID + Grund). Freischaltungen stammen
  ausschließlich aus `technologies.ron` (`unlocked_by` wird abgeleitet).
* Daten: `resources` (37), `buildings` (57, alle aus §5.1–5.5), `roads.ron` (6), `crops.ron` (4),
  `technologies` (44 in 8 Bereichen, typisierte Effekte), `units.ron` (7), `events` (12, typisierte
  Bedingungen/Effekte/Entscheidungen), `scenarios/default.ron` (§13), neu `rules.ron` (globale Balancing-Werte:
  Löhne je Berufsstufe, Werkzeugverschleiß, Puffer, Jahreszeiten-Wachstum, Wettergewichte, max. Steuersatz).
* `command` (`SetTaxRate`, `SetJobPriority`, Ablehnungsgründe), `event` (`SimEvent`, `Notification`,
  `Severity`), `policy` (Steuersatz, Berufsprioritäten), `state` (`GameState::new`/`tick`, Pipeline:
  Kommandos → Kalender), `save` (Header + bincode 2 + CRC32, Magic/Version/Längen/Fingerprint-Prüfung,
  Migrationskette, atomares Schreiben, Header-only-Lesen).

**Geprüft**
* `cargo test -p kingdoms_sim`: 73 Unit-Tests + 2 Integrationstests (Determinismus 2000 Ticks) + 1 Doctest grün.
* `cargo clippy -p kingdoms_sim --all-targets -- -D warnings` sauber; `cargo check -p kingdoms` grün.

**Offen**
* Nahrungsbilanz: §4 (≈ 1 Sättigung pro wacher Stunde) gegen die Produktionsraten aus §5 muss im
  Bevölkerungsschritt abgestimmt werden (aktuelle Erträge sind vorläufig).
* Szenario-Anwendung (Startgebäude, Siedler, Vorrat, Gold) folgt mit Bau-/Wirtschaftssystemen.
* `docs/architecture.md` §3.2 kennt `policy` und `data/rules.ron` noch nicht.

## Etappe 1 – Fundament (Phase 1) und Weltgenerator (Phase 2, Teil 1)

**Fertig**
* Workspace `kingdoms` mit `kingdoms_engine`, `kingdoms_sim`, `kingdoms_assetgen`, Binary `kingdoms`.
* Engine: Lifecycle/Main-Loop (winit 0.30), Fixed Timestep (20 Hz, Pause/1×/2×/4×), Events,
  Konfiguration (RON), Pfade (macOS/Windows/Linux), Logging (Konsole + Datei + Ringpuffer +
  Panic-Hook), Szenen-Stack, Input mit Aktionsbelegung, wgpu-30-Renderer (instanzierte Sprites,
  Iso-Tiefenmodell im Shader, MSAA 4× + Alpha-to-Coverage, statische Chunk-Batches mit Culling,
  transparente Sortierung, Wind-/Wasser-/Emissiv-Flags, Umgebungslicht), Kamera (Zoom am Cursor,
  Grenzen, Pixel-Snapping), Texturregister mit gamma-korrekten Mipmaps, Asset-Manager (RON-Atlanten,
  parallele Dekodierung, Fallback-Sprite), Screenshots (F12 bzw. `KINGDOMS_AUTOSHOT`), Frame-Metriken.
* Simulation: Tile-Welt (Terrain, Biome, Höhe 0–8, Fruchtbarkeit, Feuchte, Temperatur, Vegetation,
  Vorkommen, Flüsse), 10-stufiger deterministischer Generator inkl. garantierter Startressourcen.
* Spiel: Weltszene mit Terrain-Chunk-Renderer (Übergangsränder, Klippen, Kantenlicht/AO,
  Höhentönung), RTS-Kamera (WASD/Pfeile, Rand-Scrollen, Maus-Drag, Zoom), Tile-Picking mit
  Höhen, Raster (G), Debug-HUD (F3), Spielgeschwindigkeit (Leertaste, 1/2/3).
* Platzhalter-Grafiken: Terrain- und Overlay-Atlas aus `tools/assetgen` (deterministisch).

**Geprüft**
* `cargo test`: Engine 36 Tests, Sim 7 Tests – alle grün. Clippy sauber (bis auf 2 noch ungenutzte Generator-Helfer).
* Laufzeittest unter Xvfb + lavapipe (Software-Vulkan): Fenster, Rendering, egui, Screenshot-Readback,
  Chunk-Culling (5 sichtbar / 31 gecullt) – Sichtprüfung der Screenshots durchgeführt.
* Weltgenerator: Draufsicht-Renderings mehrerer Seeds geprüft (Küsten, Flüsse mäandrieren und münden,
  Seen, Wälder, Gebirge, Startgebiet).

**Offen / bekannt**
* Darstellung von Vegetation/Vorkommen (Phase 2, Teil 2).
* Performance auf echter GPU nicht messbar in dieser Umgebung (nur Software-Rasterizer).

## Testumgebung (Linux-Container)
```
VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json KINGDOMS_AUTOSHOT=/tmp/shot.png \
  KINGDOMS_USER_DIR=/tmp/kd xvfb-run -a -s "-screen 0 1600x900x24" ./target/debug/kingdoms
```
Umgebungsvariablen: `KINGDOMS_SEED`, `KINGDOMS_DEBUG=1` (Debug-HUD), `KINGDOMS_AUTOSHOT[_FRAME]`,
`KINGDOMS_ROOT`, `KINGDOMS_USER_DIR`, `RUST_LOG`.
