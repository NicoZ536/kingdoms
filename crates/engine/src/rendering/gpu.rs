//! wgpu instance / adapter / device / surface management.

use std::sync::Arc;

use winit::window::Window;

use super::RenderError;

/// Human-readable information about the GPU in use (shown in the debug HUD and logs).
#[derive(Clone, Debug)]
pub struct GpuInfo {
    pub name: String,
    pub backend: String,
    pub device_type: String,
    pub driver: String,
    pub surface_format: String,
    pub present_mode: String,
}

pub(crate) struct GpuContext {
    pub instance: wgpu::Instance,
    pub surface: wgpu::Surface<'static>,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub window: Arc<Window>,
    pub supports_copy_src: bool,
}

impl GpuContext {
    /// Format egui renders with (the non-sRGB variant of the surface format).
    pub fn ui_format(&self) -> wgpu::TextureFormat {
        self.config.format.remove_srgb_suffix()
    }
}

impl GpuContext {
    pub async fn new(
        window: Arc<Window>,
        display: Option<Box<dyn wgpu::wgt::WgpuHasDisplayHandle>>,
        vsync: bool,
    ) -> Result<Self, RenderError> {
        let mut desc = match display {
            Some(d) => wgpu::InstanceDescriptor::new_with_display_handle(d),
            None => wgpu::InstanceDescriptor::new_without_display_handle(),
        };
        desc.backends = wgpu::Backends::from_env().unwrap_or(wgpu::Backends::PRIMARY);
        desc.flags = wgpu::InstanceFlags::from_build_config().with_env();
        desc.backend_options = wgpu::BackendOptions::from_env_or_default();
        let instance = wgpu::Instance::new(desc);

        let surface = instance.create_surface(window.clone()).map_err(RenderError::CreateSurface)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::from_env()
                    .unwrap_or(wgpu::PowerPreference::HighPerformance),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .map_err(|e| RenderError::NoAdapter(e.to_string()))?;

        let info = adapter.get_info();
        log::info!(
            "GPU: {} ({:?}, {:?}) driver: {} {}",
            info.name,
            info.backend,
            info.device_type,
            info.driver,
            info.driver_info
        );

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("kingdoms device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits {
                    max_texture_dimension_2d: adapter.limits().max_texture_dimension_2d.min(8192),
                    ..wgpu::Limits::downlevel_defaults()
                },
                ..Default::default()
            })
            .await
            .map_err(|e| RenderError::RequestDevice(e.to_string()))?;

        device.on_uncaptured_error(Arc::new(|err| {
            log::error!("wgpu error: {err}");
        }));

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| caps.formats.first().copied())
            .ok_or_else(|| RenderError::Surface("surface reports no formats".into()))?;
        let supports_copy_src = caps.usages.contains(wgpu::TextureUsages::COPY_SRC);
        let present_mode = choose_present_mode(&caps.present_modes, vsync);
        let alpha_mode = caps.alpha_modes.first().copied().unwrap_or(wgpu::CompositeAlphaMode::Auto);
        let size = window.inner_size();
        let mut usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
        if supports_copy_src {
            usage |= wgpu::TextureUsages::COPY_SRC;
        }
        let config = wgpu::SurfaceConfiguration {
            usage,
            format,
            color_space: Default::default(),
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            // egui expects a non-sRGB target; it renders through a linear view of the surface.
            view_formats: if format.is_srgb() { vec![format.remove_srgb_suffix()] } else { vec![] },
        };
        surface.configure(&device, &config);
        log::info!("surface: {format:?}, {present_mode:?}, {}x{}", config.width, config.height);

        Ok(Self { instance, surface, adapter, device, queue, config, window, supports_copy_src })
    }

    pub fn info(&self) -> GpuInfo {
        let i = self.adapter.get_info();
        GpuInfo {
            name: i.name,
            backend: format!("{:?}", i.backend),
            device_type: format!("{:?}", i.device_type),
            driver: format!("{} {}", i.driver, i.driver_info).trim().to_owned(),
            surface_format: format!("{:?}", self.config.format),
            present_mode: format!("{:?}", self.config.present_mode),
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn reconfigure(&self) {
        self.surface.configure(&self.device, &self.config);
    }

    /// Recreates the surface after it was lost (e.g. GPU reset, display change).
    pub fn recreate_surface(&mut self) -> Result<(), RenderError> {
        let surface = self.instance.create_surface(self.window.clone()).map_err(RenderError::CreateSurface)?;
        surface.configure(&self.device, &self.config);
        self.surface = surface;
        log::warn!("surface recreated");
        Ok(())
    }

    pub fn set_vsync(&mut self, vsync: bool) {
        let caps = self.surface.get_capabilities(&self.adapter);
        self.config.present_mode = choose_present_mode(&caps.present_modes, vsync);
        self.reconfigure();
    }
}

fn choose_present_mode(available: &[wgpu::PresentMode], vsync: bool) -> wgpu::PresentMode {
    let prefer: &[wgpu::PresentMode] = if vsync {
        &[wgpu::PresentMode::Fifo]
    } else {
        &[wgpu::PresentMode::Mailbox, wgpu::PresentMode::Immediate, wgpu::PresentMode::Fifo]
    };
    prefer.iter().copied().find(|m| available.contains(m)).unwrap_or(wgpu::PresentMode::Fifo)
}
