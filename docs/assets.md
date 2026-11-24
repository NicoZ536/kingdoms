# Asset-Pipeline und Namenskonventionen

## Format

* `assets/atlases/index.ron` – Liste der zu ladenden Atlanten: `AtlasIndex(atlases: ["terrain", …])`
* `assets/atlases/<name>.ron` – Metadaten (`AtlasDef`):

```ron
AtlasDef(
    image: "terrain.png",          // relativ zur RON-Datei
    sampling: Pixel,               // Pixel (scharf beim Hineinzoomen) | Smooth
    mipmaps: true,                 // gamma-korrekte Mipmaps (max. 4 Stufen)
    source: "Herkunft / Lizenz",   // Pflicht für echte Assets
    sprites: {
        "grass_0": SpriteDef(rect: (x, y, w, h), anchor: (0.5, 0.3333)),
    },
    animations: {
        "smoke": AnimationDef(frames: ["smoke_0", "smoke_1"], fps: 6.0, looping: true),
    },
)
```

* Sprites werden als `"<atlas>/<sprite>"` adressiert. Fehlende Sprites erscheinen als
  magenta-schwarzes Schachbrett und werden einmal im Log gemeldet.
* Atlanten brauchen ≥ 8 px Abstand zwischen Regionen (Kanten werden extrudiert, siehe Generator).
  Maximale Atlasgröße 4096 × 4096 (der Generator bricht sonst mit Fehler ab).
* Die aktuellen Grafiken sind **Platzhalter**, erzeugt von `tools/assetgen`
  (`cargo run -p kingdoms_assetgen --release -- assets`). Echte Grafiken ersetzen die PNGs bei
  gleichen Sprite-Namen und Ankern – der Code muss dafür nicht geändert werden.
* Der Generator ist deterministisch (gleiche Bytes bei jedem Lauf) und packt mit einem
  Skyline-Packer in Zweierpotenz-Atlanten. Tests: `cargo test -p kingdoms_assetgen` (prüft u. a.
  Vollständigkeit aller Namen dieser Tabelle, Geometrie, Nahtlosigkeit, Determinismus).

### Atlanten (Ladereihenfolge von `index.ron`)

| Atlas | Sampling | Größe | Sprites | Animationen |
|---|---|---|---|---|
| `terrain` | Pixel | 512 × 1024 | 103 | – |
| `overlay` | Smooth | 128 × 256 | 6 | – |
| `nature` | Pixel | 512 × 512 | 63 | – |
| `buildings` | Pixel | 2048 × 2048 | 139 | 7 (`<id>_work`) |
| `roads` | Pixel | 512 × 1024 | 100 | – |
| `fields` | Pixel | 512 × 512 | 23 | – |
| `characters` | Pixel | 1024 × 1024 | 497 | 124 (`<role>_<dir>_walk`, `_work`, `_attack`, `_pickaxe`, `_shovel`, `_rod`) |
| `effects` | Smooth | 128 × 256 | 37 | `smoke`, `fire`, `dust` |
| `icons` | Smooth | 512 × 1024 | 175 | – |
| `ui` | Smooth | 256 × 256 | 7 | – |
| `lights` | Pixel | 512 × 512 | 47 | – |

## Geometrie

* Tile-Raute: 64 × 32 px. Eine Höhenstufe: 16 px.
* Terrain-Tile-Sprite: 64 × 48 (Raute + 16 px Seitenwand), Anker = Rautenmitte `(0.5, 1/3)`.
* Gebäude mit Grundfläche B×T: Sprite-Breite **genau** `(B+T) × 32` px, Unterkante = vordere
  Ecke der Grundfläche, Anker `(T/(B+T), 1.0)`. Höhe frei (Dach/Turm); leere Zeilen über dem
  Inhalt werden abgeschnitten, die Grundflächen-Raute bleibt aber immer vollständig enthalten.
  Die vordere Ecke liegt `T × 32` px vom linken Rand: die linke (helle) Vorderwand verläuft über
  die T-Seite entlang der Tile-x-Achse, die rechte (dunkle) über die B-Seite entlang der
  Tile-y-Achse. Für nicht-quadratische Gebäude (2×3, 3×4) muss die Platzierung die T-Ausdehnung
  also auf Tile-x legen (Grundfläche in Tiles = `(x: T, y: B)`).
* Figuren/Bäume: Anker an den Füßen/am Stamm `(0.5, 1.0)`.
* **Licht kommt von oben links** (für alle Atlanten): nach links unten (+y) zeigende Wände sind
  hell, nach rechts unten (+x) zeigende Wände im Schatten, Dachflächen nach links hell;
  Schlagschatten fallen nach rechts. Objekte haben eine dunkle 1-px-Kontur, Bodenplatten nicht.

## Namenskonventionen

| Atlas | Sprite-Namen |
|---|---|
| `terrain` | `<terrain>_<0..3>`, `cliff_<terrain>`, `<terrain>_edge_<ne\|se\|sw\|nw>`, `rim_<ne\|nw>`, `ao_<ne\|nw>` |
| `overlay` | `tile_outline`, `tile_fill`, `tile_solid`, `grid`, `soft_dot`, `blob_shadow` (weicher Bodenschatten unter Läufern, Tieren und Bäumen; vom Spiel nach Tageslicht abgeblendet) |
| `nature` | `<oak\|pine\|birch\|willow>_<spring\|summer\|autumn\|winter>_<sapling\|young\|mature>`, `stump`, `bush_<season>`, `boulder_<0..2>`, `deposit_<stone\|iron\|coal\|clay\|sand\|gold\|fish>` |
| `buildings` | `<building_id>` (fertig), `construction_<B>x<T>_<0..2>` (Baustellenstufen), `rubble_<B>x<T>`, `<building_id>_work_<0..3>` (Arbeitsschleife), `<building_id>_winter` (Winterbild) |
| `roads` | `<path\|gravel\|road\|paved\|trade_road>_<0..15>` (Bitmaske N=1 E=2 S=4 W=8 in Tile-Richtungen −y, +x, +y, −x), `bridge_<x\|y>`, `wall_<0..15>`, `gate_<x\|y>` |
| `fields` | `soil`, `<wheat\|barley\|vegetables>_<0..4>` (Wachstumsstufen), `orchard_<0..4>`, `orchard_bare` (Winter), `pasture` |
| `characters` | `<role>_<ne\|se\|sw\|nw>_<0..3>` (Laufanimation), Rollen: `man`, `woman`, `child`, `carrier`, `builder`, `farmer`, `soldier`, `archer`, `knight`, `merchant`, `cart`, `siege`, `raider`, `cow`, `sheep`, `chicken`; `<role>_<dir>_work_<0..3>` (`builder`, `farmer`, `man`, `woman`), `<role>_<dir>_<pickaxe\|shovel\|rod>_<0..3>` (Sammler: `man`, `woman`), `<role>_<dir>_attack_<0..3>` (`soldier`, `archer`, `knight`, `raider`, `siege`); dazu `banner` |
| `effects` | `smoke_<0..5>`, `fire_<0..3>`, `dust_<0..2>`, `rain`, `snowflake`, `sparkle`, `leaf_<0..3>`, `arrow_<0..15>`, `stone` |
| `icons` | `res_<resource_id>` (32 px), `bld_<building_id>` (48 px), `ui_<name>` (32 px), `crest_<0..6>` (40 × 48 px) |
| `ui` | Neun-Segment-Rahmen `parchment`, `wood`, `gold_frame`, Knöpfe `button_<normal\|hover\|pressed\|disabled>` |
| `lights` | `<building_id>` (Nachtlicht der Fenster, nur Gebäude mit Fenstern) |

### Details je Atlas

**nature**
* Bäume: Stamm-Fußpunkt = Anker `(0.5, 1.0)`. Ausgewachsen ca. Eiche 54 × 72, Kiefer 38 × 82,
  Birke 38 × 70, Weide 62 × 66 px; `young` ≈ 64 %, `sapling` ≈ 34 % davon. Die Silhouette ist in
  allen Jahreszeiten gleich (nur Farben/Laub ändern sich): Frühling hellgrün, Sommer satt grün,
  Herbst warm (Eiche orange-braun, Birke gelb, Weide gelbgrün), Winter kahl mit Schnee
  (Laubbäume) bzw. verschneite Kiefer.
* `stump` 18 × 14, `bush_<season>` 28 × 20, `boulder_<0..2>` (mittel, klein mit Moos, groß mit
  Nebenstein); Anker `(0.5, 1.0)`.
* `deposit_<kind>`: flache Bodenmarkierung auf einer 64 × 32-Raute, Anker = Tile-Mitte
  `(0.5, 0.5)` (wie Overlays): Gesteinsbrocken, rostrote Erzadern (iron), schwarze Kohleflöze,
  rotbraune nasse Tonflecken, Sandrippel, Goldflitter mit Funkeln, Fisch-Wellenringe mit
  Fischsilhouetten (auf Wasser).

**buildings**
* Ein fertiges Sprite je Gebäude-ID aus `docs/game_design.md` §5 (82 IDs, davon 25
  Ausbaustufen: `large_sawmill`, `master_smithy`, `large_mill`, `large_bakery` als größere
  Fassungen ihrer Werkstatt – Steinsockel, zweiter Ofen/Esse, Galerie, Verkaufstresen – und
  `stone_tower`, ein gemauerter Wehrturm mit Zinnen und Banner; seit Sim-Lane Schritt 8 dazu
  `lumber_camp` (Blockhaus auf Steinsockel, überdachter Sägebock), `large_quarry` (tiefere
  Abbaustufen, Tretradkran, Bauhütte), `blast_furnace` (Steinstock mit Hochofen, flackerndes
  Maul als Arbeitsschleife), `large_brickworks` (Ringofen mit zwei Schloten), `large_pottery`
  (zwei Brennöfen, Regal mit Ware), `large_weaver`/`large_tailor` (zweistöckig mit Steinsockel,
  Laden), `large_tannery` (Trockenboden, mehr Gerbgruben), `large_carpenter` (Werkhalle mit Tor),
  `large_brewery` (Malzturm, zwei Kupferkessel, Fasskarren), `large_fishery` (Räucherhaus,
  Steg, zwei Boote), `large_apiary` (Bienenhaus mit Kastenbeuten), `grand_market` (Markthalle auf
  Steinpfeilern, Brunnen), `cathedral` (Doppelturm, Querhaus, Rosette), `academy` (Bibliotheksflügel,
  Sternwartenkuppel, Kreuzgarten), `infirmary` (Krankensaal, Kapelle, ummauerter Kräutergarten),
  `inn` (dreistöckig, Gaststall, Biergarten), `garrison` (Torturm mit Zinnen, ummauerter Hof),
  `archer_tower` (Steinturm über der Palisade), `stud_farm` (Steinstall, Reitbahn mit zwei
  Pferden)). Icons (`icons/bld_<id>`) und Nachtlichter (`lights/<id>`) entstehen wie bei allen
  Gebäuden aus dem Sprite. `wall` und `gate` sind
  zusätzlich als Aliase vorhanden (= `roads/wall_5` bzw. `roads/gate_x`), damit ein generischer
  Zugriff `buildings/<id>` nie den Platzhalter liefert; die auto-verbundenen Segmente kommen aus
  dem `roads`-Atlas.
* Jedes Gebäude steht auf einer Bodenplatte (Gras/Erde/Pflaster) der vollen Grundfläche (0,04
  Tiles eingerückt) mit Schlagschatten nach rechts.
* **Arbeitsschleifen** `<id>_work_<0..3>` mit Animation `<id>_work` (spielt das Spiel, solange
  die Produktion läuft): `mill`, `large_mill` (die Flügel drehen sich je Bild um 22,5°; vier
  Flügel, also nahtlos; 3 fps), `smithy`, `master_smithy` (glühende Esse mit Feuermaul vorn und
  zur offenen Seite des Schleppdachs, damit das Feuer unter der Traufe sichtbar bleibt, und
  Funken, flackernd), `weaponsmith` (Esse im Hof mit Funken), `smelter` (Ofenmaul; je 6 fps).
  Feuerfarben je Bild: Bild 0 Glut `#f08a2a` (das fertige Sprite), dann Aufflackern `#ffc24a`,
  Glühen `#f59a30`, Glimmen `#c8581a` – weit genug auseinander, um auch herausgezoomt zu
  flackern. Bild 0 ist das fertige Sprite; alle Bilder haben dessen Breite und
  Anker, die Höhe darf wechseln (gestutzte Flügel).
* **Winterbild** `<id>_winter` (gleiche Grundfläche und Anker): `orchard` mit kahlen,
  angeschneiten Obstbäumen und leeren Körben; das Spiel zeigt es im Winter.
* **Schneemarke (Alpha 254):** In `buildings` und bei `wall_*`/`gate_*` im `roads`-Atlas tragen
  die Pixel, auf denen Schnee liegen bleibt, Alpha **254** statt 255 – Dachflächen, Kegeldächer
  und flache Oberseiten (Zinnen, Kisten, Fundamente). Der Generator markiert jede Fläche, deren
  Normale genug nach oben zeigt (`IsoCanvas::with_snow_mask`, Steigung bis etwa 72°; steile
  Turmhelme bleiben frei), außer Wänden, warmen Flächen (Brenn- und Backöfen, Meiler, Essen,
  Hochofen, Schornsteine, Brunnenschacht: `IsoCanvas::snow_free`) und 2D-Details, die danach
  darübergemalt wurden (Schilder, Waren, Traufkanten). Das Spiel zeichnet Gebäude mit
  `FLAG_SNOW_ON_ROOF`: Der Shader legt auf markierte Pixel die Schneedecke der Umgebung und
  zeichnet sie voll deckend. Echte Grafiken setzen diese Marke genauso (oder lassen sie weg –
  dann bleibt das Dach schneefrei). Symbole (`bld_<id>`) und das App-Symbol entfernen die Marke.
* **Schattenmarke (Alpha 253):** Den Schlagschatten der Gebäude auf ihre Bodenplatte malt der
  Generator als `#1a1208` mit Deckkraft 72/255 über den Boden (sRGB, `IsoCanvas::shadow`;
  überlappende Schatten dunkeln nur einmal) und gibt jedem Pixel, das danach noch diesen
  Schatten zeigt, Alpha **253** (in `buildings` und bei `wall_*`/`gate_*`; Farben unverändert).
  Das Spiel zeichnet Gebäude mit `FLAG_SHADOW_MARK`: Der Shader nimmt genau diese Mischung
  zurück und mischt mit der Stärke der weichen Bodenschatten neu – voll in der Sonne, schwach
  bei Wolken und nachts, wie die Schatten unter Läufern und Bäumen (Bäume haben keine gemalten
  Schatten). Echte Grafiken malen Schlagschatten in derselben Farbe und Deckkraft und setzen
  die Marke (oder lassen sie weg – dann bleibt der Schatten auch nachts gleich dunkel). Die
  Werte 253 und 254 sind reserviert; Symbole und das App-Symbol entfernen beide Marken.
* Baustellen für jede verwendete Grundfläche `1x1, 2x2, 2x3, 3x3, 3x4, 4x4, 5x5`:
  `_0` abgesteckt (Pflöcke, Schnur, Material), `_1` Steinfundament + hintere Pfosten,
  `_2` komplettes Fachwerk mit Dachsparren, Gerüst und Leiter. `rubble_<B>x<T>`: Asche,
  Steinbrocken, verkohlte Balken, Mauerrest.

**roads**
* Straßen-Tiles: 64 × 32, Anker `(0.5, 0.5)`. Jedes gesetzte Bit fügt einen Streifen von der
  Tile-Mitte zum **Mittelpunkt** der entsprechenden Rautenkante hinzu (N = obere rechte Kante,
  E = untere rechte, S = untere linke, W = obere linke); Maske 0 = einzelner runder Fleck.
  Breiten (halbe Breite in Tiles): path 0,19 (unregelmäßiger Rand), gravel 0,22, road 0,25
  (Spurrillen, Randsteine), paved 0,27 (Kopfsteinpflaster), trade_road 0,33 (große Platten mit
  Bordstein). Texturen sind je Tile periodisch → nahtlos.
* `bridge_x` / `bridge_y`: Holzbrücken-Segment mit Fahrbahn entlang der Tile-x- bzw. y-Achse
  (Pfähle, Geländer), `wall_<mask>`: Stadtmauer mit derselben Bitmaske (Turm an Ecken, Enden und
  Kreuzungen, gerade Stücke 5/10 ohne Turm), `gate_x` / `gate_y`: Torhaus in einer Mauer entlang
  der Tile-x- bzw. y-Achse (Durchgang quer dazu). Diese aufrechten Sprites folgen der
  Gebäudekonvention für 1×1: 64 px breit, Unterkante = vordere Tile-Ecke, Anker `(0.5, 1.0)`.

**fields**
* Alle Sprites 64 × 64; die Tile-Raute liegt in der unteren Hälfte (Zeilen 32–64), darüber
  ragen Pflanzen/Bäume. Anker = Tile-Mitte `(0.5, 0.75)`. Furchen/Pflanzreihen verlaufen entlang
  der Tile-x-Achse und sind je Tile periodisch.
* `wheat_<0..4>`/`barley_<0..4>`: gesät → Keimlinge → grün → reifend → reif (Weizen goldgelb,
  Gerste blassgold mit hängenden Grannen); `vegetables_<0..4>`: Kohl- und Möhrenreihen;
  `orchard_<0..4>`: Setzling mit Stütze → jung → belaubt → Blüte → Früchte; `orchard_bare`:
  dieselben Bäume wie Stufe 2 ohne Laub und Früchte, mit etwas Schnee auf den Ästen (Winter,
  für alle Stufen ab 1); `pasture`: Weide.
* Im Spiel (`src/view/fields.rs`) liegen Weide und Brache flach auf dem Boden; Feldfrüchte und
  Obstbäume sind „Bodenbewuchs“: alles über der Raute wird wie ein aufrechtes Objekt im hinteren
  Viertel des Tiles sortiert. Bei Schnee werden die Raute und grüne Pflanzenpixel weiß. Im Wind
  schert der Shader die Pixel über der Raute nach ihrer Höhe (die Raute bleibt stehen) – Pflanzen
  sollten deshalb nicht bis an den seitlichen Rand des Sprites reichen.

**characters**
* Menschen 16 × 24, `cow` 26 × 18, `sheep` 20 × 15, `chicken` 12 × 12, alle mit Anker
  `(0.5, 1.0)`. `cart` (Karren mit Esel) 56 × 44, Anker = Bodenmittelpunkt des Karrens
  `(0.5, 34/44)`.
* Richtungen: `se` = +x, `sw` = +y, `nw` = −x, `ne` = −y (Bildschirmdiagonalen);
  `se`/`sw` zeigen die Vorderseite, `ne`/`nw` den Rücken; `sw`/`nw` sind Spiegelungen.
* Animationen `<role>_<dir>_walk`: 4 Frames, 8 fps, loopend. Frame 1 (Beine geschlossen) dient im
  Spiel als Standbild; das Spiel wählt das Laufbild nach dem zurückgelegten Weg (ein Bild je
  0,5 Tiles, bei Grundtempo also 8 fps) und liest die Bilder über die Animation.
* Arbeits- und Kampfschleifen `<role>_<dir>_work` bzw. `_attack`: 4 Frames, 6 fps, loopend; das
  Spiel spielt sie in Spielzeit, solange der Läufer an seiner Arbeitsstelle steht bzw. in der
  laufenden Kampfrunde zuschlägt oder schießt. Menschen 32 × 24 (Figur mittig wie im Laufbild,
  Anker `(0.5, 1.0)`, Platz für das Werkzeug), feste Fußstellung, Werkzeug im Bogen: `builder`
  Hammer (ausholen, über Kopf, Schlag, nachschwingen), `farmer` Hacke bis in den Boden, `man`
  und `woman` Axt mit beiden Händen (Holzfäller und jeder Sammler ohne eigenes Werkzeug), `raider` Axt, `knight`
  Schwerthieb, `soldier` Speerstoß (zurück, Stoß, halten, zurück; Schild bleibt), `archer`
  Bogen spannen (Pfeil eingelegt, halb, voll, abgeschossen). `siege` (56 × 44 wie das Laufbild):
  Wurfarm gespannt mit Stein, schwingt hoch, wirft nach vorn (leer), fällt zurück.
* **Sammler-Schleifen** von `man` und `woman` neben der Axt (`characters::Gather`, gleiche Figur,
  32 × 24, Anker `(0.5, 1.0)`, 6 fps): `<role>_<dir>_pickaxe` Spitzhacke mit beiden Händen, die
  Spitze schlägt vor den Füßen in den Fels, im Schlagbild fliegen Steinsplitter (Steinbruch;
  ebenso Minen, deren Vorkommen aber direkt am Gebäude liegt – dort arbeitet man drinnen); `_shovel` Spaten wird angesetzt, in den Boden gedrückt, mit einem Erdklumpen gehoben und
  ausgeworfen (Lehm- und Sandgrube, Förster beim Pflanzen); `_rod` Angelrute über die Schulter,
  Auswurf (die Schnur fliegt), gehalten mit Schwimmer (rot-weiß) und ein Biss (Spitze und
  Schwimmer tauchen ab, Wellenringe) – die Animation wirft einmal aus und hält dann (Bildfolge
  0, 1, 2, 2, 3, 2, 2, 3); von hinten (`ne`, `nw`) liegt der Schwimmer höher, also weiter weg. Die
  Schnur ist halb durchsichtig und bekommt keine Kontur. Das Spiel wählt die Schleife nach dem
  Sammeln des Arbeitsplatzes (`src/view/citizens.rs`, `GatherTool`): Bäume → Axt (`_work`),
  Vorkommen Stein/Eisen/Kohle/Gold → `_pickaxe`, Lehm/Sand und Pflanzen → `_shovel`, Fisch →
  `_rod`; fehlt eine Schleife im Atlas, schwingt der Sammler die Axt.
* `siege` (Belagerungsgerät) 56 × 44 wie `cart`, Anker `(0.5, 34/44)`: Katapult auf einem
  Holzrahmen mit Rädern (Speichen drehen sich mit dem Laufbild), gespanntem Wurfarm mit Stein und
  Gegengewicht vorn. `raider` (Räuber) 16 × 24: karminrote Kapuze, rostbraunes Leder, knochenweiße
  Schärpe, Axt – bewusst anders als jeder Bewohner und hell genug für dunklen Boden; das Spiel
  stellt Räuber auf einen knochenweißen Ring (`theme::BANDIT_COLOR`).
* `banner` 14 × 34, Anker `(2.5/14, 1.0)` (Fuß der Stange): dunkle Stange mit weißem,
  geschwalbtem Tuch. Das Spiel tönt es in der Reichsfarbe (Heere, Karawanen, Burgen und
  Wachtürme der Reiche) bzw. gold (eigene Kompanien); die Stange bleibt dabei dunkel.
* Die Läufer der Simulation verwenden die Rollen `man`, `woman`, `child`, `carrier`, `builder`,
  `farmer`, `soldier`, `archer`, `knight`, `siege`, `raider`, `merchant`, `cart`; getragene Waren
  zeigt das Spiel mit dem Symbol `icons/res_<id>` (10 px) in der Hand.
* **Keine gemalten Menschen in Gebäuden:** Gebäude-Sprites enthalten keine Figuren, die wie
  anklickbare Bewohner aussehen (die echten Läufer zeichnet das Spiel). Wo früher Leute standen,
  stehen Requisiten (Säcke, Kisten, Fässer, Übungspuppen, Heu, ein Wimpel auf dem
  Bogenschützenposten). `props::animal` malt nur Tiere (Kühe, Schafe, Hühner) – das Werkzeug
  nimmt dafür ein Enum, Menschen sind so gar nicht möglich; die Vogelscheuche bleibt.

**effects** (Smooth)
* `smoke_<0..5>` 24 × 24, Anker `(0.5, 0.5)`, wachsend und verblassend; Animation `smoke`
  (6 fps, loop). `fire_<0..3>` 16 × 24, Anker `(0.5, 1.0)`; Animation `fire` (10 fps, loop).
  `dust_<0..2>` 24 × 16, Anker `(0.5, 1.0)`; Animation `dust` (10 fps, einmalig).
  `rain` 4 × 12 (Streifen von rechts oben nach links unten, passend zum Wind nach links; das
  Spiel spiegelt ihn bei Wind nach rechts),
  `snowflake` 8 × 8, `sparkle` 12 × 12, Anker `(0.5, 0.5)`.
  `leaf_<0..3>` 8 × 8, Anker `(0.5, 0.5)`: fallende Herbstblätter (orange, gelb, rostbraun, rot),
  schräg liegende Ellipse mit dunkler Mittelrippe.
  `arrow_<k>` 16 × 16, Anker `(0.5, 0.5)`: Pfeil im Flug, zeigt `k · 22,5°` im Uhrzeigersinn
  von „rechts“ (dunkler Schaft, Stahlspitze, helle Befiederung); `stone` 8 × 8: Wurfstein der
  Belagerungsgeräte.
* Verwendung im Spiel (`src/view/effects.rs`, `src/view/projectiles.rs`): Rauch über die
  Lebenszeit `smoke_0..5` (dunkler getönt), Staub `dust_0..2`, Regen/Schnee als
  Bildschirmpartikel (der Regenstreifen wird bei Wind nach rechts gespiegelt), Laub zufällig,
  Pfeile und Steine der letzten Kampfrunde vom Schützen zum Ziel (Pfeilbild nach Flugrichtung).

## App-Symbol (`assets/app_icon.png`)

1024 × 1024 px, vom Generator neben den Atlanten geschrieben (`tools/assetgen/src/app_icon.rs`):
die Burg des `buildings`-Atlas doppelt so groß (Pixel für Pixel) auf einem grünen Hügel vor einem
königsblauen Verlauf mit Lichthof, als Quadrat mit runden Ecken (824 px, Radius 185 px, Rand
100 px nach Apples Symbolraster) mit Goldrand; außen transparent. `tools/macos/make_app.sh` macht
daraus mit `sips` und `iconutil` `Kingdoms.icns` (16–1024 px). Ein echtes Symbol ersetzt die
Datei bei gleicher Größe.

## Emitter-Punkte (`assets/atlases/emitters.ron`)

Punkte, an denen Effekte ein Sprite verlassen – zurzeit Rauch (Schornsteinkopf, Rauchloch im
Strohdach, Abzug von Meiler, Ofen und Brennofen). Die Datei wird vom Generator zusammen mit den
Atlanten geschrieben (`IsoCanvas::mark_smoke`, Schornsteine aus `props::chimney` automatisch);
für echte Grafiken wird sie von Hand gepflegt:

```ron
EmitterPoints(
    smoke: {
        "buildings/bakery": [(34.5, -65.0)],   // "<atlas>/<sprite>": [(x, y), …]
    },
)
```

* Koordinaten in Pixeln relativ zum **Anker** des Sprites (x nach rechts, y nach unten, also
  negativ = über dem Anker), für das **ungespiegelte** Sprite; das Spiel spiegelt x mit dem
  Sprite (`FLAG_FLIP_X`). Weil der Anker die vordere Grundflächen-Ecke ist, ändert das Beschneiden
  leerer Zeilen die Werte nicht.
* Gebäude ohne Eintrag rauchen nie. Tests: Generator (jeder Punkt liegt auf Sprite-Pixeln,
  Round-Trip der RON-Datei, nur Gebäude-Sprites), Spiel (die ausgelieferte Datei lässt sich
  lesen und nennt nur vorhandene Gebäude-Sprites).

**icons** (Smooth, Anker `(0.5, 0.5)`)
* `res_<id>` 32 × 32 für alle 38 Ressourcen aus §4 (Piktogramme; `res_armor`: Brustpanzer mit
  Schulterstücken und genietetem Gurt).
* `bld_<id>` 48 × 48 für alle Gebäude-IDs aus §5: verkleinertes Gebäude-Sprite auf einem
  Kategorie-Hintergrund (Wohnen grün, Produktion braun, Landwirtschaft gold, Öffentlich blau,
  Militär rot).
* `ui_<name>` 32 × 32: `population`, `gold`, `happiness`, `food`, `wood`, `stone`, `iron`, `date`,
  `season_spring`, `season_summer`, `season_autumn`, `season_winter`, `pause`, `play`, `speed2`,
  `speed4`, `build`, `roads`, `farm`, `production`, `military`, `research`, `events`,
  `notifications`, `tasks`, `overview`, `demolish`, `upgrade`, `rotate`, `move`, `info`, `close`,
  `warning`, `trade`, `diplomacy`, `finance`, `statistics`, `storage`, `settings`, `save`, `load`,
  `inspect` (Lupe über Mauszeiger: Auswahlwerkzeug), `grid` (Raute mit Raster), `territory`
  (Raute mit goldener Grenze und Fahne), `economy` (Sack und Kiste mit Pfeilen auf/ab:
  Reiter Wirtschaft), `medicine` (Mörser mit Kraut: Forschungsbereich Medizin), `administration`
  (Urkunde mit Siegel und Feder: Forschungsbereich Verwaltung), `rank` (Königskrone mit
  Edelsteinen: Herrscherrang, Panel „Aufgaben“). Die übrigen Forschungsbereiche
  nutzen `ui_farm`, `ui_build`, `ui_production`, `res_iron_ore`, `ui_trade` und `ui_military`.

* `crest_<i>` 40 × 48: Wappen des Königreichs mit Farbindex `i` (0–6) – Dreieckschild mit
  Goldrand im Feld der Reichsfarbe (`palette::KINGDOM_COLORS`: `#b02630` karmin, `#2c56ac`
  königsblau, `#307c40` waldgrün, `#763a92` purpur, `#d47c22` ocker, `#18848e` petrol,
  `#c44884` rosé; dieselben Werte nutzt das Spiel in `theme::KINGDOM_COLORS`, ein Test vergleicht
  beide) und einer silbernen Figur je Index: Turm, Stern, Sparren, Krone, Kreuz, Mondsichel, drei
  Kugeln.

**lights** (Pixel)
* Ein Sprite je Gebäude-ID mit Fenstern (29 von 57): die beim Zeichnen mit `IsoCanvas::window`
  bzw. `IsoCanvas::pane` (Glasflächen in eigener Farbe, z. B. die Kirchenfenster) erfassten
  Scheiben, soweit sie im fertigen Sprite noch sichtbar sind (Pixel in der Glasfarbe unter dem
  jeweiligen Flächenlicht), in warmem Licht (unten heller `#ffd88a`, oben `#f6a948`) mit einem
  weichen Schein von 3 px (`#ffb35a`, Deckkraft 120/56/20). Auf die beleuchteten Pixel
  zugeschnitten; der Anker ist der Anker des Gebäude-Sprites (vordere Grundflächen-Ecke) im
  Ausschnitt und liegt deshalb meist außerhalb von `0..1`. Am Gebäudeanker gezeichnet (gespiegelt
  wie das Gebäude) liegen die Lichter genau auf den Fenstern.
* Im Spiel (`src/view/lights.rs`) transparent und emissiv (unabhängig vom Umgebungslicht), mit der
  Tiefe der Wände darunter; Deckkraft nach Tageszeit. Test (Generator): jedes voll deckende
  Lichtpixel liegt über einer Glasscheibe des Gebäude-Sprites.

**ui** (Smooth, Neun-Segment-Ränder)
* `parchment`, `wood`, `gold_frame`: 96 × 96 px, Rand **24 px** auf allen Seiten.
  `parchment` und `wood` sind gefüllte Paneele, `gold_frame` hat eine transparente Mitte.
* `button_normal|hover|pressed|disabled`: 96 × 32 px, Rand **12 px** auf allen Seiten.
* Die Ränder stehen zusätzlich im `source`-Feld von `ui.ron`.

## Audio

`assets/audio/manifest.ron` ordnet Sound-IDs Dateien zu und weist Kanäle zu (`Music`,
`Ambience`, `Effects`, `Ui`, `Voice`; Ordner `music/`, `ambience/`, `sfx/`, `ui/`, `voice/`).
Das Manifest nennt `.ogg`; gefunden wird jede Datei über ihren **Stamm** mit einer der Endungen
`.ogg`, `.mp3`, `.wav`, `.flac` (Groß-/Kleinschreibung, Leer- und Sonderzeichen egal, die
genannte Endung zuerst). Stimmen tragen in `text` den genauen Wortlaut je Datei, `description`
beschreibt Klang bzw. Vortrag (englisch, als Prompt für Suno/ElevenLabs geeignet). Mehrere
Dateien je ID sind Varianten, die reihum spielen. Fehlende Dateien sind erlaubt (stumm; Summe im
Log, Einzelheiten mit `KINGDOMS_DEBUG=1`). Die Lieferanleitung mit Formaten, Schleifen, Längen,
Lautheit und allen Dateinamen und Sprechertexten steht in `assets/audio/README.md` (aus dem
Manifest erzeugt); `KINGDOMS_AUDIO_REPORT=1 ./kingdoms` prüft eine Lieferung (Tabelle mit
gefundener Datei, Länge, Format, Spitze und Hinweisen; Rückgabewert 0 nur wenn vollständig).
