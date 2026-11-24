//! Frame capture (screenshots): GPU texture → PNG file.

use std::path::PathBuf;
use std::time::Duration;

/// A readback buffer prepared for one captured frame.
pub(crate) struct PendingCapture {
    buffer: wgpu::Buffer,
    width: u32,
    height: u32,
    padded_bytes_per_row: u32,
    bgra: bool,
    path: PathBuf,
}

/// Creates an offscreen colour target usable as a copy source.
pub(crate) fn create_offscreen_target(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("capture target"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[format.remove_srgb_suffix()],
    })
}

/// Records a copy of `texture` into a mappable buffer.
pub(crate) fn encode_copy(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    texture: &wgpu::Texture,
    path: PathBuf,
) -> PendingCapture {
    let width = texture.width();
    let height = texture.height();
    let unpadded = width * 4;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_bytes_per_row = unpadded.div_ceil(align) * align;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("capture readback"),
        size: u64::from(padded_bytes_per_row) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    let bgra = matches!(
        texture.format(),
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    );
    PendingCapture { buffer, width, height, padded_bytes_per_row, bgra, path }
}

/// Waits for the copy to finish and writes the PNG on a background thread.
pub(crate) fn finish(device: &wgpu::Device, capture: PendingCapture) {
    let slice = capture.buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |res| {
        let _ = tx.send(res);
    });
    if let Err(err) =
        device.poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(Duration::from_secs(5)) })
    {
        log::error!("screenshot: device poll failed: {err}");
        return;
    }
    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            log::error!("screenshot: buffer mapping failed: {err}");
            return;
        }
        Err(_) => {
            log::error!("screenshot: timed out waiting for GPU readback");
            return;
        }
    }
    let mut rgba = Vec::with_capacity((capture.width * capture.height * 4) as usize);
    {
        let data = match slice.get_mapped_range() {
            Ok(data) => data,
            Err(err) => {
                log::error!("screenshot: could not access mapped buffer: {err}");
                return;
            }
        };
        for row in 0..capture.height {
            let start = (row * capture.padded_bytes_per_row) as usize;
            let line = &data[start..start + (capture.width * 4) as usize];
            if capture.bgra {
                for px in line.chunks_exact(4) {
                    rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
                }
            } else {
                for px in line.chunks_exact(4) {
                    rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
                }
            }
        }
    }
    capture.buffer.unmap();
    let PendingCapture { width, height, path, .. } = capture;
    std::thread::spawn(move || {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match image::save_buffer(&path, &rgba, width, height, image::ExtendedColorType::Rgba8) {
            Ok(()) => log::info!("screenshot saved to {}", path.display()),
            Err(err) => log::error!("could not save screenshot {}: {err}", path.display()),
        }
    });
}
