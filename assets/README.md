# Inhalte (`assets/`)

| Ordner | Inhalt |
|---|---|
| `atlases/` | Alle Grafiken als Sprite-Atlanten: je Atlas ein PNG und seine RON-Metadaten (Sprites, Anker, Animationen), dazu `index.ron` (Ladereihenfolge) und `emitters.ron` (Rauchpunkte). Erzeugt von `tools/assetgen`. |
| `audio/` | `manifest.ron` (alle Klang-IDs mit Kanal, Lautstärke, Abklingzeit, Beschreibung und bei Stimmen dem Wortlaut), die Unterordner `music/`, `ambience/`, `sfx/`, `ui/`, `voice/` für die Audiodateien und `README.md` mit der Lieferanleitung (Dateinamen, Formate, Schleifen, Lautheit, Sprechertexte, Prüfung mit `KINGDOMS_AUDIO_REPORT=1`) sowie `delivery.json`, die aus dem Manifest erzeugte Lieferliste aller IDs mit Dateien, empfohlenen Längen und Sprechertexten. |
| `fonts/` | Schriften (Cinzel, Alegreya) mit ihren Lizenzen. |
| `app_icon.png` | App-Symbol (1024 × 1024, Burg auf blauem, goldgerahmtem Quadrat mit runden Ecken; erzeugt von `tools/assetgen`). `tools/macos/make_app.sh` macht daraus `Kingdoms.icns`. |

Die in der Master-Spezifikation (§19) skizzierten Grafikordner `textures/`, `sprites/`,
`buildings/`, `terrain/`, `characters/` und `ui/` gibt es nicht als lose Dateien: jede dieser
Gruppen ist ein Atlas in `atlases/` (`terrain`, `nature`, `buildings`, `roads`, `fields`,
`characters`, `effects`, `icons`, `ui`, `lights`, `overlay`), damit der Renderer je Gruppe nur
eine Textur bindet. Namenskonventionen, Geometrie und Animationen beschreibt
`docs/assets.md`; echte Grafiken ersetzen die PNGs bei gleichen Sprite-Namen und Ankern.
