//! Asset management: texture atlases described by RON metadata, sprite & animation registry,
//! caching and graceful fallbacks for missing assets.
//!
//! See `docs/assets.md` for the metadata schema. In short, `assets/atlases/index.ron` lists the
//! atlas files; each atlas file names a PNG and the sprite rectangles / anchors / animations in it.
//! Sprites are addressed as `"<atlas>/<sprite>"`, e.g. `"terrain/grass_0"`.

mod atlas;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};

use glam::Vec2;

pub use atlas::{AnimationDef, AtlasDef, AtlasIndex, SpriteDef};

use crate::rendering::Renderer;
use crate::rendering::texture::{Sampling, TextureId};

/// Handle to a registered sprite region.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpriteId(pub u32);

/// Handle to a registered animation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AnimationId(pub u32);

/// A sprite: a rectangle in an atlas texture plus its pivot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteRegion {
    pub texture: TextureId,
    /// `[u0, v0, u1, v1]`
    pub uv: [f32; 4],
    /// Size in pixels (= world pixels at zoom 1).
    pub size: Vec2,
    /// Normalised pivot inside the sprite.
    pub anchor: Vec2,
}

#[derive(Clone, Debug)]
pub struct Animation {
    pub frames: Vec<SpriteId>,
    pub fps: f32,
    pub looping: bool,
}

impl Animation {
    /// Frame to show `t` seconds after the animation started.
    pub fn frame_at(&self, t: f32) -> SpriteId {
        if self.frames.is_empty() {
            return SpriteId(0);
        }
        let n = self.frames.len();
        let i = (t.max(0.0) * self.fps) as usize;
        let i = if self.looping { i % n } else { i.min(n - 1) };
        self.frames[i]
    }

    pub fn duration(&self) -> f32 {
        if self.fps > 0.0 { self.frames.len() as f32 / self.fps } else { 0.0 }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AssetError {
    #[error("could not read {path}: {source}")]
    Io { path: String, source: std::io::Error },
    #[error("invalid metadata in {path}: {source}")]
    Metadata { path: String, source: Box<ron::error::SpannedError> },
    #[error("could not decode image {path}: {source}")]
    Image { path: String, source: Box<image::ImageError> },
    #[error("atlas {atlas}: {message}")]
    Invalid { atlas: String, message: String },
}

/// Decoded atlas waiting for GPU upload.
struct DecodedAtlas {
    name: String,
    def: AtlasDef,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

type DecodeResult = Result<DecodedAtlas, AssetError>;

/// Atlases being decoded on worker threads (see [`AssetManager::start_loading_atlases`]).
/// Dropping it stops the workers after the atlas each one is decoding.
pub struct AtlasLoading {
    names: Vec<String>,
    receiver: Option<mpsc::Receiver<(usize, DecodeResult)>>,
    /// Decoded atlases waiting for their turn (registration follows the index order).
    ready: Vec<Option<DecodeResult>>,
    /// Index of the next atlas to register.
    next: usize,
    loaded: usize,
    started: std::time::Instant,
    reported: bool,
}

impl AtlasLoading {
    fn spawn(root: PathBuf, names: Vec<String>, max_size: u32) -> Self {
        let total = names.len();
        let (sender, receiver) = mpsc::channel();
        let jobs = Arc::new(names.clone());
        let cursor = Arc::new(AtomicUsize::new(0));
        let workers = std::thread::available_parallelism().map_or(2, |n| n.get()).clamp(1, 4).min(total.max(1));
        for worker in 0..workers {
            let (root, jobs, cursor, sender) = (root.clone(), jobs.clone(), cursor.clone(), sender.clone());
            let spawned = std::thread::Builder::new().name(format!("atlas-decoder-{worker}")).spawn(move || {
                loop {
                    let i = cursor.fetch_add(1, Ordering::Relaxed);
                    let Some(name) = jobs.get(i) else { break };
                    if sender.send((i, decode_atlas(&root, name, max_size))).is_err() {
                        break;
                    }
                }
            });
            if let Err(err) = spawned {
                log::error!("cannot start an atlas decoder thread: {err}");
            }
        }
        Self {
            names,
            receiver: Some(receiver),
            ready: (0..total).map(|_| None).collect(),
            next: 0,
            loaded: 0,
            started: std::time::Instant::now(),
            reported: false,
        }
    }

    /// Takes decoded atlases from the workers; with `wait`, blocks until the next one in
    /// order has arrived (or every worker is gone).
    fn receive(&mut self, wait: bool) {
        let Some(receiver) = self.receiver.as_ref() else { return };
        loop {
            let next_ready = self.ready.get(self.next).is_none_or(Option::is_some);
            let message = if wait && !next_ready {
                receiver.recv().map_err(|_| mpsc::TryRecvError::Disconnected)
            } else {
                receiver.try_recv()
            };
            match message {
                Ok((i, result)) => {
                    if let Some(slot) = self.ready.get_mut(i) {
                        *slot = Some(result);
                    }
                }
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => {
                    // Every worker ended: whatever never arrived failed (a panicked decoder).
                    self.receiver = None;
                    for (i, slot) in self.ready.iter_mut().enumerate().skip(self.next) {
                        if slot.is_none() {
                            let atlas = self.names.get(i).cloned().unwrap_or_default();
                            *slot = Some(Err(AssetError::Invalid { atlas, message: "decoder thread ended unexpectedly".into() }));
                        }
                    }
                    return;
                }
            }
        }
    }

    /// The next atlas in index order, if it has been decoded.
    fn take_next(&mut self) -> Option<DecodeResult> {
        let result = self.ready.get_mut(self.next)?.take()?;
        self.next += 1;
        Some(result)
    }

    /// Number of atlases in the index.
    pub fn total(&self) -> usize {
        self.names.len()
    }

    /// Atlases registered or failed so far.
    pub fn done(&self) -> usize {
        self.next
    }

    /// Atlases registered successfully so far.
    pub fn loaded(&self) -> usize {
        self.loaded
    }

    /// Fraction done, 0–1 (1 for an empty index).
    pub fn progress(&self) -> f32 {
        if self.names.is_empty() { 1.0 } else { self.next as f32 / self.names.len() as f32 }
    }

    /// Name of the atlas registered next, while loading.
    pub fn current(&self) -> Option<&str> {
        self.names.get(self.next).map(String::as_str)
    }

    pub fn is_finished(&self) -> bool {
        self.next >= self.names.len()
    }
}

/// Opaque pixels of an atlas image (alpha ≥ 128), one bit per pixel, kept on the CPU so the
/// game can hit-test sprites against what is actually drawn.
#[derive(Clone, Debug, Default)]
pub struct AlphaMask {
    width: u32,
    height: u32,
    bits: Vec<u64>,
}

impl AlphaMask {
    /// Builds the mask of an RGBA8 image (`rgba.len()` must be `width × height × 4`; missing
    /// pixels count as transparent).
    pub fn from_rgba(width: u32, height: u32, rgba: &[u8]) -> Self {
        let count = width as usize * height as usize;
        let mut bits = vec![0u64; count.div_ceil(64)];
        for (i, px) in rgba.chunks_exact(4).take(count).enumerate() {
            if px[3] >= 128 {
                bits[i / 64] |= 1 << (i % 64);
            }
        }
        Self { width, height, bits }
    }

    /// True if pixel `(x, y)` is opaque (false outside the image).
    pub fn opaque(&self, x: u32, y: u32) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        let i = y as usize * self.width as usize + x as usize;
        self.bits.get(i / 64).is_some_and(|w| w & (1 << (i % 64)) != 0)
    }

    /// True if the pixel at normalised texture coordinate `uv` (0..1 over the whole image) is
    /// opaque.
    pub fn opaque_at_uv(&self, uv: Vec2) -> bool {
        if !(0.0..1.0).contains(&uv.x) || !(0.0..1.0).contains(&uv.y) {
            return false;
        }
        self.opaque((uv.x * self.width as f32) as u32, (uv.y * self.height as f32) as u32)
    }
}

/// Registry of all loaded sprites and animations.
pub struct AssetManager {
    root: PathBuf,
    sprites: Vec<SpriteRegion>,
    sprite_names: HashMap<String, SpriteId>,
    animations: Vec<Animation>,
    animation_names: HashMap<String, AnimationId>,
    atlas_textures: HashMap<String, TextureId>,
    /// Opaque pixels of every loaded atlas texture.
    masks: HashMap<TextureId, AlphaMask>,
    missing_sprite: SpriteId,
    white_sprite: SpriteId,
    warned: Mutex<HashSet<String>>,
    load_errors: Vec<String>,
}

impl AssetManager {
    /// Creates the manager and the built-in fallback textures ("missing" checkerboard, white pixel).
    pub fn new(assets_root: &Path, renderer: &mut Renderer) -> Self {
        let mut mgr = Self {
            root: assets_root.to_path_buf(),
            sprites: Vec::new(),
            sprite_names: HashMap::new(),
            animations: Vec::new(),
            animation_names: HashMap::new(),
            atlas_textures: HashMap::new(),
            masks: HashMap::new(),
            missing_sprite: SpriteId(0),
            white_sprite: SpriteId(0),
            warned: Mutex::new(HashSet::new()),
            load_errors: Vec::new(),
        };
        // Missing-asset checkerboard (magenta / black) so problems are obvious on screen.
        let size = 32u32;
        let mut rgba = Vec::with_capacity((size * size * 4) as usize);
        for y in 0..size {
            for x in 0..size {
                let on = ((x / 8) + (y / 8)) % 2 == 0;
                rgba.extend_from_slice(if on { &[255, 0, 255, 255] } else { &[20, 20, 20, 255] });
            }
        }
        let tex = renderer.create_texture("builtin/missing", size, size, &rgba, Sampling::Pixel, false);
        mgr.missing_sprite = mgr.register_sprite(
            "builtin/missing",
            SpriteRegion { texture: tex, uv: [0.0, 0.0, 1.0, 1.0], size: Vec2::splat(32.0), anchor: Vec2::new(0.5, 1.0) },
        );
        let white = renderer.create_texture("builtin/white", 4, 4, &[255u8; 64], Sampling::Smooth, false);
        mgr.white_sprite = mgr.register_sprite(
            "builtin/white",
            SpriteRegion { texture: white, uv: [0.25, 0.25, 0.75, 0.75], size: Vec2::ONE, anchor: Vec2::splat(0.5) },
        );
        mgr
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Loads every atlas listed in `atlases/index.ron` and blocks until all are registered.
    /// Individual failures are logged and collected in [`AssetManager::load_errors`]; the game
    /// continues with fallback sprites. Images are decoded on worker threads, then uploaded on the
    /// calling thread. Returns the number of atlases loaded.
    pub fn load_all_atlases(&mut self, renderer: &mut Renderer) -> usize {
        let mut loading = self.start_loading_atlases(renderer);
        while !self.continue_loading_atlases(&mut loading, renderer, None) {}
        loading.loaded()
    }

    /// Starts decoding every atlas listed in `atlases/index.ron` on worker threads and returns
    /// right away. Call [`AssetManager::continue_loading_atlases`] once per frame until it reports
    /// completion; a loading screen can show [`AtlasLoading::progress`] meanwhile. A missing or
    /// broken index is recorded as a load error (the loading is then finished at once).
    pub fn start_loading_atlases(&mut self, renderer: &Renderer) -> AtlasLoading {
        let index_path = self.root.join("atlases/index.ron");
        let names = match read_ron::<AtlasIndex>(&index_path) {
            Ok(index) => index.atlases,
            Err(err) => {
                log::error!("{err}");
                self.load_errors.push(err.to_string());
                Vec::new()
            }
        };
        AtlasLoading::spawn(self.root.clone(), names, renderer.max_texture_size())
    }

    /// Registers decoded atlases in index order (so sprite ids are the same in every run) and
    /// uploads their textures. With a `budget`, returns after the first upload that exceeds it
    /// (the rest follows on the next call); without one, waits for the decoders until every
    /// atlas is done. Returns true once all atlases are registered or failed.
    pub fn continue_loading_atlases(
        &mut self,
        loading: &mut AtlasLoading,
        renderer: &mut Renderer,
        budget: Option<std::time::Duration>,
    ) -> bool {
        let started = std::time::Instant::now();
        loop {
            loading.receive(budget.is_none());
            let Some(result) = loading.take_next() else { break };
            match result {
                Ok(atlas) => {
                    self.register_atlas(atlas, renderer);
                    loading.loaded += 1;
                }
                Err(err) => {
                    log::error!("{err}");
                    self.load_errors.push(err.to_string());
                }
            }
            if budget.is_some_and(|b| started.elapsed() >= b) {
                break;
            }
        }
        let finished = loading.is_finished();
        if finished && !loading.reported {
            loading.reported = true;
            log::info!(
                "assets: {} atlases, {} sprites, {} animations ({:.0} ms)",
                loading.loaded,
                self.sprites.len(),
                self.animations.len(),
                loading.started.elapsed().as_secs_f64() * 1000.0
            );
        }
        finished
    }

    fn register_atlas(&mut self, atlas: DecodedAtlas, renderer: &mut Renderer) {
        let DecodedAtlas { name, def, width, height, rgba } = atlas;
        let texture = renderer.create_texture(&format!("atlas/{name}"), width, height, &rgba, def.sampling, def.mipmaps);
        self.atlas_textures.insert(name.clone(), texture);
        self.masks.insert(texture, AlphaMask::from_rgba(width, height, &rgba));
        let (w, h) = (width as f32, height as f32);
        // Deterministic registration order (sorted by name) so SpriteIds are stable between runs.
        let mut sprite_names: Vec<&String> = def.sprites.keys().collect();
        sprite_names.sort();
        for sprite_name in sprite_names {
            let s = &def.sprites[sprite_name];
            let (x, y, sw, sh) = s.rect;
            let region = SpriteRegion {
                texture,
                uv: [x as f32 / w, y as f32 / h, (x + sw) as f32 / w, (y + sh) as f32 / h],
                size: Vec2::new(sw as f32, sh as f32),
                anchor: Vec2::new(s.anchor.0, s.anchor.1),
            };
            self.register_sprite(&format!("{name}/{sprite_name}"), region);
        }
        let mut anim_names: Vec<&String> = def.animations.keys().collect();
        anim_names.sort();
        for anim_name in anim_names {
            let a = &def.animations[anim_name];
            let frames = a.frames.iter().map(|f| self.sprite(&format!("{name}/{f}"))).collect();
            let id = AnimationId(self.animations.len() as u32);
            self.animations.push(Animation { frames, fps: a.fps.max(0.01), looping: a.looping });
            self.animation_names.insert(format!("{name}/{anim_name}"), id);
        }
    }

    /// Registers a sprite region under `name` (overwriting an existing one).
    pub fn register_sprite(&mut self, name: &str, region: SpriteRegion) -> SpriteId {
        if let Some(&id) = self.sprite_names.get(name) {
            self.sprites[id.0 as usize] = region;
            return id;
        }
        let id = SpriteId(self.sprites.len() as u32);
        self.sprites.push(region);
        self.sprite_names.insert(name.to_owned(), id);
        id
    }

    /// Looks up a sprite by name. Unknown names return the "missing" sprite (warned once).
    pub fn sprite(&self, name: &str) -> SpriteId {
        match self.sprite_names.get(name) {
            Some(&id) => id,
            None => {
                self.warn_once(name, "sprite");
                self.missing_sprite
            }
        }
    }

    /// Looks up a sprite, returning `None` instead of the fallback.
    pub fn try_sprite(&self, name: &str) -> Option<SpriteId> {
        self.sprite_names.get(name).copied()
    }

    pub fn has_sprite(&self, name: &str) -> bool {
        self.sprite_names.contains_key(name)
    }

    /// Whether sprite `id` is opaque (alpha ≥ 128) at `local` — normalised coordinates inside
    /// the sprite, `(0, 0)` its top left corner, `(1, 1)` its bottom right. `None` if `local`
    /// lies outside the sprite or its texture has no mask (built-in textures).
    pub fn sprite_opaque_at(&self, id: SpriteId, local: Vec2) -> Option<bool> {
        if !(0.0..=1.0).contains(&local.x) || !(0.0..=1.0).contains(&local.y) {
            return None;
        }
        let r = self.region(id);
        let mask = self.masks.get(&r.texture)?;
        let uv = Vec2::new(r.uv[0] + (r.uv[2] - r.uv[0]) * local.x, r.uv[1] + (r.uv[3] - r.uv[1]) * local.y);
        // Stay inside the sprite's own rectangle at its right and bottom edge.
        let max = Vec2::new(r.uv[2], r.uv[3]) - Vec2::splat(1e-6);
        Some(mask.opaque_at_uv(uv.min(max)))
    }

    pub fn region(&self, id: SpriteId) -> &SpriteRegion {
        self.sprites.get(id.0 as usize).unwrap_or(&self.sprites[self.missing_sprite.0 as usize])
    }

    pub fn animation(&self, name: &str) -> Option<AnimationId> {
        let id = self.animation_names.get(name).copied();
        if id.is_none() {
            self.warn_once(name, "animation");
        }
        id
    }

    /// Like [`AssetManager::animation`], but an unknown name is no warning (optional
    /// animations, e.g. work loops only some roles have).
    pub fn try_animation(&self, name: &str) -> Option<AnimationId> {
        self.animation_names.get(name).copied()
    }

    pub fn animation_data(&self, id: AnimationId) -> Option<&Animation> {
        self.animations.get(id.0 as usize)
    }

    pub fn atlas_texture(&self, atlas: &str) -> Option<TextureId> {
        self.atlas_textures.get(atlas).copied()
    }

    pub fn missing_sprite(&self) -> SpriteId {
        self.missing_sprite
    }

    /// A white 1×1 sprite for solid-colour quads.
    pub fn white_sprite(&self) -> SpriteId {
        self.white_sprite
    }

    pub fn sprite_count(&self) -> usize {
        self.sprites.len()
    }

    /// Names of all sprites with the given prefix (sorted), e.g. all `"terrain/grass_"` variants.
    pub fn sprites_with_prefix(&self, prefix: &str) -> Vec<SpriteId> {
        let mut names: Vec<(&String, &SpriteId)> =
            self.sprite_names.iter().filter(|(n, _)| n.starts_with(prefix)).collect();
        names.sort_by(|a, b| a.0.cmp(b.0));
        names.into_iter().map(|(_, id)| *id).collect()
    }

    pub fn load_errors(&self) -> &[String] {
        &self.load_errors
    }

    fn warn_once(&self, name: &str, kind: &str) {
        if let Ok(mut warned) = self.warned.lock()
            && warned.insert(format!("{kind}:{name}"))
        {
            log::warn!("missing {kind} '{name}', using fallback");
        }
    }
}

fn read_ron<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, AssetError> {
    let text = std::fs::read_to_string(path)
        .map_err(|source| AssetError::Io { path: path.display().to_string(), source })?;
    ron::from_str(&text).map_err(|source| AssetError::Metadata { path: path.display().to_string(), source: Box::new(source) })
}

fn decode_atlas(root: &Path, name: &str, max_size: u32) -> Result<DecodedAtlas, AssetError> {
    let meta_path = root.join("atlases").join(format!("{name}.ron"));
    let def: AtlasDef = read_ron(&meta_path)?;
    let image_path = meta_path.parent().unwrap_or(root).join(&def.image);
    let img = image::open(&image_path)
        .map_err(|source| AssetError::Image { path: image_path.display().to_string(), source: Box::new(source) })?
        .to_rgba8();
    let (width, height) = img.dimensions();
    if width > max_size || height > max_size {
        return Err(AssetError::Invalid {
            atlas: name.to_owned(),
            message: format!("image {width}x{height} exceeds GPU limit {max_size}"),
        });
    }
    def.validate(name, width, height).map_err(|message| AssetError::Invalid { atlas: name.to_owned(), message })?;
    Ok(DecodedAtlas { name: name.to_owned(), def, width, height, rgba: img.into_raw() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_masks_mark_opaque_pixels() {
        // 3 × 2 image: alpha 0, 127, 128 / 255, 0, 200.
        let alphas = [0u8, 127, 128, 255, 0, 200];
        let rgba: Vec<u8> = alphas.iter().flat_map(|a| [9, 9, 9, *a]).collect();
        let mask = AlphaMask::from_rgba(3, 2, &rgba);
        let opaque: Vec<bool> = (0..2).flat_map(|y| (0..3).map(move |x| (x, y))).map(|(x, y)| mask.opaque(x, y)).collect();
        assert_eq!(opaque, [false, false, true, true, false, true]);
        assert!(!mask.opaque(3, 0) && !mask.opaque(0, 2), "outside");
        assert!(mask.opaque_at_uv(Vec2::new(0.9, 0.1)));
        assert!(!mask.opaque_at_uv(Vec2::new(0.5, 0.9)));
        assert!(!mask.opaque_at_uv(Vec2::new(1.0, 0.5)) && !mask.opaque_at_uv(Vec2::new(-0.1, 0.5)));
        // Larger than one word of bits.
        let big = AlphaMask::from_rgba(100, 1, &[255u8; 400]);
        assert!(big.opaque(99, 0) && big.opaque(64, 0));
    }

    /// Decoded atlases come back in index order whatever order the workers finish in; broken
    /// atlases are reported in their place.
    #[test]
    fn atlas_loading_keeps_the_index_order() {
        let root = std::env::temp_dir().join(format!("kingdoms-atlas-loading-{}", std::process::id()));
        let dir = root.join("atlases");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let mut names = Vec::new();
        for i in 0..7u32 {
            let name = format!("a{i}");
            // Larger images for the first atlases so they tend to finish last.
            let size = 64 - i * 8;
            let img = image::RgbaImage::from_pixel(size, size, image::Rgba([i as u8, 0, 0, 255]));
            img.save(dir.join(format!("{name}.png"))).expect("png");
            let ron = format!("(image: \"{name}.png\", sprites: {{ \"s\": (rect: (0, 0, 4, 4)) }})");
            std::fs::write(dir.join(format!("{name}.ron")), ron).expect("ron");
            names.push(name);
        }
        names.insert(3, "missing".to_owned());
        let mut loading = AtlasLoading::spawn(root.clone(), names.clone(), 4096);
        assert_eq!((loading.total(), loading.done()), (8, 0));
        let mut order = Vec::new();
        while !loading.is_finished() {
            loading.receive(true);
            while let Some(result) = loading.take_next() {
                order.push(match result {
                    Ok(atlas) => atlas.name,
                    Err(_) => "error".to_owned(),
                });
            }
        }
        let expected: Vec<String> = names.iter().map(|n| if n == "missing" { "error".to_owned() } else { n.clone() }).collect();
        assert_eq!(order, expected);
        assert!((loading.progress() - 1.0).abs() < 1e-6);
        assert!(loading.current().is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn animation_frames() {
        let a = Animation { frames: vec![SpriteId(1), SpriteId(2), SpriteId(3)], fps: 2.0, looping: true };
        assert_eq!(a.frame_at(0.0), SpriteId(1));
        assert_eq!(a.frame_at(0.6), SpriteId(2));
        assert_eq!(a.frame_at(1.6), SpriteId(1));
        let once = Animation { looping: false, ..a };
        assert_eq!(once.frame_at(10.0), SpriteId(3));
        assert!((once.duration() - 1.5).abs() < 1e-6);
    }
}
