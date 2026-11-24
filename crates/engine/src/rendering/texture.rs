//! GPU textures, samplers and the texture registry.

use serde::{Deserialize, Serialize};

/// Handle to a texture owned by the renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextureId(pub(crate) u32);

impl TextureId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// How a texture is filtered when magnified.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Sampling {
    /// Crisp pixels when zoomed in (nearest magnification, smooth minification).
    #[default]
    Pixel,
    /// Bilinear filtering everywhere.
    Smooth,
}

/// Maximum number of mip levels generated for atlases. Atlases are padded by
/// [`ATLAS_PADDING`] pixels, which keeps the first few levels free of bleeding.
pub const MAX_MIP_LEVELS: u32 = 4;
/// Recommended padding (with edge extrusion) between atlas regions.
pub const ATLAS_PADDING: u32 = 8;

pub struct GpuTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub bind_group: wgpu::BindGroup,
    pub width: u32,
    pub height: u32,
    pub label: String,
}

/// Owns all sprite textures and the bind group layout they are bound with.
pub struct TextureRegistry {
    textures: Vec<GpuTexture>,
    layout: wgpu::BindGroupLayout,
    pixel_sampler: wgpu::Sampler,
    smooth_sampler: wgpu::Sampler,
}

impl TextureRegistry {
    pub fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sprite texture layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let make_sampler = |label: &str, mag: wgpu::FilterMode| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some(label),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: mag,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                lod_min_clamp: 0.0,
                lod_max_clamp: MAX_MIP_LEVELS as f32,
                ..Default::default()
            })
        };
        Self {
            textures: Vec::new(),
            pixel_sampler: make_sampler("pixel sampler", wgpu::FilterMode::Nearest),
            smooth_sampler: make_sampler("smooth sampler", wgpu::FilterMode::Linear),
            layout,
        }
    }

    pub fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }

    /// Uploads an RGBA8 (sRGB) image. `rgba.len()` must equal `width * height * 4`.
    #[allow(clippy::too_many_arguments)]
    pub fn create_rgba8(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        label: &str,
        width: u32,
        height: u32,
        rgba: &[u8],
        sampling: Sampling,
        mipmaps: bool,
    ) -> TextureId {
        assert_eq!(rgba.len(), (width * height * 4) as usize, "texture data size mismatch");
        let levels = if mipmaps { generate_mips(rgba, width, height, MAX_MIP_LEVELS) } else { vec![] };
        let mip_level_count = 1 + levels.len() as u32;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        write_level(queue, &texture, 0, width, height, rgba);
        for (i, (w, h, data)) in levels.iter().enumerate() {
            write_level(queue, &texture, i as u32 + 1, *w, *h, data);
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = match sampling {
            Sampling::Pixel => &self.pixel_sampler,
            Sampling::Smooth => &self.smooth_sampler,
        };
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
            ],
        });
        let id = TextureId(self.textures.len() as u32);
        self.textures.push(GpuTexture { texture, view, bind_group, width, height, label: label.to_owned() });
        log::debug!("texture '{label}' {width}x{height} ({mip_level_count} mips) -> {id:?}");
        id
    }

    /// Overwrites a sub-rectangle of mip level 0 (for dynamic textures such as the minimap).
    #[allow(clippy::too_many_arguments)]
    pub fn update_rgba8(
        &self,
        queue: &wgpu::Queue,
        id: TextureId,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) {
        let tex = &self.textures[id.index()];
        assert!(x + width <= tex.width && y + height <= tex.height, "texture update out of bounds");
        assert_eq!(rgba.len(), (width * height * 4) as usize);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * width), rows_per_image: Some(height) },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
    }

    pub fn get(&self, id: TextureId) -> Option<&GpuTexture> {
        self.textures.get(id.index())
    }

    pub fn len(&self) -> usize {
        self.textures.len()
    }

    pub fn is_empty(&self) -> bool {
        self.textures.is_empty()
    }
}

fn write_level(queue: &wgpu::Queue, texture: &wgpu::Texture, level: u32, w: u32, h: u32, data: &[u8]) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: level,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        data,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * w), rows_per_image: Some(h) },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
}

fn srgb_to_linear(v: u8) -> f32 {
    let c = f32::from(v) / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn linear_to_srgb(c: f32) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let s = if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0 + 0.5) as u8
}

/// Generates up to `max_levels - 1` additional mip levels with a gamma-correct, alpha-weighted
/// 2×2 box filter. Returns `(width, height, rgba)` for each level after level 0.
pub fn generate_mips(rgba: &[u8], width: u32, height: u32, max_levels: u32) -> Vec<(u32, u32, Vec<u8>)> {
    let mut levels = Vec::new();
    let (mut w, mut h) = (width, height);
    let mut src: Vec<u8> = rgba.to_vec();
    while levels.len() as u32 + 1 < max_levels && (w > 1 || h > 1) {
        let nw = (w / 2).max(1);
        let nh = (h / 2).max(1);
        let mut dst = vec![0u8; (nw * nh * 4) as usize];
        for y in 0..nh {
            for x in 0..nw {
                let mut acc = [0.0f32; 3];
                let mut alpha = 0.0f32;
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let sx = (x * 2 + dx).min(w - 1);
                    let sy = (y * 2 + dy).min(h - 1);
                    let i = ((sy * w + sx) * 4) as usize;
                    let a = f32::from(src[i + 3]) / 255.0;
                    for c in 0..3 {
                        acc[c] += srgb_to_linear(src[i + c]) * a;
                    }
                    alpha += a;
                }
                let o = ((y * nw + x) * 4) as usize;
                if alpha > 0.0 {
                    for c in 0..3 {
                        dst[o + c] = linear_to_srgb(acc[c] / alpha);
                    }
                }
                dst[o + 3] = ((alpha / 4.0) * 255.0 + 0.5) as u8;
            }
        }
        levels.push((nw, nh, dst.clone()));
        src = dst;
        w = nw;
        h = nh;
    }
    levels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mip_chain_sizes() {
        let data = vec![255u8; 64 * 32 * 4];
        let mips = generate_mips(&data, 64, 32, 4);
        let sizes: Vec<_> = mips.iter().map(|(w, h, _)| (*w, *h)).collect();
        assert_eq!(sizes, vec![(32, 16), (16, 8), (8, 4)]);
    }

    #[test]
    fn transparent_pixels_do_not_darken() {
        // Half white opaque, half transparent black: the average colour must stay white.
        let mut data = vec![0u8; 2 * 2 * 4];
        for px in [0usize, 1] {
            data[px * 4..px * 4 + 4].copy_from_slice(&[255, 255, 255, 255]);
        }
        let mips = generate_mips(&data, 2, 2, 2);
        let (_, _, px) = &mips[0];
        assert_eq!(&px[0..3], &[255, 255, 255]);
        assert!((i32::from(px[3]) - 128).abs() <= 1);
    }
}
