# Audio-Prompts für KINGDOMS – Rise of an Empire

Diese Datei enthält fertige Prompts für **Suno** (Musik) und **ElevenLabs** (Umgebung, Effekte,
Oberflächenklänge, Stimmen). Jede Zeile gehört zu genau einer Klang-ID des Spiels
(`assets/audio/manifest.ron`); die vollständige, maschinenlesbare Liste mit allen Dateinamen und
Sprechertexten steht in `assets/audio/delivery.json`, die technischen Lieferregeln in
`assets/audio/README.md`.

Die Prompt-Texte selbst sind auf Englisch, weil beide Werkzeuge damit am zuverlässigsten
arbeiten. Die Sprechertexte sind deutsch und **wörtlich** zu verwenden.

---

## 0. So lieferst du die Dateien

* **Dateiname = Stamm aus der Spalte „Datei“** (Endung egal: `.mp3`, `.wav`, `.ogg` oder `.flac`).
  Beispiel: Das Menü-Thema als `menu_theme.mp3` liefern. Groß-/Kleinschreibung und Leerzeichen
  spielen keine Rolle, aber der Name muss erkennbar derselbe sein.
* Du kannst mir alle Dateien einfach geben (auch gemischt in einem Ordner oder ZIP). Ich lege sie
  in die richtigen Ordner (`music/`, `ambience/`, `sfx/`, `ui/`, `voice/`) und prüfe die Lieferung
  mit dem eingebauten Audio-Bericht (`kingdoms --audio-report`): fehlende Dateien, Länge,
  Übersteuerung, Stille am Anfang, Lücken in Schleifen.
* **Umgebungsschleifen bitte als WAV** (nicht MP3): MP3 fügt technisch bedingt kurze Stille ein,
  die man in einer Endlosschleife hört. Alles andere darf MP3 sein.
* Bei mehreren Ergebnissen pro Prompt: nimm das beste. Für die mit „Varianten“ markierten IDs
  werden mehrere Dateien gebraucht (z. B. `construction_1` und `construction_2`).

---

## 1. Suno – Musik (15 Stücke, Ordner `music/`)

**Einstellungen für alle Stücke**

* Modus **Custom**, neuestes Modell.
* **Lyrics-Feld:** `[Instrumental]` (Ausnahmen mit wortlosem Chor/Summen stehen beim Stück),
  oder den Schalter **Instrumental** aktivieren.
* **Exclude Styles** (falls verfügbar): `modern drums, synthesizer, electric guitar, rap, pop vocals, EDM, dubstep`
* **Länge:** 2:30–4:00 Minuten. Ein natürliches Ende ist in Ordnung; das Spiel blendet die Stücke
  einer Stimmung ineinander über. Bitte keine lange Stille am Anfang oder Ende.
* Die Stücke sollen sich wie ein zusammenhängender Soundtrack anfühlen: gleiche akustische
  Mittelalter-Besetzung, warmer Raumklang, keine modernen Instrumente.

| # | Datei | Titel | Style of Music (Prompt) |
|---|---|---|---|
| 1 | `menu_theme` | Rise of an Empire | `epic medieval orchestral main theme, majestic and hopeful, slow build from solo lute and soft strings to full orchestra with french horns, timpani and a soft wordless choir, D major, 80 bpm, cinematic fantasy kingdom, warm hall reverb, instrumental` |
| 2 | `peaceful_dawn` | Morgen im Dorf | `calm medieval folk, early morning in a small village, fingerpicked lute, wooden recorder melody, gentle harp arpeggios, warm and peaceful, G major, 72 bpm, soft and intimate, acoustic, instrumental` |
| 3 | `peaceful_fields` | Über die Felder | `pastoral medieval folk, sunny spring fields, hurdy-gurdy drone and melody, wooden flute, light frame drum and tambourine, carefree and bright, D mixolydian, 96 bpm, acoustic, instrumental` |
| 4 | `peaceful_village` | Dorfleben | `lively but relaxed medieval village dance in 6/8, fiddle lead, lute rhythm, frame drum, small bells, cheerful bustling town life, A dorian, 108 bpm, acoustic folk ensemble, instrumental` |
| 5 | `peaceful_river` | Am Fluss | `reflective flowing medieval ballad, harp arpeggios like running water, warm solo cello melody, soft recorder answers, tranquil and contemplative, E minor to G major, 66 bpm, acoustic, instrumental` |
| 6 | `prosperous_city` | Blühende Stadt | `proud uplifting medieval orchestral, thriving prosperous town, brass fanfare motifs, strings, festive percussion with timpani and snare, bright and triumphant but not war-like, C major, 100 bpm, instrumental` |
| 7 | `prosperous_market` | Markttag | `busy medieval market day, lively folk dance, bagpipe lead, fiddle, tambourine, hand drums, joyful crowd energy without vocals, G mixolydian, 120 bpm, acoustic, instrumental` |
| 8 | `royal_court` | Am Hofe | `regal renaissance court dance, harpsichord-like plucked strings, recorder consort, viola da gamba, stately pavane rhythm, elegant and noble, F major, 76 bpm, chamber ensemble, instrumental` |
| 9 | `winter_hearth` | Am Herdfeuer | `cold winter night, warm fireside, slow tender strings, celesta-like bell tones, soft lute, gentle wordless female humming, cozy and melancholic, D minor resolving to F major, 60 bpm, intimate` |
| 10 | `winter_snowfall` | Schneefall | `quiet snowfall, sparse harp notes, low string drone, distant soft flute, melancholic but hopeful, lots of space between notes, A minor, 56 bpm, ambient medieval, instrumental` |
| 11 | `night_watch` | Nachtwache | `calm night over a sleeping medieval town, soft low strings, distant wooden flute, very quiet and peaceful, gentle harp, B minor, 58 bpm, minimal, instrumental` |
| 12 | `tension_unrest` | Unruhe | `uneasy medieval tension, unrest and hardship, minor key low strings, frame drum heartbeat pulse, dissonant hurdy-gurdy drone, brooding and restless, C minor, 84 bpm, instrumental` |
| 13 | `tension_famine` | Hungersnot | `sparse mournful medieval lament, hunger and scarcity, solo fiddle over a low drone, occasional distant bell, sorrowful and empty, D phrygian, 60 bpm, acoustic, instrumental` |
| 14 | `war_march` | Heerzug | `medieval war march, armies marching, heavy war drums, deep horns, low wordless male choir, driving relentless rhythm, determined and grim, D minor, 96 bpm, epic orchestral` |
| 15 | `war_siege` | Belagerung | `intense medieval siege battle, pounding orchestral percussion, brass stabs, dramatic wordless choir, fast string ostinatos, chaotic and heroic, C minor, 132 bpm, epic cinematic` |

**Stücke mit Stimme (9, 14, 15, optional 1):** Falls Suno im Instrumental-Modus keinen Chor bzw.
kein Summen erzeugt, Instrumental ausschalten und ins Lyrics-Feld nur Laute ohne Wörter schreiben:

* `winter_hearth`:
  ```
  [Intro]
  [Instrumental]
  [Verse]
  Mmm, mmm, mmm…
  Hmm, hmm…
  [Outro]
  [Instrumental]
  ```
* `war_march`, `war_siege` (und optional `menu_theme`):
  ```
  [Intro]
  [Instrumental]
  [Chorus]
  Ooh… aah…
  Oh, oh, oh…
  [Instrumental]
  ```

Das Spiel erwartet **keine verständlichen Liedtexte**: Wenn eine Version Wörter singt, bitte eine
andere wählen.

---

## 2. ElevenLabs – Umgebung, Effekte, Oberfläche (80 Dateien)

**Werkzeug:** ElevenLabs → *Sound Effects*.

* **Dauer** wie in der Tabelle einstellen (ElevenLabs erlaubt höchstens etwa 30 s; Werte unter
  dem Minimum auf das Minimum setzen).
* **Prompt Influence:** etwa 0,5 (bei UI-Klängen höher, 0,7).
* **Umgebung:** die Option **Loop/Schleife** einschalten (falls angeboten) und als **WAV**
  herunterladen.
* Grundregel für alle Effekte: mittelalterlich, keine Musik (außer wo genannt), keine modernen
  Geräusche, der Klang beginnt **sofort** ohne Stille.

### 2.1 Umgebung – nahtlose Schleifen (Ordner `ambience/`, 11 Dateien, je 30 s, Loop an)

| ID | Datei | Prompt |
|---|---|---|
| `amb_birds_day` | `birds_day` | `Seamless loop of a peaceful daytime countryside meadow, many distant songbirds, light breeze through grass, no people, no traffic, no music` |
| `amb_night` | `night_crickets` | `Seamless loop of a calm summer night in the countryside, steady crickets, an owl hooting occasionally in the distance, very quiet, no wind gusts, no music` |
| `amb_wind` | `wind_meadow` | `Seamless loop of soft wind blowing over open meadows, gentle rustling grass, steady and calm, no birds, no music` |
| `amb_winter_wind` | `winter_wind` | `Seamless loop of cold winter wind over a snowy landscape, low howling with occasional soft gusts, crisp and lonely, no music` |
| `amb_river` | `river` | `Seamless loop of a gentle river stream flowing over stones, steady babbling water, close and calm, no birds, no music` |
| `amb_sea` | `sea_waves` | `Seamless loop of calm sea waves washing onto a pebble shore, slow regular rhythm, light wind, no seagulls, no music` |
| `amb_forest` | `forest` | `Seamless loop of a deep forest, leaves rustling in the wind, a woodpecker tapping now and then, scattered birdsong, no music` |
| `amb_rain` | `rain` | `Seamless loop of steady medium rain falling on a wooden village, rain on thatched roofs and puddles, no thunder, no music` |
| `amb_storm` | `storm` | `Seamless loop of a heavy storm, strong wind and pouring rain, distant rolling thunder every few seconds, no music` |
| `amb_town` | `town_bustle` | `Seamless loop of a busy medieval town street, footsteps on dirt and cobblestones, distant indistinct voices, a wooden cart passing, a hammer far away, chickens, no modern sounds, no music, no intelligible speech` |
| `amb_market` | `market_crowd` | `Seamless loop of a crowded medieval market square, murmuring crowd, merchants calling out in the distance with unintelligible words, clinking coins, wooden stalls, no modern sounds, no music` |

### 2.2 Effekte auf der Karte und Ereignisse (Ordner `sfx/`, 58 IDs / 59 Dateien)

| ID | Datei | Dauer | Prompt |
|---|---|---|---|
| `sfx_place_building` | `place_building` | 0,8 s | `A heavy wooden beam set down on the ground with a solid thump and a short rope creak, close, dry` |
| `sfx_construction` | `construction_1` | 2 s | `Medieval construction site: carpenter hammering nails into wooden beams, steady rhythm, outdoors` |
| `sfx_construction` (Variante 2) | `construction_2` | 2 s | `Medieval construction site: hand saw cutting through a wooden plank, then two hammer strikes, outdoors` |
| `sfx_construction_complete` | `construction_complete` | 2 s | `A final strong hammer strike on wood followed by a short cheerful bright chime, satisfying completion sound` |
| `sfx_demolish` | `demolish` | 2 s | `A small wooden building collapsing, planks cracking and falling, dust and debris settling` |
| `sfx_road_place` | `road_place` | 0,8 s | `A shovel digging into gravel and stones being laid on a road, short and crunchy` |
| `sfx_upgrade` | `upgrade` | 2 s | `Rising bright chime with a soft warm brass note, a building being upgraded, positive and proud` |
| `sfx_downgrade` | `downgrade` | 2 s | `A falling muted sequence of soft bells, slightly wistful, something got worse, gentle` |
| `sfx_road_remove` | `road_remove` | 1 s | `A pickaxe loosening paving stones, gravel trickling down, short` |
| `sfx_tree_plant` | `tree_plant` | 1,2 s | `A spade cutting into soft soil, then earth being pressed down by hand around a sapling` |
| `sfx_wood_chop` | `wood_chop` | 1,5 s | `An axe chopping into a tree trunk, two solid strikes, outdoors in a forest` |
| `sfx_tree_fall` | `tree_fall` | 3 s | `A tree creaking, cracking and falling onto grass with a heavy thud and rustling branches` |
| `sfx_saw` | `sawmill` | 2,5 s | `A large two-man saw cutting through a log into planks, rhythmic wooden sawing` |
| `sfx_stone_pick` | `stone_pick` | 1 s | `A pickaxe striking hard stone in a quarry, sharp ringing hit and small rocks falling` |
| `sfx_anvil` | `anvil` | 2 s | `A blacksmith's hammer striking a hot iron bar on an anvil, three rhythmic ringing strikes` |
| `sfx_mill` | `windmill` | 3 s | `A wooden windmill: sails creaking as they turn, heavy millstones grinding grain inside` |
| `sfx_bakery` | `bakery` | 3 s | `A medieval bakery: wood fire crackling in a brick oven, dough being kneaded and slapped on a wooden table` |
| `sfx_brewery` | `brewery` | 3 s | `A medieval brewery: bubbling liquid in large wooden vats, a wooden barrel being rolled and knocked` |
| `sfx_loom` | `loom` | 2,5 s | `A wooden weaving loom clacking rhythmically, shuttle sliding through threads` |
| `sfx_kiln` | `kiln` | 3 s | `A stoked kiln or charcoal pile: low roar of fire, crackling embers, a shovel of fuel thrown in` |
| `sfx_workshop` | `workshop` | 2,5 s | `A medieval craft workshop: a hide scraped on a wooden beam, small mallet and chisel taps` |
| `sfx_bees` | `bees` | 3 s | `Beehives: a calm humming swarm of bees, a wooden frame lifted out of a hive` |
| `sfx_mine` | `mine` | 2,5 s | `Pickaxes echoing inside a stone mine tunnel, rocks falling into a wooden cart` |
| `sfx_cow` | `cow` | 1,5 s | `A single cow mooing calmly in a pasture` |
| `sfx_sheep` | `sheep` | 1,2 s | `A sheep bleating once, then a second sheep answering, pasture` |
| `sfx_chicken` | `chicken` | 1,5 s | `A few chickens clucking in a farmyard` |
| `sfx_church_bell` | `church_bell` | 5 s | `A medieval church bell ringing three times with a long natural decay, heard from the village` |
| `sfx_harvest` | `harvest` | 2 s | `A scythe swishing through ripe wheat, then a bundle of straw being tied with twine` |
| `sfx_fishing` | `fishing` | 2 s | `A fishing net splashing into a lake, a small wooden boat creaking` |
| `sfx_cart` | `cart` | 3 s | `Wooden cart wheels rolling over a dirt road, a horse's hooves walking, slight creaking` |
| `sfx_coins` | `coins` | 1 s | `A handful of gold coins clinking into a leather purse` |
| `sfx_event_good` | `event_good` | 2 s | `A short bright medieval horn fanfare, positive news, two horns, joyful` |
| `sfx_event_bad` | `event_bad` | 2 s | `A low ominous horn note with a deep drum hit, bad news sting, medieval` |
| `sfx_fire` | `fire` | 3 s | `A wooden building on fire: roaring flames, crackling and popping wood, a beam cracking` |
| `sfx_alarm_bell` | `alarm_bell` | 3 s | `An urgent medieval alarm bell ringing fast and repeatedly, danger` |
| `sfx_war_horn` | `war_horn` | 3 s | `A single deep medieval war horn blast echoing across a valley` |
| `sfx_battle` | `battle` | 4 s | `A distant medieval battle: swords clashing, shields hit, men shouting, arrows whooshing, no music` |
| `sfx_research_complete` | `research_complete` | 2,5 s | `A quill scratching on parchment, then a warm shimmering chime of discovery` |
| `sfx_birth` | `birth` | 1,5 s | `A gentle soft high chime, tender and happy, for a newborn, no crying` |
| `sfx_death_bell` | `death_bell` | 4 s | `A single distant strike of a deep funeral bell with a long fading decay` |
| `sfx_wedding` | `wedding` | 3 s | `A short joyful peal of two bright church bells for a wedding` |
| `sfx_season_change` | `season_change` | 2,5 s | `A soft magical harp glissando, upward and gentle, a new season begins` |
| `sfx_rank_up` | `rank_up` | 4 s | `A royal medieval fanfare with trumpets and timpani roll, triumphant and grand` |
| `sfx_trade` | `trade` | 1,5 s | `Coins clinking followed by a firm wooden knock like a deal sealed on a table` |
| `sfx_event_pending` | `event_pending` | 2 s | `A parchment scroll being unrolled followed by a short military drum roll, a decision is needed` |
| `sfx_event_resolved` | `event_resolved` | 1,5 s | `A wax seal being pressed firmly into warm wax on parchment` |
| `sfx_flood` | `flood` | 3 s | `Rushing flood water flowing through narrow village streets, wooden planks creaking under the pressure` |
| `sfx_storm_damage` | `storm_damage` | 3 s | `A howling wind gust, wood splintering and roof shingles falling to the ground` |
| `sfx_plague` | `plague` | 3 s | `Several people coughing weakly in a dim room, a funeral bell far away, somber` |
| `sfx_raid` | `raid` | 3 s | `Bandits raiding a farm: rough shouts, wooden crates smashing, horses galloping away, no intelligible words` |
| `sfx_deposit_found` | `deposit_found` | 2 s | `A pickaxe hitting a rich ore vein with a bright ringing clink, followed by a short shimmering sparkle tone` |
| `sfx_war_drums` | `war_drums` | 3 s | `Heavy war drums in a slow marching rhythm, one distant horn blast, a declaration of war` |
| `sfx_victory` | `victory` | 4 s | `A bright victory fanfare from two trumpets with soldiers cheering, medieval` |
| `sfx_defeat` | `defeat` | 4 s | `A deep descending horn, a muffled drum beat, then silence, defeat` |
| `sfx_treaty` | `treaty` | 3 s | `A quill scratching a signature on parchment, a wax seal pressed, a single soft bell tone` |
| `sfx_herald` | `herald` | 3 s | `A short herald trumpet fanfare as horses trot up and come to a halt` |
| `sfx_caravan_bell` | `caravan_bell` | 3 s | `A trade caravan arriving: small bells on a harness jingling, wooden wagon wheels creaking, a donkey snorting` |
| `sfx_wall_break` | `wall_break` | 2,5 s | `A battering ram hitting a wooden gate with a deep boom, wood bursting and stones falling` |
| `sfx_drill` | `drill` | 3 s | `Soldiers drilling: a drill instructor's short barked commands (no intelligible words), boots marching in step, spears clinking` |

### 2.3 Oberfläche (Ordner `ui/`, 11 Dateien)

Kürzeste Dauer einstellen (bei Zielwerten unter dem Minimum das Minimum nehmen), Prompt Influence
hoch. Leise, trocken, ohne Hall; der Klang muss sofort beginnen.

| ID | Datei | Ziel | Prompt |
|---|---|---|---|
| `ui_click` | `click` | 0,1 s | `A single soft wooden button click, very short, dry, UI sound` |
| `ui_hover` | `hover` | 0,05 s | `A very subtle short paper rustle tick, extremely short and quiet, UI hover sound` |
| `ui_open` | `open` | 0,4 s | `A short parchment scroll unrolling, quick, UI window opening sound` |
| `ui_close` | `close` | 0,4 s | `A short parchment scroll rolling up, quick, UI window closing sound` |
| `ui_error` | `error` | 0,2 s | `A dull short wooden knock, invalid action, UI error sound, not harsh` |
| `ui_notification` | `notification` | 0,4 s | `A small clear bell ding, single hit, short decay, UI notification` |
| `ui_select` | `select` | 0,15 s | `A short single pluck of a lute string, UI selection sound` |
| `ui_speed` | `speed` | 0,2 s | `A short mechanical clockwork tick, UI sound` |
| `ui_pause` | `pause` | 0,2 s | `A soft muted single drum tap, UI pause sound` |
| `ui_rotate` | `rotate` | 0,15 s | `A short wooden swivel click, UI rotate sound` |
| `ui_save` | `save` | 0,5 s | `A quick quill scribble followed by a wax seal stamp, UI save sound` |

---

## 3. ElevenLabs – Stimmen (48 IDs / 50 Dateien, Ordner `voice/`)

### 3.1 Der königliche Berater (47 Sätze)

**Stimme anlegen** (ElevenLabs → *Voice Design*), Beschreibung:

```
A mature German man in his late fifties with a warm, dignified baritone voice. He is the trusted
royal advisor at a medieval court. He speaks clear standard High German (Hochdeutsch) with a calm,
measured pace, respectful and slightly formal, with quiet gravitas. Studio quality, close
microphone, completely dry, no reverb, no background noise.
```

Vorschautext für Voice Design: `Majestät, willkommen in Eurem neuen Land. Möge Euer Reich gedeihen!`

**Text to Speech:**

* Modell **Eleven v3** (versteht die Stimmungs-Tags in eckigen Klammern; sie werden nicht
  mitgesprochen). Falls du **Multilingual v2** nimmst: Tags weglassen, Stimmung über die
  Einstellungen steuern (Stabilität ca. 45–55 %, Ähnlichkeit ca. 75 %, Stil ca. 20–35 %).
* Sprache Deutsch; **genau den Text** der Spalte „Eingabe“ verwenden, eine Datei je Zeile.
* Tags: `[warmly]` = erfreut · `[calmly]` = ruhig, respektvoll · `[gravely]` = ernst, gedämpft ·
  `[urgently]` = alarmiert, aber beherrscht.

| ID | Datei | Eingabe (Eleven v3) |
|---|---|---|
| `vo_welcome` | `welcome` | `[warmly] Majestät, willkommen in Eurem neuen Land. Möge Euer Reich gedeihen!` |
| `vo_building_complete` | `building_complete` | `[warmly] Der Bau ist vollendet, Majestät.` |
| `vo_food_shortage` | `food_shortage` | `[urgently] Majestät, unsere Vorräte gehen zur Neige. Das Volk hungert!` |
| `vo_low_happiness` | `low_happiness` | `[gravely] Das Volk ist unzufrieden, Majestät. Es murrt in den Gassen.` |
| `vo_treasury_low` | `treasury_low` | `[gravely] Die Schatzkammer ist beinahe leer, Majestät.` |
| `vo_research_complete` | `research_complete` | `[warmly] Unsere Gelehrten haben eine neue Erkenntnis gewonnen!` |
| `vo_immigrants` | `immigrants` | `[warmly] Neue Siedler sind in unserem Reich eingetroffen.` |
| `vo_trade_offer` | `trade_offer` | `[calmly] Ein Händler bittet um Audienz, Majestät.` |
| `vo_war_declared` | `war_declared` | `[urgently] Majestät! Man hat uns den Krieg erklärt!` |
| `vo_peace` | `peace` | `[warmly] Der Frieden ist besiegelt. Möge er lange währen.` |
| `vo_enemy_attack` | `enemy_attack` | `[urgently] Zu den Waffen! Feinde greifen an!` |
| `vo_fire` | `fire` | `[urgently] Feuer! In der Stadt brennt es!` |
| `vo_plague` | `plague` | `[urgently] Eine Seuche breitet sich aus, Majestät. Wir brauchen Heiler!` |
| `vo_rank_up` | `rank_up` | `[warmly] Euer Ruhm wächst, Majestät! Unser Reich ist aufgestiegen.` |
| `vo_harvest_good` | `harvest_good` | `[warmly] Eine prächtige Ernte, Majestät! Die Speicher sind gefüllt.` |
| `vo_harvest_bad` | `harvest_bad` | `[gravely] Die Ernte ist schlecht ausgefallen, Majestät.` |
| `vo_spring` | `spring` | `[calmly] Der Frühling ist gekommen. Zeit zur Aussaat!` |
| `vo_summer` | `summer` | `[calmly] Der Sommer beginnt. Die Felder stehen in voller Pracht.` |
| `vo_autumn` | `autumn` | `[calmly] Der Herbst naht. Bringt die Ernte ein!` |
| `vo_winter` | `winter` | `[calmly] Der Winter bricht herein. Sorgt für Holz und Vorräte!` |
| `vo_raiders` | `raiders` | `[urgently] Räuber überfallen unsere Höfe, Majestät!` |
| `vo_game_saved` | `game_saved` | `[calmly] Die Chronik wurde festgehalten.` |
| `vo_bankrupt` | `bankrupt` | `[urgently] Majestät, die Staatskasse ist leer! Wir sind bankrott.` |
| `vo_emigrants` | `emigrants` | `[gravely] Familien verlassen unser Reich, Majestät. Sie sind unzufrieden.` |
| `vo_deposit_depleted` | `deposit_depleted` | `[gravely] Ein Vorkommen ist erschöpft, Majestät. Wir müssen neue Lagerstätten erschließen.` |
| `vo_storage_full` | `storage_full` | `[calmly] Unsere Lager sind voll, Majestät! Waren gehen verloren.` |
| `vo_event_decision` | `event_decision` | `[calmly] Majestät, eine Entscheidung erwartet Euch.` |
| `vo_flood` | `flood` | `[urgently] Das Wasser steigt, Majestät! Der Fluss tritt über die Ufer.` |
| `vo_storm_damage` | `storm_damage` | `[urgently] Der Sturm hat Dächer abgedeckt, Majestät.` |
| `vo_drought` | `drought` | `[gravely] Die Felder dörren aus, Majestät. Es fehlt an Regen.` |
| `vo_refugees` | `refugees` | `[calmly] Flüchtlinge bitten um Aufnahme in unserem Reich, Majestät.` |
| `vo_festival` | `festival` | `[warmly] Das Volk möchte ein Fest feiern, Majestät!` |
| `vo_unrest` | `unrest` | `[urgently] Unruhe im Volk, Majestät! Man verlangt Euer Gehör.` |
| `vo_deposit_found` | `deposit_found` | `[warmly] Unsere Späher melden neue Vorkommen, Majestät!` |
| `vo_rebuilt` | `rebuilt` | `[warmly] Der Wiederaufbau ist vollendet, Majestät.` |
| `vo_army_approaching` | `army_approaching` | `[urgently] Majestät, ein feindliches Heer sammelt sich an unserer Grenze!` |
| `vo_war_we_declared` | `war_we_declared` | `[calmly] Der Krieg ist erklärt, Majestät. Die Truppen erwarten Eure Befehle.` |
| `vo_treaty` | `treaty` | `[warmly] Der Vertrag ist unterzeichnet, Majestät.` |
| `vo_envoy` | `envoy` | `[calmly] Ein Gesandter bittet um Gehör, Majestät.` |
| `vo_tribute_demand` | `tribute_demand` | `[urgently] Majestät, ein fremder Herrscher fordert Tribut von uns!` |
| `vo_caravan_arrived` | `caravan_arrived` | `[calmly] Fremde Händler sind auf dem Weg zu unserem Handelskontor, Majestät.` |
| `vo_caravan_back` | `caravan_back` | `[warmly] Unsere Karawane ist zurück, Majestät, mit vollen Wagen.` |
| `vo_caravan_robbed` | `caravan_robbed` | `[urgently] Majestät, unsere Karawane wurde unterwegs ausgeraubt!` |
| `vo_victory` | `victory` | `[warmly] Sieg, Majestät! Der Feind ist geschlagen.` |
| `vo_battle_lost` | `battle_lost` | `[gravely] Wir haben die Schlacht verloren, Majestät. Der Feind ist zu stark.` |
| `vo_battle_over` | `battle_over` | `[gravely] Der Kampf ist vorüber, Majestät. Keine Seite hat gesiegt.` |
| `vo_game_over` | `game_over` | `[sorrowfully] Unser Reich ist gefallen, Majestät … Die Geschichte wird sich an uns erinnern.` |

### 3.2 Bürgerantworten (`vo_citizen_yes`, 3 Dateien, verschiedene Stimmen)

Kurz, eifrig, bodenständig, trocken aufgenommen. Am besten drei verschiedene Stimmen aus der
ElevenLabs-Bibliothek (deutschsprachig) oder per Voice Design:

| Datei | Stimme (Voice Design) | Eingabe (Eleven v3) |
|---|---|---|
| `citizen_yes_1` | `A young German farmhand in his twenties, eager and friendly, rural, clear standard German, dry recording` | `[eagerly] Jawohl!` |
| `citizen_yes_2` | `A sturdy German craftsman in his forties, gruff but willing, clear standard German, dry recording` | `[briskly] Sofort, Herr!` |
| `citizen_yes_3` | `A cheerful German peasant woman in her thirties, quick and good-natured, clear standard German, dry recording` | `[cheerfully] Wird gemacht!` |

---

## 4. Checkliste

* 15 Musikstücke (`music/`)
* 11 Umgebungsschleifen als WAV (`ambience/`)
* 59 Effektdateien (`sfx/`, davon `construction_1` und `construction_2`)
* 11 Oberflächenklänge (`ui/`)
* 50 Sprachdateien (`voice/`: 47 Beratersätze und 3 Bürgerantworten)

Insgesamt **146 Dateien** für **143 Klang-IDs**. Fehlende Dateien sind kein Fehler: Das Spiel
bleibt an diesen Stellen still, du kannst also auch in Etappen liefern.
