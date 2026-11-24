# KINGDOMS – Rise of an Empire · Game Design

Dieses Dokument ist die **verbindliche fachliche Spezifikation**. Alle IDs in diesem Dokument sind
kanonisch: Datendateien (`data/`), Sprites (`assets/`), Simulation und UI verwenden exakt diese
Bezeichner. Neue Inhalte werden hier zuerst ergänzt.

## 1. Spielprinzip

Der Spieler beginnt als Herrscher über ein kleines Stück Land mit einem Dorfzentrum, wenigen
Siedlern und einem kleinen Vorrat. Ziel ist der Aufbau eines mächtigen mittelalterlichen
Königreichs: Siedlung erweitern, Produktionsketten aufbauen, Bewohner versorgen und zufrieden
halten, forschen, handeln, Diplomatie betreiben und das Reich verteidigen oder vergrößern.

Ein gut aufgebautes Königreich organisiert sich weitgehend selbst: Bewohner suchen sich Arbeit,
Träger transportieren Waren, Märkte verteilen Güter. Der Spieler steuert über Bauentscheidungen,
Prioritäten, Steuern, Forschung, Handel und Diplomatie.

**Sieg/Fortschritt:** offenes Spiel (Sandbox) mit Meilensteinen (Rangstufen des Herrschers:
Siedlung → Dorf → Marktflecken → Stadt → Großstadt → Königreich → Imperium), abhängig von
Bevölkerung, Wohlstand, Gebäuden und Gebiet. **Niederlage:** Burg/Rathaus erobert oder Bevölkerung 0.

**Rangstufen** (`rules.ron` → `ranks`, stündlich geprüft; ein erreichter Rang bleibt):

Rang | Einwohner | Wohlstand | Gebäude | Gebiet (Felder) | Gebäude-Pflicht | Ansehen | Hofhaltung (Gold/Tag, §7)
---|---|---|---|---|---|---|---
Siedlung | – | – | – | – | – | 0 | –
Dorf | 25 | 1 500 | 12 | – | – | +2 | –
Marktflecken | 50 | 4 000 | 25 | – | Marktplatz | +4 | –
Stadt | 100 | 10 000 | 45 | 1 600 | Kirche | +6 | 15
Großstadt | 200 | 25 000 | 80 | 2 400 | Schule | +8 | 60
Königreich | 350 | 50 000 | 130 | 3 500 | Burg | +10 | 120
Imperium | 600 | 100 000 | 200 | 5 000 | – | +12 | 200

*Wohlstand* = Staatsschatz (mindestens 0) + Königreichslager zum Basispreis; *Gebäude* = fertige
Gebäude ohne Mauern und Tore. *Gebiet:* Das Rathaus beansprucht etwa 1 260 Felder; für die Stadt
genügen drei bis vier Wachtürme am Rand (je etwa 100 neue Felder), die höheren Ränge brauchen
Lagerhäuser weiter draußen, die Burg (Umkreis 26) und abgetretenes Land. Geprüft (Sim-Lane
Schritt 7b): ein ehrgeiziger Spieler (Kirche, Türme an der Grenze, zwei Häuser zugleich,
Verkauf von Überschüssen) wird auf drei von fünf Kampagnenkarten nach 67–69 Tagen Stadt;
auf jeder kleinen Karte bleiben über 12 000 Felder freies Land – mehr als das Doppelte des
Imperiums.

**Fortschrittskurve (Langzeitlauf, Sim-Lane Schritt 9):** Ein vorausschauender Spieler (der
ehrgeizige Spieler, der zusätzlich Holz und Nahrung vor dem Wachstum hält, einen Steinbruch je
80 Einwohner betreibt, Lehm für den Ausbau der Häuser zu Bauernhäusern gewinnt, Buchhaltung,
Getreideanbau, Schulwesen, Handel, Festungsbau und Verhüttung erforscht, Schule, Lagerhaus,
zweiten Markt und Handelskontor baut, Stein, Bretter und Nahrung über dem Bedarf verkauft,
Lager, Werkstätten und Türme ausbaut und Feste feiert, sobald der Schatz 3 000 Gold über den
Kosten liegt) wächst in acht Jahren (192 Tage, bei 1× rund 10 Stunden) auf 204–351 Einwohner:
Dorf nach 7 Tagen, Marktflecken nach 28–41, Stadt nach 50–75, **Großstadt auf allen drei Karten**
nach 83, 108 bzw. 133 Tagen (Nachmessung nach Polish-Runde 10, mit Ernte- und Dürre-Ereignissen
nach dem Weltzustand: Großstadt nur noch auf Seed 7 nach 93 Tagen; Seeds 23 und 31 bleiben mit
215 bzw. 283 Einwohnern bei der Stadt, weil das Gebiet fehlt – 1 751 bzw. 2 057 von 2 400 Feldern,
bei Seed 23 auch die Schule). Der Schatz endet bei 8 000–34 000 Gold (vorher 25 000–49 000):
Hofhaltung (Großstadt 60 Gold je Tag) und Feste (bei 330 Einwohnern rund 1 400 Gold alle
12 Tage) binden etwa ein Viertel des Überschusses (Tagesbilanz mit 330 Einwohnern etwa +420
Steuern, +160 Marktgebühren, +50–700 Ausfuhren gegen −165 Löhne, −260 Unterhalt samt Hofhaltung).
**Gebiet:** Baustellen werden nur aus Lagern beliefert, deren Trägerradius sie erreicht (§7;
Rathaus 20, Lagerhaus 24 Felder). Nach den ersten Türmen liegt die Grenze außerhalb der
Reichweite des Rathauses – der frühere Stillstand bei 1 474 von 1 600 Feldern (Seed 7) war
kein Regelfehler, sondern ein Spieler, der nur in Reichweite des Rathauses und seiner
zentralen Lager baute. Wer weiterwächst, setzt ein Lagerhaus an den Rand (Außenposten), daneben
einen Markt, und baut Türme und Häuser in dessen Reichweite; die Bauvorschau warnt jetzt,
wenn kein Lager einen Bauplatz erreicht („Kein Lager erreicht diesen Bauplatz: Das Baumaterial
käme nie an. Baut zuerst ein Lagerhaus in der Nähe.“, `PlacementWarning::NoStorageInReach`).
So erreicht Seed 7 die Stadt nach 50 Tagen und 2 833 Felder.
**Königreich** erreicht in acht Jahren keine der drei Karten, und das ist gewollt: Einwohner
(bis 351 von 350), Wohlstand (115 000–182 000 von 50 000) und Gebäude (207–237 von 130) reichen,
es fehlen jeweils die Burg und das Gebiet (2 514–3 032 von 3 500). Die Burg (Ausbau des
Rathauses) braucht 20 Eisenbarren – eigenes Eisenerz, Holzkohle und eine Schmelze, Handel mit
einem Reich, das Eisen übrig hat, oder Land aus einem Krieg –, und erst ihr Umkreis von
26 Feldern (rund 870 Felder mehr) schließt die Lücke im Gebiet. Auf Seed 7 liegt kein
Eisenvorkommen im erreichbaren Land und die Nachbarn kaufen selbst Eisen; auf Seed 31 steht
die Mine, die Köhlerei fand keinen Platz. Königreich und Imperium sind Ziele eines langen Spiels
jenseits von acht Jahren, die Eisen voraussetzen; die Anforderungen bleiben. Test:
`expansive_player_grows_beyond_the_town_in_eight_years` (ignoriert, `--release --ignored`).

Der Aufstieg wird gemeldet („Euer Reich ist zum Rang „Dorf“
aufgestiegen!“, `SimEvent::RankAchieved`). **Ansehen** gilt bei allen KI-Königreichen als Teil der
Beziehung („Ansehen: Dorf“, §10); Freischaltungen bringen die Ränge nicht. Der Bericht (Panel
„Aufgaben“) zeigt Rang, Ansehen, Wohlstand, den nächsten Rang mit jeder Anforderung (aktueller
Wert, Ziel, Fortschritt in Prozent; der Gesamtfortschritt ist der schwächste Wert) und die
erreichten Ränge mit Datum.

**Niederlage:** (1) Ein feindliches Heer hat das Hauptquartier gestürmt (Schaden ≥ Trefferpunkte,
§11) und hält es `military.hq_capture_hours` (2 h) lang, ohne dass ein eigener Soldat im Umkreis
von `hq_defense_radius` (8 Feldern) steht; (2) zur vollen Stunde lebt niemand mehr im Reich. Das
Spiel ist dann verloren (`GameState::game_over`, `SimEvent::GameOver` mit Grund, Meldung „Das
Spiel ist verloren. …“): jeder weitere Tick lehnt alle Kommandos ab („Das Spiel ist verloren.“,
ebenso die Bauplatz- und Straßenprüfung der Werkzeug-Vorschau) und lässt die Zeit stillstehen; der Bericht nennt Grund und Datum. Tests einzelner Systeme
können die Niederlage abschalten (`GameSettings::defeat_enabled`).

## 2. Zeit und Takt

| Größe | Wert |
|---|---|
| Simulationstakt | 20 Ticks pro Sekunde bei 1× (2×: 40, 4×: 80) |
| 1 Tick | 18 Spielsekunden |
| 1 Spielstunde | 200 Ticks = 10 s real bei 1× |
| 1 Spieltag | 4800 Ticks = 4 min real bei 1× |
| Tage pro Jahreszeit | 6 |
| 1 Spieljahr | 4 Jahreszeiten = 24 Tage ≈ 96 min (1×) / 24 min (4×) |
| Demografisches Alter | Bewohner altern **1 Lebensjahr pro Jahreszeit** (entkoppelt, damit Generationenwechsel in einer Spielsitzung erlebbar ist) |
| Laufgeschwindigkeit | Weg mit Tempo 1,0 (Trampelpfad): 0,2 Tiles/Tick (5 Ticks = 1,5 Spielminuten je Tile); Straßen × Straßentempo (§5.6, z. B. gepflastert 1,4 → 0,28); Gelände langsamer nach Wegkosten (§3, z. B. Gras 1250 ‰ → 0,16); × Alter, Gesundheit und Wetter (§6) |

Kalender: Jahr 1 beginnt im Frühling, Tag 1, 06:00 Uhr.

**Dauern in Texten** haben eine Form in Simulation und Oberfläche (`report::minutes_text`,
`hours_text`, `ticks_text`): unter einem Tag „45 min“, „18 h“, „7 h 30 min“, ab einem Tag „1 Tag“,
„1 Tag 6 h“, „3 Tage“; Schätzungen „etwa 14 h“ / „etwa 3 Tage“; Uhrzeiten „gegen 17:00 Uhr“,
„morgen 06:00 Uhr“, später mit dem Datum der Kopfleiste „Sommer · Tag 6 · Jahr 1, 06:00 Uhr“
(`report::deadline_clock_text`); Tage stehen in Texten immer im Kalender der Kopfleiste
(`time::day_date_string`), nie als fortlaufende Tageszahl.

**Meldungen** sind Hinweise, keine Dauerschleife: Warnungen nur für Zustände, an denen der
Spieler etwas ändern kann, wiederholt höchstens in festen Abständen (Rohstoffmangel je Gebäude
alle 12 h, Hunger, Kälte und Obdachlosigkeit alle 6 h, „Keine Träger“ alle 6 h, „Kein Weg“ je
Gebäude gedrosselt); ein dichter Wald am Försterhaus ist eine neutrale Information (einmal je
Episode, höchstens einmal je Gebäude und Tag); ein schwerer Sturm wird an seinem ersten Tag
gemeldet. Meldungen zu Orten tragen ihre Kartenposition (Gebäude, Felder, Kartenrand,
Hauptquartier bei Obdachlosen, Sitz eines Reichs bei Kriegen und Frieden der Reiche
untereinander und bei Überfällen auf Karawanen).

## 3. Welt

Kachelbasierte Welt (Größen: 128², 192², 256², 384²), isometrisch dargestellt, mit Höhenstufen
0–8, generiert aus einem Seed (siehe `docs/architecture.md`, Weltgenerator).

**Terrain:** `DeepWater, ShallowWater, Sand, Swamp, Steppe, Dirt, Grass, Meadow, Forest, Rock, Snow`.
**Biome:** gemäßigter Wald, Grasland, fruchtbare Ebene, Gebirge, Sumpf, Küste, Trockengebiet,
Schnee (+ Meer, See). Jedes Biom bestimmt Fruchtbarkeit, Vegetation und Vorkommen.
**Vorkommen (`DepositKind`):** `Stone, Iron, Coal, Clay, Sand, Gold, Fish` mit endlicher Menge
(Fisch regeneriert sich langsam). **Vegetation:** Bäume (`Deciduous, Conifer, Birch, Willow`,
Wachstum 0–255 mit dem Wachstumsfaktor der Jahreszeit (§8.1), fällbar, nachwachsend durch
Förster), Baumstümpfe gefällter Bäume (verrotten nach 48 Spielstunden; Förster pflanzen darauf,
Felder und Bauplätze räumen sie wie Büsche), Büsche, Felsbrocken (Hindernisse).

**Begehbarkeit** (`rules.ron` → `walking`): Läufer bewegen sich in vier Richtungen (N, O, S, W –
die vier Laufrichtungen der Figuren) von Tile-Mitte zu Tile-Mitte. Jedes Tile hat Wegkosten in
Promille eines Wege-Tiles mit Tempo 1,0:

Untergrund | Wegkosten ‰
---|---
Straße / Brücke | 1000 / Tempo (§5.6): Pfad 1000, Schotter 870, Straße 800, Pflaster 714, Handelsstraße 667, Brücke 800
Steppe, Erde, Gras, Wiese | 1250 (abseits der Wege 25 % langsamer)
Sand | 1600
Waldboden | 1750
Fels | 2250
Schnee | 2500
Sumpf | 3750
Wasser (flach und tief) | unpassierbar – nur über Brücken, die nur in Längsrichtung betreten und verlassen werden

Benachbarte Tiles dürfen höchstens **eine Höhenstufe** auseinanderliegen (Klippen ab zwei Stufen
sind unpassierbar); ein Schritt eine Stufe hinauf oder hinunter kostet 250 ‰ zusätzlich.
Felsbrocken, Gebäude-Grundflächen (auch Baustellen) und Mauerabschnitte blockieren; Tore sind der
Durchgang durch eine Mauer. Gebäude werden über ihr Eingangs-Tile vor der Tür betreten (§5.6).

Bauen ist nur auf ebenem, trockenem, bebaubarem Terrain derselben Höhenstufe möglich.
Bäume/Büsche/Felsbrocken auf der Fläche werden beim Bau automatisch gerodet (Rodungskosten:
Bauzeit +10 % je Tile, Holzertrag 2 je Baum).

## 4. Ressourcen (`data/resources/resources.ron`)

ID | Name | Kategorie | Basispreis (Gold)
---|---|---|---
`wood` | Holz | Rohstoff | 4
`stone` | Stein | Rohstoff | 5
`loam` | Lehm | Rohstoff | 3
`clay` | Ton | Rohstoff | 4
`iron_ore` | Eisenerz | Rohstoff | 8
`coal` | Kohle | Rohstoff | 6
`sand` | Sand | Rohstoff | 2
`wool` | Wolle | Rohstoff | 7
`grain` | Getreide (Gerste) | Rohstoff | 4
`hides` | Häute | Rohstoff | 6
`gold_ore` | Golderz | Rohstoff | 30
`wheat` | Weizen | Nahrung | 4
`flour` | Mehl | Nahrung | 7
`bread` | Brot | Nahrung | 10
`meat` | Fleisch | Nahrung | 12
`fish` | Fisch | Nahrung | 9
`milk` | Milch | Nahrung | 6
`cheese` | Käse | Nahrung | 14
`vegetables` | Gemüse | Nahrung | 7
`fruit` | Obst | Nahrung | 8
`honey` | Honig | Nahrung | 12
`planks` | Bretter | Ware | 9
`tools` | Werkzeuge | Ware | 30
`bricks` | Ziegel | Ware | 10
`iron_bars` | Eisenbarren | Ware | 24
`cloth` | Stoff | Ware | 18
`clothes` | Kleidung | Ware | 34
`furniture` | Möbel | Ware | 40
`weapons` | Waffen | Ware | 55
`ceramics` | Keramik | Ware | 16
`beer` | Bier | Ware | 14
`leather` | Leder | Ware | 16
`armor` | Rüstungen | Ware | 45
`jewelry` | Schmuck | Luxus | 120
`wine` | Wein | Luxus | 45
`spices` | Gewürze | Luxus (nur Import) | 70
`silk` | Seide | Luxus (nur Import) | 90
`art` | Kunstgegenstände | Luxus | 150

Geld (Gold/Münzen) ist **keine** Lagerressource, sondern der Staatsschatz (`treasury`).

Nahrungswerte (Sättigung je Einheit): bread 3, meat 3, fish 2,5, cheese 3, vegetables 2,
fruit 2, honey 1,5, milk 1,5, wheat/grain 0 (Rohware). Ein Erwachsener isst **4 Sättigung pro
Tag** (Kinder 60 %, im Winter +20 %; `rules.ron` → `population.food_per_day`). Damit ernährt ein
Hühnerhof (12 Fleisch/Tag = 36 Sättigung, 1 Arbeiter) etwa 9 Erwachsene, eine Fischerei
(16 Fisch/Tag = 40 Sättigung, 2 Arbeiter) etwa 10; eine voll belieferte Bäckerei (48 Brot/Tag)
später rund 36 – Felder lohnen sich.

Brennstoff (`heating_value`): wood 1, coal 2,5; ein Bewohner verheizt 0,6 je Wintertag (× Heizbedarf
des Wetters, §8.2: bei Schnee bis 125 %, bei Sturm bis 130 %).
Haushaltswaren (`household_use`, Einheiten je Bewohner und Tag, solange die Wohnstufe die Ware
verlangt oder die nächste Stufe sie braucht): clothes 0,05, ceramics 0,05, furniture 0,02,
beer 0,2, wine 0,1, jewelry 0,02, art 0,01, silk 0,05, spices 0,05.

## 5. Gebäude (`data/buildings/*.ron`)

Größe = Grundfläche in Tiles (B×T). Kosten in Gold + Materialien. Bauzeit in Spielstunden
(Arbeitsaufwand für einen Bauarbeiter; mehrere Bauarbeiter teilen sich die Arbeit). Bauarbeiter
arbeiten **an der Baustelle**: sie stehen auf den begehbaren, vom Eingang der Baustelle aus
erreichbaren Tiles rund um die Grundfläche (keine Brücken; jeder auf seinem Platz, vom Eingang aus
verteilt) und bauen nur dort; Baumaterial bringen Träger (§7).
„Stufen" = Upgrade-Stufen mit Folge-ID.

**Ausbau (Umbau zur nächsten Stufe):** Jeder Gebäudetyp mit `upgrade: (to, cost, hours)` in den
Daten lässt sich zur nächsten Stufe ausbauen (Kommando `UpgradeBuilding`); Kosten und Arbeit
sind ohne eigene Angabe die Baukosten und die Bauzeit des Zieltyps. Voraussetzungen: das
Gebäude ist fertig und nicht stillgelegt, der Zieltyp ist erforscht, seine Höchstzahl
(`MaxCount`, z. B. eine Burg) ist nicht erreicht, eine größere Grundfläche hat Platz (sie wächst
nach hinten und zur Seite, die Vorderkante bleibt), Gold und Material sind vorhanden; Häuser
zusätzlich die Voraussetzungen der nächsten Wohnstufe (§5.1). Das Gebäude **behält** Id,
Lagerbestand, Produktionspuffer, Rezept und Aufträge; nach dem Ausbau verteilt die
Arbeitszuteilung die (meist zusätzlichen) Stellen neu. Während des Ausbaus halten Bauarbeiter die
Arbeitsplätze: Werkstätten ruhen (Status „Wird ausgebaut“), Dienste, Träger und Forschung des
Gebäudes pausieren; Bewohner bleiben, Lager bleiben Teil des Reichslagers. Ein Lager im Ausbau
nimmt keine Transportaufträge an (es hat keine Träger; wartende Aufträge gehen an andere Lager in
Reichweite) und **beliefert seine eigene Baustelle direkt** aus dem eigenen Bestand (vor jeder
Planungsrunde, ohne Träger; nur Waren, die keinem wartenden Auftrag versprochen sind) – liegen
Stein, Bretter und Eisen für die Burg nur im Rathaus, kommt der Ausbau trotzdem voran. Eine
größere Grundfläche verschiebt den Gebietsanspruch (Kreis um die Mitte) sofort beim Start, ein
Abbruch sofort zurück. Würden Häuser während des Ausbaus einen Dienst verlieren (z. B. der einzige
Markt in Reichweite), warnt der Bericht `upgrade_service_gaps` („Während des Ausbaus fehlt
12 Häusern die Versorgung „Markt“.“). Ausbaustufen erben die Forschungsboni ihres Ausgangstyps
(`ProductionBonus`/`ServiceRadiusBonus` für den Steinbruch gelten auch für den Großen Steinbruch)
und bilden weiter dieselben Einheiten aus. Die Datenprüfung
erlaubt nur verträgliche Ziele: gleiche Rolle, Haus nur zur nächsten Wohnstufe, Produktion mit
derselben Sammelart und denselben Rezepten zuerst (Feld- und Weidefarmen, Mauern und Tore
nicht), Lager bleibt Lager, keine kleinere Grundfläche, nicht weniger Arbeiter, dieselben
ausgebildeten Einheiten, keine Zyklen. Automatisch ausgebaut werden nur Häuser.

Ausbauketten | Kosten des Ausbaus | Freischaltung
---|---|---
Wohnhäuser `house_wood` → … → `manor` (§5.1) | Baukosten der nächsten Stufe | Wohnstufen (§9)
`storehouse` → `warehouse` (Lager für alle Waren, 400 je Art) | 50 G, 15 wood, 10 stone, 6 h | Start
`town_hall` → `castle` (Hauptquartier mit Garnison 20, Gebiet 26) | 1500 G, 120 stone, 30 planks, 20 iron_bars, 48 h | Festungsbau
`watchtower` → `stone_tower` (Wehrturm) | 60 G, 20 stone, 4 planks, 6 h | Stadtmauern
`sawmill` → `large_sawmill`, `smithy` → `master_smithy`, `mill` → `large_mill`, `bakery` → `large_bakery` | Baukosten der Stufe 2 (§5.2) | Zunftwesen
`woodcutter` → `lumber_camp`, `weaver` → `large_weaver`, `tailor` → `large_tailor`, `tannery` → `large_tannery`, `pottery` → `large_pottery`, `brickworks` → `large_brickworks`, `carpenter` → `large_carpenter`, `brewery` → `large_brewery` | Baukosten der Stufe 2 (§5.2) | Zunftwesen
`quarry` → `large_quarry`, `smelter` → `blast_furnace` | Baukosten der Stufe 2 | Tiefbau
`market` → `grand_market`, `tavern` → `inn`, `fishery` → `large_fishery`, `apiary` → `large_apiary` | Baukosten der Stufe 2 (§5.3, §5.4) | Größere Gebäude
`church` → `cathedral` | 500 G, 80 stone, 30 bricks, 20 planks, 30 h | Prachtbauten
`school` → `academy`, `hospital` → `infirmary` | Baukosten der Stufe 2 (§5.4) | Gelehrtentum, Hygiene
`barracks` → `garrison`, `archer_post` → `archer_tower`, `stables` → `stud_farm` | Baukosten der Stufe 2 (§5.5) | Stadtmauern, Armbrüste, Reiterei

**Endstufen ohne Ausbau** (bewusst): die oberste Stufe jeder Kette (Adelssitz, Burg, Lagerhaus,
Wehrturm und alle Stufe-2-Gebäude); **Mauern und Tore** (Stärke über Forschung, §11); **Feld- und
Weidefarmen** (Weizen-, Gemüsefarm, Obstgarten, Viehzucht, Schafzucht – ihre Leistung wächst mit
Feldern, Boden und Forschung; die Datenprüfung schließt sie aus); kleine Ein-Mann-Betriebe, deren
Menge man über weitere Gebäude steigert (Försterhaus, Hühnerhof, Köhlerei, Käserei, Goldschmied,
Kunsthandwerker, Brunnen); Gruben und Minen (Lehm-, Sandgrube, Eisen-, Kohle-, Goldmine – ihr
Ertrag hängt am Vorkommen, mehr Förderung bringt Tiefbau); Waffenschmiede und Weinkellerei
(Spätspiel-Waren, deren Menge der Bedarf begrenzt); Vorratsspeicher (wird zum Lagerhaus) und
Handelskontor (Handelsmenge über Aufträge und Karawanen, nicht über das Gebäude).

### 5.1 Wohngebäude (Kapazität = Bewohner)
ID | Name | Größe | Kapazität | Kosten | Bauzeit | Upgrade → | Voraussetzung (Bedarf erfüllt)
---|---|---|---|---|---|---|---
`house_wood` | Einfaches Holzhaus | 2×2 | 5 | 20 G, 12 wood | 6 h | `farmhouse` | –
`farmhouse` | Bauernhaus | 2×2 | 7 | 40 G, 10 planks, 6 loam | 8 h | `citizen_house` | Nahrung, Wasser, Markt
`citizen_house` | Bürgerhaus | 2×2 | 9 | 80 G, 16 planks, 10 bricks | 10 h | `townhouse` | + Kirche, Kleidung, Keramik
`townhouse` | Stadthaus | 2×2 | 12 | 150 G, 20 bricks, 10 stone, 6 tools | 12 h | `residence` | + Schule, Möbel, Bier, Gasthaus
`residence` | Wohlhabende Residenz | 3×3 | 10 (wohlhabend) | 400 G, 30 bricks, 20 stone, 12 furniture | 18 h | `manor` | + Krankenhaus, Wein/Schmuck
`manor` | Adelssitz | 4×4 | 8 (Adel) | 1200 G, 60 stone, 30 bricks, 20 furniture, 10 jewelry | 30 h | – | + Kunst, Seide/Gewürze

Häuser steigen automatisch auf, wenn die Voraussetzungen erfüllt sind, Upgrade-Kosten im
Lager vorhanden sind und das Upgrade nicht gesperrt ist (geprüft einmal je Spielstunde, je Haus
in seinem eigenen Takt-Slot: Gebäudeindex mod 200); sie steigen ab, wenn Bedürfnisse
längere Zeit unerfüllt bleiben. Höhere Stufen zahlen mehr Steuern.

Die Voraussetzungen einer Stufe gelten für das **Erreichen und Halten** und werden an den
Bewohnern gemessen: jedes geforderte Bedürfnis im Mittel der Bewohner ≥ 60, jede geforderte
Versorgung (besetztes Gebäude) erreicht das Haus, jeder Haushalt des Hauses hat eine der
geforderten Waren im Vorrat (vom Markt). Ein unbewohntes Haus steigt nicht auf und nicht ab.
Bleiben die Voraussetzungen der eigenen Stufe **48 Spielstunden** am Stück unerfüllt
(stündliche Prüfung), fällt das Haus auf die vorige Stufe zurück (größere Grundflächen
schrumpfen um den Eingang); Haushalte über der neuen Kapazität werden obdachlos und ziehen in
freie Häuser.

### 5.2 Produktion
ID | Name | Größe | Arbeiter | Rezept(e) (Eingang → Ausgang, Dauer in Spielstunden)
---|---|---|---|---
`woodcutter` | Holzfällerhütte | 2×2 | 2 | fällt Bäume im Radius 8 → 1 wood je Baum, zurück bleibt ein Stumpf (Arbeitsgang 1,5 h am Baum, dazu die Wege)
`forester` | Försterhaus | 2×2 | 1 | pflanzt Bäume im Radius 7, auch auf Stümpfe (1 Setzling / 2 h); ist kein Platz frei: Status „Kein freier Platz zum Pflanzen“ (untätig, eine Meldung je Episode)
`sawmill` | Sägewerk | 2×3 | 2 | 2 wood → 3 planks (2 h)
`quarry` | Steinbruch | 3×3 | 3 | Stone-Vorkommen Radius 6 → 1 stone (2 h)
`clay_pit` | Lehmgrube | 2×2 | 2 | Clay-Vorkommen Radius 5 → 2 loam (2 h) · alternativ 1 clay (2 h)
`sand_pit` | Sandgrube | 2×2 | 1 | Sand-Vorkommen/Strand Radius 5 → 2 sand (2 h)
`iron_mine` | Eisenmine | 2×2 | 3 | Iron-Vorkommen unter/neben Gebäude → 1 iron_ore (3 h)
`coal_mine` | Kohlemine | 2×2 | 3 | Coal-Vorkommen → 1 coal (3 h)
`charcoal_kiln` | Köhlerei | 2×2 | 1 | 3 wood → 1 coal (4 h)
`gold_mine` | Goldmine | 2×2 | 3 | Gold-Vorkommen → 1 gold_ore (6 h)
`smelter` | Eisenschmelze | 2×2 | 2 | 2 iron_ore + 1 coal → 1 iron_bars (3 h)
`smithy` | Schmiede | 2×2 | 2 | 1 iron_bars + 1 wood → 2 tools (3 h)
`weaponsmith` | Waffenschmiede | 2×2 | 2 | 2 iron_bars + 1 wood + 1 coal → 1 weapons (4 h); 1 iron_bars + 2 leather + 1 coal → 1 armor (5 h)
`brickworks` | Ziegelei | 2×3 | 2 | 2 clay + 1 sand + 1 coal → 4 bricks (3 h)
`pottery` | Töpferei | 2×2 | 2 | 2 clay + 1 wood → 2 ceramics (3 h)
`weaver` | Weberei | 2×2 | 2 | 2 wool → 1 cloth (2 h)
`tailor` | Schneiderei | 2×2 | 2 | 1 cloth + 1 leather → 1 clothes (3 h) · 2 cloth → 1 clothes (3 h)
`tannery` | Gerberei | 2×2 | 2 | 2 hides → 1 leather (3 h)
`carpenter` | Tischlerei | 2×2 | 2 | 3 planks + 1 tools(0,25) → 1 furniture (4 h)
`mill` | Mühle | 2×2 | 1 | 2 wheat → 2 flour (2 h)
`bakery` | Bäckerei | 2×2 | 2 | 2 flour + 1 wood → 4 bread (2 h)
`brewery` | Brauerei | 2×3 | 2 | 3 grain + 0,5 honey(optional) → 2 beer (4 h)
`cheese_dairy` | Käserei | 2×2 | 1 | 3 milk → 1 cheese (3 h)
`winery` | Weinkellerei | 2×3 | 2 | 4 fruit → 1 wine (6 h)
`jeweler` | Goldschmied | 2×2 | 1 | 1 gold_ore + 1 coal → 1 jewelry (6 h)
`artisan` | Kunsthandwerker | 2×2 | 1 | 1 ceramics + 1 cloth + 1 jewelry → 1 art (8 h)

Stufe 2 der wichtigsten Werkstätten (nur durch Ausbau, Puffer 16 statt 8):

ID | Name | Größe | Arbeiter | Rezept | Kosten | Bauzeit
---|---|---|---|---|---|---
`large_sawmill` | Großes Sägewerk | 2×3 | 3 | 2 wood → 3 planks (1,25 h) | 80 G, 20 planks, 12 stone, 2 tools | 8 h
`master_smithy` | Meisterschmiede | 2×2 | 3 | 1 iron_bars + 1 wood → 2 tools (2 h) | 150 G, 15 bricks, 10 planks, 4 iron_bars | 10 h
`large_mill` | Große Mühle | 2×2 | 2 | 2 wheat → 2 flour (1,2 h) | 70 G, 20 planks, 10 stone | 8 h
`large_bakery` | Großbäckerei | 2×2 | 3 | 2 flour + 1 wood → 4 bread (1,33 h) | 80 G, 12 bricks, 10 planks | 8 h
`lumber_camp` | Holzfällerlager | 2×2 | 3 | Bäume im Radius 10 → 1 wood (1 h) | 40 G, 20 wood, 8 planks, 2 tools | 5 h
`large_quarry` | Großer Steinbruch | 3×3 | 4 | Stone-Vorkommen Radius 8 → 1 stone (1,33 h) | 90 G, 20 wood, 15 planks, 4 tools | 8 h
`blast_furnace` | Hochofen | 2×2 | 3 | 2 iron_ore + 1 coal → 1 iron_bars (2 h) | 240 G, 40 stone, 20 bricks, 6 iron_bars | 14 h
`large_brickworks` | Große Ziegelei | 2×3 | 3 | 2 clay + 1 sand + 1 coal → 4 bricks (2 h) | 160 G, 30 stone, 15 planks, 10 bricks | 11 h
`large_pottery` | Große Töpferei | 2×2 | 3 | 2 clay + 1 wood → 2 ceramics (2 h) | 100 G, 15 planks, 15 bricks | 8 h
`large_weaver` | Große Weberei | 2×2 | 3 | 2 wool → 1 cloth (1,33 h) | 100 G, 20 planks, 8 bricks | 8 h
`large_tailor` | Große Schneiderei | 2×2 | 3 | beide Rezepte in 2 h | 140 G, 20 planks, 12 bricks | 8 h
`large_tannery` | Große Gerberei | 2×2 | 3 | 2 hides → 1 leather (2 h) | 100 G, 15 planks, 15 stone | 8 h
`large_carpenter` | Große Tischlerei | 2×2 | 3 | 3 planks + 0,25 tools → 1 furniture (2,67 h) | 140 G, 25 planks, 8 bricks, 6 tools | 8 h
`large_brewery` | Große Brauerei | 2×3 | 3 | 3 grain (+0,5 honey) → 2 beer (2,67 h) | 160 G, 25 planks, 20 bricks | 11 h

„Werkzeugverbrauch": Produktionsgebäude verbrauchen pro 20 Arbeitsgängen 1 `tools`
(ohne Werkzeuge −40 % Produktivität). Ausnahmen: forester, mill.

**Schichtbetrieb:** Die Dauern gelten für Betrieb rund um die Uhr. Gearbeitet wird aber nur,
solange Arbeiter **anwesend** sind (im Gebäude, in ihrer Arbeitszeit, außerhalb der Mittagspause,
§6). Anwesende Arbeiter schaffen deshalb je Spielstunde drei Betriebsstunden (Schichtfaktor =
24 h / Schichtlänge 8 h). Eine volle Schicht ohne Arbeitsweg ergibt die Tagesleistung; jede
Minute Arbeitsweg kostet Leistung. Dasselbe gilt für Feldarbeit (§8.3) und Baustellen.

**Sammler gehen zu ihrem Ziel** (Schritt 3b): Holzfäller, Förster, Steinbruch, Lehm- und
Sandgrube und Fischerei suchen ihr Ziel (Baum, Pflanzstelle, Vorkommen, Fischgrund) im Radius,
nur unter den zu Fuß erreichbaren, und arbeiten auf dem **Arbeitsplatz** daneben: das Ziel-Tile
selbst oder – bei Wasser – das erste begehbare Nachbar-Tile (N, O, S, W), das vom Eingang aus
erreichbar ist. Ein Arbeitsgang läuft nur mit Arbeitern auf diesem Platz. Ist er fertig, trägt der
erste Arbeiter dort den Ertrag zur Hütte (er landet erst bei seiner Ankunft im Ausgangslager, der
Platz dafür ist reserviert), die übrigen gehen gleich zum nächsten Ziel. Förster haben keinen
Ertrag und gehen von Pflanzstelle zu Pflanzstelle. Verschwindet das Ziel (gefällt, überbaut,
erschöpft) oder wird der Platz unerreichbar, zieht der Arbeitsgang mit seinem Fortschritt zum
nächsten Ziel um. Minen (Vorkommen unter oder neben dem Gebäude), Viehzucht, Hühnerhof und
Imkerei arbeiten im Gebäude. Der Holzfäller-Arbeitsgang ist deshalb mit 1,5 h kürzer als die
übrigen Sammler-Dauern: zusammen mit Wegen und Heimtragen ergibt er etwa die frühere
Tagesleistung.

### 5.3 Landwirtschaft
ID | Name | Größe | Arbeiter | Funktion
---|---|---|---|---
`wheat_farm` | Weizenfarm | 3×3 | 3 | bewirtschaftet bis 24 Feld-Tiles im Radius 5 (Feldfrucht `wheat` oder `barley`→ grain, wählbar)
`vegetable_farm` | Gemüsefarm | 3×3 | 3 | bis 16 Feld-Tiles im Radius 4 (`vegetables`)
`orchard` | Obstgarten | 3×3 | 2 | bis 16 Feld-Tiles Obstbäume im Radius 4 (`fruit`, mehrjährig)
`cattle_farm` | Viehzucht | 3×3 | 2 | Weide 16 Tiles im Radius 4: meat + hides + milk (Futter: 1 grain/Tag optional +30 %)
`chicken_farm` | Hühnerhof | 2×2 | 1 | meat (klein, schnell)
`sheep_farm` | Schafzucht | 3×3 | 2 | Weide 16 Tiles im Radius 4: wool (+ meat selten)
`fishery` | Fischerei | 2×2 | 2 | am Wasser, Fish-Vorkommen Radius 8 → fish
`apiary` | Imkerei | 2×2 | 1 | honey (+60 % × Anteil von Wiesen- und Obstgarten-Tiles im Radius 6)
`large_fishery` | Große Fischerei (nur Ausbau) | 2×2 | 3 | Fischgründe Radius 10, 2 fish je 2 h; 70 G, 20 wood, 15 planks, 6 h
`large_apiary` | Große Imkerei (nur Ausbau) | 2×2 | 2 | honey je 2,67 h, Bonus-Radius 8; 60 G, 15 wood, 10 planks, 6 h

Felder und Weiden gehören exklusiv einer Farm; Aussaat, Wachstum, Ernte, Fruchtbarkeit und
Wetter regelt §8.3. **Bauern arbeiten auf den Feldern** (Schritt 3b): Säen und Ernten kommen nur
mit Arbeitern voran, die auf einem Feld-Tile ihrer Farm höchstens 2 Tiles von der laufenden
Aufgabe entfernt stehen; sie gehen zur Aufgabe (der n-te Bauer auf das n-nächste erreichbare Feld
in diesem Umkreis) und bleiben auf ihrem Feld, solange die Aufgabe höchstens 2 Tiles entfernt ist.
Ohne Aufgabe warten sie im Hof. Felder, die vom Hofeingang aus nicht zu Fuß erreichbar sind (etwa
von Gebäuden eingeschlossen), bleiben unbestellt, bis wieder ein Weg hinführt. Die Ernte
eines Feldes kommt mit dem Abschluss der Aufgabe in die Scheune (die Erntezeit umfasst das
Einbringen); von dort holen sie Träger. Feldfarmen lagern ihre Ernte in der eigenen Scheune (Ausgangslager 800 je
Feldfrucht), Träger bringen sie nach und nach ins Lager. Alle Landwirtschaftsgebäude arbeiten
im Freien (Wetter, §8.2).

Feldfrüchte (`data/crops.ron`):

Frucht | Ertrag je Feld | Wachstum bis reif | Aussaat | Ernte | Säen / Ernten je Feld | Erschöpfung
---|---|---|---|---|---|---
`wheat` (Weizen) | 32 wheat | 14 Tage | Frühling | bei Reife (früher Herbst) | 0,25 h / 0,5 h | 40
`barley` (Gerste) | 28 grain | 12 Tage | Frühling | bei Reife | 0,25 h / 0,5 h | 30
`vegetables` (Gemüse) | 45 vegetables | 6,5 Tage | Frühling, Sommer | bei Reife (2 Ernten im Jahr bei Aussaat zu Frühlingsbeginn) | 0,25 h / 0,5 h | 25
`fruit` (Obstbäume) | 40 fruit | 18 Tage bis tragfähig, dann je Jahr 12 Tage Fruchtwachstum | Frühling–Herbst (pflanzen) | Herbst | 1 h / 1 h | –

Säen und Ernten sind Farm-Stunden bei voller Besetzung (16 Felder Gemüse: 4 h säen, 8 h ernten).
Einjährige Früchte erfrieren unreif zu Winterbeginn; Obstbäume überstehen den Winter.

### 5.4 Öffentliche Gebäude
ID | Name | Größe | Arbeiter | Wirkung (Radius in Tiles)
---|---|---|---|---
`town_hall` | Rathaus (Dorfzentrum) | 4×4 | 2 | Steuern, Verwaltung, Startgebäude, Lager 200, Gebietsradius 20
`market` | Marktplatz | 4×4 | 3 | verteilt Nahrung & Waren an Haushalte (R 18), Handelsplatz
`church` | Kirche | 3×4 | 2 | Religion (R 20), Zufriedenheit
`school` | Schule | 3×3 | 2 | Bildung (R 16), 2 Forschungspunkte/h bei voller Besetzung (§9)
`hospital` | Krankenhaus | 3×3 | 3 | Gesundheit (R 18), Seuchenschutz
`well` | Brunnen | 1×1 | 0 | Wasser (R 8), Brandschutz
`tavern` | Gasthaus | 3×3 | 2 | Unterhaltung, verkauft Bier (R 16; Ausschank s. u.)
`warehouse` | Lagerhaus | 3×3 | 3 | Lager 400 je Ressourcenart, Trägerbasis (R 24)
`storehouse` | Vorratsspeicher | 2×2 | 1 | Lager 150, Nahrung (R 14)
`trading_post` | Handelskontor | 3×3 | 2 | Fernhandel, Karawanen
`grand_market` | Großer Markt (nur Ausbau) | 4×4 | 4 | Markt R 26; 160 G, 30 stone, 20 planks, 12 h
`cathedral` | Dom (nur Ausbau) | 3×4 | 3 | Religion R 30; 500 G, 80 stone, 30 bricks, 20 planks, 30 h
`academy` | Akademie (nur Ausbau) | 3×3 | 3 | Bildung R 22, 4 Forschungspunkte/h; 350 G, 30 bricks, 20 stone, 15 planks, 18 h
`infirmary` | Spital (nur Ausbau) | 3×3 | 4 | Gesundheit R 26; 400 G, 35 bricks, 20 stone, 20 planks, 20 h
`inn` | Gasthof (nur Ausbau) | 3×3 | 3 | Unterhaltung R 22, verkauft Bier; 200 G, 30 planks, 15 bricks, 12 h

**Ausschank (`sells`):** Ein Gebäude mit `sells` (Gasthaus: Bier) hält einen kleinen Vorrat
(`shop_stock` = 12 je Ware), den Träger aus den Lagern auffüllen, sobald er höchstens halb voll
ist (Belieferung wie ein Eingangslager). Gäste, die abends zur Freizeit ins Gasthaus kommen,
kaufen für ihren Haushalt bis zu 1,5 Tage Bedarf (`household_use`, mindestens 1 Einheit) zum
Marktpreis, wenn der Haushalt es bezahlen kann (§6 Wohlstand); Bier im Vorrat wird zu Hause
verbraucht und zählt auch für die Bier-Anforderung des Stadthauses. Unterhaltung: Gasthaus mit
Bier (im Ausschank oder im Haushaltsvorrat) → 100, ohne Bier → `tavern_without_goods_entertainment`
(60) und nur der halbe Gemeinschaftsbonus. Berichte: `report::shop_report` (Bestand,
„Kein Bier vorrätig“), `PopulationSummary::taverns` (offene / mit Bier).

### 5.5 Militär
ID | Name | Größe | Funktion | Garnison | Verteidigung (`defense`)
---|---|---|---|---|---
`barracks` | Kaserne | 3×3 | Rekrutierung & Ausbildung Infanterie und Belagerungsgerät; 150 G, 25 stone, 15 planks, Unterhalt 4 | 10 | 600 TP
`watchtower` | Wachturm | 1×1 | Sicht/Sicherheit R 10, Gebiet +8 | 1 | Fernkampf 5, Reichweite 8, 250 TP
`stone_tower` | Wehrturm (nur Ausbau des Wachturms) | 1×1 | Sicherheit R 12, Gebiet +10, Unterhalt 3 | 2 | Fernkampf 8, Reichweite 10, 600 TP
`archer_post` | Bogenschützenposten | 2×2 | Bogenschützen & Armbrustschützen, Sicherheit R 8 | 4 | Fernkampf 6, Reichweite 8, 350 TP
`stables` | Stallungen | 3×3 | Kavallerie (je fertige Viehzucht Pferde für 4 Reiter) | 6 | 500 TP
`garrison` | Garnison (nur Ausbau der Kaserne) | 3×3 | bildet weiter Infanterie aus, Sicherheit R 18; 300 G, 50 stone, 20 planks, 6 iron_bars, Unterhalt 6 | 20 | 1 200 TP
`archer_tower` | Schützenturm (nur Ausbau des Postens) | 2×2 | bildet weiter Schützen aus, Sicherheit R 12; 220 G, 40 stone, 15 planks, Unterhalt 6 | 8 | Fernkampf 10, Reichweite 10, 700 TP
`stud_farm` | Gestüt (nur Ausbau der Stallungen) | 3×3 | bildet weiter Kavallerie aus; 300 G, 40 planks, 30 stone, Unterhalt 8 | 10 | 800 TP
`wall` | Stadtmauer | 1×1 (Segment, wie Straße gezogen) | blockiert Feinde, auto-verbunden | – | 400 TP
`gate` | Tor | 1×1 (auf Mauerlinie) | Durchgang für eigene Leute, Feinde müssen es brechen | – | 300 TP
`castle` | Burg | 5×5 | Hauptquartier, Garnison, Lager, Sicherheit R 26 | 20 | Fernkampf 8, Reichweite 10, 2 500 TP
`town_hall` | Rathaus (§5.4) | 4×4 | Hauptquartier | 4 | 1 200 TP
`weaponsmith` | siehe Produktion | | | |

Gebäude ohne eigene Verteidigungswerte haben `military.building_hp_per_tile` (60) Trefferpunkte je
Grundflächen-Tile. Mauern und Tore gewinnen durch `WallStrengthBonus` (§9).

### 5.6 Straßen (`data/roads.ron`)
ID | Name | Kosten je Tile | Tempo | Freischaltung
---|---|---|---|---
`path` | Trampelpfad | 0 G | 1,0 | Start
`gravel` | Schotterweg | 1 G + 1 stone | 1,15 | Start
`road` | Straße | 2 G + 1 stone | 1,25 | Architektur I
`paved` | Gepflasterte Straße | 4 G + 2 stone + 1 sand | 1,4 | Steinbau
`trade_road` | Handelsstraße | 6 G + 2 stone + 1 sand | 1,5, Handel +15 % | Handelsverträge
`bridge` | Brücke | 15 G + 4 planks + 2 stone je Wasser-Tile | 1,25 | Start (max. 6 Tiles Spannweite, nur flaches Wasser)

**Handelsstraßen** (`trade_bonus_percent`): Eine Karawane zählt auf jedem Weg über die Karte
(zum Kontor und vom Kontor zum Kartenrand) die betretenen Tiles und den Handelsbonus der Straße
darunter; ihr Handel (fremde Händler am Kontor: der Hinweg, eigene Karawanen beim Partner: der
Weg hinaus) erhält den Durchschnitt als zusätzlichen Preisvorteil (nur Handelsstraße → +15 %,
halb → etwa +7 %; zusammen mit Forschung, Beziehung und Verträgen höchstens +50 %). Ohne
Königreiche zählt für den Weltmarkt der kürzeste Straßenweg vom Kontor zum Kartenrand (ohne
Straßenweg dorthin: kein Bonus). Der Handelsbericht nennt den Bonus („Handelsstraßen zum
Kartenrand: Preise 15 % besser.“).

Straßen verbinden sich automatisch (4-Nachbar-Bitmaske N=1,E=2,S=4,W=8 → 16 Sprites je Typ).
Gebäude benötigen Straßenzugang an einem Eingangs-Tile (vor der Vorderkante); beim Platzieren
wird automatisch ein Pfad zum nächsten Straßenstück vorgeschlagen. Er wird mit dem Gebäude in
einem Kommando gebaut (`PlaceBuildingWithAccess`) – nur, wenn das Gebäude angenommen wurde.
Wird ein Tile einer Brücke entfernt, verschwindet die ganze Brücke – aber nicht, solange jemand
darauf steht oder sie gerade betritt oder verlässt („Auf der Brücke bei … ist gerade jemand
unterwegs …“); ohne Deck stünde er auf Wasser. Arbeitsplätze im Freien (Bauarbeiter, Sammler)
liegen deshalb nie auf Brücken.

## 6. Bevölkerung

Jeder Bewohner ist eine eigene Entität: ID, Name, Alter, Geschlecht (für Familien relevant),
Beruf, Wohnung, Arbeitsplatz, Bedürfnisse, Gesundheit, Zufriedenheit, Bildung, Wohlstand
(Ersparnis), Beziehungen (Partner, Eltern, Kinder, Freunde), aktuelle Tätigkeit, Position.

**Bedürfnisse** (0–100, sinken über Zeit, werden durch Gebäude/Waren erfüllt):
`hunger` (Nahrung aus Haushaltsvorrat), `thirst` (Brunnen/Fluss), `shelter` (Wohnung),
`warmth` (Holz/Kohle im Winter, Kleidung), `safety` (Wachtürme/Kaserne, keine Räuber),
`health` (Krankenhaus, Nahrungsvielfalt), `social` (Familie, Gasthaus, Markt, Freunde),
`entertainment` (Gasthaus – voll nur mit Bier, §5.4 –, Feste), `faith` (Kirche).
Zufriedenheit = gewichteter Mittelwert der Bedürfnisse + Modifikatoren (Steuern, Löhne,
Arbeitslosigkeit, Wohlstand, Ereignisse, Krieg – Kriegsmüdigkeit §10). Zufriedenheit < 30 →
Abwanderung/Unruhen; > 70 → Zuwanderung.

**Wohlstand (Ersparnis des Haushalts)** (`rules.ron` → `population.wealth`, Modul
`population::wealth`): Jeder Haushalt führt eine Geldbörse in Milli-Gold (nie negativ). Einmal je
Spielstunde (Wirtschaftsschritt) verdient er 1/24 seines Tageseinkommens: je arbeitendem Mitglied
(Beschäftigte, Soldaten) `income_per_worker` seiner Wohnstufe (Holzhaus 30, Bauernhaus 36,
Bürgerhaus 48, Stadthaus 64, Residenz 150, Adelssitz 400 Gold; Obdachlose 20) plus den Lohn der
Krone (§7) bzw. den Sold; Alte erhalten 50 %, Erwachsene ohne Arbeit 25 % des Arbeitsverdienstes,
Kinder nichts. Er zahlt daraus 1/24 der Kopfsteuer und der Marktgebühr (§7) – soweit die Ersparnis
reicht; die Staatskasse bucht genau das Gezahlte. Jede Einheit, die der Haushalt aus dem Lager,
vom Markt oder im Gasthaus holt, kostet den aktuellen Marktpreis: Nahrung und Brennstoff bekommt
er auch ohne Geld (er zahlt, was er hat), Waren und Bier nur, wenn er sie bezahlen kann. Neue
Haushalte bringen 30 Gold je Mitglied mit; wer in einen anderen Haushalt zieht (Heirat,
Aufteilung einer zu großen Familie), nimmt seinen Anteil mit. Unter 5 Gold je Mitglied ist ein
Haushalt **arm** (Zufriedenheit −15), ab 150 Gold je Mitglied **wohlhabend** (+4). Armut senkt so
die Zufriedenheit bis zur Abwanderung; arme Haushalte kaufen keine Waren und halten deshalb
höhere Wohnstufen nicht. Berichte: je Haushalt Ersparnis, Klasse, gestern verdient/ausgegeben
(`HouseholdRow`), je Bewohner Ersparnis und Klasse des Haushalts (`CitizenDetail`), im
Bevölkerungsbericht Summe, Durchschnitt, Median, Haushalte je Klasse und die Tagessummen
(`WealthSummary`).

**Freunde** (`population::friends`): Einmal je Spielstunde lernen sich Bewohner kennen, die
zusammen an einem Ort sind – Kollegen im Arbeitsgebäude, Schüler in der Schule, Gäste im
Gasthaus oder in der Kirche in ihrer Freizeit, die Haushalte eines Hauses; Kinder freunden sich
mit Kindern an, Erwachsene mit Erwachsenen und Alten, Familienmitglieder sind keine Freunde. Jeder
mit freiem Platz trifft einen anderen der Gruppe (Hash der Stunde, kein Zufallszug) und wird mit
2 % sein Freund (gegenseitig, höchstens 3 Freunde). Freundschaften enden mit Tod oder
Abwanderung. Jeder Freund hebt das Bedürfnis Gemeinschaft um 10. `CitizenDetail::friends`.

**Tagesablauf** (Stunden in `rules.ron` → `walking.routine`):
06:00 Aufstehen · 07:00 Frühstück (zu Hause) · 08:00 Arbeitsweg – die Arbeit beginnt mit der
Ankunft · bis 12:00 Arbeit · 12:00–13:00 Mittagspause am Arbeitsplatz · 13:00–17:00 Arbeit ·
17:00 Feierabend und Heimweg · 18:00 Besorgungen: der erste Erwachsene jedes bewohnten Hauses
(Soldaten nur, wenn kein anderer Erwachsener oder Alter im Haushalt lebt) geht zum nächsten besetzten Markt, dessen Reichweite das Haus erreicht (ohne Markt zum nächsten
Lager, dessen Trägerradius das Haus erreicht) und füllt dort den Vorrat des Haushalts auf (§6.1);
ist der Vorrat ohne Nahrung, geht er schon vorher, sobald er nicht arbeitet und es dort Nahrung
gibt · 19:00 Freizeit: Erwachsene und
Alte gehen jeden zweiten Abend (abwechselnd je Bewohner und Tag) in ein Gasthaus oder eine Kirche
in Reichweite, sonst bleiben sie bei der Familie · 21:00 Heimweg · 22:00 Schlafen. Bauern (Feld-
und Weidehöfe) arbeiten im Sommer bis 18:00, im Winter bis 16:00. Kinder ab 6 mit einer besetzten
Schule in Reichweite gehen von 08:00 bis 13:00 zur Schule; Alte (> 60) arbeiten nicht, Erwachsene
ohne Arbeit bleiben tagsüber zu Hause. Obdachlose lagern im Haupt-Hauptquartier.

Der Tag ändert sich mit den Ereignissen:
* **Festtage** (Ereignis „Religiöses Fest“, Modifikator `Festival`): vom Ende der Mittagspause
  bis 21:00 feiern alle (außer Soldaten im Dienst und Kranken) bei der Kirche in Reichweite,
  sonst auf dem Markt, statt zu arbeiten (Tätigkeit „Feiert“) – die Nachmittagsproduktion fällt
  aus.
* **Kranke** (Gesundheit < 40, `sick_health`) arbeiten nicht, gehen nicht zur Schule und nicht
  aus: tagsüber gehen sie in ein Krankenhaus in Reichweite („Im Krankenhaus“, Gesundheit erholt
  sich dort doppelt so schnell, `hospital_recovery_percent`), sonst bleiben sie zu Hause („Krank
  zu Hause“). Die Besorgungen macht ein gesundes Mitglied; nur wenn keines da ist, geht der
  Kranke selbst. Ein Kranker, der im eigenen Krankenhaus behandelt wird, ist dort Patient und
  zählt nicht als anwesender Arbeiter. Wer `sick_leave_after_hours` (24) Stunden am Stück krank
  ist, geht in den **Krankenstand**: er gibt seine Stelle ab (die Arbeitszuteilung besetzt sie neu)
  und nimmt erst gesund wieder Arbeit an (`Citizen::is_on_sick_leave`, Bürgerbericht
  `sick_leave`) – Kranke blockieren so keine Holzfällerhütte durch den Winter.
* **Feinde auf der Karte:** Ein Zivilist, der selbst oder dessen Ziel im Alarmradius
  (`military.alert_radius`) eines Feindes liegt, flieht nach Hause, liegt auch das Haus im
  Radius, ins Haupt-Hauptquartier („Flieht vor den Feinden“, dort „In Sicherheit“); Soldaten und
  Träger mit Ladung folgen weiter ihrem Dienst.

**Anwesenheit:** Ein Arbeiter zählt nur als anwesend, solange er an seinem **Arbeitsort** ist –
in seiner Arbeitszeit, außerhalb der Mittagspause, nicht unterwegs und ohne Transport: in
Werkstätten, Lagern, Diensten und Minen im Gebäude; Bauarbeiter auf einem Tile rund um ihre
Baustelle (§5); Bauern auf einem Feld ihrer Farm nahe der Aufgabe, ohne Aufgabe im Hof (§5.3); Sammler auf dem
Arbeitsplatz des laufenden Arbeitsgangs (§5.2). Produktion, Feldarbeit und Bau rechnen nur mit
anwesenden Arbeitern (Schichtfaktor §5.2); ohne Anwesende ruht das Gebäude („Arbeiter auf dem
Weg“, „Mittagspause“, „Feierabend“). Der Produktionsbericht zeigt den **Arbeitsweg** (Ø-Dauer
des Wegs von zu Hause bis zur ersten Ankunft am Arbeitsort und Anteil an der Schicht; Wege
zwischen Arbeitsplätzen, Abholungen und Lieferungen zählen nicht). Märkte und Dienste wirken,
solange Arbeiter zugeteilt sind.

**Arbeit:** automatische Zuweisung nach Berufspriorität (vom Spieler pro Kategorie
einstellbar: Nahrung, Rohstoffe, Verarbeitung, Bau, Dienste, Militär), Entfernung und
Qualifikation (Bildung erforderlich für Schule, Krankenhaus, Goldschmied, Kunsthandwerker).
Produktivität = Gesundheit × Zufriedenheit-Faktor × Werkzeuge × Ausbildung (Einarbeitung).

**Qualifikation und Einarbeitung** (`population.training`): Jeder Bewohner zählt seine
Arbeitstage je Berufskategorie (`Citizen::experience_days`, um Mitternacht +1 für jeden, der
Arbeit hat und nicht krank ist, höchstens 255; gespeichert). Hilfsarbeit (`JobTier::Unskilled`)
braucht keine Einarbeitung. In Facharbeit und gelehrter Arbeit (`Skilled`, `Educated`) leistet ein
Neuling der Kategorie 80 % (Gebildete 90 %, `start_percent`/`educated_start_percent`) und ist nach
`full_after_days` (3) Tagen voll eingearbeitet (dazwischen linear). Erfahrung bleibt je Kategorie
erhalten: wer wechselt und zurückkehrt, ist sofort wieder eingearbeitet. Die Siedler des
Spielstarts bringen ihr Handwerk mit (voll eingearbeitet), Zuwanderer und Heranwachsende lernen
im Beruf. Der Faktor wirkt in der Belegschaft (`production::staffing_permille`,
`production::worker_permille`), in der Tagesleistung (Wirtschaftlichkeit) und in der Forschung
der Gelehrten; Berichte: `ProductionReport::training_percent` (Mittel der Belegschaft),
`CitizenDetail::training` („In Einarbeitung: 1 von 3 Tagen, 86 %“, „Eingearbeitet“,
„Hilfsarbeit – keine Einarbeitung nötig“).

**Bewegung:** keine Teleportation – Bewohner laufen von Tür zu Tür (A* über die Wegkosten von
§3, Höhenstufen nur mit Δ ≤ 1). Im Gebäude sind sie unsichtbar. Geschwindigkeit: 0,2 Tiles/Tick
auf einem Pfad, geteilt durch die Wegkosten des betretenen Tiles, × Kinder 90 % / Alte 75 %,
× Gesundheit (unter 50 linear bis 60 % bei 0), × Schneedecke (bis 80 %), × Regen (95 %) bzw.
Sturm (85 %) bei voller Stärke. Wege werden gesucht, sobald ein neues Ziel ansteht; die Suche hat
ein Budget je Tick (10 000 Knoten, dazu 100 je wartender Anfrage, höchstens 25 000), weitere
Anfragen warten (auch die Wartezeit zählt zum Arbeitsweg). Gefundene Wege merkt sich die
Simulation (bis 8192 Paare, die am längsten unbenutzten werden vergessen; der Rückweg ist der
umgekehrte Hinweg): ein gemerkter Weg kostet nur 25 Knoten des Budgets. Er wird vergessen, sobald
ein Tile in seinem Umriss teurer oder gesperrt wird oder ein billiger gewordenes Tile (neue
Straße, abgerissenes Gebäude) eine kürzere Route möglich macht (untere Schranke über das
geänderte Rechteck unter den gemerkten Kosten); gemerkte Wege sind daher immer kürzeste Wege. Das
Gedächtnis wird mitgespeichert. Wird unterwegs etwas auf den Weg gebaut, wird neu gesucht; wer selbst überbaut
wird, geht auf kürzestem Weg aus der Grundfläche hinaus. Wer in einem Gebäude ist, das versetzt
oder vergrößert wird, tritt an der alten Tür heraus und läuft zur neuen. Ohne Weg (Insel,
verbauter Eingang, Klippe) bleibt der Bewohner, wo er ist, versucht es nach 2 Stunden erneut und
meldet höchstens alle 6 Stunden je Gebäude „Kein Weg zur Arbeit/nach Hause: … ist nicht
erreichbar.“ Träger (die Arbeiter der Lager) laufen mit den Waren zwischen Produktionsgebäuden,
Baustellen und Lagern (§7); Sammler tragen ihren Ertrag zur Hütte (§5.2).

**Demografie:** Paare bilden Haushalte, Geburten bei Wohnraum + Zufriedenheit, Alterung,
Tod durch Alter/Krankheit/Hunger, Zu- und Abwanderung abhängig von Zufriedenheit und freiem
Wohnraum. Kinder erben Nachnamen.

**Simulation-LOD:** Bedürfnisse und Entscheidungen (auch das Ziel des Tagesablaufs) werden
gestaffelt (jeder Bewohner alle 20 Ticks, verteilt nach ID) aktualisiert; Bewegung läuft jeden
Tick nur für aktive Läufer.
Wirtschaftliche Ergebnisse sind davon unabhängig (aggregierte Raten). Haushalte verbrauchen
einmal je Spielstunde (verteilt nach ID; aufgefüllt wird beim Besorgungsgang, Obdachlose
stündlich, Haushalte nur aus Kindern – etwa nach dem Tod der Eltern im Kampf – ebenfalls stündlich
durch die Nachbarn aus den Märkten und Lagern, die das Haus versorgen) und durchlaufen einmal am Tag die Demografie; Häuser
prüfen ihre Stufe stündlich, der automatische Ausbau prüft jedes Haus stündlich in seinem
eigenen Takt-Slot; Zuwanderung stündlich, Hochzeiten um Mitternacht. Besorgungsgänge, die im
selben Tick ankommen, füllen ihre Vorräte gemeinsam am Ende des Bewegungsschritts auf (in
Ankunftsreihenfolge). Messwerte bei 5000 Bewohnern: docs/architecture.md §3.6.

### 6.1 Werte (`data/rules.ron` → `population`, Namen in `data/names.ron`)

**Lebensphasen:** Kind < 14, Erwachsen 14–60 (arbeitet), Alt ≥ 61 (arbeitet nicht). Alter
steigt um 1 Jahr je Jahreszeit (6 Tage), Geburtstage verteilt. Namen: deutsche Vornamen nach
Geschlecht und Familiennamen; Kinder erben den Familiennamen ihres Haushalts.

**Haushalt und Versorgung:** Jeder Bewohner gehört zu einem Haushalt (Familie), der in einem
Haus wohnt oder obdachlos beim Haupt-Hauptquartier lagert. Der Haushalt hält einen **Vorrat
für 1,5 Tage** (Nahrung, im Winter und schon am letzten Herbsttag Brennstoff, Waren) und füllt ihn
**auf dem Besorgungsgang** auf (§6, Tagesablauf): kommt der Läufer am besetzten Markt in
Reichweite an, holt der Haushalt Nahrung, Brennstoff und die Waren seiner Wohnstufe (und der
nächsten, wenn der Ausbau nicht gesperrt ist) aus dem Königreichslager – die Marktstände
schöpfen aus den dem Haus nächstgelegenen Lagern. Ohne Markt holt er im Lager, dessen
Trägerradius das Haus erreicht, nur Nahrung und Brennstoff. Wartet der Läufer dort, wird stündlich
nachgefüllt. Obdachlose lagern am Hauptquartier und nehmen stündlich aus jedem erreichbaren Lager.
„Nahrungsmangel“ wird nur gemeldet, wenn auch Markt bzw. Lager keine Nahrung für den Haushalt
haben. An Baustellen gebundenes Material wird nie genommen.
Entnahmen zählen in der Ressourcenstatistik als Verbrauch (Spalte „Haushalte“). Gegessen wird
stündlich, zuerst die Sorte mit dem meisten Vorrat; beim Auffüllen kommen fehlende Sorten
zuerst (Abwechslung).

**Bedürfnisse (0–100)** folgen ihrem Versorgungsniveau mit 10 Punkten je Stunde:

Bedürfnis | Niveau
---|---
Nahrung | gegessen → 100; ohne Essen −3 je Stunde
Wasser | Brunnen in Reichweite oder Fluss/See ≤ 8 Felder → 100, sonst 25 (bei Dürre 10, §8.2)
Unterkunft | Haus → 100, obdachlos → 0
Wärme | an warmen Tagen (≥ 5 °C, §8.1) 100; an kalten Tagen (immer im Winter) mit Brennstoff 100, ohne 10; Kleidung +30 (Brennstoffbedarf × Heizbedarf des Wetters)
Sicherheit | Wachturm/Kaserne/Burg → 100, sonst 70 (Räuber plündern nur unbewachte Lager, §12)
Gesundheit | 30 + 10 je weiterer Nahrungssorte im Vorrat (max. +30) + Krankenhaus 40
Gemeinschaft | 20 + Familie (Haushalt ≥ 2) 30 + Markt 25 + Gasthaus 25 (ohne Bier 12) + 10 je Freund (höchstens 3)
Unterhaltung | Gasthaus mit Bier → 100, Gasthaus ohne Bier → 60, sonst 10
Glaube | Kirche → 100, sonst 10

**Zufriedenheit** = gewichteter Mittelwert der Bedürfnisse (Nahrung 2, Wasser 1,5,
Unterkunft 1,5, Wärme 1, Sicherheit 1, Gesundheit 1,5, Gemeinschaft 1, Unterhaltung 1,
Glaube 1) + Modifikatoren: Steuern ±1 je Prozentpunkt unter/über 10 %, Lohn +5 (beschäftigte
Erwachsene und Soldaten, die Sold erhalten), Arbeitslosigkeit −10 (Erwachsene ohne Arbeit), fehlende Waren der eigenen
Wohnstufe −8 je Ware, Wohlstand des Haushalts (arm −15, wohlhabend +4). Sie folgt diesem Ziel mit 5 Punkten je Stunde. Ein einfaches Dorf
(Haus, Nahrung, Brunnen, Markt) erreicht etwa 72; Kirche, Gasthaus und Krankenhaus heben sie
gegen 100.

**Gesundheit** folgt (3 × Nahrung + 2 × Wasser + 2 × Wärme + 3 × Gesundheitsbedürfnis) / 10 mit
2 Punkten je Stunde; Alte verlieren je Lebensjahr über 60 zwei Punkte Höchstgesundheit.
Hungernde (Nahrung 0) verlieren zusätzlich 3, Frierende (Winter, Wärme < 15) 2 Punkte je
Stunde; bei Gesundheit 0 stirbt der Bewohner (Hunger, Kälte bzw. Krankheit).

**Produktivität** eines Arbeiters bzw. Bauarbeiters: voll ab Gesundheit 50 und Zufriedenheit 30,
darunter linear bis je 50 % (zusammen min. 25 %); die Besetzung eines Gebäudes ist die Summe
dieser Anteile.

**Demografie:**
* Tod durch Alter: Sterbewahrscheinlichkeit je Lebensjahr ab 50: 1 %, ab 60: 5 %, ab 65: 12 %,
  ab 70: 30 %, ab 75: 60 %, ab 80: 100 % (täglich anteilig).
* Krankheit: täglich 0,5 ‰ × ((100 − Gesundheit) / 50)², mit Krankenhaus 25 %, Kinder und Alte
  × 1,5, × Krankheitsrisiko des Wetters (§8.2, bis 130 % bei Schnee), Forschung
  `DiseaseResistance` senkt weiter.
* Geburten: Paar im selben Haushalt, Mutter 18–40, freier Platz im Haus, Haushalt satt und
  Zufriedenheit ≥ 50: 5 % je Tag, frühestens 6 Tage nach der letzten Geburt.
* Hochzeiten: täglich 20 % je ledige, kinderlose Frau ab 18 mit einem ledigen Mann aus einem
  anderen Haushalt (Altersunterschied ≤ 12); einer zieht in den Haushalt des anderen, wenn dort
  Platz ist.
* Zuwanderung: stündlich, wenn die mittlere Zufriedenheit > 70 ist, niemand obdachlos ist, ein
  Haus Platz hat und das Lager Nahrung für einen Tag aller Bewohner hält; Chance 2 % je Punkt
  über 70 (max. 60 %), höchstens max(8, 10 % der Bevölkerung) Siedler je Tag. Die Siedler
  kommen **zu Fuß vom Kartenrand**: vom Rand-Tile, das dem Eingang des Haupt-Hauptquartiers am
  nächsten liegt und von dort erreichbar ist (eine Straße, die den Kartenrand erreicht, hat
  Vorrang; ohne erreichbaren Rand erscheinen sie am Hauptquartier), und gehen in das am besten
  passende Haus. **Jedes Haus behält einen Platz
  für Geburten und Hochzeiten** (`immigration_reserved_places: 1`); Zuwanderer nehmen nur die
  übrigen freien Plätze (Bevölkerungs-Tooltip „Frei für Zuwanderer“). Haushalte: 30 % Einzelne,
  30 % Paare, sonst Paare mit 1–3 Kindern; 10 % der Erwachsenen sind gebildet.
* Zu große Haushalte: Ein obdachloser Haushalt, der größer ist als das größte baubare Haus
  (Holzhaus: 5), teilt sich auf – das erste Paar behält die jüngsten Kinder, erwachsene Kinder
  (mit Partner), weitere Erwachsene und die ältesten Kinder bilden neue Haushalte gleichen
  Namens. So bleibt niemand dauerhaft obdachlos (etwa nach Abriss oder Abstieg eines
  Bauernhauses mit 7 Bewohnern), und die Zuwanderung bleibt nicht für immer gesperrt.
* Abwanderung: ein Haushalt mit mittlerer Zufriedenheit < 30 an 2 aufeinanderfolgenden Tagen
  verlässt das Reich: Er zählt sofort nicht mehr zur Bevölkerung und wandert zu Fuß zum
  Kartenrand (Ausgang wie bei der Zuwanderung), wo er verschwindet. Neugeborene erscheinen in
  ihrem Haus, Verstorbene verschwinden dort, wo sie sind.
* Bildung: Kinder ab 6 mit Schule in Reichweite +4 Bildungspunkte je Tag; wer bei Volljährigkeit
  ≥ 50 hat, ist gebildet (Voraussetzung für Schule, Krankenhaus, Goldschmied, Kunsthandwerker).

**Arbeit:** Zuerst bekommt jedes Lager einen Träger, jeder Markt einen Arbeiter und die erste
Baustelle mit Material (nach Gebäude-Priorität, dann ID) einen Bauarbeiter (**Grundbesetzung**,
vor allen Prioritäten: ohne Träger erreicht keine Ware das Lager, ohne Markt keine Nahrung die
Haushalte, und ohne Bauarbeiter entstünde in einer Stadt, in der alle Erwachsenen arbeiten, nie
das Haus für die Zuwanderer; ein stillgelegtes Lager braucht keinen). Danach werden
Erwachsene nach Kategorie-Priorität → Gebäude-Priorität → Kategorie-Reihenfolge
→ Gebäude-ID verteilt; Bauarbeiter (Kategorie Bau) gehen an Baustellen mit Material, Lagerarbeiter
sind die Träger. Produktionsgebäude ohne Arbeit (keine Aufträge, keine Rohstoffe im Umkreis,
Felder wachsen oder ruhen bis zur Saison) kommen zuletzt an die Reihe: Sie behalten ihre
Arbeiter nur, solange niemand sonst sie braucht. Zuweisungen bleiben stabil; freie Stellen
erhält der nächstwohnende Arbeitslose, überzählige Arbeiter mit dem weitesten Weg werden zuerst
frei. Stilllegen, Abriss oder das Altwerden (ab 61) beenden die Arbeit sofort. **Soldaten** (§11)
stehen dem Arbeitsmarkt nicht zur Verfügung (sie zählen nicht zu den Erwachsenen im
arbeitsfähigen Alter der Arbeitszuteilung und nicht als arbeitslos). Lager ohne Träger,
deren Aufträge seit einer Stunde warten, melden alle 6 Stunden „Keine Träger“.

**Benachrichtigungen** (deutsch, je Thema höchstens alle 2–6 Stunden): Nahrungs- und
Brennstoffmangel, Obdachlose, Zuwanderung, Abwanderung, Geburten, Todesfälle, Abstieg.

## 7. Wirtschaft

* **Lager:** Lagerhäuser/Speicher/Rathaus halten Bestände; Produktionsgebäude haben
  Eingangs-/Ausgangspuffer (Standard 8). Volles Ausgangslager → Produktion stoppt; fehlende
  Eingänge → Pause mit Status „Rohstoffmangel".
* **Träger** (`rules.ron`: `carrier_capacity` 12, `carrier_handling_ticks` 10): die Arbeiter
  eines Lagers sind seine Träger. Alle 20 Ticks plant die Logistik Aufträge: Ausgangslager →
  nächstes Lager mit Platz (ab halbem Puffer oder voll), Überschuss im Eingangslager → Lager,
  Lager → Eingangslager (sobald höchstens halb voll), Lager → Baustelle; je Auftrag höchstens
  eine Traglast, nur zwischen Gebäuden am selben Straßennetz und im Trägerradius des Lagers.
  Ein Lager nimmt neue Aufträge nur an, solange höchstens 4 je Träger warten (mindestens 4),
  sonst wird das nächste Lager gewählt oder später geplant. Ein Träger, der in seiner
  Arbeitszeit außerhalb der Mittagspause im Lager ist, übernimmt den wichtigsten wartenden
  Auftrag seines Lagers (Priorität, dann Alter); wartende Aufträge derselben Ware zwischen
  denselben Gebäuden kommen bis zur Traglast mit. Er **geht zur Quelle**, lädt 10 Ticks – erst
  dann verlassen die Waren die Quelle –, **trägt sie zum Ziel**, lädt 10 Ticks ab – erst dann
  kommen sie an – und geht zurück zu seinem Lager; liegt bei der Quelle oder höchstens 6 Tiles
  davon eine wartende Ware seines Lagers (Rückfracht), nimmt er sie gleich mit. Ein begonnener
  Gang wird auch nach Feierabend beendet. Waren unterwegs gehören dem Träger: Nimmt das Ziel sie
  nicht (mehr) an – abgerissen, voll, unerreichbar –, bringt er sie zum nächsten erreichbaren Lager
  mit Platz, ohne eines in sein eigenes Lager. Ist ein Lager als Ziel voll und hat kein anderes
  erreichbares Lager Platz, stapelt er seine Last dort auch über die Kapazität hinaus (der
  Überschuss wird zuerst verbraucht), statt mit ihr zu warten und die übrigen Aufträge seines
  Lagers zu blockieren; nur wer keines davon erreicht, wartet und versucht es stündlich erneut.
  Geprüfte Regel (Sim-Lane Schritt 6): Waren gehen nie verloren und werden nicht versetzt; ein
  Lager liegt höchstens um die Last der Träger über der Kapazität, die schon unterwegs waren, als
  es voll wurde, nimmt danach keine Aufträge an, bis es wieder unter der Kapazität ist, und
  zeigt „Bestand / Kapazität“ ehrlich an (z. B. „308 / 300“).
  Andere Zugänge (Holz vom Roden, Käufe, Rückerstattungen, Ereignisse) belegen den Platz, den
  Träger für unterwegs befindliche Waren reserviert haben, erst, wenn kein Lager mehr freien
  Platz hat. Wer seine Stelle verliert,
  liefert erst ab; stirbt ein Träger oder wandert er ab, kommen seine Waren ins Ziel, sein Lager
  oder das nächste Lager mit Platz (nur was nirgends Platz hat, geht verloren und wird gemeldet).
  Verschwindet die Quelle vor dem Abholen oder ist sie unerreichbar, entfällt der Auftrag. Berichte:
  Träger unterwegs/frei, Waren unterwegs, Ø Dauer eines Transports, je Gebäude „Wartet auf Träger:
  … Transporte (… Einheiten) seit … min“.
* **Produktionswarteschlange & Prioritäten:** je Gebäude Rezeptwahl, Priorität
  (niedrig/normal/hoch – Träger und Arbeiter bevorzugen hohe Priorität), Pausieren,
  Warteschlange „N Einheiten von Rezept X" oder „dauerhaft".
* **Preise:** je Ressource dynamischer Marktpreis `p = base × clamp((Nachfrage+1)/(Angebot+1))^0.5`
  mit Glättung; Angebot = Lagerbestand + Produktion/Tag, Nachfrage = Verbrauch/Tag + Bauaufträge.
* **Geld:** Einnahmen: Steuern (Kopf-/Wohnsteuer je Haustyp × Steuersatz 0–30 %),
  Marktgebühren, Exporte, Tribute. Ausgaben: Löhne (je Arbeiter/Tag nach Berufsstufe),
  Gebäudeunterhalt, Importe, Armee-Sold, Forschung (Gold beim Beginn einer Technologie, §9).
  Ereignisse (§12) buchen ihre Goldwirkungen als „Ereignisse“, Geschenke, Tribute und
  Friedenszahlungen (§10) als „Diplomatie“. Bilanz im Finanzfenster; jede Änderung des Schatzes
  steht im Kassenbuch. Die Warnung zur Staatskasse nennt die Resttage („reicht nur noch für etwa
  1 Tag“ / „… für etwa 3 Tage“, unter einem Tag „nicht einmal mehr für 1 Tag“) und bei leerer
  Kasse „Die Staatskasse ist leer.“
  Steuer und Marktgebühr zahlen die Haushalte aus ihrer Ersparnis (§6 Wohlstand); die
  Staatskasse bucht stündlich genau das Gezahlte, die Tagesvorschau rechnet mit dem, was Ersparnis
  und Einkommen decken. Steuern zahlt jeder Haushalt eines Hauses im Verwaltungsradius (Rathaus/Burg): Mitglieder ×
  `tax_per_resident` der Wohnstufe × Steuersatz (Obdachlose zahlen nichts); Marktgebühr
  0,5 Gold je von einem besetzten Markt versorgtem Bewohner; Löhne je beschäftigtem Bewohner
  (Hilfskraft 0,6, Fachkraft 1,0, Gelehrter 1,8 Gold/Tag; Bauarbeiter als Hilfskraft).
  Beim Standardsteuersatz 10 % zahlt ein Bewohner eines Holzhauses 1 Gold am Tag.
* **Wirtschaftlichkeit** (Bericht je Produktionsgebäude): Wert der Erzeugnisse gegen Rohstoffe,
  Werkzeugverschleiß, Löhne und Unterhalt je Tag zu heutigen Marktpreisen, Herstellkosten je
  Einheit gegen den Marktpreis. Gebäude mit Rezepten rechnen mit der Arbeitsleistung der
  zugewiesenen Belegschaft rund um die Uhr („möglich“); daneben steht, was sie **gemessen**
  hergestellt haben: jedes Produktionsgebäude zählt seine fertigen Arbeitsgänge und Waren je
  Tag (heute bisher und die letzten 24 abgeschlossenen Tage, gespeichert), z. B. „Gestern 5
  Arbeitsgänge (10 Einheiten Bretter), heute bisher 2 – möglich sind 6,0 je Tag.“ Feldfarmen
  und Obstgärten ernten nach den Jahreszeiten; ihr Ertrag ist das Mittel der geernteten Waren
  je Tag über die gespeicherten Tage („Ernte im Mittel der letzten 12 Tage: 23,5 Einheiten
  Gemüse je Tag.“), jedes abgeerntete Feld zählt als Arbeitsgang; Viehzucht, Schafzucht und
  Imkerei haben Rezepte und rechnen wie Werkstätten.
* **Märkte** verteilen Nahrung, Brennstoff und Waren an die Haushalte in ihrer Reichweite, wenn
  deren Läufer zum Besorgungsgang kommt (§6.1); der Haushaltsverbrauch erscheint in der
  Ressourcenstatistik und wirkt auf die Preise.
* **Feste und Hofhaltung** (Goldsenken des späten Spiels; `rules.ron` → `festival`,
  `ranks[].court_upkeep`): Ab etwa 100 Einwohnern ist Gold kein Engpass mehr (§1), der Schatz
  wuchs im Acht-Jahres-Lauf ohne Senke auf 25 000–49 000 Gold.
  * **Hofhaltung:** Ein Herrscher von Rang unterhält Beamte, Boten, Gesandte und den Hofstaat –
    ab der Stadt, wenn Gold kein Engpass mehr ist: je Tag Stadt 15, Großstadt 60, Königreich 120,
    Imperium 200 Gold (ein Zwanzigstel bis ein Zehntel der Einnahmen einer Stadt dieses Rangs),
    stündlich mit dem Unterhalt gebucht und in der Tagesvorschau „Unterhalt“ enthalten (die
    Forschung „Verwaltung“ senkt nur den Gebäudeunterhalt). Bericht `report::court_report`:
    „Hofhaltung als Großstadt: 60 Gold je Tag (im Unterhalt enthalten)“. Schon 10 Gold je Tag im
    Marktflecken hielten den ehrgeizigen Spieler der Kampagne (Seed 23) unter 100 Einwohnern –
    deshalb erst ab der Stadt.
  * **Fest ausrichten** (`Command::HoldFestival`): kostet 100 Gold + 4 Gold je Einwohner
    (Kassenbuch „Ereignisse“; bei 250 Einwohnern 1 100 Gold), braucht eine fertige Kirche oder
    einen Marktplatz (dort wird gefeiert), höchstens alle 12 Tage, nicht während eines
    Festtags und nicht im Bankrott. Wirkung wie das prächtige religiöse Fest (§12, Quelle „Euer
    Fest“): 1 Festtag (nachmittags wird gefeiert statt gearbeitet, §6) und Zufriedenheit +8 für
    6 Tage; jedes Reich ohne Krieg schickt Gesandte als Gäste – Erinnerung „Festgäste“ +4 (höchstens
    +12 aus Festen, verblasst wie jede Erinnerung, §10). Meldung „Ihr richtet ein Fest aus
    (1100 Gold): Das Volk feiert, die Zufriedenheit steigt für 6 Tage um 8. Gesandte aus 3 Reichen
    sind zu Gast.“, `SimEvent::FestivalHeld`. Gründe dagegen (`FestivalReport::problem`): „Für ein
    Fest braucht es eine Kirche oder einen Marktplatz.“, „Es wird bereits gefeiert.“, „Das nächste
    Fest ist in 10 Tagen möglich (Herbst · Tag 1 · Jahr 1).“ (Datum im Kalender der Kopfleiste), „Im Bankrott kann kein Fest ausgerichtet werden.“,
    „Es fehlen 160 Gold.“
  * Prunkbauten oder Denkmäler gibt es nicht: Ein neuer Gebäudetyp bräuchte eigene Grafik,
    Symbol und Daten-Fingerabdruck; Dom, Akademie und Burg erfüllen diese Rolle als Ausbaustufen
    (§5).
* **Handel:** Das Handelskontor (besetzt, mit dem Lagernetz verbunden) ermöglicht den Fernhandel
  mit Karawanen (§10). Import/Export-Aufträge mit Schwellwerten („verkaufe Holz über 200",
  „kaufe Gewürze bis 20") und Tagesmenge. **Gibt es KI-Königreiche, läuft jeder Handel über
  Karawanen:** Händler eines Reichs mit Zugang kommen zum Kontor, wenn sie Waren anbieten, die ein
  Kaufauftrag sucht, oder Waren suchen, die ein Verkaufsauftrag anbietet, und handeln dort nach
  den Aufträgen (die Tagesmenge eines Auftrags gilt für alle Karawanen zusammen). **Nur ohne
  Königreiche** (Szenario mit `rival_kingdoms: 0`) handelt das Kontor stündlich mit dem
  abstrakten Weltmarkt (Marge `trade_margin_percent` 20 %). Krieg sperrt den Handel mit dem
  Gegner, Karawanen auf gefährlichen Routen können überfallen werden.

## 8. Jahreszeiten, Wetter, Landwirtschaft

Werte in `data/rules.ron` (`season_growth`, `season_gathering`, `winter_*`, `weather`,
`weather_effects`, `climate`, `agriculture`) und `data/crops.ron`.

### 8.1 Jahreszeiten

Jahreszeit | Wachstum (Felder, Bäume) | Fischerei | Weide | Imkerei | Weiteres
---|---|---|---|---|---
Frühling | 1,0 | 100 % | 100 % | 100 % | Aussaat
Sommer | 1,2 | 110 % | 100 % | 120 % | Dürregefahr, zweite Gemüseaussaat
Herbst | 0,6 | 100 % | 90 % | 70 % | Ernte
Winter | 0 | 60 % | 70 % | 0 % (Winterruhe) | Frost tötet unreife einjährige Feldfrüchte, Heizbedarf (0,6 Heizwert je Bewohner und Tag), Nahrungsverbrauch +20 %, Arbeitstempo −15 % in allen Betrieben und auf Baustellen

**Temperatur** (`climate.temperature`, ganze °C, `WeatherState::temperature_c`): Mittel der
Jahreszeit (Frühling 10, Sommer 21, Herbst 9, Winter −3) + Wetter (`temperature_c` je Wetter bei
100 % Stärke: klar +2, bewölkt 0, Regen −2, Sturm −3, Schnee −4, Dürre +6; anteilig zur Stärke) +
eine Tagesabweichung von höchstens ±3 (Hash des Tages, kein Zufallszug) + das Klima des
Startgebiets (Generator-Temperatur des Start-Tiles, ±5). Unter **5 °C** heizen die Haushalte
(Brennstoff, Wärmebedürfnis, Erfrieren – im Winter immer, an kalten Herbst- und Frühlingstagen
auch; Vorrat wird am letzten Herbsttag und an jedem kalten Tag besorgt), an Tagen mit **0 °C und
weniger** erfriert unreife Saat (zusätzlich zum Frost am Winteranfang); das Obst an den Bäumen
verdirbt nur zu Winterbeginn, ein Frosttag im Frühling oder Herbst lässt es hängen. Mit den
ausgelieferten Klimadaten sind Frosttage außerhalb des Winters selten (kaltes Startgebiet und
Schnee oder Sturm). Der Wetterbericht nennt
die Temperatur („Temperatur: −3 °C“), ob geheizt wird und ob Frost herrscht.

### 8.2 Wetter

Das Wetter eines Tages wird um Mitternacht bestimmt (der erste Tag beim Spielstart): mit 35 %
bleibt das Wetter des Vortags, sofern es in der Jahreszeit vorkommt, sonst wird nach den
Gewichten der Jahreszeit gezogen; die Stärke ist gleichverteilt 20–100 %. Zufall aus einem
eigenen, im Spielstand gespeicherten Zufallsgenerator.

Jahreszeit | `Clear` Klar | `Cloudy` Bewölkt | `Rain` Regen | `Storm` Sturm | `Snow` Schnee | `Drought` Dürre
---|---|---|---|---|---|---
Frühling | 35 | 30 | 30 | 5 | – | –
Sommer | 50 | 20 | 15 | 7 | – | 8
Herbst | 25 | 35 | 30 | 10 | – | –
Winter | 20 | 35 | – | 5 | 40 | –

Wirkungen bei Stärke 100 %; schwächeres Wetter liegt anteilig näher an 100 % bzw. 0:

Wetter | Wachstum | Bodenfeuchte je Tag | Arbeit im Freien | Heizbedarf (Winter) | Krankheitsrisiko | Feldschaden
---|---|---|---|---|---|---
Klar | 100 % | −4 | 100 % | 100 % | 100 % | –
Bewölkt | 95 % | −2 | 100 % | 100 % | 100 % | –
Regen | 110 % | +35 | 85 % | 110 % | 115 % | –
Sturm | 80 % | +25 | 50 % | 130 % | 115 % | 20 %
Schnee | 50 % | +10 | 75 % | 125 % | 130 % | –
Dürre | 60 % | −35 | 90 % | 100 % | 115 % | –

* **Arbeit im Freien** gilt für Gebäude mit `outdoor` (Holzfäller, Förster, Steinbruch, Lehm-
  und Sandgrube, alle Landwirtschaftsgebäude) und für alle Baustellen; Werkstätten arbeiten
  wetterunabhängig (im Winter aber mit −15 %).
* Bei **Dürre** fällt das Wasserbedürfnis ohne Brunnen oder Frischwasser auf 10 (statt 25).
* **Sturm** ab 50 % Stärke kann das Ereignis „Sturmschäden“ auslösen, das Holzgebäude beschädigt
  (§12); Dürre und Überschwemmung gibt es zusätzlich als Ereignisse, die das Wetter erzwingen
  (die Dürre nur nach einer Trockenperiode: das Wetter zählt die Tage in Folge ohne Regen, Sturm
  oder Schnee mit, `WeatherState::dry_days`).
* **Wind** (stündlich, deterministisch aus Seed, Tag und Stunde – ohne die Wetterziehung zu
  ändern): eine Hauptrichtung je Tag (16 Himmelsrichtungen), die je Stunde um höchstens eine
  Richtung dreht (im Sturm nicht); Stärke 0–100 nach Wetter (Klar 20, Bewölkt 30, Regen 45,
  Sturm 55 + 45 × Stärke, Schnee 35, Dürre 25) ± 10 Böen. Er neigt Bäume, Regen und Rauch und
  trägt Brände (§12): in Windrichtung (innerhalb von etwa 45°) springt ein Brand bis zu
  Stärke/34 Felder weiter (höchstens +2) und zuerst dorthin, gegen den Wind ab Stärke 50 ein Feld
  weniger weit.
* **Darstellung** (ohne Spielwirkung): Schneedecke 0–100 – stündlich bei Schnee +6 × Stärke,
  sonst Schmelze (Klar −2, Bewölkt −1, Regen −6, Dürre −4, Sturm ±0; außerhalb des Winters
  zusätzlich −8); Bodennässe 0–100 – bewegt sich stündlich um höchstens +10 / −3 auf den
  Zielwert des Wetters zu (Klar 0, Bewölkt 10, Regen 100, Sturm 90, Schnee 40, Dürre 0, × Stärke).
* **Meldungen:** Sturm ab 50 % Stärke (Warnung), Beginn einer Dürre (Warnung), Regen nach einer
  Dürre, erster Schnee des Winters, strenger Frost (Schnee ab 80 % im Winter, einmal je Winter,
  Warnung), Frostschäden an Feldern, eingebrachte Ernte je Farm; Jahreszeitwechsel wie bisher.

### 8.3 Felder und Weiden

**Beanspruchung:** Feldfarmen und Weidehöfe beanspruchen bei Fertigstellung und danach stündlich
die nächstgelegenen geeigneten Tiles in ihrem Radius bis zur Höchstzahl (§5.3), **exklusiv**:
im eigenen Gebiet, ohne Gebäude, Straße, Vorkommen, Baum oder Felsbrocken und nicht vor einem
Gebäudeeingang; Felder auf Grasland, Wiese, Erde oder Steppe mit natürlicher Fruchtbarkeit ≥ 60
(Rohwert 0–255, so zeigt ihn auch die Feldinfo: „128 · Ackerboden“; Büsche und Baumstümpfe
werden gerodet), Weiden auf Grasland, Wiese oder Steppe (Büsche bleiben).
Ein Gebäude oder eine Straße auf einem Feld, Gebietsverlust, Abriss oder Versetzen der Farm geben
Tiles frei; was darauf wuchs, ist verloren. Feldfarmen beanspruchen nur Tiles, die ihre Bauern zu
Fuß erreichen; ein Feld, das 24 Stunden lang unerreichbar ist (eingemauert, abgeschnitten), wird
freigegeben (die Farm beansprucht dafür erreichbares Land). Förster pflanzen nicht auf beanspruchten Tiles.
Bauplatz-Prüfung: mindestens die Hälfte der Höchstzahl muss frei (von keiner anderen Farm
beansprucht) verfügbar sein.

**Farmarbeit** (je Tick, Produktivität wie §6 × Jahreszeit/Wetter × Forschung × Bankrott): reife
Felder ernten (sofern die Ernte in die Scheune passt), sonst säen bzw. Bäume pflanzen. Gesät wird
nur in einer Aussaat-Jahreszeit der gewählten Frucht, auf Felder mit mindestens 60 % ihrer
natürlichen Fruchtbarkeit (ärmere bleiben Brache) und – bei frostempfindlichen Früchten – nur,
wenn die Frucht bei normalem Wetter mit 10 % Reserve vor dem Winter reift. Eine Ernte (alle
reifen und reifenden Felder) wird gemeldet, wenn sie eingebracht ist.

**Wachstum** je Stunde: 1 Wachstumstag/24 × Jahreszeit × Wetter × Feuchte × Boden.
* Bodenqualität = min(Fruchtbarkeit, 200) / 200; Boden-Faktor 90 % + 10 % × Bodenqualität.
* Feuchte-Faktor 40 % bei trockenem Boden, linear bis 100 % ab Bodenfeuchte 60.
* Einjährige reifen bei `growth_days`; Stufe 0–3 nach Viertel des Wachstums, 4 = reif.
* Obstbäume: Setzling (Stufe 0) und junger Baum (1) bis `growth_days`, danach belaubt (2), Blüte
  im Frühling (3), reife Früchte im Herbst (4). Fruchtwachstum sammelt sich außerhalb der
  Erntezeit (Frühling, Sommer); nicht geerntetes Obst verdirbt zu Winterbeginn. Im Winter sind
  Bäume ab Stufe 1 kahl (Darstellung `fields/orchard_bare`).

**Ertrag** je Feld = `yield_per_tile` × (40 % + 60 % × Bodenqualität) × Zustand × (100 % +
Forschung `FieldYieldBonus` + Bewässerung `IrrigationBonus`, wenn Fluss oder See höchstens
6 Tiles entfernt) × (Obst) min(1, Fruchtwachstum / `fruit_days`).

**Zustand** (100 %, nach jeder Ernte wieder 100 %): ein Sturm nimmt stehenden Früchten
20 % × Stärke ihres Zustands; Felder mit Bodenfeuchte < 40 verdorren um 6 Punkte je Tag.

**Boden:** Fruchtbarkeit und Feuchte jedes Feldes starten mit den Werten des Tiles. Täglich
ändert das Wetter die Feuchte, danach kehrt sie um 20 % der Differenz zur natürlichen Feuchte
zurück. Jede Ernte einer einjährigen Frucht kostet ihre Erschöpfung (Weizen 40, Gerste 30,
Gemüse 25); unbestellte Felder gewinnen täglich 3 zurück (Fruchtwechsel +50 %), höchstens bis
zur natürlichen Fruchtbarkeit.

**Weiden und Herden:** Jeder Weidehof hält eine Herde: ein Tier je 2 Weide-Tiles
(`pasture_tiles_per_animal`); ein neuer Hof startet mit der Hälfte (`herd_start_percent` 50), die
Herde wächst außerhalb des Winters täglich um ein Viertel der Weidekapazität (mindestens ein Tier)
und wird sofort verkleinert, wenn Weide verloren geht. Produktivität der Viehhöfe = beweidete
Fläche (Tiere × 2, höchstens die beanspruchte Weide) / benötigte Tiles × Jahreszeit (§8.1) – ein
voller Hof mit 16 Tiles hat 8 Tiere und 100 %. **Imkerei:** Wiesen zählen, solange sie nicht bebaut, gepflastert oder als Acker
bestellt sind (Weiden auf Wiesen zählen), dazu Obstgarten-Felder mit Bäumen.

### 8.4 Darstellung und Berichte

Jedes beanspruchte Tile hat einen Zustand für die Darstellung: `fields/soil` (Brache),
`fields/<wheat|barley|vegetables>_<0..4>`, `fields/orchard_<0..4>`, `fields/pasture`; Änderungen
markieren den Chunk für die Feld-Ebene. Dazu das Wetter des Tages mit Stärke, Schneedecke und
Bodennässe. Berichte: Farmfenster (Felder mit Frucht, Stufe, Wachstum, Boden, Feuchte, Zustand,
erwartetem Ertrag, laufender Arbeit, Scheune, letzter Ernte), Landwirtschaftsübersicht,
Wetter- und Jahreszeitanzeige mit deutschen Wirkungstexten und eine Nahrungsprognose bis zum
Ende des Winters (Vorrat, Scheunen, erwartete Ernte, laufende Erzeugung ohne Felder, Verbrauch).
Visuell: Laubfarben je Jahreszeit, Schnee im Winter, Tag-Nacht-Zyklus.

## 9. Forschung (`data/technologies/technologies.ron`)

Bereiche: `agriculture, architecture, crafts, mining, trade, medicine, military, administration`.
Forschung kostet Gold + Zeit (Forschungspunkte durch Schulen/Gelehrte) und hat Voraussetzungen.
Beispiele: Verbesserte Pflüge (+15 % Feldertrag), Fruchtwechsel (Fruchtbarkeit regeneriert),
Bewässerung (Felder bis 6 Tiles vom Wasser +20 %), Steinbau (schaltet `paved`, Stadthaus frei),
Größere Gebäude, Verbesserte Dächer (Brandschutz), Stadtmauern, Erweiterte Lagerhaltung
(+50 % Lager), Handelsverträge, Effizientere Märkte (+Radius), Medizin (Krankenhaus), …
Jede Technologie hat konkrete Effekte (Freischaltungen oder Modifikatoren); Feldertrag,
Bewässerung und Fruchtwechsel wirken wie in §8.3 beschrieben.

**Forschungspunkte** (`rules.ron` → `research`, Gebäude: `research_points_per_hour`):
* Ein fertiges Hauptquartier (Rathaus, Burg) forscht 1 Punkt je Stunde, unabhängig von seiner
  Besetzung – die Forschung steht also nie ganz still.
* Eine **Schule** forscht 2 Punkte je Stunde bei voller Besetzung. Ihre zwei Arbeiter sind
  **Gelehrte** (Berufsstufe „gebildet“, §6). Wie in der Produktion zählen nur die Arbeiter am
  Arbeitsplatz, gewichtet nach Gesundheit und Zufriedenheit, mit dem Schichtfaktor 3 (§5.2): eine
  volle Schicht ohne Arbeitsweg ergibt 48 Punkte am Tag. Im Bankrott forschen Schulen halb so schnell.
* Erforschtes **Gelehrtentum** (`ResearchSpeedBonus` +25 %) erhöht die gesamte Forschung.
* Beispiel: Rathaus allein 24 Punkte am Tag (Getreideanbau mit 24 Punkten dauert einen Tag), mit
  einer voll besetzten Schule 72.

**Forschungsliste:** höchstens 8 Technologien (`max_queue`). Eingereiht wird nur, was noch nicht
erforscht ist und dessen Voraussetzungen erforscht oder weiter vorn eingereiht sind. Die erste
Technologie wird erforscht: Ihr Gold wird fällig, wenn sie an die Spitze kommt und beginnt (fehlt
es, wartet sie, mit einer einzigen Warnung „Für die Forschung „…“ fehlen noch … Gold.“). Jeden
Tick kommt die aktuelle Forschungsleistung hinzu (ganzzahlig: Milli-Punkte je Stunde, je Tick
aufaddiert). Punkte ohne laufende Forschung verfallen. **Abbrechen** behält Fortschritt und
bezahltes Gold – erneut eingereiht geht es ohne neue Kosten weiter – und entfernt eingereihte
Technologien, die darauf aufbauen. **Umsortieren** nur so, dass jede Technologie hinter ihren
Voraussetzungen bleibt. `QueueResearchWithPrerequisites` reiht eine Technologie samt ihren noch
fehlenden Voraussetzungen (Voraussetzungen zuerst) in einem Schritt ein – ganz oder gar nicht
(passt der Plan nicht in die Liste, wird er abgelehnt). Kommandos: `QueueResearch`,
`QueueResearchWithPrerequisites`, `CancelResearch`, `MoveResearch`.

**Abschluss:** Die Technologie wirkt sofort: Gebäude und Straßen sind freigeschaltet,
Modifikatoren gelten (Lagerkapazitäten bestehender Lager und Gebiet werden neu berechnet,
Arbeits- und Wohnungszuteilung laufen erneut). Meldung „Forschung abgeschlossen: … Neu verfügbar:
…“ (bzw. die erste Wirkung) mit der nächsten Technologie der Liste.

**Wirkungen** (summieren sich je Art):

Effekt | Wirkt auf
---|---
`UnlockBuilding`, `UnlockRoad` | Baumenü (Gebäude, Straßentypen)
`ProductionBonus` | Arbeitstempo der Zielgebäude (alle, Kategorie oder Typ)
`FieldYieldBonus`, `IrrigationBonus`, `SoilRegeneration` | Ernte und Boden (§8.3)
`StorageBonus` | Kapazität aller Lager (bestehende sofort)
`ServiceRadiusBonus` | Reichweite aller Dienste des Gebäudetyps (Versorgung, Ausbau-Voraussetzungen, Brandschutz, Sicherheit, Tagesablauf)
`HousingCapacityBonus` | Plätze in Wohnhäusern
`TaxBonus`, `UpkeepReduction`, `TradePriceBonus` | Steuern, Unterhalt, Handelspreise (§7)
`BuildSpeedBonus` | Baugeschwindigkeit
`ResearchSpeedBonus` | Forschungsleistung
`FireRiskReduction` | Gewicht des Ereignisses Brand (§12)
`DiseaseResistance` | Krankheitstod (§6.1), Gewicht und Anteil von Seuchen (§12)
`ToolWearReduction` | ein Werkzeug hält 100/(100 − x) so viele Arbeitsgänge (50 % → 40 statt 20)
`UnlockUnit` | Einheiten, die rekrutiert werden können (§11)
`UnitBonus` | Angriff und Verteidigung (Fern- wie Nahkampf) der eigenen Einheiten eines Typs oder aller (§11)
`WallStrengthBonus` | Trefferpunkte von Mauern und Toren (§11)

**Bericht** (Forschungsfenster): jede Technologie mit Bereich, Zustand (Erforscht, In Arbeit,
Vorgemerkt, Verfügbar, Gesperrt mit den fehlenden Voraussetzungen), Kosten, Punkten,
Fortschritt, Restzeit nach der erwarteten Tagesleistung und deutschen Wirkungstexten; die Liste
mit voraussichtlichem Abschluss; Forschungsleistung je Stunde und Tag mit Quellen; Punkte der
letzten 48 Tage.

## 10. Königreiche & Diplomatie

Werte in `rules.ron` → `diplomacy`, Reiche, Wirtschaftsprofil und Ortsplan in `data/kingdoms.ron`.

**Königreiche:** Anzahl nach Kartengröße – Klein 3, Mittel 4, Groß 5, Riesig 5 –, höchstens
`rival_kingdoms` des Szenarios (Standard 5; 0 = keine Königreiche). Der Weltgenerator wählt ihre
Sitze unter den besten Siedlungsstellen weit vom Spieler (mindestens Kartenbreite/3,2, untereinander
80 % davon; auf engen Karten schrittweise bis 40 % gelockert). Jedes Reich hat einen Namen und einen
Herrscher (Titel, Name; zufällig aus zehn Reichen gezogen), eine **Persönlichkeit** (gewichtet
gezogen, keine mehr als zweimal), eine Farbe (0–6), 80–160 Einwohner, Gold (8 je Einwohner),
Soldaten, Vorräte und ein **Gebiet**: alle Tiles im Radius `9 + Einwohner/30` (höchstens 22) um den
Sitz, die weder dem Spieler noch einem anderen Reich gehören. Der Spieler kann dort nicht bauen
(„Dieses Land gehört einem anderen Königreich.“), sein eigenes Gebiet (§3) endet an fremden
Grenzen. Gebiete wachsen mit den Einwohnern um höchstens ein Tile Radius pro Tag; in Kriegen
verlorenes Land (`war.border_ceded` 3) wird abgezogen.

Persönlichkeit | Gewicht | Grundhaltung | Soldaten/100 Einw. | Angriffslust | Großzügigkeit | Handelsinteresse | Expansion
---|---|---|---|---|---|---|---
händlerisch | 3 | +10 | 5 | 10 | 35 | 80 | 20
expansiv | 2 | −5 | 9 | 40 | 10 | 30 | 70
friedlich | 3 | +15 | 4 | 0 | 40 | 45 | 10
aggressiv | 2 | −15 | 12 | 75 | 0 | 15 | 45

**Siedlungen:** Jedes Reich baut nach dem Ortsplan sichtbare Bauwerke (Sprites und Grundflächen
der Gebäudetypen): die Burg am Sitz, Holz- und Bauernhäuser (3 + 1 je 12 Einwohner), Markt,
Kirche, Bäckerei, Schmiede, Gasthaus und Lagerhaus in der inneren Hälfte, Weizen-, Gemüse-,
Rinder- und Schaffarmen, Holzfäller, Steinbruch und Eisenmine in der äußeren Hälfte (nur mit
genug passendem Land, Bergwerke nahe ihren Vorkommen), Wachtürme an der Grenze. Ein Bauwerk
braucht ebenes, bebaubares Land des Reichs ohne Vorkommen und Felsbrocken, hält ein Tile Abstand
zu anderen Bauwerken (Läufer kommen dazwischen durch), blickt zum Sitz; Bäume darauf werden
gerodet. Bei Spielbeginn steht der Ort schon (bis zu 48 Bauwerke), danach kommt höchstens eines
pro Tag hinzu. Bauwerke blockieren Läufer.

**Wirtschaft (abstrakt, täglich um Mitternacht):** Produktion je Tag aus dem Land des Gebiets –
Ackerland (Wiese, fruchtbares Gras ab Fruchtbarkeit 168), Weide (übriges Gras, Steppe, Erde),
Wald (Bäume, Waldboden), Wasser (flach, Süßwasser), Vorkommen (Stein, Lehm, Eisen, Kohle, Gold) –
je Tile, Handwerk je 100 Einwohner (nur mit mindestens 6 Tiles des nötigen Landes, z. B.
Werkzeug und Waffen nur mit Eisen) und bei händlerischen Reichen Fernhandelswaren (Gewürze, Seide,
Wein, Schmuck). Acker- und Weideertrag im Winter 50 %. Verbrauch je 100 Einwohner und Tag (z. B.
9 Brot, 5 Gemüse, 10 Holz, 1 Werkzeug); fehlende Nahrung wird durch andere Nahrung ersetzt. Ein
Reich **sucht** Waren, von denen es weniger als 4 Tagesverbräuche hat (bis 8), und **bietet**, was
8 Tagesverbräuche übersteigt; Vorräte über 14 Tagen Produktion bzw. Verbrauch gehen an ferne
Märkte. Preise wie §7 (Angebot = Vorrat + Tagesproduktion, Nachfrage = 4 Tagesverbräuche).
Mindestens 90 % Nahrung → +1 % Einwohner pro Tag (höchstens 420), unter 50 % → −1,5 %. Steuern
0,8 Gold je Einwohner, Sold 1 Gold je Soldat und Tag; Soldaten folgen der Persönlichkeit (im
Krieg +50 %). **Stärke** = 4 je Soldat + 1 je 5 Einwohner. Beim Spieler zählen die
ausgebildeten Soldaten nach ihrer Einheit (Angriff + Verteidigung + Fernkampf) × 4 / 12 – ein
durchschnittlicher Soldat (12 Punkte) zählt wie ein Soldat eines Reichs, eine Miliz (7) etwas
weniger, ein Schwertkämpfer (14) etwas mehr – plus 1 je 5 Einwohner.

**Beziehungen** −100…+100 → Zustände `Hostile < −50 ≤ Rival < −10 ≤ Neutral < 30 ≤ Friendly`;
**Verbündet** nur mit Bündnisvertrag. Die Beziehung zum Spieler ist die Summe benannter Teile
(Aufschlüsselung im Bericht): Grundhaltung der Persönlichkeit, Verträge (Handelsvertrag +10,
Nichtangriffspakt +6, Bündnis +20), Krieg −50, gemeinsamer Feind +15 je Reich, gemeinsame Grenze
(expansiv/aggressiv −10, sonst −3), **Ansehen** des Herrscherrangs (§1, Dorf +2 … Imperium +12) und
**Erinnerungen** je Ursache, die täglich um 3 % (mindestens
0,2 Punkte) verblassen (höchstens 12): Geschenke (+4 je 100 Gold Wert, zusammen höchstens +30),
Handel mit Karawanen (+1 je Karawane, höchstens +15), gezahlter Tribut +8, verweigerte Forderung
−15, abgelehnter Vorschlag −3, eigene Forderung −12, Kriegserklärung −35, Angriff auf ein
befreundetes Reich −12, Friedensschluss +10, Grenzzwischenfall −5, Vertragsbruch (Handelsvertrag
−12, Pakt −30, Bündnis −45), Wortbruch gegenüber anderen −6, verlorene Schlachten −5 je
geschlagenem Heer bzw. verlorener Feldschlacht (höchstens −30, §11). Zustandswechsel werden gemeldet. Die
Beziehungen der Reiche untereinander nähern sich täglich um einen Punkt dem Mittel ihrer
Grundhaltungen (−20 mit gemeinsamer Grenze, ±2 Zufall).

**Aktionen des Spielers** (`Command::Diplomacy`): Handelsvertrag (24 Tage), Nichtangriffspakt
(24 Tage), Bündnis (36 Tage, nur mit Pakt) vorschlagen oder kündigen; Geschenk (Gold und Waren);
Tribut fordern; Krieg erklären (nur ohne Pakt und Bündnis; beendet den Handelsvertrag);
Frieden anbieten (ohne Bedingungen, gegen Gold des Spielers, gegen Entschädigung des Reichs oder
gegen Gebietsabtretung). Vorschläge entscheidet das Reich sofort und deterministisch: Punkte aus
Beziehung, Persönlichkeit, Interessen (Handelskontor vorhanden, gesuchte Waren auf Lager,
gemeinsamer Feind), relativer Stärke und Bedingungen gegen eine Schwelle (Handelsvertrag 20, Pakt
30, Bündnis 60, Tribut 40, Frieden 0); die Antwort nennt die stärksten Gründe („Beziehung
„Rivalisierend“ (−40)“). Der Bericht zeigt die Aussicht vorab.

**Initiativen der KI** täglich um 10 Uhr (ab Tag 2; je Reich höchstens alle 2 Tage, 30 %
Chance, nach Gewicht; Vorschläge, Forderungen und Angebote im Posteingang sind **gedrosselt**,
`ai.max_open_proposals` usw.: je Reich höchstens einer offen, höchstens 2 offen insgesamt, keiner
vor Tag 3, zwischen zwei neuen mindestens 24 Stunden – in den ersten 12 Tagen 48 Stunden; die
übrigen Initiativen wie Geschenke und Grenzzwischenfälle bleiben möglich, ein Friedensangebot
im Krieg kommt immer): Handelsvertrag, Pakt oder Bündnis vorschlagen, Geschenk (Gold oder
Überschusswaren – freundlich, oder um einen stärkeren Spieler zu beschwichtigen), Tribut fordern
(Angriffslust ≥ 30, Stärke ≥ 130 % des Spielers, ab Tag 12, höchstens alle 6 Tage: 10 % des
Schatzes, 50–400 Gold), Kauf- und Verkaufsangebote (gesuchte Waren 25 % über, Überschüsse 15 %
unter ihrem Preis, höchstens 30 Einheiten), Grenzzwischenfälle (gemeinsame Grenze, Expansion
≥ 40), **Kriegserklärung** (Angriffslust ≥ 30, Feindlich, Stärke ≥ 130 %, kein Pakt) – **nicht vor
Tag 36** (Schonfrist: das erste Jahr und eine halbe Jahreszeit). Vorschläge, Forderungen und
Angebote warten 18 Stunden im diplomatischen Posteingang (`Command::AnswerProposal`; Meldung
„… Antwort binnen 18 h (bis morgen 04:00 Uhr).“), danach gelten
sie als abgelehnt. Ein angenommenes Kaufangebot bezahlt das Reich aus seinem Schatz, wie er beim
Annehmen ist: hat es inzwischen Gold ausgegeben, kauft es anteilig nur so viel, wie es noch
bezahlen kann; reicht es für keine Einheit, kann das Angebot nur noch abgelehnt werden. Wer einen Tribut verweigert, riskiert nach der Schonfrist den Krieg mit einem
überlegenen, sehr angriffslustigen Reich (≥ 50). Reiche untereinander führen ab Tag 10 Kriege
(Feindlich, der angriffslustigere Angreifer mit Angriffslust ≥ 30 und ≥ 130 % Stärke, täglich
Angriffslust/5 % Chance), verlieren täglich Soldaten und Einwohner und schließen
nach mindestens 5 Tagen mit 15 % Chance pro Tag Frieden: der Schwächere tritt Land ab und zahlt
20 % seines Schatzes.

**Krieg mit dem Spieler:** kein Handel, Karawanen kehren um; alle 3 Tage
(`war.raid_interval_days`) um 14 Uhr schickt das Reich ein **Heer** (§11: die Hälfte seiner
Soldaten, höchstens 40; mit weniger als 6 Soldaten versucht es es am nächsten Tag wieder), das
nach der Reisezeit einer Karawane am Kartenrand in Richtung des Reichs erscheint – sehr
angriffslustige Reiche (ab 50) wollen das Hauptquartier erobern, die übrigen plündern das
reichste Lager (15 % der Nahrung, Waren und Luxusgüter, `war.raid_percent`). Überlebende und
Beute kehren heim; ein geschlagenes Heer macht das Reich müde (+30) und zornig (−5). Bei
**Frieden** kehrt ein Heer im Anmarsch ohne Kampfbericht um; ein Heer auf der Karte zieht ab (§11). **Kriegsmüdigkeit** der Bürger:
−3 Zufriedenheit, −1 je weitere 4 Kriegstage, −2 je weiteren Krieg, höchstens −10 (Faktor
„Krieg“). Das Reich wird täglich um 6 kriegsmüde (+15 nach einem Heereszug ohne Beute, +30 nach einem
geschlagenen Heer, +35 nach einer verlorenen Feldschlacht) und bietet
ab 60 (oder wenn der Spieler deutlich stärker ist) Frieden an – bei großer Überlegenheit gegen
Gold, bei Unterlegenheit mit Entschädigung, sonst ohne Bedingungen; danach höchstens alle 4 Tage.
Greift ein Reich den Spieler an, erklären ihm die Verbündeten des Spielers den Krieg.

**Handelskarawanen:** Zugang hat der Spieler zu Reichen ohne Krieg, die mindestens Neutral sind
oder einen Handelsvertrag haben. Ein Reich mit Zugang schickt Händler, wenn das Kontor besetzt ist
und die Aufträge passen (§7); Abstände: Neutral 36 h, Freundschaftlich/Verbündet 24 h,
Handelsvertrag 12 h (händlerische Reiche 25 % öfter), ohne passende Waren neuer Versuch nach 6 h.
Ladung (höchstens 80 Einheiten) verlässt die Vorräte, die Börse (höchstens 30 % des Schatzes) die
Kasse des Reichs. Nach einer Reise außerhalb der Karte (8 h je 100 Tiles zwischen Kontor und Sitz,
mindestens 3 h) laufen ein **Händler** und ein **Karren** (14 Ticks später) vom Kartenrand in
Richtung ihres Reichs zum Kontor, handeln dort, bleiben 2 Stunden und laufen zurück; was übrig
ist, geht heim. Preise: Preis des Reichs ∓ 15 % Marge, verbessert (höchstens bis auf den Preis
des Reichs selbst: Einkaufen kostet nie weniger, als der Verkauf derselben Ware an dasselbe Reich
bringt, `economy::trade::trade_percent`; ebenso auf dem Weltmarkt) um erforschte Handelsboni,
1 % je 10 Beziehungspunkte (bei schlechter Beziehung teurer) und den besten Vertragsbonus
(Handelsvertrag 10 %, Bündnis 15 %). **Eigene Karawanen** (`Command::SendCaravan`, höchstens 3
unterwegs): Waren aus den Lagern am Kontor und Gold laufen zum Kartenrand, reisen zum Partner,
verkaufen dort, was er sucht, kaufen die gewünschten Waren und kommen zurück; das Kassenbuch
bucht Verkäufe als Exporte und Käufe als Importe. **Gefahr:** Jede Reise außerhalb der Karte kann
von einem Reich nahe der Route (Sitz höchstens Gebietsradius + 12 Tiles von der Linie Kontor –
Partner) überfallen werden: im Krieg mit dem Spieler 25 %, feindlich 10 %, rivalisierend 3 %
(zusammen höchstens 50 %). Ladung und Börse gehen an die Räuber – Waren und Gold gehen dabei nie
verloren, sie wechseln nur den Besitzer.

**Ereignisse:** `war_refugees` (Kriegsflüchtlinge, Bedingung `WarNearby`: ein Krieg des Spielers
oder zwischen Reichen, §12).

**Entwicklerbefehle:** `DebugGrant { gold, goods }`, `DebugSpawnHouseholds { count }` (1–50
Haushalte laufen vom Kartenrand ein), `DebugSetRelation { kingdom, value }` (über eine
verblassende Erinnerung „Entwicklerwert“).

## 11. Militär

Werte in `data/units.ron` (Einheiten), `buildings.ron` (`garrison`, `defense`) und `rules.ron` →
`military`. Das Militär hat einen eigenen, gespeicherten Zufallsgenerator (Streuung der Treffer,
Brandstiftung, Ankunftsort der Räuber).

**Einheiten** (Freischaltung durch Forschung `UnlockUnit`, §9):

ID | Name (Mehrzahl) | Klasse | Angriff | Verteidigung | Fernkampf (Reichweite) | TP | Tempo | Moral | Kosten | Bewohner | Sold/Tag | Ausbildung | Ausgebildet in | Forschung
---|---|---|---|---|---|---|---|---|---|---|---|---|---|---
`militia` | Miliz (Milizen) | Fußvolk | 4 | 3 | – | 100 | 1,0 | 50 | 10 G, 3 wood | 1 | 1 | 4 h | Kaserne | Bürgerwehr
`spearman` | Speerträger (Speerträger) | Fußvolk | 6 | 8 | – (+50 % gegen Reiter) | 100 | 0,95 | 65 | 30 G, 1 weapons, 2 wood | 1 | 2 | 10 h | Kaserne | Bürgerwehr
`swordsman` | Schwertkämpfer (Schwertkämpfer) | Fußvolk | 8 | 6 | – | 100 | 1,0 | 70 | 40 G, 1 weapons, 1 armor | 1 | 3 | 12 h | Kaserne | Waffenschmiedekunst
`archer` | Bogenschütze (Bogenschützen) | Schützen | 2 | 3 | 7 (6) | 100 | 1,05 | 55 | 30 G, 3 wood, 1 leather | 1 | 2 | 10 h | Bogenschützenposten | Bogenschießen
`crossbowman` | Armbrustschütze (Armbrustschützen) | Schützen | 3 | 5 | 10 (7) | 100 | 0,9 | 60 | 50 G, 1 weapons, 2 wood | 1 | 3 | 14 h | Bogenschützenposten | Armbrüste
`cavalry` | Kavallerie (Reiter) | Reiter | 10 | 7 | – (+50 % gegen Schützen) | 120 | 1,8 | 80 | 100 G, 1 weapons, 1 armor, 1 leather | 1 | 6 | 20 h | Stallungen | Reiterei
`siege` | Belagerungsgerät (Belagerungsgeräte) | Belagerung | 2 | 2 | 12 (8), 20 gegen Bauwerke | 200 | 0,5 | 50 | 200 G, 20 planks, 5 iron_bars | 4 | 8 | 30 h | Kaserne | Belagerungstechnik

Die Waren unter den Kosten (Kategorie Ware) sind die **Ausrüstung**: eine Neuausrüstung braucht
je eine Einheit davon (die Miliz braucht keine). Kavallerie braucht Pferde: je fertige Viehzucht
vier Reiter.

**Rekrutierung** (`Command::Recruit { building, unit, count }`): an einem fertigen, nicht
stillgelegten Militärgebäude, das die Einheit ausbildet; je Gebäude höchstens
`training_slots` (6) Rekruten gleichzeitig in Ausbildung. Gold (Kassenbuch „Militär“) und
Ausrüstung aus den Lagern werden sofort fällig. Je Einheit werden `population` Erwachsene zu
Soldaten: zuerst Arbeitslose, dann Arbeiter, jeweils die dem Gebäude am nächsten Wohnenden
(niemand, der gerade Waren trägt); sie verlassen ihre Arbeit – die Arbeitskraft sinkt –, bleiben
Mitglieder ihres Haushalts und wohnen weiter zu Hause. Beim Belagerungsgerät ist der erste der
Bediener, die drei anderen sind seine **Mannschaft**, die ihm folgt. Ausgebildet wird nur, solange
der Rekrut im Gebäude ist (also tagsüber), für die Ausbildungszeit der Einheit; ohne Ausbilder
(die Arbeiter des Gebäudes) mit `training_without_staff_percent` (50 %) des Tempos, voll besetzt
mit 100 %. Meldung „Ausbildung abgeschlossen: 3 Milizen bereit.“ (Mehrzahl `plural` aus `units.ron`) Die Begründungen, warum eine
Rekrutierung nicht geht, stehen im Bericht („Es stehen nur 1 von 2 Erwachsenen für den Dienst
bereit.“, „Es fehlen Pferde …“). `DismissSoldiers` entlässt Soldaten (mit ihrer Mannschaft) ins
zivile Leben; wer alt wird, scheidet aus; ohne Ausbildungsgebäude bricht die Ausbildung ab.

**Unterhalt** (`military`): Sold stündlich wie Löhne (Tagessumme der Einheiten; der Bediener
zahlt für seine Mannschaft). Im Bankrott gilt der Sold als ausgeblieben („Der Sold bleibt aus
…“). Im Krieg isst jeder einsatzbereite Soldat zusätzlich `war_ration_milli` (0,5) Nahrung je Tag
aus den Lagern (größte Vorräte zuerst); die Ration wird um Mitternacht für den vergangenen Tag
gegessen, es bleibt also nichts übrig, wenn ein Befehl endet oder Frieden kommt. Ein Feldzug
nimmt den Proviant für die ganze Dauer mit; unterwegs isst jeder Soldat (und jeder Mann der
Mannschaft eines Geräts) stündlich seinen Anteil davon, und was beim Heimkehren übrig ist –
nach einem Rückzug, einem Frieden oder weil Soldaten gefallen sind –, kommt ins Lager zurück
(„Vom Proviant kommen 12 Einheiten Nahrung zurück ins Lager.“; nur wenn niemand heimkehrt,
ist er verloren).
**Moral** (0–100) bewegt sich täglich um 10 Punkte auf ein Ziel zu: Moral der Einheit, −25 ohne
Sold, −15 wenn der Haushalt hungert oder Rationen fehlen, ±2 je 10 Zufriedenheit über/unter 50,
Kriegsmüdigkeit (§10), −10 unter halber Ausrüstung, dazu erinnerte Kampfergebnisse (Sieg +10,
Niederlage −15, sofort wirksam, täglich halbiert). **Ausrüstung** (‰) sinkt je gekämpfter
Runde um 6 ‰; ein Soldat am Posten wird um Mitternacht mit je einer Einheit seiner
Ausrüstungswaren um 500 ‰ neu ausgerüstet. **Gesundheit** heilt stündlich um 2 Punkte (mit
Krankenhaus im Umkreis des Hauses 4), solange er nicht kämpft. **Erfahrung** (0–100): +1 je
gekämpfter Runde, +5 je besiegtem Feind; bei 100 +20 % Angriff.

**Dienst:** Soldaten in Ausbildung gehen in den Arbeitsstunden (8–17 Uhr) ins Ausbildungsgebäude;
einsatzbereite Soldaten folgen in den Arbeitsstunden dem Befehl ihrer Kompanie, sonst – und nachts
– nur bei **Alarm** (Feinde auf der Karte oder binnen einer Stunde am Kartenrand, Ereignis-Alarm);
außer Dienst leben sie den normalen Tagesablauf. In ein Gebäude passen so viele, wie seine
Garnison erlaubt, die übrigen stehen auf den freien Feldern rundum (nächste zuerst). Verwundete
(unter 25 % Gesundheit) und Mutlose (Moral unter 15) ziehen sich zu ihrem Posten zurück.

**Kompanien** (`CreateCompany`, `AssignSoldiers`, `DisbandCompany`; höchstens 12, je 40
Soldaten; Namen „1. Kompanie“ …) mit **Befehlen** (`CompanyOrder`):
* *Bereitschaft* (Standard, auch für Soldaten ohne Kompanie): am Posten – dem Gebäude, in dem
  sie ausgebildet wurden, sonst am Hauptquartier;
* *Verteidigen* eines Gebäudes (Garnison) oder eines Gebiets (Mittelpunkt auf der Karte, Umkreis
  1–12, `DEFEND_RADIUS`; ein Spielstand mit anderem Gebiet wird beim Laden abgelehnt);
* *Patrouillieren*: 2–8 Wegpunkte im Kreis ablaufen;
* *Feldzug* oder *Belagerung* gegen ein Reich im Krieg (siehe unten);
* *Rückzug*: zurück zu den Posten (ohne die Feinde zu verfolgen); ein Feldzug kehrt um.

**Eingreifen:** Ein Soldat greift den nächsten Feind an, der im **Alarmradius** liegt –
`alert_radius` (12) um seinen Posten bzw. das verteidigte Gebäude, um das Gebiet zusätzlich dessen
Umkreis, `patrol_alert_radius` (8) um ihn selbst auf Patrouille; einen Feind, mit dem er schon
kämpft, verfolgt er, solange er ihm 8 Felder nah bleibt; beim Ereignis-Alarm jagt jeder Soldat
die Feinde auf der ganzen Karte. Nahkämpfer laufen hin, Schützen bleiben in Reichweite stehen,
Schützen in einem Garnisonsgebäude bleiben drinnen.

**Bedrohungen:**
* **Räuber** (Ereignis Räuberüberfall, §12; `military.bandits`): Angriff 3, Verteidigung 1,
  60 TP, Moral 30, Tempo 1,05.
  **Schonfrist:** kein Überfall vor Tag 24 (zweiter Frühling; Leicht Tag 36 wie die
  Kriegsschonfrist, Schwer Tag 18, erster Winter) und vor 40 Einwohnern (Bedingung
  `RaidGraceOver`). Abgeleitet aus Forschung und Rang: Bürgerwehr (100 Gold, 36 Punkte – das
  Rathaus allein forscht 1 Punkt je Stunde, also 1,5 Tage), Kaserne (150 Gold, 14 Bau-Stunden),
  ein Wachturm (40 Gold, 4 h) und vier Milizen (je 10 Gold, 4 h Ausbildung) kosten ein Dorf
  (Rang 1, nach etwa 7 Tagen) rund 350 Gold und eine Woche; mit den Steuern eines Dorfs ist das
  bis etwa Tag 20 zu schaffen – der erste Winter, in dem die Siedlung mit Hunger und Kälte
  ringt, bleibt frei.
  **Größe:** 3 + 1 je 12 Bewohnern (höchstens 12) × Schwierigkeit (Leicht 75 %, Normal 100 %,
  Schwer 125 %, gerundet); Banden meiden wehrhafte Städte: je 2 Verteidiger (einsatzbereite
  Soldaten auf der Karte, fertige Türme, Posten und Burg mit Fernkampfwert) ein Räuber weniger,
  höchstens die Hälfte der Bande (`military::bandit_band`, `military::raid_defenders`). Ein Dorf
  mit 48 Einwohnern zieht ohne Schutz 7 Räuber an, mit einem Wachturm und vier Milizen 5 – sie
  schlagen die Bande ohne eigene Verluste in die Flucht (Test
  `a_small_defended_town_repels_the_band_it_draws`).
  **Warnung:** Die Späher melden die Bande mit dem Ereignis; sie erreicht den Kartenrand
  8 Stunden nach dieser Meldung (`warning_hours`), mindestens 2 Stunden nach der Entscheidung
  (`delay_hours`) – die Wahl „Abwarten“ zeigt die Ankunftszeit („Etwa 5 Räuber greifen gegen
  17:00 Uhr unbewachte Lager an“), die Meldung „Späher melden eine Räuberbande von 5 Mann am Rand
  des Reiches. Sie greift in etwa 8 h an (gegen 17:00 Uhr).“ Die Wahl „Soldaten ausschicken“
  hält die Soldaten bis zur Ankunft in Bereitschaft und lässt sie die Räuber danach 8 Stunden
  jagen. Dann kommt sie an einem zufälligen Punkt des Kartenrands und geht
  zum nächsten Lager ohne Sicherheitsdienst mit Waren (sonst zu einem unbewachten Gebäude, sonst
  zum nächsten Lager) – unter diesen zum ersten, das sie zu Fuß erreicht, sonst zum ersten
  überhaupt (dann brechen sie durch die Mauer). Dort plündert sie 24 Kampfrunden lang; dann trägt jeder Räuber bis zu 15
  Waren (Nahrung, Waren, Luxus; größte Bestände zuerst) und 10 Gold aus dem Staatsschatz
  (Kassenbuch „Ereignisse“), mit 35 % Wahrscheinlichkeit brennt das Lager nieder (Ruine, §12), und
  die Bande flieht beladen (80 % Tempo) auf dem Weg, den sie kam. Sie flieht auch, sobald sie 35 %
  ihrer Leute verloren hat. Wer fällt, lässt seine Beute fallen: Waren gehen ins nächste Lager,
  Gold in den Staatsschatz.
* **Heere** der Reiche im Krieg (§10, `military.armies`): je Zug die Hälfte der Soldaten des
  Reichs (höchstens 40, nicht unter 6) in der Mischung 30 % Miliz, 25 % Speerträger, 25 %
  Schwertkämpfer, 20 % Bogenschützen (größte Reste); Ankunft nach der Reisezeit einer Karawane
  (8 h je 100 Felder, mindestens 3 h), angekündigt mit Größe, Ziel und Ankunftszeit. Ab
  Angriffslust 50 ziehen sie auf das Hauptquartier und belagern es; die übrigen plündern das
  reichste (erreichbare) Lager wie Räuber. Sie greifen eigene Soldaten in 6 Feldern Umkreis an und ziehen sich
  nach 50 % Verlusten zurück; Überlebende und Beute kehren heim.
* **Einstieg:** Feinde betreten die Karte am Randfeld in ihrer Richtung, von dem aus sie das
  Hauptquartier erreichen; ist die Stadt ummauert oder abgeschnitten, an einem Randfeld, von dem aus
  sie eine Mauer oder ein Tor erreichen, sonst an irgendeinem begehbaren Randfeld. Eine Mauer ohne
  Tor macht also nicht unangreifbar.
* **Weg zum Ziel:** Sie laufen zum Eingang des Ziels, ist er verbaut, zum nächsten freien Feld am
  Gebäude. Eine gescheiterte Wegsuche wird nach `walking.no_path_retry_ticks` wiederholt.
* **Mauern und Tore:** Feinde gehen durch kein Tor – stehen sie davor, brechen sie es auf, auch
  wenn sie einen eigenen Soldaten dahinter angreifen wollen (einen Soldaten, den sie gar nicht
  erreichen, greifen nur Schützen in Reichweite an); finden sie gar keinen Weg zum Ziel, schlagen
  sie eine Bresche in die Mauer bzw. das Tor nahe ihrem Ziel. Gibt es auch keine Mauer, nehmen sie
  das nächste erreichbare Ziel (ein Heer, das das Hauptquartier nicht erreicht, plündert das
  reichste erreichbare Lager) oder ziehen ohne Beute ab („… findet keinen Weg zu seinem Ziel und
  zieht ab.“). Auch Fliehende brechen Tore und Mauern auf ihrem Weg hinaus; ist ihr Randfeld
  unerreichbar, nehmen sie das nächste erreichbare. Finden Fliehende weder einen Rand noch eine
  Mauer zum Brechen (eine Insel, deren Brücke abgerissen wurde), warten sie wie ein eingemauertes
  Heer `armies.withdraw_stuck_hours` (4) Stunden und zerstreuen sich dann („Die Räuber finden
  keinen Weg aus Eurem Land und zerstreuen sich.“; Heere: „… löst sich auf“) – kein Daueralarm,
  Bericht und Beute werden verbucht. Gebrochene Mauerstücke und Tore verschwinden
  ohne Erstattung (`SimEvent::StructureDestroyed`).
* **Frieden:** Ein Heer im Anmarsch kehrt um (die Soldaten kehren heim, kein Kampfbericht). Ein
  Heer auf der Karte **zieht ab** (`WarbandStage::Withdrawing`): es kämpft nicht mehr, wird nicht
  mehr angegriffen, geht durch die Tore und zählt nicht als Bedrohung (kein Alarm). Einen
  Kampfbericht gibt es nur, wenn es vorher gekämpft, beschädigt oder geplündert hat (Ausgang
  Unentschieden, Niederlage bei behaltener Beute; keine Erinnerung „Verlorene Schlachten“).
  Findet ein abziehendes Heer keinen Weg von der Karte (eingemauert ohne Tor), wartet es
  `armies.withdraw_stuck_hours` (4) Stunden auf einen Weg und löst sich dann auf: „Das Heer von
  … findet keinen Weg aus Euren Mauern und löst sich auf: Seine Soldaten schlagen sich einzeln
  in die Heimat durch.“ (ohne Mauer im Weg: „… aus Eurem Land …“). Die Soldaten und ihre Beute
  kehren heim wie bei einem Abzug.
* **Eroberung:** Das gestürmte Hauptquartier (Schaden ≥ Trefferpunkte) ist verloren, wenn das Heer
  es 2 Stunden hält und kein eigener Soldat in 8 Feldern Umkreis steht (§1, Niederlage).

**Kampf** (alle `round_ticks` = 10 Ticks, solange Feinde auf der Karte sind, deterministisch):
Teilnehmer sind die Feinde auf der Karte, die einsatzbereiten Soldaten auf der Karte oder in einem
Garnisonsgebäude und die Gebäude mit Fernkampfwert (Türme, Posten, Burg; Schützen darin +2
Reichweite). Zuerst schießt jeder Schütze auf den nächsten Feind in Reichweite (Abstand ≥ 2,
Gebäude auch auf angrenzende), die Treffer wirken gleichzeitig, die Gefallenen fallen; dann
schlägt jeder auf den nächsten angrenzenden Feind (aus einem Gebäude heraus bzw. hinein). Ein
Treffer macht Angriff × 30 / (Verteidigung + 10) Schaden (mindestens 1) ± 20 % Streuung.
Angriff: Einheitswert × (100 % + erforschter `UnitBonus` + Erfahrungsbonus) × Ausrüstung
(50 % … 100 %) × Moral (70 % … 100 %), Klassenbonus gegen die Klasse des Ziels; Verteidigung:
× Ausrüstung, +20 % im Wald, +25 % auf höherem Gelände, +30 % neben einer eigenen Mauer, +50 % in
einem Gebäude. Feinde an einem Gebäude, das sie angreifen, ohne angrenzenden Soldaten,
beschädigen es mit ihrem Bauwerksschaden. Gefallene Soldaten sterben als Bewohner (Todesursache
„Kampf“, Statistik, Haushalte); ein zerstörtes Gebäude wird zur Ruine (Ursache „Angriff“, 50 %
der Waren darin verloren); Schäden an Bauwerken heilen ohne Feinde täglich um 25 %.
Für die Darstellung hält die Simulation die letzte Runde fest (nicht gespeichert, von den
Regeln nie gelesen): jeden Schuss (Schütze oder Turm → Ziel) und jeden Hieb mit Angreifer und
Ziel – einen Kämpfer oder das Feld des Gebäudes (Tor, Mauer, Lager, Hauptquartier), auf das ein
Feind einschlägt; wer angreift, schaut zu seinem Ziel.

**Kampfbericht** je abgewehrter (oder erfolgreicher) Bande bzw. jedem Heer und je Feldschlacht und
Belagerung (die letzten 32): Art, Gegner, Ort, Dauer, eigene Kämpfer und Gefallene, Feinde und
Gefallene, beschädigte Gebäude, geraubte und zurückgewonnene Waren und Gold, Ausgang (Sieg: Feind
geschlagen oder ohne Beute vertrieben; Niederlage: der Feind behielt Beute oder schlug die
Truppen; Unentschieden), deutscher Text.

**Feldzug** (`Order::Campaign { kingdom, siege: false }`, nur im Krieg und nur, wenn vom
Hauptquartier ein Weg zum Kartenrand führt – sonst wird der Befehl abgelehnt): die Kompanie sammelt
sich binnen 12 Stunden am Kartenrand in Richtung des Reichs; wer ankommt, verlässt die Karte, der
Rest bleibt daheim. Wer fort ist, wartet unsichtbar im Haupt-Hauptquartier; wird dieses Gebäude
abgerissen (ein zweites Hauptquartier steht noch), warten die Soldaten im verbleibenden. Der Proviant für Hin-, Rückweg und Belagerung wird mitgenommen. Nach der
Reisezeit kommt es zur **Feldschlacht** gegen das Feldheer des Reichs (60 % seiner Soldaten in der
Heeresmischung): bis zu 40 Runden mit derselben Schadensformel, Schützen zuerst; der feindliche
Schaden verteilt sich gleichmäßig auf die lebenden Soldaten (Nahkämpfer vor Schützen), der eigene
nach Größe auf die feindlichen Truppen. Eine Seite bricht, sobald ihre Verluste 30 % + die Hälfte
ihrer mittleren Moral erreichen. Ein Sieg nimmt 15 % des Schatzes des Reichs als Kriegsbeute
(Kassenbuch „Militär“) und macht es müde (+35).
**Belagerung** (`siege: true`, braucht ein Belagerungsgerät): nach gewonnener Feldschlacht 24
Stunden vor dem Sitz: die Geräte (und der Bauwerksschaden aller) schlagen die Mauern (8 TP je
Einwohner), während die Besatzung – die übrigen Soldaten des Reichs – mit +40 % Verteidigung
zurückkämpft; sind die Mauern gebrochen, wird gestürmt. Erfolg erzwingt den **Frieden**: das
Reich tritt Land ab (§10) und zahlt 2 Gold je Einwohner (soweit sein Schatz reicht). Danach
marschiert die Kompanie heim und erscheint wieder am Kartenrand. Frieden beendet jeden Feldzug.

**Wirkung des Krieges** im Überblick: Sold, Rationen und Ausrüstung kosten Gold und Waren; Rekruten
fehlen als Arbeitskräfte; der Handel mit dem Feind ruht und Karawanen-Routen werden gefährlich
(§10); Kriegsmüdigkeit senkt die Zufriedenheit und die Moral; Gefallene fehlen den Familien;
geschlagene Heere und verlorene Feldschlachten schwächen das Reich (Soldaten, Einwohner, Schatz)
und bringen es zum Frieden.

**Entwicklerbefehl:** `DebugSpawnThreat { kingdom, size }` – Räuber (`kingdom: None`) oder ein
Heer eines Reichs im Krieg (1–120 Mann) erreichen binnen einer Stunde den Kartenrand.

**Bericht** (Militärfenster): Soldaten (bereit, in Ausbildung, auf Feldzug, verwundet), Sold je
Tag, Stärke, Einheiten je Typ mit Durchschnitt von Gesundheit, Moral und Ausrüstung, Kompanien mit
Befehl, Mitgliedern, Lage, Moral, Ausrüstung und Sold, Soldaten ohne Kompanie, Bedrohungen (Art,
Größe, Zusammensetzung, Ankunft, Ziel, Lage), Kampfberichte, Kriege (Dauer, Kriegsmüdigkeit,
Heere, Schlachten), Feldzüge (Stufe, Restzeit); je Militärgebäude die Rekrutierungsoptionen mit
Kosten, Werten, möglicher Anzahl oder Hinderungsgrund.

## 12. Ereignisse (`data/events/events.ron`)

Ereignisse haben Bedingungen (Weltzustand), Gewichte, Abklingzeiten und Entscheidungen mit
Konsequenzen: Schlechte Ernte, Dürre, Überschwemmung (Flussnähe), Seuche (Dichte, kein
Krankenhaus), Brand (Holzbauten, keine Brunnen), Räuberüberfall (geringe Sicherheit, nach der
Schonfrist),
Händlerbesuch, Flüchtlingsgruppe, Religiöses Fest (Kirche), Politische Unruhen (niedrige
Zufriedenheit, hohe Steuern), Besonders erfolgreiche Ernte, Entdeckung neuer Vorkommen,
Sturmschäden an Gebäuden.

**Auslosung** (`rules.ron` → `events`): ab dem 7. Tag – die erste Frühlingswoche einer neuen
Siedlung bleibt ruhig, eine Überschwemmung (nur im Frühling) kommt also frühestens im zweiten
Jahr – täglich um 9:00 Uhr (das Tageswetter steht dann fest), wenn kein Ereignis offen ist und
das letzte mindestens 2 Tage zurückliegt: Chance
35 % × Schwierigkeit (Leicht 60 %, Normal 100 %, Schwer 150 % davon). Kandidaten sind die
Ereignisse, deren Bedingungen alle gelten und deren Abklingzeit abgelaufen ist; sie konkurrieren
nach Gewicht. Risiko-Ereignisse: **Brand** × Anteil der Holzgebäude ohne Brunnen in Reichweite ×
(100 % − erforschte Brandgefahr-Senkung, Verbesserte Dächer −50 %), **Seuche** × (100 % −
erforschte Seuchenresistenz). Zufall aus einem eigenen, im Spielstand gespeicherten Generator,
damit Ereignisse die übrigen Zufallszahlen nicht verschieben. `GameSettings::events_enabled`
(Standard: an) schaltet nur die tägliche Auslosung ab (Tests einzelner Systeme); ein offenes
Ereignis und der Entwicklerbefehl `DebugTriggerEvent` wirken weiter.

**Entscheidung:** Der Spieler hat 12 Spielstunden (`decision_hours`), danach gilt die erste
Wahl. Eine Wahl ist nur möglich, wenn ihre Voraussetzungen gelten (Gold; Lagerbestand =
verfügbarer Bestand ohne den Bedarf der Baustellen; Soldaten). Ereignisse mit Gebäudeschaden
wählen beim Eintritt das erste Gebäude (Brandherd, Haus am Fluss …); die Meldung zeigt dorthin,
und der Text nennt es, wo die Daten einen eigenen Text dafür haben (`focus_text`: „Im Gebäude
„Holzfällerhütte“ ist Feuer ausgebrochen!“; ohne Brandherd „In einem Holzgebäude …“).
Kommando `ResolveEvent { serial, choice }`.

**Weltzustand statt Zufall** (Master-Spez. §12): Ernte- und Dürre-Ereignisse lesen das Wetter und
die Felder. Die **Dürre** folgt einer Trockenperiode (`MinDryDays`: Tage in Folge ohne Regen,
Sturm oder Schnee, im Spielstand beim Wetter geführt) und kommt nicht, solange das Wetter selbst
schon Dürre bringt. Die **Ernte-Ereignisse** hängen am mittleren Zustand der bestellten Felder
(`MinFieldCondition`/`MaxFieldCondition`, Anteil des vollen Ertrags, den Stürme und Welken
übrig ließen; ohne bestellte Felder – vor der Aussaat, nach der Ernte – kommt keines) und
schließen sich innerhalb einer Jahreszeit aus (`NoEventThisSeason`). Der Wetterbericht nennt
eine Trockenperiode ab 3 Tagen („Seit 4 Tagen kein Regen.“), die Landwirtschaftsübersicht führt
den mittleren Zustand der Felder (`average_condition`).

ID | Name | Bedingungen | Gewicht | Abklingzeit | Wahl (die erste gilt ohne Entscheidung)
---|---|---|---|---|---
`bad_harvest` | Schlechte Ernte | Herbst, bestellte Felder im Schnitt in höchstens 85 % Zustand (von Stürmen und Welken geschwächt, §8.3), in diesem Herbst noch keine schlechte oder besonders erfolgreiche Ernte | 8 | 24 Tage | Hinnehmen: Feldertrag −30 % für 6 Tage · Nahrung zukaufen (150 G): +40 Brot, Feldertrag −30 % für 6 Tage
`drought` | Dürre | Sommer, ab Tag 7, seit mindestens 3 Tagen kein Regen, Sturm oder Schnee, heute klar oder bewölkt (eine Dürre des Wetters selbst ist schon gemeldet) | 6 | 24 | Auf Regen hoffen: Dürre für 4 Tage · Bewässern (100 G): Dürre für 4 Tage, Feldertrag +25 % für 4 Tage
`flood` | Überschwemmung | Frühling, ein Gebäude ≤ 3 Felder vom Fluss | 5 | 24 | Ablaufen lassen: Regen 2 Tage, 2 Gebäude am Fluss und 6 Felder · Dämme (80 G, 20 Stein): Regen 2 Tage, 1 Gebäude
`plague` | Seuche | ≥ 60 Bewohner, kein Krankenhaus; Risiko Seuche | 4 | 36 | Pflegen: 15 % erkranken, Produktion −20 % für 3 Tage · Abriegeln: 6 %, Produktion −40 % für 3 Tage, Zufriedenheit −8 für 6 Tage · Heiler (250 G): 4 %
`fire` | Brand | ≥ 3 Holzgebäude ohne Brunnen in Reichweite; Risiko Brand | 6 | 10 | Löschketten: 2 Holzgebäude · Feuerwehrleute (60 G): 1 Holzgebäude
`bandit_raid` | Räuberüberfall | Schonfrist vorbei (§11: Normal ab Tag 24, Leicht 36, Schwer 18, ≥ 40 Bewohner), ≤ 40 % der Häuser bewacht | 6 | 18 | Abwarten: eine Räuberbande (§11) kommt 8 h nach der Meldung (mindestens 2 h nach der Entscheidung), Wachen und Türme sollen sie aufhalten, Zufriedenheit −3 für 4 Tage · Lösegeld (150 G): keine Räuber · Soldaten ausschicken (≥ 5 einsatzbereite Soldaten): die Bande kommt, die Soldaten bleiben bis zur Ankunft in Bereitschaft und jagen sie danach 8 h lang (Alarm), Zufriedenheit +5 für 6 Tage
`merchant_visit` | Händlerbesuch | ab Tag 5, Marktplatz | 10 | 8 | Kein Interesse · 5 Gewürze (200 G) · 6 Werkzeuge (120 G) · 40 Holz verkaufen (+220 G)
`refugees` | Flüchtlingsgruppe | ab Tag 9, ≥ 4 freie Wohnplätze | 6 | 20 | Aufnehmen: 8 Siedler, Zufriedenheit +3 für 6 Tage · Abweisen: Zufriedenheit −3 für 4 Tage
`war_refugees` | Kriegsflüchtlinge | ab Tag 11, ein Krieg (des Spielers oder zwischen Königreichen, §10), ≥ 4 freie Wohnplätze | 6 | 12 | Aufnehmen: 6 Siedler, Zufriedenheit +2 für 4 Tage · Abweisen: Zufriedenheit −2 für 3 Tage
`religious_festival` | Religiöses Fest | Kirche | 6 | 12 | Bescheiden: 1 Festtag (`Festival`, §6), Zufriedenheit +4 für 4 Tage · Prächtig (150 G): 2 Festtage, +12 für 8 Tage
`political_unrest` | Politische Unruhen | ≥ 20 Bewohner, Zufriedenheit ≤ 35 oder Steuersatz ≥ 20 % | 7 | 12 | Ignorieren: Zufriedenheit −10 für 6 Tage, Produktion −15 % für 3 Tage · Steuern senken: höchstens 10 % für 6 Tage, +6 für 6 Tage · Zugeständnisse (200 G): +10 für 8 Tage · Wachen (≥ 8 einsatzbereite Soldaten): die Soldaten ziehen 8 h vor dem Hauptquartier auf, −4 für 6 Tage
`bountiful_harvest` | Besonders erfolgreiche Ernte | Herbst, bestellte Felder im Schnitt in mindestens 95 % Zustand (kein Sturm, keine Dürre hat ihnen geschadet), in diesem Herbst noch keine schlechte oder besonders erfolgreiche Ernte | 6 | 24 | Erntedankfest: Feldertrag +30 % für 6 Tage, Zufriedenheit +5 für 6 Tage
`new_deposit` | Entdeckung neuer Vorkommen | ab Tag 13, Steinbruch, Lehmgrube oder Mine | 4 | 30 | Erschließen: Stein, Eisen, Kohle oder Lehm (bis 18 Felder vom Hauptquartier, 400 je Feld) · Nach Gold suchen (100 G): Gold oder Eisen (bis 22 Felder, 250 je Feld)
`storm_damage` | Sturmschäden | Sturm ≥ 50 % Stärke, ≥ 2 Wohnhäuser | 40 | 6 | Später beheben: 2 Holzgebäude · Dächer sichern (60 G, 10 Holz): 1 Holzgebäude

**Konsequenzen:**
* **Gold und Waren:** Buchung im Kassenbuch als „Ereignisse“; Abzüge höchstens bis zum leeren
  Schatz bzw. bis zum verfügbaren Bestand; neue Waren kommen in die Lager nahe dem Hauptquartier.
* **Befristete Modifikatoren** (addieren sich): Zufriedenheit aller Bewohner (Faktor
  „Ereignisse“ in der Zufriedenheitsaufschlüsselung), Produktion (Arbeitstempo aller Betriebe und
  der Feldarbeit; in der Produktivität), Feldertrag (Ernte), Festtage (`Festival(days)`:
  nachmittags wird gefeiert statt gearbeitet, §6).
* **Wetter:** erzwingt das Wetter ab sofort für die angegebenen Tage (Stärke 60 %, eine mittlere
  Dürre, wenn die Daten keine nennen) über denselben Weg wie `DebugSetWeather`; das erzwungene
  Wetter merkt sich das Ereignis, der Wetterbericht nennt es („Wetter vom Ereignis „Dürre“
  vorgegeben (noch 3 Tage)“ statt „vom Entwicklerbefehl“).
* **Flüchtlinge** kommen zu Fuß vom Kartenrand wie Zuwanderer, als Familien (höchstens 5
  Personen) passend zu den freien Plätzen der Häuser; sie dürfen alle freien Plätze nehmen (auch
  den, den Zuwanderer freilassen). Wer keinen Platz findet, lagert am Hauptquartier.
* **Krankheit:** Anteil × (100 % − Seuchenresistenz) der Bewohner (aufgerundet) erkrankt zufällig
  und verliert 40 Gesundheitspunkte (mit Krankenhaus in Reichweite 25 % davon, also 10). Geringere
  Gesundheit erhöht das Krankheitstodesrisiko (§6.1) und senkt Arbeitskraft und Gehtempo.
* **Gebäudeschaden** (Brand, Überschwemmung, Sturm; nie Hauptquartier, Mauern, Tore): Das Gebäude
  wird zur **Ruine** – einer Wiederaufbau-Baustelle mit 50 % der Bauzeit und 50 % der Baumaterialien
  (aufgerundet), ohne Gold; Bauarbeiter und Träger übernehmen sie wie jede Baustelle. Bis zum
  ersten Drittel zeigt sie `buildings/rubble_<B>x<T>`, danach die Baustellenstufen. Solange hat das
  Gebäude keine Funktion: Arbeiter verlieren ihre Stelle, Bewohner ihre Wohnung, ein Lager zählt
  nicht zum Königreichslager. Von den Waren darin gehen verloren: Brand 100 %, Überschwemmung 50 %,
  Sturm nichts – der Rest bleibt im Gebäude. Eine Ruine kann abgerissen (erstattet nur nicht
  verbautes Wiederaufbau-Material und die Waren), aber nicht abgebrochen werden. Fertig:
  „Wiederaufgebaut: …“. Ziele: das gewählte Gebäude, dann die nächstgelegenen weiteren Kandidaten;
  ein **Brand** springt nur auf Holzgebäude ohne Brunnen in Reichweite bis 4 Felder Abstand über
  (Brunnen stoppen ihn; der Wind trägt ihn weiter bzw. hält ihn zurück, §8.2), eine **Überschwemmung** trifft Gebäude bis 3 Felder vom Fluss, ein
  **Sturm** Holzgebäude (Stein hält stand). Bei **Feldern** verlieren die nächstgelegenen
  bestellten Felder ihre Saat (Obstgärten ihre Früchte).
* **Räuber** (`Raiders`): eine echte Bande läuft vom Kartenrand ein und geht auf Lager ohne
  Sicherheitsdienst in Reichweite (Wachturm, Kaserne, Bogenschützenposten, Burg); Kampf, Beute,
  Brandstiftung und Flucht wie in §11. Die Wahl nennt Größe und Uhrzeit der Bande, die
  einsatzbereiten Soldaten und die fertigen Wehrbauten, die schießen, nach Art („dazu 2 Wachtürme
  und 1 Bogenschützenposten“; ohne solche entfällt der Teil; Mehrzahl `plural` in den
  Gebäudedaten). **Alarm** (`Alarm { hours }`): alle einsatzbereiten
  Soldaten jagen die Feinde auf der ganzen Karte; **Aufmarsch** (`Muster { hours }`): sie stehen
  vor dem Hauptquartier. „Soldaten“ in den Voraussetzungen (`MinSoldiers`) sind die
  einsatzbereiten Soldaten auf der Karte (ausgebildet, nicht auf Feldzug, ohne Mannschaften).
* **Neue Vorkommen:** zufällige Art aus der Liste (Lehm nur bis 2 Felder vom Süßwasser), Mittelpunkt
  zufällig im Umkreis des Hauptquartiers auf freiem Land ohne Gebäude, Straße, Feld, Felsbrocken
  oder Vorkommen (bevorzugt im eigenen Gebiet); bis zu 5 Felder (Mitte und vier Nachbarn gleicher
  Höhe) mit je der angegebenen Menge, Bäume dort verschwinden. Die Karte meldet die Chunks neu.
* **Steuerdeckel:** Der Steuersatz sinkt sofort auf höchstens den Wert und darf für die Dauer nicht
  darüber steigen (`SetTaxRate` wird mit Begründung abgelehnt).

**Bericht** (Ereignisfenster): das offene Ereignis mit Titel, Text, Frist und Wahlmöglichkeiten
(verfügbar oder Grund, vorhergesagte Wirkungen auf Deutsch, z. B. „Bis zu 2 Holzgebäude ohne
Brunnen in der Nähe brennen nieder, zuerst das Gebäude „Einfaches Holzhaus““), aktive Modifikatoren mit
Restdauer, Steuerdeckel und die letzten 32 Ereignisse mit Wahl und Ergebnis.

## 13. Startbedingungen (Szenario `data/scenarios/default.ron`)

Rathaus am Startpunkt, 2 Holzhäuser, 1 Lagerplatz im Rathaus, 12 Siedler (4 Familien: vier
Paare, die übrigen Siedler als ihre Kinder so verteilt, dass die Häuser möglichst voll werden –
10 wohnen, 2 brauchen ein neues Haus),
Vorrat: 60 wood, 30 stone, 20 planks, 60 bread, 20 vegetables, 10 tools, 500 Gold (die Nahrung
reicht für etwa fünf Tage; gebaut und gearbeitet wird nur tagsüber, die ersten Häuser und Höfe
brauchen entsprechend länger).
Startforschung: keine. Freigeschaltet: Grundgebäude (Wohnen I, Holz, Stein, Lehm, Nahrung I,
Brunnen, Markt, Lager, Pfade). Bis zu 5 KI-Königreiche (`rival_kingdoms: 5`; die Kartengröße
entscheidet, §10).
