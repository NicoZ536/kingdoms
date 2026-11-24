# Audio-Lieferung für KINGDOMS

Diese Anleitung beschreibt, welche Klänge das Spiel erwartet, wie die Dateien heißen und
abgelegt werden und wie sich eine Lieferung ohne Anhören prüfen lässt. Maßgeblich ist
`manifest.ron` in diesem Ordner: Es ordnet jeder Klang-ID ihre Datei(en), den Lautstärkekanal und
die Abspielregeln zu. Fehlende Dateien sind kein Fehler – das Spiel bleibt an diesen Stellen still.

## Ordner und Dateinamen

| Ordner | Kanal im Spiel | Inhalt |
|---|---|---|
| `music/` | Musik | Musikstücke (Suno), je Stimmung mehrere Stücke |
| `ambience/` | Umgebung | Hintergrundschleifen (Vögel, Regen, Stadt …) |
| `sfx/` | Effekte | Geräusche auf der Karte (Bau, Werkstätten, Tiere, Kampf, Ereignisse) |
| `ui/` | Oberfläche | Klicks, Fenster, Speichern |
| `voice/` | Stimmen | Ansagen des königlichen Beraters und Bürgerantworten (ElevenLabs) |

* **Der Dateiname ohne Endung zählt** (der „Stamm“ in den Tabellen unten). `music/menu_theme`
  wird von `music/menu_theme.ogg`, `music/menu_theme.mp3`, `music/Menu Theme.wav` oder
  `music/menu-theme.flac` erfüllt: Groß-/Kleinschreibung, Leerzeichen, Bindestriche und andere
  Sonderzeichen spielen keine Rolle (jede Folge davon gilt als ein `_`). Liegen mehrere passende
  Dateien vor, gewinnt die im Manifest genannte Endung (`.ogg`), danach `.ogg`, `.mp3`, `.wav`,
  `.flac` in dieser Reihenfolge.
* Jede Datei gehört in den Ordner ihres Kanals (siehe Tabellen); Unterordner werden nicht
  durchsucht.
* Das Spiel liest die Ordner beim Start einmal. Neue Dateien wirken nach einem Neustart.

## Formate

* Angenommen werden **OGG Vorbis, MP3, WAV (PCM) und FLAC**.
* **Empfehlung:** OGG Vorbis (Qualität 6, etwa 192 kbit/s) für Musik und Umgebung, WAV oder OGG
  für kurze Effekte und Stimmen. MP3 aus Suno oder ElevenLabs funktioniert direkt.
* **Abtastrate:** 44,1 kHz oder 48 kHz (das Spiel rechnet um). 16 Bit genügt.
* **Mono/Stereo:** Musik und Umgebung in **Stereo**; Effekte auf der Karte und Stimmen bevorzugt
  in **Mono** – Mono-Effekte werden je nach Ort auf dem Bildschirm links/rechts verteilt,
  Stereo-Effekte spielen unverändert.

## Längen und Lautheit

| Kanal | Empfohlene Länge | Lautheit (Richtwert) |
|---|---|---|
| Musik | 2–4 min je Stück | etwa −16 LUFS integriert, Spitzen unter −1 dBFS |
| Umgebung | 30–90 s, nahtlos | etwa −20 LUFS (liegt unter allem anderen), Spitzen unter −3 dBFS |
| Effekte | 0,2–3 s (Glocken, Fanfaren bis 6 s) | etwa −14 LUFS kurzzeitig, Spitzen unter −1 dBFS |
| Oberfläche | 0,05–0,5 s | leise und kurz, Spitzen unter −6 dBFS |
| Stimmen | 1,5–6 s je Satz | etwa −16 LUFS, Spitzen unter −1 dBFS, ohne Hall und Musik |

* Am Anfang eines Effekts keine Stille (sonst klingt er verzögert), am Ende kurz ausklingen
  lassen statt hart abzuschneiden.
* Stimmen trocken aufnehmen (kein Hall, keine Hintergrundgeräusche); während eine Stimme spricht,
  senkt das Spiel Musik und Umgebung automatisch ab.
* Die Feinabstimmung der Lautstärken je ID steht im Manifest (`volume`); die Spieler regeln
  Gesamt- und Kanallautstärken in den Einstellungen.

## Schleifen (Musik und Umgebung)

* **Umgebung** läuft als Endlosschleife: Ende und Anfang müssen nahtlos ineinander übergehen –
  keine Stille, kein Ein- oder Ausblenden, kein Knacken an der Schnittstelle (Nulldurchgang,
  gleiche Stimmung am Ende wie am Anfang).
* **Musik** spielt die Stücke einer Stimmung nacheinander und blendet jeweils kurz vor dem Ende
  (2,5 s) ins nächste über; hat eine Stimmung nur ein Stück, wiederholt es sich. Ein Stück darf
  deshalb ein natürliches Ende haben, sollte aber nicht mit langer Stille beginnen oder enden.
  Bei einem Stimmungswechsel (z. B. Krieg) wird ebenfalls übergeblendet.
* **MP3 fügt am Anfang und Ende technisch bedingt kurze Stille ein** – für Umgebungsschleifen
  daher OGG, WAV oder FLAC liefern. Der Audio-Bericht weist darauf hin.

## Varianten

Manche IDs haben mehrere Dateien (z. B. `sfx_construction`: `sfx/construction_1` und
`sfx/construction_2`, `vo_citizen_yes`: drei Bürgerantworten). Das Spiel wechselt beim Abspielen
reihum zwischen ihnen; Effekte mit `pitch_variation` werden zusätzlich leicht in der Tonhöhe
variiert. Weitere Varianten werden im Manifest bei `files` ergänzt (z. B.
`files: ["sfx/hammer_1.ogg", "sfx/hammer_2.ogg"]`); fehlt eine Variante, bleibt nur diese still.

## Stimmen (ElevenLabs)

* Sprecher: ein **königlicher Berater**, reife männliche Stimme, Hochdeutsch, respektvoll
  („Majestät“). Die Spalte „Vortrag“ nennt die Stimmung je Satz.
* **Der Wortlaut ist verbindlich** (Feld `text` im Manifest, in der Tabelle unten): genau diese
  Sätze sprechen, ohne Zusätze. Eine Datei je Satz.
* Die Bürgerantworten (`vo_citizen_yes`) sind kurze Zurufe einfacher Leute; verschiedene Stimmen
  sind erwünscht.

## Lieferung prüfen (Audio-Bericht)

Nach dem Einspielen der Dateien im Spielordner aufrufen:

```bash
KINGDOMS_AUDIO_REPORT=1 ./kingdoms        # oder: ./kingdoms --audio-report
```

Der Bericht öffnet kein Fenster und spielt nichts ab. Er listet jede ID mit Kanal, erwarteter
Datei, gefundener Datei (mit Endung), Länge, Abtastrate, Mono/Stereo und Spitzenpegel und nennt
Auffälligkeiten: fehlt, nicht lesbar, übersteuert, Stille am Anfang oder Ende einer Schleife,
MP3-Schleife, ungewöhnliche Länge. Am Ende stehen die Summen und alle Sprechertexte. Der
Rückgabewert ist 0, wenn jede Datei vorhanden und lesbar ist, sonst 1. Mit `KINGDOMS_DEBUG=1`
schreibt das Spiel beim Start zusätzlich ins Log, welche Dateien unter anderem Namen oder Format
gefunden wurden und welchen IDs noch Dateien fehlen.

## Lieferliste (`delivery.json`)

`delivery.json` in diesem Ordner ist eine aus `manifest.ron` erzeugte Liste aller 143 IDs
(15 Musikstücke, 11 Umgebungsschleifen, 58 Effekte, 11 Oberflächenklänge, 48 Ansagen; zusammen
146 Dateien) zum Abhaken oder für eigene Skripte. Je ID stehen darin: `id`, `channel` (Kanal),
`kind` (`music`, `ambience`, `sfx`, `ui`, `voice`), `expected_files` (Datei mit Ordner, je Variante
eine), `loop` (nur Umgebung; Musik blendet über), `description`, `text` (bei Ansagen der genaue
Wortlaut, bei mehreren Varianten eine Liste, sonst `null`), `recommended_length_s` (empfohlene
Länge in Sekunden nach der Tabelle oben) und `variations`. Maßgeblich bleibt `manifest.ron`; ein
Test (`cargo test`) prüft, dass jede ID des Manifests genau einmal mit allen Dateien und
Sprechertexten in der Liste steht.

## Alle IDs

Die Beschreibungen sind englisch gehalten und eignen sich als Ausgangspunkt für Prompts. Wann
welche Musik läuft: **Hauptmenü** im Menü; **Krieg** bei Krieg oder Feinden auf der Karte;
**Angespannt** bei anrückenden Feinden, Tributforderungen oder unbezahlten Soldaten; **Nacht**
nachts; **Winter** im Winter; **Blühend** an Festtagen oder in einer zufriedenen Stadt ab 50
Bewohnern; sonst **Friedlich**.

### Musik (`music/`, Kanal Musik)

| Datei (Stamm) | Stimmungen | Beschreibung (englisch, als Prompt geeignet) |
|---|---|---|
| `music/menu_theme` | Hauptmenü | Main menu theme: majestic, hopeful medieval orchestral piece with lute, strings, soft choir, slow build |
| `music/peaceful_dawn` | Friedlich | Calm morning in a small village: lute, recorder, harp, gentle and warm |
| `music/peaceful_fields` | Friedlich | Pastoral spring/summer music: hurdy-gurdy, flute, light percussion, carefree |
| `music/peaceful_village` | Friedlich | Bustling-but-relaxed village life: fiddle, lute, frame drum, dance-like 6/8 |
| `music/peaceful_river` | Friedlich, Nacht | Reflective, flowing melody: harp, cello, soft recorder |
| `music/prosperous_city` | Blühend | Thriving medieval town: brass, strings, festive percussion, proud and uplifting |
| `music/prosperous_market` | Blühend, Friedlich | Busy market day: bagpipe, fiddle, tambourine, lively folk dance |
| `music/royal_court` | Blühend | Regal court music: harpsichord-like plucked strings, recorders, stately dance |
| `music/winter_hearth` | Winter | Cold winter, warm hearth: slow strings, celesta-like bells, solo voice humming |
| `music/winter_snowfall` | Winter, Nacht | Quiet snowfall: sparse harp and low drone, melancholic but hopeful |
| `music/night_watch` | Nacht | Night over the town: soft low strings, distant flute, very calm |
| `music/tension_unrest` | Angespannt | Unrest and hardship: minor key, low strings, frame drum heartbeat, uneasy |
| `music/tension_famine` | Angespannt | Shortage and hunger: sparse, mournful fiddle and drone |
| `music/war_march` | Krieg | Armies marching: war drums, horns, low male choir, driving rhythm |
| `music/war_siege` | Krieg | Siege battle: intense percussion, brass stabs, dramatic choir |

### Umgebung (`ambience/`, Kanal Umgebung, nahtlose Schleifen)

| ID | Datei(en) (Stamm) | Beschreibung (englisch, als Prompt geeignet) |
|---|---|---|
| `amb_birds_day` | `ambience/birds_day` | Looping daytime countryside: songbirds, light breeze |
| `amb_night` | `ambience/night_crickets` | Looping night: crickets, distant owl, very calm |
| `amb_wind` | `ambience/wind_meadow` | Looping soft wind over meadows |
| `amb_winter_wind` | `ambience/winter_wind` | Looping cold winter wind, occasional gusts |
| `amb_river` | `ambience/river` | Looping gentle river stream |
| `amb_sea` | `ambience/sea_waves` | Looping calm sea waves on a shore |
| `amb_forest` | `ambience/forest` | Looping forest: leaves rustling, woodpecker, birds |
| `amb_rain` | `ambience/rain` | Looping steady rain |
| `amb_storm` | `ambience/storm` | Looping heavy storm with rain and distant thunder |
| `amb_town` | `ambience/town_bustle` | Looping medieval town bustle: footsteps, distant voices, carts, no modern sounds |
| `amb_market` | `ambience/market_crowd` | Looping market crowd murmur with merchants calling (unintelligible) |

### Effekte (`sfx/`, Kanal Effekte)

| ID | Datei(en) (Stamm) | Beschreibung (englisch, als Prompt geeignet) |
|---|---|---|
| `sfx_place_building` | `sfx/place_building` | Short wooden thump + rope creak when placing a building |
| `sfx_construction` | `sfx/construction_1`, `sfx/construction_2` | Hammering on wood and sawing at a construction site (2 s) |
| `sfx_construction_complete` | `sfx/construction_complete` | Satisfying completion: final hammer strike + short cheerful chime |
| `sfx_demolish` | `sfx/demolish` | Wooden structure collapsing, planks and dust |
| `sfx_road_place` | `sfx/road_place` | Shovel into gravel / stones being laid |
| `sfx_upgrade` | `sfx/upgrade` | Building upgraded: rising chime with a soft brass note |
| `sfx_downgrade` | `sfx/downgrade` | Haus steigt ab: fallende, gedämpfte Glockenfolge, leicht wehmütig |
| `sfx_road_remove` | `sfx/road_remove` | Straße wird entfernt: Spitzhacke lockert Steine, Schotter rieselt |
| `sfx_tree_plant` | `sfx/tree_plant` | Setzling wird gepflanzt: Spaten sticht in Erde, Erde wird angedrückt |
| `sfx_wood_chop` | `sfx/wood_chop` | Axe chopping a tree, two strikes |
| `sfx_tree_fall` | `sfx/tree_fall` | Tree creaking and falling onto grass |
| `sfx_saw` | `sfx/sawmill` | Hand saw / water-powered saw cutting planks |
| `sfx_stone_pick` | `sfx/stone_pick` | Pickaxe striking stone in a quarry |
| `sfx_anvil` | `sfx/anvil` | Blacksmith hammer on anvil, three rhythmic strikes |
| `sfx_mill` | `sfx/windmill` | Windmill sails creaking and grinding stones |
| `sfx_bakery` | `sfx/bakery` | Oven fire crackle and dough being kneaded |
| `sfx_brewery` | `sfx/brewery` | Bubbling vats and wooden barrels |
| `sfx_loom` | `sfx/loom` | Wooden weaving loom clacking |
| `sfx_kiln` | `sfx/kiln` | Kiln or charcoal pile: low roar of a stoked fire, crackling embers, a shovel of fuel thrown in |
| `sfx_workshop` | `sfx/workshop` | Craft workshop: a hide scraped on a wooden beam, small mallet and chisel taps |
| `sfx_bees` | `sfx/bees` | Beehives: calm humming swarm, a wooden frame lifted out of a hive |
| `sfx_mine` | `sfx/mine` | Pickaxes echoing inside a mine, rocks falling into a cart |
| `sfx_cow` | `sfx/cow` | Single cow moo |
| `sfx_sheep` | `sfx/sheep` | Sheep bleating |
| `sfx_chicken` | `sfx/chicken` | Chickens clucking |
| `sfx_church_bell` | `sfx/church_bell` | Medieval church bell ringing three times |
| `sfx_harvest` | `sfx/harvest` | Scythe swishes through wheat, bundles being tied |
| `sfx_fishing` | `sfx/fishing` | Net splashing into water, small boat creak |
| `sfx_cart` | `sfx/cart` | Wooden cart wheels rolling on a dirt road, horse hooves |
| `sfx_coins` | `sfx/coins` | Gold coins clinking into a purse |
| `sfx_event_good` | `sfx/event_good` | Positive event fanfare: short bright horns (2 s) |
| `sfx_event_bad` | `sfx/event_bad` | Negative event sting: low ominous horn and drum hit |
| `sfx_fire` | `sfx/fire` | Building on fire: roaring flames, crackling wood |
| `sfx_alarm_bell` | `sfx/alarm_bell` | Urgent alarm bell ringing fast |
| `sfx_war_horn` | `sfx/war_horn` | Deep war horn blast |
| `sfx_battle` | `sfx/battle` | Distant battle: swords clashing, shouting, arrows |
| `sfx_research_complete` | `sfx/research_complete` | Discovery: quill scratch then a warm magical chime |
| `sfx_birth` | `sfx/birth` | Gentle soft chime for a newborn (no crying) |
| `sfx_death_bell` | `sfx/death_bell` | Ein Bewohner ist gestorben: einzelner, ferner Schlag einer tiefen Totenglocke |
| `sfx_wedding` | `sfx/wedding` | Hochzeit: kurzes, fröhliches Glockenspiel mit zwei hellen Glocken |
| `sfx_season_change` | `sfx/season_change` | Seasonal transition: soft harp glissando |
| `sfx_rank_up` | `sfx/rank_up` | Royal fanfare for a new realm rank (4 s) |
| `sfx_trade` | `sfx/trade` | Trade completed: coins + handshake-like wooden knock |
| `sfx_event_pending` | `sfx/event_pending` | Ein Ereignis verlangt eine Entscheidung: Pergament wird entrollt, kurzer Trommelwirbel |
| `sfx_event_resolved` | `sfx/event_resolved` | Entscheidung getroffen: Siegel wird in warmes Wachs gedrückt |
| `sfx_flood` | `sfx/flood` | Überschwemmung: rauschendes Wasser, das durch Gassen strömt, knarrendes Holz |
| `sfx_storm_damage` | `sfx/storm_damage` | Sturmschaden: heulende Böe, splitterndes Holz, herabfallende Dachschindeln |
| `sfx_plague` | `sfx/plague` | Seuche: gedämpftes Husten mehrerer Menschen, ferne Totenglocke |
| `sfx_raid` | `sfx/raid` | Räuberüberfall: raue Rufe, splitternde Kisten, davongaloppierende Pferde |
| `sfx_deposit_found` | `sfx/deposit_found` | Neues Vorkommen: Spitzhacke trifft hell klingend auf Erz, kurzer Glanzton |
| `sfx_war_drums` | `sfx/war_drums` | Kriegserklärung: schwere Kriegstrommeln im langsamen Marschschritt, ein ferner Hornstoß (3 s) |
| `sfx_victory` | `sfx/victory` | Sieg in der Schlacht: helle Fanfare aus zwei Trompeten, Jubelrufe der Soldaten |
| `sfx_defeat` | `sfx/defeat` | Niederlage: tiefes, absteigendes Horn, gedämpfte Trommel, Stille danach |
| `sfx_treaty` | `sfx/treaty` | Vertrag besiegelt: Feder kratzt über Pergament, Siegel wird in Wachs gedrückt, kurzer Glockenton |
| `sfx_herald` | `sfx/herald` | Eine Gesandtschaft trifft ein: kurze Heroldsfanfare, Pferdehufe kommen zum Stehen |
| `sfx_caravan_bell` | `sfx/caravan_bell` | Karawane am Handelskontor: Glöckchen am Zaumzeug, knarrende Wagenräder, Esel schnaubt |
| `sfx_wall_break` | `sfx/wall_break` | Feinde brechen eine Mauer oder ein Tor auf: dumpfer Rammbockstoß, berstendes Holz, fallende Steine |
| `sfx_drill` | `sfx/drill` | Rekruten melden sich: Exerzierrufe eines Ausbilders, Stiefel im Gleichschritt, Speere klirren |

### Oberfläche (`ui/`, Kanal Oberfläche)

| ID | Datei(en) (Stamm) | Beschreibung (englisch, als Prompt geeignet) |
|---|---|---|
| `ui_click` | `ui/click` | Soft wooden button click |
| `ui_hover` | `ui/hover` | Very subtle paper rustle tick |
| `ui_open` | `ui/open` | Parchment unrolling (window opens) |
| `ui_close` | `ui/close` | Parchment rolling up (window closes) |
| `ui_error` | `ui/error` | Dull wooden knock indicating invalid action |
| `ui_notification` | `ui/notification` | Small bell ding for a new message |
| `ui_select` | `ui/select` | Short pluck of a lute string (select building/tool) |
| `ui_speed` | `ui/speed` | Mechanical clockwork tick (game speed changed) |
| `ui_pause` | `ui/pause` | Soft muted drum tap (pause) |
| `ui_rotate` | `ui/rotate` | Wooden swivel click (rotate building) |
| `ui_save` | `ui/save` | Quill writing + wax seal stamp (game saved) |

### Stimmen (`voice/`, Kanal Stimmen)

| ID | Datei (Stamm) | Genauer Wortlaut | Vortrag |
|---|---|---|---|
| `vo_welcome` | `voice/welcome` | Majestät, willkommen in Eurem neuen Land. Möge Euer Reich gedeihen! | Royal advisor (mature male voice, German): warm and pleased |
| `vo_building_complete` | `voice/building_complete` | Der Bau ist vollendet, Majestät. | Royal advisor (mature male voice, German): warm and pleased |
| `vo_food_shortage` | `voice/food_shortage` | Majestät, unsere Vorräte gehen zur Neige. Das Volk hungert! | Royal advisor (mature male voice, German): urgent and alarmed, but composed |
| `vo_low_happiness` | `voice/low_happiness` | Das Volk ist unzufrieden, Majestät. Es murrt in den Gassen. | Royal advisor (mature male voice, German): grave and subdued |
| `vo_treasury_low` | `voice/treasury_low` | Die Schatzkammer ist beinahe leer, Majestät. | Royal advisor (mature male voice, German): grave and subdued |
| `vo_research_complete` | `voice/research_complete` | Unsere Gelehrten haben eine neue Erkenntnis gewonnen! | Royal advisor (mature male voice, German): warm and pleased |
| `vo_immigrants` | `voice/immigrants` | Neue Siedler sind in unserem Reich eingetroffen. | Royal advisor (mature male voice, German): warm and pleased |
| `vo_trade_offer` | `voice/trade_offer` | Ein Händler bittet um Audienz, Majestät. | Royal advisor (mature male voice, German): calm and respectful |
| `vo_war_declared` | `voice/war_declared` | Majestät! Man hat uns den Krieg erklärt! | Royal advisor (mature male voice, German): urgent and alarmed, but composed |
| `vo_peace` | `voice/peace` | Der Frieden ist besiegelt. Möge er lange währen. | Royal advisor (mature male voice, German): warm and pleased |
| `vo_enemy_attack` | `voice/enemy_attack` | Zu den Waffen! Feinde greifen an! | Royal advisor (mature male voice, German): urgent and alarmed, but composed |
| `vo_fire` | `voice/fire` | Feuer! In der Stadt brennt es! | Royal advisor (mature male voice, German): urgent and alarmed, but composed |
| `vo_plague` | `voice/plague` | Eine Seuche breitet sich aus, Majestät. Wir brauchen Heiler! | Royal advisor (mature male voice, German): urgent and alarmed, but composed |
| `vo_rank_up` | `voice/rank_up` | Euer Ruhm wächst, Majestät! Unser Reich ist aufgestiegen. | Royal advisor (mature male voice, German): warm and pleased |
| `vo_harvest_good` | `voice/harvest_good` | Eine prächtige Ernte, Majestät! Die Speicher sind gefüllt. | Royal advisor (mature male voice, German): warm and pleased |
| `vo_harvest_bad` | `voice/harvest_bad` | Die Ernte ist schlecht ausgefallen, Majestät. | Royal advisor (mature male voice, German): grave and subdued |
| `vo_spring` | `voice/spring` | Der Frühling ist gekommen. Zeit zur Aussaat! | Royal advisor (mature male voice, German): calm and respectful |
| `vo_summer` | `voice/summer` | Der Sommer beginnt. Die Felder stehen in voller Pracht. | Royal advisor (mature male voice, German): calm and respectful |
| `vo_autumn` | `voice/autumn` | Der Herbst naht. Bringt die Ernte ein! | Royal advisor (mature male voice, German): calm and respectful |
| `vo_winter` | `voice/winter` | Der Winter bricht herein. Sorgt für Holz und Vorräte! | Royal advisor (mature male voice, German): calm and respectful |
| `vo_raiders` | `voice/raiders` | Räuber überfallen unsere Höfe, Majestät! | Royal advisor (mature male voice, German): urgent and alarmed, but composed |
| `vo_game_saved` | `voice/game_saved` | Die Chronik wurde festgehalten. | Royal advisor (mature male voice, German): calm and respectful |
| `vo_bankrupt` | `voice/bankrupt` | Majestät, die Staatskasse ist leer! Wir sind bankrott. | Royal advisor (mature male voice, German): urgent and alarmed, but composed |
| `vo_emigrants` | `voice/emigrants` | Familien verlassen unser Reich, Majestät. Sie sind unzufrieden. | Royal advisor (mature male voice, German): grave and subdued |
| `vo_deposit_depleted` | `voice/deposit_depleted` | Ein Vorkommen ist erschöpft, Majestät. Wir müssen neue Lagerstätten erschließen. | Royal advisor (mature male voice, German): grave and subdued |
| `vo_storage_full` | `voice/storage_full` | Unsere Lager sind voll, Majestät! Waren gehen verloren. | Royal advisor (mature male voice, German): calm and respectful |
| `vo_event_decision` | `voice/event_decision` | Majestät, eine Entscheidung erwartet Euch. | Royal advisor (mature male voice, German): calm and respectful |
| `vo_flood` | `voice/flood` | Das Wasser steigt, Majestät! Der Fluss tritt über die Ufer. | Royal advisor (mature male voice, German): urgent and alarmed, but composed |
| `vo_storm_damage` | `voice/storm_damage` | Der Sturm hat Dächer abgedeckt, Majestät. | Royal advisor (mature male voice, German): urgent and alarmed, but composed |
| `vo_drought` | `voice/drought` | Die Felder dörren aus, Majestät. Es fehlt an Regen. | Royal advisor (mature male voice, German): grave and subdued |
| `vo_refugees` | `voice/refugees` | Flüchtlinge bitten um Aufnahme in unserem Reich, Majestät. | Royal advisor (mature male voice, German): calm and respectful |
| `vo_festival` | `voice/festival` | Das Volk möchte ein Fest feiern, Majestät! | Royal advisor (mature male voice, German): warm and pleased |
| `vo_unrest` | `voice/unrest` | Unruhe im Volk, Majestät! Man verlangt Euer Gehör. | Royal advisor (mature male voice, German): urgent and alarmed, but composed |
| `vo_deposit_found` | `voice/deposit_found` | Unsere Späher melden neue Vorkommen, Majestät! | Royal advisor (mature male voice, German): warm and pleased |
| `vo_rebuilt` | `voice/rebuilt` | Der Wiederaufbau ist vollendet, Majestät. | Royal advisor (mature male voice, German): warm and pleased |
| `vo_army_approaching` | `voice/army_approaching` | Majestät, ein feindliches Heer sammelt sich an unserer Grenze! | Royal advisor (mature male voice, German): urgent and alarmed, but composed |
| `vo_war_we_declared` | `voice/war_we_declared` | Der Krieg ist erklärt, Majestät. Die Truppen erwarten Eure Befehle. | Royal advisor (mature male voice, German): calm and respectful |
| `vo_treaty` | `voice/treaty` | Der Vertrag ist unterzeichnet, Majestät. | Royal advisor (mature male voice, German): warm and pleased |
| `vo_envoy` | `voice/envoy` | Ein Gesandter bittet um Gehör, Majestät. | Royal advisor (mature male voice, German): calm and respectful |
| `vo_tribute_demand` | `voice/tribute_demand` | Majestät, ein fremder Herrscher fordert Tribut von uns! | Royal advisor (mature male voice, German): urgent and alarmed, but composed |
| `vo_caravan_arrived` | `voice/caravan_arrived` | Fremde Händler sind auf dem Weg zu unserem Handelskontor, Majestät. | Royal advisor (mature male voice, German): calm and respectful |
| `vo_caravan_back` | `voice/caravan_back` | Unsere Karawane ist zurück, Majestät, mit vollen Wagen. | Royal advisor (mature male voice, German): warm and pleased |
| `vo_caravan_robbed` | `voice/caravan_robbed` | Majestät, unsere Karawane wurde unterwegs ausgeraubt! | Royal advisor (mature male voice, German): urgent and alarmed, but composed |
| `vo_victory` | `voice/victory` | Sieg, Majestät! Der Feind ist geschlagen. | Royal advisor (mature male voice, German): warm and pleased |
| `vo_battle_lost` | `voice/battle_lost` | Wir haben die Schlacht verloren, Majestät. Der Feind ist zu stark. | Royal advisor (mature male voice, German): grave and subdued |
| `vo_battle_over` | `voice/battle_over` | Der Kampf ist vorüber, Majestät. Keine Seite hat gesiegt. | Royal advisor (mature male voice, German): grave and subdued |
| `vo_game_over` | `voice/game_over` | Unser Reich ist gefallen, Majestät … Die Geschichte wird sich an uns erinnern. | Royal advisor (mature male voice, German): grave and subdued |
| `vo_citizen_yes` | `voice/citizen_yes_1` | Jawohl! | Villager acknowledgement when clicked: short and eager, one line per file (different voices welcome) |
| `vo_citizen_yes` | `voice/citizen_yes_2` | Sofort, Herr! | Villager acknowledgement when clicked: short and eager, one line per file (different voices welcome) |
| `vo_citizen_yes` | `voice/citizen_yes_3` | Wird gemacht! | Villager acknowledgement when clicked: short and eager, one line per file (different voices welcome) |
