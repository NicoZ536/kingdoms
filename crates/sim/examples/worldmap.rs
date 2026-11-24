//! Writes a top-down overview of a generated world as a PPM image.
//! Usage: `cargo run -p kingdoms_sim --example worldmap -- <seed> <small|medium|large|huge> <out.ppm>`
use std::io::Write;

use kingdoms_sim::world::{GeneratorSettings, Terrain, Vegetation, WorldSize, generate};

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let seed = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let size = match args.get(2).map(String::as_str) {
        Some("small") => WorldSize::Small,
        Some("large") => WorldSize::Large,
        Some("huge") => WorldSize::Huge,
        _ => WorldSize::Medium,
    };
    let out = args.get(3).cloned().unwrap_or_else(|| "world.ppm".into());
    let map = generate(&GeneratorSettings::with_seed(seed, size));
    let mut f = std::io::BufWriter::new(std::fs::File::create(&out)?);
    write!(f, "P6\n{} {}\n255\n", map.width(), map.height())?;
    for p in map.positions() {
        let t = map.tile(p);
        let mut c: [f32; 3] = match t.terrain {
            Terrain::DeepWater => [36.0, 85.0, 127.0],
            Terrain::ShallowWater => [59.0, 134.0, 176.0],
            Terrain::Sand => [220.0, 198.0, 142.0],
            Terrain::Swamp => [88.0, 104.0, 58.0],
            Terrain::Steppe => [184.0, 167.0, 102.0],
            Terrain::Dirt => [143.0, 108.0, 69.0],
            Terrain::Grass => [110.0, 154.0, 63.0],
            Terrain::Meadow => [120.0, 168.0, 63.0],
            Terrain::Forest => [77.0, 117.0, 49.0],
            Terrain::Rock => [140.0, 135.0, 129.0],
            Terrain::Snow => [233.0, 239.0, 243.0],
        };
        if t.vegetation.is_tree() { c = [c[0] * 0.55, c[1] * 0.7, c[2] * 0.55]; }
        if t.vegetation == Vegetation::Boulder { c = [90.0, 90.0, 90.0]; }
        if t.deposit.is_some() && !t.is_water() { c = [200.0, 60.0, 60.0]; }
        let shade = 0.8 + f32::from(t.elevation) * 0.04;
        if (p - map.start).abs().max_element() <= 1 { c = [255.0, 0.0, 255.0]; }
        f.write_all(&[(c[0] * shade).min(255.0) as u8, (c[1] * shade).min(255.0) as u8, (c[2] * shade).min(255.0) as u8])?;
    }
    println!("wrote {out}: start {:?}, sites {:?}", map.start, map.settlement_sites);
    Ok(())
}
