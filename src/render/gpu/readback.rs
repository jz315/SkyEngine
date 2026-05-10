//! CPU texture readback helpers for renderer debugging.

use std::fmt;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use std::sync::mpsc;

use crate::gpu::GpuContext;
use crate::render::gpu::{RenderTarget, Texture};

#[derive(Debug)]
pub enum TextureReadbackError {
    UnsupportedFormat(wgpu::TextureFormat),
    InvalidSubresource { mip_level: u32, array_layer: u32 },
    MissingCopySrcUsage,
    MapFailed(String),
    Io(io::Error),
    Image(image::ImageError),
}

impl fmt::Display for TextureReadbackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedFormat(format) => write!(f, "unsupported readback format {format:?}"),
            Self::InvalidSubresource {
                mip_level,
                array_layer,
            } => write!(
                f,
                "invalid texture readback subresource mip={mip_level} layer={array_layer}"
            ),
            Self::MissingCopySrcUsage => write!(f, "texture readback requires COPY_SRC usage"),
            Self::MapFailed(message) => write!(f, "texture readback buffer map failed: {message}"),
            Self::Io(err) => write!(f, "{err}"),
            Self::Image(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for TextureReadbackError {}

impl From<io::Error> for TextureReadbackError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<image::ImageError> for TextureReadbackError {
    fn from(value: image::ImageError) -> Self {
        Self::Image(value)
    }
}

#[derive(Debug, Clone)]
pub struct TextureReadback {
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    bytes_per_pixel: u32,
    data: Vec<u8>,
}

impl TextureReadback {
    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[inline]
    pub fn height(&self) -> u32 {
        self.height
    }

    #[inline]
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    #[inline]
    pub fn bytes_per_pixel(&self) -> u32 {
        self.bytes_per_pixel
    }

    #[inline]
    pub fn bytes_per_row(&self) -> u32 {
        self.width * self.bytes_per_pixel
    }

    #[inline]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn write_raw_bin(&self, path: impl AsRef<Path>) -> Result<(), TextureReadbackError> {
        std::fs::write(path, &self.data)?;
        Ok(())
    }

    pub fn write_png(&self, path: impl AsRef<Path>) -> Result<(), TextureReadbackError> {
        let pixels = self.to_rgba8()?;
        image::save_buffer(
            path,
            &pixels,
            self.width,
            self.height,
            image::ColorType::Rgba8,
        )?;
        Ok(())
    }

    pub fn write_hdr(&self, path: impl AsRef<Path>) -> Result<(), TextureReadbackError> {
        let mut file = File::create(path)?;
        file.write_all(b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n")?;
        writeln!(file, "-Y {} +X {}", self.height, self.width)?;
        for y in 0..self.height {
            for x in 0..self.width {
                file.write_all(&float_rgb_to_rgbe(self.pixel_rgb_f32(x, y)?))?;
            }
        }
        Ok(())
    }

    fn to_rgba8(&self) -> Result<Vec<u8>, TextureReadbackError> {
        let mut out = vec![0; (self.width * self.height * 4) as usize];
        for y in 0..self.height {
            for x in 0..self.width {
                let dst = ((y * self.width + x) * 4) as usize;
                match self.format {
                    wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb => {
                        let src = self.pixel_offset(x, y);
                        out[dst..dst + 4].copy_from_slice(&self.data[src..src + 4]);
                    }
                    wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb => {
                        let src = self.pixel_offset(x, y);
                        out[dst] = self.data[src + 2];
                        out[dst + 1] = self.data[src + 1];
                        out[dst + 2] = self.data[src];
                        out[dst + 3] = self.data[src + 3];
                    }
                    wgpu::TextureFormat::R32Float | wgpu::TextureFormat::Depth32Float => {
                        let v = (self.pixel_rgb_f32(x, y)?[0].clamp(0.0, 1.0) * 255.0) as u8;
                        out[dst..dst + 4].copy_from_slice(&[v, v, v, 255]);
                    }
                    _ => return Err(TextureReadbackError::UnsupportedFormat(self.format)),
                }
            }
        }
        Ok(out)
    }

    fn pixel_rgb_f32(&self, x: u32, y: u32) -> Result<[f32; 3], TextureReadbackError> {
        let offset = self.pixel_offset(x, y);
        Ok(match self.format {
            wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb => [
                self.data[offset] as f32 / 255.0,
                self.data[offset + 1] as f32 / 255.0,
                self.data[offset + 2] as f32 / 255.0,
            ],
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb => [
                self.data[offset + 2] as f32 / 255.0,
                self.data[offset + 1] as f32 / 255.0,
                self.data[offset] as f32 / 255.0,
            ],
            wgpu::TextureFormat::R32Float | wgpu::TextureFormat::Depth32Float => {
                let v = f32::from_le_bytes(self.data[offset..offset + 4].try_into().unwrap());
                [v, v, v]
            }
            wgpu::TextureFormat::Rg16Float => [
                f16_to_f32(u16::from_le_bytes(
                    self.data[offset..offset + 2].try_into().unwrap(),
                )),
                f16_to_f32(u16::from_le_bytes(
                    self.data[offset + 2..offset + 4].try_into().unwrap(),
                )),
                0.0,
            ],
            wgpu::TextureFormat::Rgba16Float => [
                f16_to_f32(u16::from_le_bytes(
                    self.data[offset..offset + 2].try_into().unwrap(),
                )),
                f16_to_f32(u16::from_le_bytes(
                    self.data[offset + 2..offset + 4].try_into().unwrap(),
                )),
                f16_to_f32(u16::from_le_bytes(
                    self.data[offset + 4..offset + 6].try_into().unwrap(),
                )),
            ],
            wgpu::TextureFormat::Rgba32Float => [
                f32::from_le_bytes(self.data[offset..offset + 4].try_into().unwrap()),
                f32::from_le_bytes(self.data[offset + 4..offset + 8].try_into().unwrap()),
                f32::from_le_bytes(self.data[offset + 8..offset + 12].try_into().unwrap()),
            ],
            _ => return Err(TextureReadbackError::UnsupportedFormat(self.format)),
        })
    }

    #[inline]
    fn pixel_offset(&self, x: u32, y: u32) -> usize {
        ((y * self.width + x) * self.bytes_per_pixel) as usize
    }
}

pub fn read_render_target(
    ctx: &GpuContext,
    target: &RenderTarget,
) -> Result<TextureReadback, TextureReadbackError> {
    read_render_target_subresource(ctx, target, 0, 0)
}

pub fn read_render_target_subresource(
    ctx: &GpuContext,
    target: &RenderTarget,
    mip_level: u32,
    array_layer: u32,
) -> Result<TextureReadback, TextureReadbackError> {
    if !target.usage().contains(wgpu::TextureUsages::COPY_SRC) {
        return Err(TextureReadbackError::MissingCopySrcUsage);
    }
    if mip_level >= target.mip_level_count() || array_layer >= target.array_layer_count() {
        return Err(TextureReadbackError::InvalidSubresource {
            mip_level,
            array_layer,
        });
    }
    let (width, height) = target.mip_extent(mip_level);
    read_texture_subresource(
        ctx,
        target.texture(),
        target.format(),
        [width, height],
        mip_level,
        array_layer,
    )
}

pub fn read_texture(
    ctx: &GpuContext,
    texture: &Texture,
) -> Result<TextureReadback, TextureReadbackError> {
    if !texture.usage().contains(wgpu::TextureUsages::COPY_SRC) {
        return Err(TextureReadbackError::MissingCopySrcUsage);
    }
    read_texture_subresource(
        ctx,
        texture.texture(),
        texture.format(),
        [texture.width(), texture.height()],
        0,
        0,
    )
}

pub fn read_texture_subresource(
    ctx: &GpuContext,
    texture: &wgpu::Texture,
    format: wgpu::TextureFormat,
    size: [u32; 2],
    mip_level: u32,
    array_layer: u32,
) -> Result<TextureReadback, TextureReadbackError> {
    let info = format_info(format).ok_or(TextureReadbackError::UnsupportedFormat(format))?;
    let width = size[0].max(1);
    let height = size[1].max(1);
    let tight_row_bytes = width * info.bytes_per_pixel;
    let padded_row_bytes = align_to(tight_row_bytes, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer_size = padded_row_bytes as u64 * height as u64;
    let buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("texture_readback_buffer"),
        size: buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = ctx
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("texture_readback_encoder"),
        });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level,
            origin: wgpu::Origin3d {
                x: 0,
                y: 0,
                z: array_layer,
            },
            aspect: info.aspect,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row_bytes),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    ctx.queue().submit(std::iter::once(encoder.finish()));

    let slice = buffer.slice(..);
    let (sender, receiver) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result.map(|_| ()));
    });
    let _ = ctx.device().poll(wgpu::PollType::wait_indefinitely());
    receiver
        .recv()
        .map_err(|err| TextureReadbackError::MapFailed(err.to_string()))?
        .map_err(|err| TextureReadbackError::MapFailed(err.to_string()))?;

    let mapped = slice.get_mapped_range();
    let mut data = vec![0; (tight_row_bytes * height) as usize];
    for y in 0..height as usize {
        let src = y * padded_row_bytes as usize;
        let dst = y * tight_row_bytes as usize;
        data[dst..dst + tight_row_bytes as usize]
            .copy_from_slice(&mapped[src..src + tight_row_bytes as usize]);
    }
    drop(mapped);
    buffer.unmap();

    Ok(TextureReadback {
        width,
        height,
        format,
        bytes_per_pixel: info.bytes_per_pixel,
        data,
    })
}

#[derive(Clone, Copy)]
struct FormatInfo {
    bytes_per_pixel: u32,
    aspect: wgpu::TextureAspect,
}

fn format_info(format: wgpu::TextureFormat) -> Option<FormatInfo> {
    let aspect = if matches!(
        format,
        wgpu::TextureFormat::Depth16Unorm | wgpu::TextureFormat::Depth32Float
    ) {
        wgpu::TextureAspect::DepthOnly
    } else {
        wgpu::TextureAspect::All
    };
    let bytes_per_pixel = match format {
        wgpu::TextureFormat::Rgba8Unorm
        | wgpu::TextureFormat::Rgba8UnormSrgb
        | wgpu::TextureFormat::Bgra8Unorm
        | wgpu::TextureFormat::Bgra8UnormSrgb
        | wgpu::TextureFormat::Rgba8Snorm
        | wgpu::TextureFormat::Rgba8Uint
        | wgpu::TextureFormat::Rgba8Sint
        | wgpu::TextureFormat::R32Float
        | wgpu::TextureFormat::Rg16Float
        | wgpu::TextureFormat::Depth32Float => 4,
        wgpu::TextureFormat::Rgba16Float => 8,
        wgpu::TextureFormat::Rgba32Float => 16,
        wgpu::TextureFormat::Depth16Unorm => 2,
        _ => return None,
    };
    Some(FormatInfo {
        bytes_per_pixel,
        aspect,
    })
}

#[inline]
fn align_to(value: u32, alignment: u32) -> u32 {
    value.div_ceil(alignment) * alignment
}

fn f16_to_f32(bits: u16) -> f32 {
    let sign = ((bits & 0x8000) as u32) << 16;
    let exponent = ((bits >> 10) & 0x1f) as i32;
    let mantissa = (bits & 0x03ff) as u32;
    let out = if exponent == 0 {
        if mantissa == 0 {
            sign
        } else {
            let mut mantissa = mantissa;
            let mut exponent = -14;
            while (mantissa & 0x0400) == 0 {
                mantissa <<= 1;
                exponent -= 1;
            }
            mantissa &= 0x03ff;
            sign | (((exponent + 127) as u32) << 23) | (mantissa << 13)
        }
    } else if exponent == 0x1f {
        sign | 0x7f80_0000 | (mantissa << 13)
    } else {
        sign | (((exponent - 15 + 127) as u32) << 23) | (mantissa << 13)
    };
    f32::from_bits(out)
}

fn float_rgb_to_rgbe(rgb: [f32; 3]) -> [u8; 4] {
    let r = rgb[0].max(0.0);
    let g = rgb[1].max(0.0);
    let b = rgb[2].max(0.0);
    let max_component = r.max(g).max(b);
    if max_component < 1.0e-32 || !max_component.is_finite() {
        return [0, 0, 0, 0];
    }

    let exponent = max_component.log2().floor() as i32 + 1;
    let scale = 256.0 / 2.0f32.powi(exponent);
    [
        (r * scale).clamp(0.0, 255.0) as u8,
        (g * scale).clamp(0.0, 255.0) as u8,
        (b * scale).clamp(0.0, 255.0) as u8,
        (exponent + 128).clamp(0, 255) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("No suitable GPU adapter found for readback tests");

        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("readback_test_device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            ..Default::default()
        }))
        .expect("Failed to create test GPU device")
    }

    #[test]
    fn readback_copies_render_target_to_tight_cpu_rows() {
        let (device, queue) = create_test_device();
        let ctx = GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [4, 4]);
        let target = RenderTarget::from_descriptor(
            &ctx,
            crate::render::gpu::RenderTargetDescriptor::new(2, 2, wgpu::TextureFormat::Rgba8Unorm)
                .usage(
                    wgpu::TextureUsages::COPY_DST
                        | wgpu::TextureUsages::COPY_SRC
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                ),
        );
        let pixels = [
            255u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ];
        ctx.queue().write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: target.texture(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(8),
                rows_per_image: Some(2),
            },
            wgpu::Extent3d {
                width: 2,
                height: 2,
                depth_or_array_layers: 1,
            },
        );

        let readback = read_render_target(&ctx, &target).expect("readback should succeed");
        assert_eq!(readback.width(), 2);
        assert_eq!(readback.height(), 2);
        assert_eq!(readback.bytes_per_row(), 8);
        assert_eq!(readback.data(), pixels);
    }

    #[test]
    fn readback_writes_png_hdr_and_raw_bin() {
        let readback = TextureReadback {
            width: 1,
            height: 1,
            format: wgpu::TextureFormat::Rgba8Unorm,
            bytes_per_pixel: 4,
            data: vec![255, 128, 0, 255],
        };
        let dir = std::env::temp_dir();
        let raw = dir.join("sky_readback_test.bin");
        let png = dir.join("sky_readback_test.png");
        let hdr = dir.join("sky_readback_test.hdr");

        readback.write_raw_bin(&raw).expect("raw dump should write");
        readback.write_png(&png).expect("png dump should write");
        readback.write_hdr(&hdr).expect("hdr dump should write");

        assert_eq!(
            std::fs::read(&raw).expect("raw dump should exist"),
            readback.data()
        );
        assert!(
            std::fs::metadata(&png)
                .expect("png dump should exist")
                .len()
                > 0
        );
        assert!(std::fs::read(&hdr)
            .expect("hdr dump should exist")
            .starts_with(b"#?RADIANCE"));

        let _ = std::fs::remove_file(raw);
        let _ = std::fs::remove_file(png);
        let _ = std::fs::remove_file(hdr);
    }
}
