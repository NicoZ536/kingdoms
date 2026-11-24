//! The frame renderer: sprite pipelines, static batches, dynamic instance streaming, egui overlay.

use std::path::PathBuf;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec2};
use wgpu::util::DeviceExt;
use winit::window::Window;

use super::capture;
use super::gpu::{GpuContext, GpuInfo};
use super::iso::DEPTH_RANGE;
use super::camera::Camera2D;
use super::sprite::{DrawItem, RenderList, RenderListPool, SpriteInstance, StaticBatchHandle};
use super::texture::{Sampling, TextureId, TextureRegistry};
use crate::math::Rect;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Vertices per sprite instance (two quads, see the shader).
const VERTICES_PER_SPRITE: u32 = 12;

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("could not create window surface: {0}")]
    CreateSurface(wgpu::CreateSurfaceError),
    #[error("no suitable GPU adapter found: {0}")]
    NoAdapter(String),
    #[error("could not create GPU device: {0}")]
    RequestDevice(String),
    #[error("surface error: {0}")]
    Surface(String),
}

/// Per-frame rendering statistics.
#[derive(Clone, Copy, Debug, Default)]
pub struct RenderStats {
    pub draw_calls: u32,
    pub sprites_dynamic: u32,
    pub sprites_static: u32,
    pub static_batches_drawn: u32,
    pub static_batches_culled: u32,
    pub sprites_culled_cpu: u32,
    pub textures: u32,
    pub instance_buffer_bytes: u64,
    pub frames_skipped: u64,
    /// CPU time of the frame's steps in milliseconds: waiting for the surface texture, sorting
    /// and uploading the dynamic instances (and egui buffers), encoding the passes, submitting
    /// and presenting.
    pub acquire_ms: f32,
    pub prepare_ms: f32,
    pub encode_ms: f32,
    pub submit_ms: f32,
}

/// egui output for one frame, produced by the app loop.
pub struct EguiPaint {
    pub primitives: Vec<egui::ClippedPrimitive>,
    pub textures_delta: egui::TexturesDelta,
    pub pixels_per_point: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    ambient: [f32; 4],
    params: [f32; 4],
    environment: [f32; 4],
    /// x: wind direction on screen (−1 … 1), yzw: unused.
    wind: [f32; 4],
}

struct StaticBatch {
    buffer: wgpu::Buffer,
    count: u32,
    texture: TextureId,
    bounds: Rect,
}

struct StaticSlot {
    generation: u32,
    batch: Option<StaticBatch>,
}

/// A contiguous run of instances in the instance buffer sharing one texture.
struct DrawRun {
    texture: TextureId,
    start: u32,
    count: u32,
}

struct Targets {
    depth: wgpu::TextureView,
    msaa: Option<wgpu::TextureView>,
}

pub struct Renderer {
    gpu: GpuContext,
    sample_count: u32,
    /// The adapter supports 4× multisampling for the surface and depth formats.
    msaa_supported: bool,
    /// Kept to rebuild the sprite pipelines when the sample count changes.
    pipeline_layout: wgpu::PipelineLayout,
    shader: wgpu::ShaderModule,
    targets: Targets,
    textures: TextureRegistry,
    globals_buffer: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    opaque_pipeline: wgpu::RenderPipeline,
    transparent_pipeline: wgpu::RenderPipeline,
    instance_buffer: wgpu::Buffer,
    instance_capacity: u64,
    statics: Vec<StaticSlot>,
    free_static_slots: Vec<u32>,
    egui_renderer: egui_wgpu::Renderer,
    stats: RenderStats,
    frames_skipped: u64,
    screenshot_requests: Vec<PathBuf>,
    start: std::time::Instant,
    /// Buffers reused every frame (no per-frame allocation once they have grown).
    scratch: FrameScratch,
    /// Buffers of drawn render lists for the next frame's lists.
    lists: RenderListPool,
}

/// Per-frame working buffers of [`Renderer::render`].
#[derive(Default)]
struct FrameScratch {
    /// Draw order of the dynamic sprites: indices into the opaque list, then the transparent.
    opaque_order: Vec<u32>,
    transparent_order: Vec<u32>,
    /// Sort keys of the transparent sprites: (depth key, texture, index).
    transparent_keys: Vec<(f32, TextureId, u32)>,
    /// Instances per texture while counting (opaque grouping).
    texture_counts: Vec<(TextureId, u32)>,
    instances: Vec<SpriteInstance>,
    opaque_runs: Vec<DrawRun>,
    transparent_runs: Vec<DrawRun>,
}

impl FrameScratch {
    /// Orders the dynamic sprites of `list` and fills the instance and run buffers: opaque
    /// sprites grouped by texture in submission order (a stable counting sort), transparent
    /// sprites back to front by depth, equal depths by texture, then submission order – the
    /// order a stable sort by `(depth, texture)` gives, with each key computed once.
    fn prepare(&mut self, opaque: &[DrawItem], transparent: &[DrawItem]) {
        group_by_texture(opaque, &mut self.texture_counts, &mut self.opaque_order);
        self.transparent_keys.clear();
        self.transparent_keys.extend(transparent.iter().enumerate().map(|(i, d)| (d.instance.depth().sort_key(), d.texture, i as u32)));
        self.transparent_keys.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        self.transparent_order.clear();
        self.transparent_order.extend(self.transparent_keys.iter().map(|k| k.2));
        self.instances.clear();
        self.instances.reserve(opaque.len() + transparent.len());
        build_runs(opaque, &self.opaque_order, &mut self.instances, &mut self.opaque_runs);
        build_runs(transparent, &self.transparent_order, &mut self.instances, &mut self.transparent_runs);
    }
}

/// Indices of `items` grouped by texture (ascending texture id), keeping the submission order
/// within a texture: a counting sort over the few textures of a frame.
fn group_by_texture(items: &[DrawItem], counts: &mut Vec<(TextureId, u32)>, order: &mut Vec<u32>) {
    counts.clear();
    for item in items {
        match counts.iter_mut().find(|(t, _)| *t == item.texture) {
            Some((_, n)) => *n += 1,
            None => counts.push((item.texture, 1)),
        }
    }
    counts.sort_unstable_by_key(|(t, _)| *t);
    // Counts become start offsets.
    let mut next = 0;
    for (_, n) in counts.iter_mut() {
        let len = *n;
        *n = next;
        next += len;
    }
    order.clear();
    order.resize(items.len(), 0);
    for (i, item) in items.iter().enumerate() {
        if let Some((_, slot)) = counts.iter_mut().find(|(t, _)| *t == item.texture) {
            order[*slot as usize] = i as u32;
            *slot += 1;
        }
    }
}

impl Renderer {
    /// Creates the renderer for `window`. `display` is the event loop's display handle (needed by
    /// some backends); `msaa_samples` must be 1 or 4.
    pub fn new(
        window: Arc<Window>,
        display: Option<Box<dyn wgpu::wgt::WgpuHasDisplayHandle>>,
        vsync: bool,
        msaa_samples: u32,
    ) -> Result<Self, RenderError> {
        let gpu = pollster::block_on(GpuContext::new(window, display, vsync))?;
        let device = &gpu.device;

        let format_features = gpu.adapter.get_texture_format_features(gpu.config.format);
        let msaa_supported = format_features.flags.sample_count_supported(4)
            && gpu.adapter.get_texture_format_features(DEPTH_FORMAT).flags.sample_count_supported(4);
        let sample_count = if msaa_samples >= 4 && msaa_supported { 4 } else { 1 };
        if sample_count != msaa_samples {
            log::warn!("MSAA x{msaa_samples} not supported, using x{sample_count}");
        }

        let textures = TextureRegistry::new(device);
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("globals"),
            contents: bytemuck::bytes_of(&Globals {
                view_proj: Mat4::IDENTITY.to_cols_array_2d(),
                ambient: [1.0; 4],
                params: [0.0, DEPTH_RANGE, 0.5, 0.0],
                environment: [1.0, 0.0, 0.0, 0.0],
                wind: [0.0; 4],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals_buffer.as_entire_binding() }],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sprite shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/sprite.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sprite pipeline layout"),
            bind_group_layouts: &[Some(&globals_layout), Some(textures.layout())],
            immediate_size: 0,
        });
        let (opaque_pipeline, transparent_pipeline) =
            create_sprite_pipelines(device, &layout, &shader, gpu.config.format, sample_count);

        let instance_capacity = 16_384u64;
        let instance_buffer = create_instance_buffer(device, instance_capacity);
        let targets = create_targets(device, &gpu.config, sample_count);
        let egui_renderer = egui_wgpu::Renderer::new(
            device,
            gpu.ui_format(),
            egui_wgpu::RendererOptions { msaa_samples: 1, depth_stencil_format: None, ..Default::default() },
        );

        Ok(Self {
            gpu,
            sample_count,
            msaa_supported,
            pipeline_layout: layout,
            shader,
            targets,
            textures,
            globals_buffer,
            globals_bind_group,
            opaque_pipeline,
            transparent_pipeline,
            instance_buffer,
            instance_capacity,
            statics: Vec::new(),
            free_static_slots: Vec::new(),
            egui_renderer,
            stats: RenderStats::default(),
            frames_skipped: 0,
            screenshot_requests: Vec::new(),
            start: std::time::Instant::now(),
            scratch: FrameScratch::default(),
            lists: RenderListPool::default(),
        })
    }

    pub fn info(&self) -> GpuInfo {
        self.gpu.info()
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.gpu.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.gpu.queue
    }

    pub fn max_texture_size(&self) -> u32 {
        self.gpu.device.limits().max_texture_dimension_2d
    }

    /// Surface size in physical pixels.
    pub fn surface_size(&self) -> Vec2 {
        Vec2::new(self.gpu.config.width as f32, self.gpu.config.height as f32)
    }

    pub fn sample_count(&self) -> u32 {
        self.sample_count
    }

    /// True if 4× MSAA is available on this adapter.
    pub fn msaa_supported(&self) -> bool {
        self.msaa_supported
    }

    /// Switches multisampling (`samples` ≥ 4: 4×, else off) at runtime by rebuilding the
    /// sprite pipelines and render targets. Unsupported requests fall back to 1. Returns the
    /// sample count in use.
    pub fn set_msaa_samples(&mut self, samples: u32) -> u32 {
        let count = if samples >= 4 && self.msaa_supported { 4 } else { 1 };
        if count == self.sample_count {
            return count;
        }
        let device = &self.gpu.device;
        let (opaque, transparent) =
            create_sprite_pipelines(device, &self.pipeline_layout, &self.shader, self.gpu.config.format, count);
        self.opaque_pipeline = opaque;
        self.transparent_pipeline = transparent;
        self.sample_count = count;
        self.targets = create_targets(device, &self.gpu.config, count);
        log::info!("MSAA set to x{count}");
        count
    }

    pub fn stats(&self) -> RenderStats {
        self.stats
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.gpu.resize(width, height);
        self.targets = create_targets(&self.gpu.device, &self.gpu.config, self.sample_count);
    }

    pub fn set_vsync(&mut self, vsync: bool) {
        self.gpu.set_vsync(vsync);
    }

    /// Queues a screenshot of the next rendered frame.
    pub fn request_screenshot(&mut self, path: PathBuf) {
        self.screenshot_requests.push(path);
    }

    // ----- textures -------------------------------------------------------------------------

    pub fn create_texture(
        &mut self,
        label: &str,
        width: u32,
        height: u32,
        rgba: &[u8],
        sampling: Sampling,
        mipmaps: bool,
    ) -> TextureId {
        self.textures.create_rgba8(&self.gpu.device, &self.gpu.queue, label, width, height, rgba, sampling, mipmaps)
    }

    pub fn update_texture(&self, id: TextureId, x: u32, y: u32, width: u32, height: u32, rgba: &[u8]) {
        self.textures.update_rgba8(&self.gpu.queue, id, x, y, width, height, rgba);
    }

    pub fn texture_size(&self, id: TextureId) -> Option<(u32, u32)> {
        self.textures.get(id).map(|t| (t.width, t.height))
    }

    /// Makes an engine texture usable in egui (e.g. building icons in menus).
    pub fn register_egui_texture(&mut self, id: TextureId) -> Option<egui::TextureId> {
        let tex = self.textures.get(id)?;
        Some(self.egui_renderer.register_native_texture(
            &self.gpu.device,
            &tex.view,
            wgpu::FilterMode::Linear,
        ))
    }

    // ----- static batches -------------------------------------------------------------------

    /// Uploads a batch of opaque sprites that will be drawn every frame it is queued, until
    /// removed. Ideal for terrain chunks. An empty instance list yields a valid empty batch.
    pub fn create_static_batch(&mut self, texture: TextureId, instances: &[SpriteInstance]) -> StaticBatchHandle {
        let batch = self.make_static_batch(texture, instances);
        if let Some(index) = self.free_static_slots.pop() {
            let slot = &mut self.statics[index as usize];
            slot.generation = slot.generation.wrapping_add(1);
            slot.batch = Some(batch);
            StaticBatchHandle { index, generation: slot.generation }
        } else {
            let index = self.statics.len() as u32;
            self.statics.push(StaticSlot { generation: 0, batch: Some(batch) });
            StaticBatchHandle { index, generation: 0 }
        }
    }

    /// Replaces the contents of a static batch. Returns false if the handle is stale.
    pub fn update_static_batch(&mut self, handle: StaticBatchHandle, texture: TextureId, instances: &[SpriteInstance]) -> bool {
        let valid = self
            .statics
            .get(handle.index as usize)
            .is_some_and(|s| s.generation == handle.generation && s.batch.is_some());
        if !valid {
            return false;
        }
        let batch = self.make_static_batch(texture, instances);
        self.statics[handle.index as usize].batch = Some(batch);
        true
    }

    /// Static batches currently alive (created and not yet removed).
    pub fn static_batch_count(&self) -> usize {
        self.statics.iter().filter(|s| s.batch.is_some()).count()
    }

    pub fn remove_static_batch(&mut self, handle: StaticBatchHandle) {
        if let Some(slot) = self.statics.get_mut(handle.index as usize)
            && slot.generation == handle.generation
            && slot.batch.take().is_some()
        {
            self.free_static_slots.push(handle.index);
        }
    }

    fn make_static_batch(&self, texture: TextureId, instances: &[SpriteInstance]) -> StaticBatch {
        let mut bounds = Rect::EMPTY;
        for inst in instances {
            bounds = bounds.union(&inst.world_bounds());
        }
        // Wind sway can move vertices a couple of pixels.
        let bounds = if instances.is_empty() { Rect::EMPTY } else { bounds.expanded(4.0) };
        let contents: &[u8] = if instances.is_empty() {
            bytemuck::bytes_of(&ZERO_INSTANCE)
        } else {
            bytemuck::cast_slice(instances)
        };
        let buffer = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("static sprite batch"),
            contents,
            usage: wgpu::BufferUsages::VERTEX,
        });
        StaticBatch { buffer, count: instances.len() as u32, texture, bounds }
    }

    fn static_batch(&self, handle: StaticBatchHandle) -> Option<&StaticBatch> {
        let slot = self.statics.get(handle.index as usize)?;
        if slot.generation != handle.generation {
            return None;
        }
        slot.batch.as_ref()
    }

    // ----- frame ----------------------------------------------------------------------------

    /// An empty render list for `camera` with the buffers of an earlier frame's list (give the
    /// list back with [`recycle_list`](Self::recycle_list) once it was drawn).
    pub fn begin_list(&mut self, camera: Camera2D) -> RenderList {
        self.lists.list(camera)
    }

    /// Keeps the buffers of a drawn list for the next [`begin_list`](Self::begin_list).
    pub fn recycle_list(&mut self, list: RenderList) {
        self.lists.recycle(list);
    }

    /// Renders one frame. Returns `Ok(false)` if the frame was skipped (minimised, surface
    /// temporarily unavailable).
    pub fn render(&mut self, list: &RenderList, mut egui: Option<EguiPaint>) -> Result<bool, RenderError> {
        let started = std::time::Instant::now();
        let frame = match self.gpu.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) => Some((f, false)),
            wgpu::CurrentSurfaceTexture::Suboptimal(f) => Some((f, true)),
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => None,
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.gpu.reconfigure();
                None
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.gpu.recreate_surface()?;
                None
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err(RenderError::Surface("validation error while acquiring frame".into()));
            }
        };
        let Some((frame, suboptimal)) = frame else {
            self.frames_skipped += 1;
            self.stats.frames_skipped = self.frames_skipped;
            // Still apply egui texture changes so deltas are not lost.
            if let Some(egui) = egui.as_mut() {
                self.apply_egui_textures(&mut egui.textures_delta);
                self.free_egui_textures(&mut egui.textures_delta);
            }
            return Ok(false);
        };

        let mut stats = RenderStats {
            textures: self.textures.len() as u32,
            sprites_culled_cpu: list.culled_count() as u32,
            frames_skipped: self.frames_skipped,
            acquire_ms: started.elapsed().as_secs_f32() * 1000.0,
            ..Default::default()
        };
        let step = std::time::Instant::now();

        let screenshot = self.screenshot_requests.pop();

        // Globals.
        let alpha_threshold = if self.sample_count > 1 { 0.02 } else { 0.5 };
        let time = list.shader_time.unwrap_or_else(|| self.start.elapsed().as_secs_f32());
        let env = list.environment;
        let globals = Globals {
            view_proj: list.camera.view_projection().to_cols_array_2d(),
            ambient: list.ambient.to_array(),
            params: [time, DEPTH_RANGE, alpha_threshold, env.wind_lean],
            environment: [
                env.wind.max(0.0),
                env.wind_time.unwrap_or(time),
                env.snow.clamp(0.0, 1.0),
                env.wetness.clamp(0.0, 1.0),
            ],
            wind: [
                if env.wind_dir.is_finite() { env.wind_dir.clamp(-1.0, 1.0) } else { 0.0 },
                if env.shadow.is_finite() { env.shadow.clamp(0.0, 1.0) } else { 1.0 },
                0.0,
                0.0,
            ],
        };
        self.gpu.queue.write_buffer(&self.globals_buffer, 0, bytemuck::bytes_of(&globals));

        // Dynamic instances: opaque grouped by texture, transparent sorted back to front.
        let mut scratch = std::mem::take(&mut self.scratch);
        scratch.prepare(&list.opaque, &list.transparent);
        stats.sprites_dynamic = scratch.instances.len() as u32;
        self.ensure_instance_capacity(scratch.instances.len() as u64);
        if !scratch.instances.is_empty() {
            self.gpu.queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&scratch.instances));
        }
        stats.instance_buffer_bytes = self.instance_capacity * std::mem::size_of::<SpriteInstance>() as u64;

        let mut encoder =
            self.gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });

        // egui buffers (shared by all render targets of this frame).
        let egui_screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.gpu.config.width, self.gpu.config.height],
            pixels_per_point: egui.as_ref().map_or(1.0, |e| e.pixels_per_point),
        };
        let mut extra_cmds = Vec::new();
        if let Some(egui) = egui.as_mut() {
            self.apply_egui_textures(&mut egui.textures_delta);
            extra_cmds = self.egui_renderer.update_buffers(
                &self.gpu.device,
                &self.gpu.queue,
                &mut encoder,
                &egui.primitives,
                &egui_screen,
            );
        }

        stats.prepare_ms = step.elapsed().as_secs_f32() * 1000.0;
        let step = std::time::Instant::now();

        // Visible static batches.
        let view = list.camera.view_rect();
        let mut visible_statics: Vec<&StaticBatch> = Vec::new();
        for handle in &list.statics {
            if let Some(batch) = self.static_batch(*handle) {
                if batch.count > 0 && batch.bounds.intersects(&view) {
                    visible_statics.push(batch);
                } else {
                    stats.static_batches_culled += 1;
                }
            }
        }

        let surface_view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let offscreen = screenshot.as_ref().filter(|_| !self.gpu.supports_copy_src).map(|_| {
            capture::create_offscreen_target(&self.gpu.device, self.gpu.config.format, self.gpu.config.width, self.gpu.config.height)
        });

        let ui_view_desc = wgpu::TextureViewDescriptor { format: Some(self.gpu.ui_format()), ..Default::default() };
        let mut targets: Vec<(wgpu::TextureView, wgpu::TextureView)> =
            vec![(surface_view, frame.texture.create_view(&ui_view_desc))];
        if let Some(tex) = offscreen.as_ref() {
            targets.push((tex.create_view(&wgpu::TextureViewDescriptor::default()), tex.create_view(&ui_view_desc)));
        }
        for (target, ui_target) in &targets {
            let pass_stats = self.encode_world_pass(
                &mut encoder,
                target,
                list,
                &visible_statics,
                &scratch.opaque_runs,
                &scratch.transparent_runs,
            );
            stats.draw_calls = pass_stats.draw_calls;
            stats.sprites_static = pass_stats.sprites_static;
            stats.static_batches_drawn = pass_stats.static_batches_drawn;
            if let Some(egui) = egui.as_ref() {
                let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: ui_target,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                let mut pass = pass.forget_lifetime();
                self.egui_renderer.render(&mut pass, &egui.primitives, &egui_screen);
                stats.draw_calls += egui.primitives.len() as u32;
            }
        }

        let pending = screenshot.map(|path| {
            let source = offscreen.as_ref().unwrap_or(&frame.texture);
            capture::encode_copy(&self.gpu.device, &mut encoder, source, path)
        });

        extra_cmds.push(encoder.finish());
        stats.encode_ms = step.elapsed().as_secs_f32() * 1000.0;
        let step = std::time::Instant::now();
        self.gpu.queue.submit(extra_cmds);
        if let Some(pending) = pending {
            capture::finish(&self.gpu.device, pending);
        }
        self.gpu.queue.present(frame);
        if suboptimal {
            self.gpu.reconfigure();
        }
        if let Some(egui) = egui.as_mut() {
            self.free_egui_textures(&mut egui.textures_delta);
        }
        stats.submit_ms = step.elapsed().as_secs_f32() * 1000.0;
        self.stats = stats;
        self.scratch = scratch;
        Ok(true)
    }

    fn apply_egui_textures(&mut self, delta: &mut egui::TexturesDelta) {
        for (id, image_deltas) in delta.set.drain() {
            for image_delta in image_deltas {
                self.egui_renderer.update_texture(&self.gpu.device, &self.gpu.queue, id, &image_delta);
            }
        }
    }

    fn free_egui_textures(&mut self, delta: &mut egui::TexturesDelta) {
        for id in delta.free.drain() {
            self.egui_renderer.free_texture(&id);
        }
    }

    fn encode_world_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        list: &RenderList,
        statics: &[&StaticBatch],
        opaque_runs: &[DrawRun],
        transparent_runs: &[DrawRun],
    ) -> RenderStats {
        let mut stats = RenderStats::default();
        let (view, resolve_target) = match self.targets.msaa.as_ref() {
            Some(msaa) => (msaa, Some(target)),
            None => (target, None),
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("world"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(list.clear_color.to_wgpu()),
                    store: if resolve_target.is_some() { wgpu::StoreOp::Discard } else { wgpu::StoreOp::Store },
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.targets.depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, &self.globals_bind_group, &[]);
        pass.set_pipeline(&self.opaque_pipeline);

        for batch in statics {
            let Some(tex) = self.textures.get(batch.texture) else { continue };
            pass.set_bind_group(1, &tex.bind_group, &[]);
            pass.set_vertex_buffer(0, batch.buffer.slice(..));
            pass.draw(0..VERTICES_PER_SPRITE, 0..batch.count);
            stats.draw_calls += 1;
            stats.sprites_static += batch.count;
            stats.static_batches_drawn += 1;
        }

        if !opaque_runs.is_empty() || !transparent_runs.is_empty() {
            pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        }
        for run in opaque_runs {
            let Some(tex) = self.textures.get(run.texture) else { continue };
            pass.set_bind_group(1, &tex.bind_group, &[]);
            pass.draw(0..VERTICES_PER_SPRITE, run.start..run.start + run.count);
            stats.draw_calls += 1;
        }
        if !transparent_runs.is_empty() {
            pass.set_pipeline(&self.transparent_pipeline);
            for run in transparent_runs {
                let Some(tex) = self.textures.get(run.texture) else { continue };
                pass.set_bind_group(1, &tex.bind_group, &[]);
                pass.draw(0..VERTICES_PER_SPRITE, run.start..run.start + run.count);
                stats.draw_calls += 1;
            }
        }
        stats
    }

    fn ensure_instance_capacity(&mut self, needed: u64) {
        if needed <= self.instance_capacity {
            return;
        }
        let mut cap = self.instance_capacity.max(1);
        while cap < needed {
            cap *= 2;
        }
        log::debug!("growing sprite instance buffer to {cap} instances");
        self.instance_buffer = create_instance_buffer(&self.gpu.device, cap);
        self.instance_capacity = cap;
    }
}

const ZERO_INSTANCE: SpriteInstance = SpriteInstance {
    position: [0.0; 2],
    size: [0.0; 2],
    anchor: [0.0; 2],
    uv_rect: [0.0; 4],
    color: [0.0; 4],
    depth_tent: [0.0; 4],
    depth_flat: [0.0; 2],
    flags: 0,
    phase: 0.0,
};

/// Appends the instances of `items` in `order` to `out` and their runs of equal texture to
/// `runs` (cleared first).
fn build_runs(items: &[DrawItem], order: &[u32], out: &mut Vec<SpriteInstance>, runs: &mut Vec<DrawRun>) {
    runs.clear();
    for item in order.iter().filter_map(|&i| items.get(i as usize)) {
        let index = out.len() as u32;
        out.push(item.instance);
        match runs.last_mut() {
            Some(run) if run.texture == item.texture => run.count += 1,
            _ => runs.push(DrawRun { texture: item.texture, start: index, count: 1 }),
        }
    }
}

fn create_instance_buffer(device: &wgpu::Device, capacity: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("sprite instances"),
        size: capacity * std::mem::size_of::<SpriteInstance>() as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// The opaque and the transparent sprite pipeline for `sample_count` samples.
fn create_sprite_pipelines(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    sample_count: u32,
) -> (wgpu::RenderPipeline, wgpu::RenderPipeline) {
    let make_pipeline = |label: &str, fs: &str, transparent: bool| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(SpriteInstance::layout())],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(!transparent),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: !transparent && sample_count > 1,
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some(fs),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: if transparent { Some(wgpu::BlendState::ALPHA_BLENDING) } else { None },
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    (make_pipeline("sprite opaque", "fs_opaque", false), make_pipeline("sprite transparent", "fs_transparent", true))
}

fn create_targets(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration, sample_count: u32) -> Targets {
    let size = wgpu::Extent3d { width: config.width.max(1), height: config.height.max(1), depth_or_array_layers: 1 };
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size,
        mip_level_count: 1,
        sample_count,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let msaa = (sample_count > 1).then(|| {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("msaa colour"),
                size,
                mip_level_count: 1,
                sample_count,
                dimension: wgpu::TextureDimension::D2,
                format: config.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default())
    });
    Targets { depth: depth.create_view(&wgpu::TextureViewDescriptor::default()), msaa }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rendering::iso::SpriteDepth;
    use glam::Vec2;
    use wgpu::naga;

    /// `count` sprites on `textures` textures with pseudo-random depths (many equal ones).
    fn items(count: u32, textures: u32, seed: u64) -> Vec<DrawItem> {
        let mut x = seed;
        (0..count)
            .map(|i| {
                x = x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                let depth = SpriteDepth::ground(Vec2::new(((x >> 33) % 7) as f32, ((x >> 40) % 5) as f32), 0.0);
                let instance = SpriteInstance::new(Vec2::new(i as f32, 0.0), Vec2::ONE, Vec2::ZERO, [0.0; 4], depth);
                DrawItem { texture: TextureId(((x >> 20) % u64::from(textures)) as u32), instance }
            })
            .collect()
    }

    #[test]
    fn frame_order_matches_a_stable_sort_and_reuses_its_buffers() {
        for seed in 1..6 {
            let opaque = items(300, 5, seed);
            let transparent = items(400, 3, seed + 100);
            let mut scratch = FrameScratch::default();
            scratch.prepare(&opaque, &transparent);
            // The order the renderer used before: stable sorts of references.
            let mut want_opaque: Vec<&DrawItem> = opaque.iter().collect();
            want_opaque.sort_by_key(|d| d.texture);
            let mut want_transparent: Vec<&DrawItem> = transparent.iter().collect();
            want_transparent.sort_by(|a, b| {
                a.instance.depth().sort_key().total_cmp(&b.instance.depth().sort_key()).then(a.texture.cmp(&b.texture))
            });
            let want: Vec<[f32; 2]> = want_opaque.iter().chain(&want_transparent).map(|d| d.instance.position).collect();
            let have: Vec<[f32; 2]> = scratch.instances.iter().map(|i| i.position).collect();
            assert_eq!(have, want, "seed {seed}");
            let runs = |r: &[DrawRun]| r.iter().map(|r| (r.texture, r.start, r.count)).collect::<Vec<_>>();
            assert!(scratch.opaque_runs.len() <= 5 && runs(&scratch.opaque_runs).iter().map(|r| r.2).sum::<u32>() == 300);
            assert_eq!(runs(&scratch.transparent_runs).iter().map(|r| r.2).sum::<u32>(), 400);
            // A second frame reuses the grown buffers.
            let capacity = scratch.instances.capacity();
            scratch.prepare(&opaque[..10], &transparent[..10]);
            assert_eq!(scratch.instances.len(), 20);
            assert_eq!(scratch.instances.capacity(), capacity);
        }
        let mut empty = FrameScratch::default();
        empty.prepare(&[], &[]);
        assert!(empty.instances.is_empty() && empty.opaque_runs.is_empty() && empty.transparent_runs.is_empty());
    }

    #[test]
    fn sprite_shader_is_valid_wgsl() {
        let source = include_str!("shaders/sprite.wgsl");
        let module = match naga::front::wgsl::parse_str(source) {
            Ok(m) => m,
            Err(e) => panic!("sprite.wgsl does not parse: {}", e.emit_to_string(source)),
        };
        let mut validator = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all());
        if let Err(e) = validator.validate(&module) {
            panic!("sprite.wgsl is invalid: {}", e.emit_to_string(source));
        }
        // The uniform block must match `Globals`: a 4×4 matrix and four vec4 (32 floats).
        assert_eq!(std::mem::size_of::<super::Globals>(), 128);
        // The flag constants of the shader are those of `sprite.rs`.
        use crate::rendering::sprite::*;
        for (name, value) in [
            ("FLAG_WIND", FLAG_WIND),
            ("FLAG_WATER", FLAG_WATER),
            ("FLAG_EMISSIVE", FLAG_EMISSIVE),
            ("FLAG_FLIP_X", FLAG_FLIP_X),
            ("FLAG_GROUND", FLAG_GROUND),
            ("FLAG_DEPTH_MAX", FLAG_DEPTH_MAX),
            ("FLAG_TRODDEN", FLAG_TRODDEN),
            ("FLAG_SNOW_ON_GREEN", FLAG_SNOW_ON_GREEN),
            ("FLAG_SNOW_ON_ROOF", FLAG_SNOW_ON_ROOF),
            ("FLAG_SHADOW_MARK", FLAG_SHADOW_MARK),
        ] {
            assert!(source.contains(&format!("const {name}: u32 = {value}u;")), "{name}");
        }
        // The marks and the shadow paint of the shader are those of `sprite.rs`.
        assert!(source.contains(&format!("const SNOW_MARK: f32 = {SNOW_MARK_ALPHA}.0;")), "snow mark alpha");
        assert!(source.contains(&format!("const SHADOW_MARK: f32 = {SHADOW_MARK_ALPHA}.0;")), "shadow mark alpha");
        let [r, g, b] = SHADOW_MARK_RGB;
        assert!(source.contains(&format!("const SHADOW_SRGB: vec3<f32> = vec3<f32>({r}.0, {g}.0, {b}.0) / 255.0;")), "shadow colour");
        assert!(source.contains(&format!("const SHADOW_OPACITY: f32 = {SHADOW_MARK_OPACITY}.0 / 255.0;")), "shadow opacity");
    }
}
