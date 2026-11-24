# Entwicklungs-Roadmap

Status: ✅ fertig · 🔶 in Arbeit · ⬜ offen

## Phase 1 – Technisches Fundament ✅
Workspace, Abhängigkeiten, Fenster (winit), wgpu-Initialisierung, Sprite-Pipeline (instanziert,
Iso-Tiefenmodell, MSAA), Kamera (Zoom, Pan, Grenzen), Input + Aktionsbelegung, Fixed Timestep,
Event-Queues, Szenen-Stack, Konfiguration, Logging, Asset-Manager mit Fallbacks, Debug-HUD,
isometrische Tiles, Screenshot-Funktion.

## Phase 2 – Spielwelt ✅
✅ Grid, Terrain, Höhenstufen, 10-stufiger Seed-Generator (Höhe, Temperatur, Hydrologie,
Feuchtigkeit, Biome, Vorkommen, Vegetation, Hindernisse, Startgebiet), Chunk-System mit
statischen Batches und Culling, Übergangsränder, Klippen, Kantenlicht.
✅ Darstellung aus dem Simulationszustand: Bäume nach Art, Jahreszeit und Wachstumsstufe (Wind,
natürliche Streuung), Baumstümpfe, Büsche, Felsbrocken, Vorkommen-Markierungen; Chunks werden
bei Änderungen (Fällen, Pflanzen, Wachstum, Abbau) und beim Jahreszeitwechsel neu aufgebaut.
✅ Minikarte (Gelände, Bewuchs, Straßen, Gebäude, Gebietsgrenze; inkrementell je Chunk,
Sichtrahmen, Klick/Ziehen bewegt die Kamera).

## Phase 3 – Bausystem ✅
✅ Simulation: Gebäudedefinitionen (RON), Platzierungsprüfung, Rotation, Kosten, Baustellen mit
Bauzeit, Abriss, Upgrades, Verschieben, Straßen (Typen, Auto-Verbindung, Brücken), Mauern/Tore,
Gebiet.
✅ Darstellung: fertige Gebäude (Sprite-Ausrichtung je Drehung), Baustellenstufen, Ausbau-Gerüst,
Straßen je Typ mit Verbindungsmasken, Brücken, Mauersegmente/Tore, Gebietsgrenze (T), Hover und
Auswahl (Linksklick/Esc).
✅ Bauwerkzeuge im Spiel: Baumenü mit Kategorien (B), Platzierungsvorschau (Geist grün/rot,
Feldmarkierungen, Eingang, vorgeschlagene Zufahrt, Gründe und Kosten), Drehen (R / Shift+Mausrad),
Straßen ziehen mit Live-Planung, Abriss mit Bestätigung und Erstattung, Versetzen, Ausbau und
Bauabbruch im Info-Fenster.

## Phase 4 – Wirtschaft ✅
✅ Simulation: Ressourcen, Lager, Produktionsketten mit Puffern, Warteschlangen und Prioritäten,
Märkte, Geld, Steuern, Löhne, Unterhalt, dynamische Preise, Handel, Logistik mit laufenden Trägern
(Schritt 3b).
✅ Darstellung: Status-Tönung (stillgelegt, ohne Arbeiter), Simulationsdaten im Debug-Fenster.
✅ HUD: Leiste oben (Schatz mit Tagesbilanz, Bevölkerung/Wohnraum, Zufriedenheit, Kernwaren,
Datum, Tempo, Menü), Info-Fenster je Gebäude (Produktion mit Rezepten, Puffern, Aufträgen,
Priorität und Produktivitätsfaktoren; Lager; Gebiet), Meldungen und Hinweise, Schnellspeichern.
✅ Verwaltungsfenster „Reichsverwaltung“ (Tasten L/P/F/K/J/V/N/C, Knöpfe in der Leiste): Lager &
Waren, Produktion, Finanzen mit Steuersatz und Verlauf, Handel mit Aufträgen und Preisen,
Arbeitskräfte mit Prioritäten, Bevölkerung, Landwirtschaft & Wetter, Statistik mit wählbaren
Verläufen; eigenes Diagramm-Widget.

## Phase 5 – Bevölkerung ✅
✅ Simulation Teil 1: Bewohner-Entitäten und Haushalte, Bedürfnisse, Zufriedenheit, Gesundheit,
Haushaltsversorgung über Märkte/Lager, Arbeitsplätze, Demografie, Hausauf- und -abstieg.
✅ Darstellung: Bewohner, Haushalte, Bedürfnisse, Zufriedenheit und Ausbau-Voraussetzungen im
Info-Fenster der Häuser; Bevölkerung, Wohnraum und Zufriedenheit (mit Faktoren) in der Leiste.
✅ Bevölkerungsfenster (Lebensabschnitte, Bewegung mit Verlauf, Bedürfnisse, Zufriedenheit mit
Faktoren, Häuserliste mit Ausbau-Hindernissen).
✅ Simulation Teil 2 (Schritt 3a): Navigation (Wegkosten-Raster, A* mit Budget und Cache),
Positionen und Laufwege ohne Teleport, Tagesablauf (Arbeit, Schule, Markt, Gasthaus/Kirche),
Anwesenheit und Arbeitsweg in der Produktion, Zu- und Abwanderung zu Fuß, Läufer-API für Renderer.
✅ Simulation Teil 3 (Schritt 3b): Träger laufen mit den Waren (Waren unterwegs beim Träger,
Umleiten, Rückfracht), Bauarbeiter rund um die Baustelle, Bauern auf den Feldern, Sammler am Ziel
mit Heimtragen, Besorgungsgang füllt den Haushaltsvorrat, Logistik-Berichte.
✅ Darstellung der Läufer: Bewohner, Träger mit getragener Ware, Bauarbeiter und Bauern auf der
Karte (zwischen den Ticks interpoliert, Laufanimation in vier Richtungen, Höhen und Brücken,
Iso-Tiefe gegen Gebäude und Bäume, Culling über den räumlichen Index, LOD), Anklicken öffnet das
Bürgerfenster (Tätigkeit, Ziel, Familie, Arbeit, Bedürfnisse, Zufriedenheit; Folgen mit der
Kamera), Bürgerliste in der Reichsverwaltung, Arbeitsweg, Wartezeit auf Träger und Wegprobleme
im Info-Fenster.

## Phase 6 – Landwirtschaft & Jahreszeiten ✅
✅ Simulation: exklusive Felder und Weiden je Farm, Feldfrüchte (Aussaat, Wachstum nach
Jahreszeit/Wetter/Boden/Feuchte, Stufen, Ernte in die Scheune, Frost, Erschöpfung und Brache,
Obstgärten mehrjährig), Jahreszeit-Faktoren (Winter-Arbeitstempo, Fischerei, Weide, Imkerei),
Tageswetter mit Stärke und Wirkungen (Arbeit im Freien, Heizbedarf, Krankheit, Dürre, Sturmschäden),
Schneedecke/Bodennässe, Berichte (Farm, Landwirtschaft, Wetter, Nahrungsprognose), Balance über ein
ganzes Jahr.
✅ Farmfenster (Felder, Fruchtwahl, Boden, Ernte) im Info-Fenster; Wetter, Jahreszeit,
Landwirtschaft und Nahrungsprognose in der Reichsverwaltung.
✅ Darstellung: Feld-Ebene (`fields`-Atlas) auf der Karte mit korrekter Tiefe (Pflanzen als
Bodenbewuchs), Weiden mit grasenden Kühen und Schafen (reine Darstellung), Wetter-Optik
(Licht, Regen, Sturm mit Wind in den Bäumen, Schneefall, Schneedecke auf Boden, Feldern und
Grundstücken, nasser Boden), Rauch aus Werkstätten und – im Winter – aus bewohnten Häusern,
Staub über Baustellen, fallendes Laub im Herbst.

## Phase 7 – Fortgeschrittene Systeme ✅
✅ Simulation Forschung (Sim-Lane Schritt 4): Forschungspunkte aus Hauptquartier und Schulen
(anwesende Gelehrte), Goldkosten beim Beginn, Voraussetzungen, Forschungsliste mit Einreihen,
Abbrechen und Umsortieren, Abschluss mit allen Wirkungen (die militärischen gelten seit Sim-Lane
Schritt 5b), Meldungen, Tagesstatistik, Bericht für den Forschungsbaum.
✅ Simulation Ereignisse mit Entscheidungen (Sim-Lane Schritt 4): 13 Ereignisse mit Bedingungen,
Gewichten, Abklingzeiten und Risiko, offene Entscheidung mit Frist, Konsequenzen (Gold/Waren,
befristete Modifikatoren, Wetter, Flüchtlinge, Krankheit, Ruinen mit Wiederaufbau, Felder,
Plünderung, Vorkommen, Steuerdeckel), Sturmschäden an Gebäuden, Berichte.
✅ Darstellung (Merge 4): Ruinen (`rubble_*`) bis zur ersten Wiederaufbaustufe, „Ereignisse“ in der
Produktivitäts- und Zufriedenheits-Aufschlüsselung, Kassenbuch-Kategorien Forschung und
Ereignisse; Ereignisse erscheinen im Meldungsfeed, ohne Entscheidung gilt nach Fristende die erste Wahl.
✅ Darstellung (Spiel-Lane Stufe 5b): Forschung als Reiter der Reichsverwaltung (Technologiebaum
nach Bereichen mit Voraussetzungs-Verbindungen, Zuständen, Kosten, Restzeiten und den
Wirkungstexten der Simulation, Forschungsliste mit Umsortieren/Entfernen, Einreihen mit
fehlenden Voraussetzungen, Quellen, Punkte je Tag), Ereignis-Dialog (Wahlmöglichkeiten mit
vorhergesagten Wirkungen, Frist, „Später entscheiden“, Fortsetzen nach automatischer Pause),
Ereignis-Fenster (Wirkungen, Steuerdeckel, Verlauf), Wirtschaft-Reiter (Angebot/Nachfrage je
Warengruppe, Engpässe/Überschüsse, Preisindex, Produktionsketten), Klänge und Beraterstimmen zu
Ereignissen, Schäden und Forschung.
✅ Simulation Königreiche, Diplomatie, Karawanen (Sim-Lane Schritt 5a): 3–5 KI-Königreiche nach
Kartengröße mit Gebiet (Land-Karte, Grenzen, Bauverbot für den Spieler), wachsenden Siedlungen aus
Bauwerken, abstrakter Wirtschaft (Land, Handwerk, Verbrauch, Preise, Bedarf/Angebot),
Persönlichkeit, Beziehungen mit Aufschlüsselung und verblassenden Erinnerungen, Beziehungen und
Kriege untereinander; Verträge, Geschenke, Forderungen, Krieg und Frieden mit deterministischem
Urteil und Gründen; KI-Initiativen mit Posteingang, Schonfrist vor Kriegserklärungen, Überfälle und
Kriegsmüdigkeit; Handelskarawanen (Händler und Karren laufen zum Kontor, eigene Karawanen,
Überfälle auf gefährlichen Routen, Aufträge über Karawanen); Berichte, Entwicklerbefehle.
✅ Darstellung Diplomatie (Spiel-Lane Stufe 6): Reiter Diplomatie (Reiche mit Wappen, Beziehung
mit Anteilen, Krieg, Verträge, Schätzungen, Waren, Handlungen mit Kosten, Urteil und Gründen,
Dialoge für Geschenk, Tribut und Frieden, Verlauf), Entscheidungskarte für Anfragen mit Frist,
Karawanen und Partnerpreise im Handelsreiter mit Sendedialog, KI-Siedlungen und Grenzen auf Karte
und Minikarte, Händler mit Karren und Banner.
✅ Simulation Militär und Krieg (Sim-Lane Schritt 5b): sieben Einheiten, Rekrutierung echter
Bewohner mit Kosten, Ausbildung, Sold, Rationen, Moral, Ausrüstung, Erfahrung; Kompanien mit
Befehlen (Bereitschaft, Verteidigen, Patrouille, Feldzug, Belagerung, Rückzug); Räuberbanden und
KI-Heere als Läufer mit angekündigter Ankunft, Plünderung, Brandstiftung, Breschen in Mauern und
Toren; deterministischer Kampf mit Türmen, Garnisonen, Gelände und Forschungsboni; Gefallene als
echte Tote; Feldschlachten und Belagerungen mit erzwungenem Frieden; militärische
Forschungswirkungen; Ereignisse mit echten Soldaten; Herrscherränge mit Aufgaben und Ansehen;
Niederlage (Hauptquartier erobert, Bevölkerung 0); Berichte, Entwicklerbefehl.
✅ Darstellung Militär (Spiel-Lane Stufe 6): Reiter Militär (Einheiten, Rekrutierung je Gebäude
und im Info-Fenster, Kompanien mit Befehlen, Verteidigen und Patrouille auf der Karte mit
Vorschau, Feldzug/Belagerung, Bedrohungen mit Ankunftszeit und Kamerasprung, Kriege, Feldzüge,
Kampfberichte), Feinde getönt mit Bannern und Ringen, Randmarken, Funken/Staub im Nahkampf,
brennende angegriffene Gebäude, Panel „Aufgaben“ mit Rang, Aufstiegsfeier,
Niederlage-Bildschirm, Musik Spannung/Krieg, Sprites `siege_*`, `raider_*`, `banner`, Wappen.

## Phase 8 – Polish 🔶
✅ UI-Theme (Pergament, dunkles Holz, Gold, Cinzel/Alegreya), Pausenmenü, Schnellspeichern und
-laden (F5/F9), Audio-Anbindung (UI-Klänge, Weltklänge mit Stereo-Position, Sprachausgabe,
Umgebungsgeräusche nach Kameraumgebung, Musikstimmung; Audiodateien fehlen noch).
✅ Partikelsystem der Engine (Pool mit Obergrenze, Emitter, Welt- und Bildschirmpartikel,
Iso-Tiefe) und Umgebungs-Shader (Wind, Schnee, Nässe).
✅ Tag-Nacht: Licht nach Tageszeit mit Dämmerungen und jahreszeitlicher Tageslänge (nur die Welt,
nicht die Oberfläche), nachts beleuchtete Fenster bewohnter Häuser und arbeitender Betriebe
(`lights`-Atlas), Nacht-Umgebungsklänge und Nachtmusik; Laufanimationen der Bewohner.
✅ Hauptmenü (lebende Stadt im Hintergrund, Fortsetzen, Neues Spiel mit Seed, Kartengröße,
Schwierigkeit, Ereignissen und Landschaftsvorschau, Credits), Ladebildschirm mit Stufen
(Atlanten, Weltgenerierung und Spielstand-Prüfung asynchron), Spielstand-Liste mit Metadaten,
Verträglichkeit und Löschen, Speichern unter Namen mit Überschreib-Rückfrage, drei
Autospeicher-Plätze, sicheres Schreiben mit Sicherung, Pausenmenü mit Rückfrage bei
ungespeichertem Fortschritt, Einstellungen (Audio-Kanäle, Grafik inkl. Vollbild/VSync/MSAA/
UI-Größe/Partikel/Tag-Nacht, Spiel, Tastenbelegung mit Konfliktprüfung), dauerhaft gespeichert.
✅ HUD nach Master-Spezifikation §13: oben Bevölkerung, Gold, Nahrung, Holz, Stein, Eisen,
Zufriedenheit, Jahreszeit, Datum; links Bauen, Straßen, Landwirtschaft, Produktion, Militär (mit
Bauleiste) und Forschung; rechts Ereignisse, Benachrichtigungen, Übersicht; unten
Auswahlwerkzeuge, Geschwindigkeit, Pause, Tageszeit; passt auf 1280 × 720. „Aufgaben“ rechts
mit Herrscherrang und Anforderungen (Stufe 6).
✅ Grafik: keine gemalten Menschen in Gebäude-Sprites (Requisiten statt Figuren).
✅ Simulation Skalierung, Robustheit und Kampagnen-Balance (Sim-Lane Schritt 6): Messstadt mit
5000 Bewohnern, 999 Gebäuden, fünf KI-Reichen, Karawanen und einer Schlacht unter den Zielen
(Ø < 2 ms, max < 8 ms je Tick), gespeichertes Weggedächtnis mit gezielter Invalidierung,
gestaffelter Hausausbau, gebündelte Besorgungen; Befehls-Fuzzing über alle Kommandos und
manipulierte Spielstände; Drei-Jahres-Kampagnen auf fünf Seeds mit Krieg; Darstellungshilfen
(Anwesende je Gebäude, Baustellen-Status, Heizen, Wind, Herden, kahle Obstbäume, Orte von
Verlusten, Forschung samt Voraussetzungen, Niederlage in der Vorschau).
✅ Simulation Vollständigkeit (Sim-Schritt 7, Lücken der Prüfung gegen die
Master-Spezifikation): Ausbau aller Gebäudearten mit Daten-Ketten (Vorratsspeicher → Lagerhaus,
Rathaus → Burg, Wachturm → Wehrturm, Stufe 2 von Sägewerk, Schmiede, Mühle, Bäckerei; Bestand,
Puffer und Aufträge bleiben) samt Grafiken, Handelsstraßen-Bonus im Karawanen- und Weltmarkthandel,
Gasthaus mit Bierausschank, Wohlstand der Haushalte (Einkommen, Steuer, Käufe zum Marktpreis,
Armut), Freundschaften, Tagesablauf mit Festtagen, Kranken und Schutz vor Feinden, Tagestemperatur
(Heizen, Frost), „seit heute / 1 Tag / N Tagen“ und Gebäudenamen mit Artikel, Spielkennung (UUID)
in Spielständen; Spielstand-Format 13.
✅ Simulation letzte Punkte und Balance (Sim-Lane Schritt 7b): Hiebe mit Ziel in der Kampfrunde,
gemessener Ertrag je Gebäude (auch Feldfarmen), Proviant zurück ins Lager, Soldaten beim Abriss des
Hauptquartiers, eingemauerte Heere lösen sich auf, schnellere Arbeitszuteilung (Differenztest),
Bauarbeiter-Grundbesetzung (Wachstum aller Kampagnen-Seeds), Ränge mit erreichbarem Gebiet und
ehrgeizigem Spieler bis zur Stadt, Fristen und deutsche Mengen; Spielstand-Format 14.
✅ Simulation Prüfbefunde, Ausbauketten und Langzeitlauf (Sim-Lane Schritt 8): Ausbau Rathaus → Burg
aus dem eigenen Lager (Lager im Ausbau beliefert die eigene Baustelle), Gebiet folgt der
Grundfläche beim Ausbau, fliehende Banden ohne Ausweg zerstreuen sich, Krankenstand statt
blockierter Stellen, Einarbeitung in Fach- und Gelehrtenarbeit, Handelspreise ohne Umkehr,
Langzeitstatistik je Jahreszeit, „Kein freier Platz zum Pflanzen“, Frost lässt Obst außerhalb
des Winters hängen, Mengen und Texte, Spielstand-Grenzen gegen präparierte Werte; 20 neue
Ausbaustufen mit Grafik, Symbol und Nachtlicht (Holzfällerlager, Großer Steinbruch, Hochofen,
sieben Werkstätten, Große Fischerei und Imkerei, Großer Markt, Dom, Akademie, Spital, Gasthof,
Garnison, Schützenturm, Gestüt), bewusste Endstufen dokumentiert; Acht-Jahres-Lauf bis zur
Großstadt (zwei von drei Karten), Fortschrittskurve in §1; Spielstand-Format 15.
✅ Darstellung der Sim-Schritt-8-Daten (Spiel-Schritt 8): Langzeitreihe „Jahreszeiten (gesamt)“
und Anzeigen „Wohlstand“ sowie „Gebiet und Bauten“ im Reiter Statistik, Einarbeitung und
Krankenstand im Bürgerfenster und Info-Fenster, Warnung vor Versorgungslücken beim Ausbau,
Status „Kein freier Platz zum Pflanzen“ als neutrale Wartezeit, Handelshinweis zur Bonusgrenze.
✅ Prüfbefunde Spiel-Schritt 8: Einstellungen beim Schließen des Fensters, wartende Bauaufträge in
der Pause als Geister, Dachschnee im Winter (Schneemarke der Sprites, `FLAG_SNOW_ON_ROOF`),
Einzahl und Zahlenformate, Arbeitsklänge aller Werkstätten, macOS-Kürzel nach Tastaturbelegung,
Render-Listen ohne globalen Zustand.
✅ Darstellung der Sim-Schritt-7b-Daten (Merge Schritt 7): Funken und Staub genau zwischen
Angreifer und Ziel (auch an Tor, Mauer und Gebäude), Angreifer in der Menge wenden sich ihrem Ziel
zu; gemessener gegen möglichen Ertrag im Info-Fenster und als Spalte „Gestern“ im Reiter
Produktion, Wirtschaftlichkeit der Feldfarmen und Obstgärten nach ihren Ernten (auch in den
Produktionsketten), Proviant der Feldzüge im Reiter Militär, Fristen mit „morgen“, Restzeit der
Ereigniswirkungen in „h/min“.
✅ Darstellung der Sim-Schritt-7-Daten (Spiel-Schritt 7a): „Ausbauen“ für alle Gebäude im
Info-Fenster (Kosten, Bauzeit, Änderungen, Hinderungsgründe; Sperre nur für Häuser), Wohlstand und
Freunde im Bürger-, Haus- und Bevölkerungsfenster, Krankheit und Festtag im Bürgerfenster,
Gasthaus-Bestand, Temperatur in Wetterreiter, oberer Leiste und Jahreszeit-Tooltip,
Handelsstraßen-Bonus im Handelsreiter und je Karawane, Festtag in Übersicht und oberer Leiste mit
Festgästen als Menge vor dem Festort, Spielstände nach Spielkennung gruppiert; Audio-Lieferung
vorbereitet (Dateien nach Stamm in OGG/MP3/WAV/FLAC, Lieferbericht `KINGDOMS_AUDIO_REPORT`,
Anleitung `assets/audio/README.md` mit Sprechertexten, Überblendung am Stückende, Absenken von
Musik und Umgebung bei Stimmen, Stimmung „Blühend“); einheitliche Dauern („1 h 35 min“),
Militärbanner und Bedrohungsmarken frei von Meldungskarten, Speicherstand im Pausenmenü.
✅ Darstellung Vollständigkeit (Spiel-Schritt 7, Lücken der Prüfung gegen die
Master-Spezifikation): Wind der Simulation in Bäumen, Feldern, Obstgärten (Scherung im Shader),
Rauch, Laub, Regen und Schnee; kahle Obstbäume und Obstgarten im Winter; Schornsteinrauch nur bei
brennendem Ofen, Heizung im Info-Fenster; Weidetiere nach der Herde der Simulation;
Arbeits- und Kampfschleifen der Läufer (Hammer, Hacke, Axt, Speer, Bogen, Schwert, Katapult) und
Gebäudeschleifen (Mühlenflügel, Essenfeuer) über die Animationen der Atlanten; Bodenschatten nach
Tageslicht; Pfeile und Steine der Kampfrunde; alle Klang-IDs verdrahtet (Marktgedränge,
Karrenrumpeln, Berater bei Hunger und Unzufriedenheit, Knopf-Hover, Bürgerantwort);
Forschung über `QueueResearchWithPrerequisites`; Reichsverwaltung mit benannten Reitergruppen;
Handelsbilanz und Wirtschaftlichkeit der Betriebe; Wind im Wetterreiter.
✅ Grafik-Vollständigkeit (Spiel-Schritt 7b): Sammler arbeiten mit dem Werkzeug ihres
Arbeitsplatzes (Axt, Spitzhacke, Spaten, Angel), Essen der Waffenschmiede flackert, Feuer der
Schmieden unter der Traufe sichtbar und mit deutlicherem Flackern, Gebäudeschau
`KINGDOMS_DEV_SHOWCASE` (jedes Gebäude fertig und besetzt) mit den Schleifen auf der Karte geprüft,
App-Symbol.
✅ Performance-Optimierung der Darstellung (Spiel-Schritt 7b): Messläufe `KINGDOMS_PROFILE` und
pixelgleiche Vergleiche `KINGDOMS_FIXED_DT`; Gelände-, Straßen-, Feld- und Grenzebenen nur bei
geänderter Geländeart oder Höhe neu gebaut (Baumwachstum baut nur die Vegetation), Renderer ohne
Speicheranforderungen je Frame (Zählsortieren, einmal berechnete Tiefenschlüssel,
wiederverwendete Puffer und Listen); Bilder vor und nach pixelgleich.
✅ macOS-Vorbereitung (Spiel-Schritt 7b): App-Bundle `tools/macos/make_app.sh` (Info.plist,
`.icns`, Ad-hoc-Signatur), Inhalte aus `Contents/Resources`, Cmd-Kürzel (Cmd+Q mit Rückfrage,
Cmd+S/L/P/D, ⌫), Trackpad (Wischen verschiebt, Zusammenziehen zoomt, Retina-gerechte Pixel),
verdecktes Fenster gedrosselt, Anleitung im README. Auf einem Mac noch nicht gestartet.
✅ Abschlussprüfung (Schritt 9): Release-Build, alle Tests, Clippy und Rustdoc ohne Warnungen,
Atlanten bytegleich reproduzierbar, keine TODO/FIXME/`todo!`/`unimplemented!`/`dbg!`; 15 Bilder
in `docs/screenshots/` (Abschnitt „Bilder“ im README); Audio-Lieferliste (JSON) aller 143 IDs aus
`assets/audio/manifest.ron`, gegen den Audio-Bericht des Spiels geprüft.
✅ Polish-Runde 9 (Spiel): Anfragen der Reiche auf einer angedockten Karte („1 von 3“, Vor/Zurück,
„Später“; Ereignisse haben Vorrang), Minikarte mit Schneedecke und Herbstfarben, Fenstersymbol
unter Linux/Windows, Tabellen der Reiter Wirtschaft und Handel mit Spaltenbreiten nach Inhalt und
Umbruch, gemalte Gebäudeschatten mit Schattenmarke (Alpha 253) und Tageslicht wie die
Bodenschatten, Beziehungsbalken ohne Überdeckung, Räuber in Karminrot auf knochenweißem Ring mit
Beschriftung über der Gruppe, Meldungsliste im Spielstand, `KINGDOMS_SELECT` mit Eingangs-Tile,
ruhigere Meldungen (gleiche Meldung am selben Tag einmal, dichter Wald als Information),
Belagerungs-Setup `KINGDOMS_DEV_WALLS`.
✅ Feinschliff Simulation (Sim-Lane Schritt 9, Befunde der Abschlussprüfung): Räuber mit
Schonfrist (Normal ab Tag 24 und 40 Einwohnern), Größe nach Stadt, Schwierigkeit und
Verteidigern, Warnung 8 h vorher; gedrosselter Posteingang der KI-Reiche; Goldsenken „Fest
ausrichten“ und Hofhaltung je Rang; Bauwarnung fern jedes Lagers, Langzeit-Spieler mit
Außenposten (Großstadt auf allen drei Karten); eine Dauerform in allen Texten, ruhigere Meldungen
mit Kartenpositionen; Spielstand-Format 16.
✅ Darstellung der Sim-Schritt-9-Daten (Zusammenführung Schritt 9): „Fest ausrichten“ im Reiter
Bevölkerung (Kosten, Wirkungen, gesperrt mit Grund), „Hof und Feste“ im Reiter Finanzen,
Bauplatz ohne Lager in Reichweite bernsteinfarben mit roter Warnung, Ankunftszeit der Räuber im
Tooltip der Bedrohungsmarke, Durchbruchstext mit Artikel.
✅ Feinschliff Simulation (Polish-Runde 10, Befunde der letzten Prüfung): Ernte und Dürre folgen
dem Weltzustand (Trockenperiode, Zustand der Felder, höchstens ein Ernte-Ereignis je Herbst),
Räuberwarnung nennt nur Wehrbauten, die stehen, Brandtext nennt den Brandherd, Spielstände mit
Ereignistagen in der Zukunft werden abgelehnt, erzwungenes Wetter nennt sein Ereignis;
Spielstand-Format 17.
✅ Spiel-Feinschliff Runde 10: Fest-Wartezeit und spätere Fristen im Kalender der Kopfleiste,
Meldungsdrosselung je Kalendertag, Wintervorrat als „Vorrat · Bedarf im Winter“, feste
Bildlaufleiste in den Einstellungen, Zustand der Felder und Grund des erzwungenen Wetters im
Reiter Landwirtschaft, „Gebäude zeigen“ im Ereignisdialog, Bilder 04–06, 08, 09, 11–13, 15 neu
und 16 „Fest ausrichten“.
✅ Abschlussprüfung nach Polish-Runde 10: Release-Build, alle Tests, Clippy und Rustdoc ohne
Warnungen, Atlanten und App-Symbol bytegleich reproduzierbar, keine TODO/FIXME/`todo!`/
`unimplemented!`/`dbg!`; Bilder 01–03, 07, 10 und 14 neu, neu 17 „Winter und Minikarte“
(17 Bilder, alle angesehen); Audio-Lieferliste als `assets/audio/delivery.json` im Projekt
(143 IDs, per Test mit dem Manifest abgeglichen); Langzeitläufe grün, die Großstadt erreicht
dabei aber nur noch einer von drei Seeds (Balance offen, siehe `progress.md`).
⬜ Fehlerbehebung nach Rückmeldungen vom Mac: In der Entwicklungsumgebung gibt es keinen Mac; das
Spiel und `tools/macos/make_app.sh` (Symbol, Signatur) wurden auf macOS weder gebaut noch
gestartet. Offen, bis Rückmeldungen von echter Hardware vorliegen.
⬜ Audiodateien einspielen und anhören: Musik, Umgebung, Effekte und Sprecher liefert der
Auftraggeber (Anleitung `assets/audio/README.md`, Prüfung mit `KINGDOMS_AUDIO_REPORT=1`); bis dahin
bleibt das Spiel still, Überblendung, Absenken und Schleifen sind nur per Test geprüft.
⬜ Finale Grafiken statt der erzeugten Platzhalter: Austausch ohne Codeänderung über dieselben
Sprite-Namen (`docs/assets.md`, inkl. Schneemarke Alpha 254 für Dächer und Schattenmarke Alpha 253
für gemalte Schlagschatten); wartet auf Lieferung.
