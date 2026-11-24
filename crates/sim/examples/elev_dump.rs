//! Prints an elevation grid excerpt: `cargo run -p kingdoms_sim --example elev_dump -- <seed> <x0> <y0> <w> <h>`
use glam::IVec2;
use kingdoms_sim::world::{GeneratorSettings, WorldSize, generate};
fn main() {
    let a: Vec<i32> = std::env::args().skip(1).map(|s| s.parse().unwrap()).collect();
    let map = generate(&GeneratorSettings::with_seed(a[0] as u64, WorldSize::Medium));
    for y in a[2]..a[2] + a[4] {
        let row: Vec<String> = (a[1]..a[1] + a[3]).map(|x| {
            let t = map.tile(IVec2::new(x, y));
            format!("{}{}", t.elevation, if t.is_water() { '~' } else { ' ' })
        }).collect();
        println!("y{y:3}: {}", row.join(""));
    }
}
