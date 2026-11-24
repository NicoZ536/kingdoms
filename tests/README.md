# Tests

Die Tests liegen bei dem Crate, das sie prüfen:

* `crates/sim/tests/` – Integrationstests der Simulation: Determinismus, Speichern und Laden,
  Befehls-Fuzzing mit manipulierten Spielständen, vernünftiger Spieler und
  Drei-Jahres-Kampagnen (Balance).
* Unit-Tests stehen in jedem Crate neben dem Code (`#[cfg(test)] mod tests`): Simulation,
  Engine (`crates/engine`), Spiel (`src/`, z. B. Darstellung, Oberfläche, Klänge, Spielstände)
  und Grafik-Generator (`tools/assetgen`).

Alles zusammen: `cargo test --workspace`. Ignorierte Leistungsmessungen laufen mit
`cargo test -p kingdoms_sim --release -- --ignored`.
