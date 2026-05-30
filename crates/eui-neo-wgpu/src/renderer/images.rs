#![allow(unused_imports)]
use super::backdrop::*;
use super::buffers::*;
use super::collect::*;
use super::primitives::*;
use super::text::*;
use super::*;

#[derive(Clone)]
pub(super) struct ImageItem {
    pub(super) image: ImageRef,
}

pub(super) struct CachedNeoImage {
    pub(super) _texture: Option<wgpu::Texture>,
    pub(super) bind_group: wgpu::BindGroup,
    pub(super) size: [u32; 2],
    pub(super) uv_rect: [f32; 4],
    pub(super) revision: u64,
}

/// Borrowed GPU image supplied by a host image provider.
#[derive(Clone, Copy)]
pub struct GpuImage<'a> {
    pub revision: u64,
    pub size: [u32; 2],
    pub uv_rect: [f32; 4],
    pub view: &'a wgpu::TextureView,
    pub sampler: Option<&'a wgpu::Sampler>,
}

/// Borrowed decoded RGBA8 image supplied by a host image provider.
#[derive(Clone, Copy)]
pub struct ImagePixels<'a> {
    pub revision: u64,
    pub size: [u32; 2],
    pub uv_rect: [f32; 4],
    pub rgba: &'a [u8],
}

/// Current state for an image reference requested by the frame.
#[derive(Clone, Copy)]
pub enum ImageState<'a> {
    Gpu(GpuImage<'a>),
    Pixels(ImagePixels<'a>),
    Pending,
    Missing,
    Failed,
}

/// Host-provided resource lookup used by the renderer.
pub trait Resources {
    fn image<'a>(&'a mut self, image: &ImageRef) -> ImageState<'a>;
}

/// Empty resources that report every image as missing.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoResources;

impl Resources for NoResources {
    fn image<'a>(&'a mut self, _image: &ImageRef) -> ImageState<'a> {
        ImageState::Missing
    }
}

impl WgpuRenderer {
    pub(super) fn prepare_images(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        draw_list: &UiDrawList,
        resources: &mut dyn Resources,
    ) -> bool {
        let mut pending = false;
        for command in draw_list.commands() {
            let key = match command {
                UiDrawCommand::Image(draw) => &draw.image,
                UiDrawCommand::NineSlice(draw) => &draw.image,
                _ => continue,
            };
            if key.is_empty() {
                continue;
            }
            match resources.image(key) {
                ImageState::Gpu(image) => {
                    self.cache_gpu_image(device, key, image);
                }
                ImageState::Pixels(pixels) => {
                    self.cache_pixel_image(device, queue, key, pixels);
                }
                ImageState::Pending => {
                    pending = true;
                }
                ImageState::Missing => {}
                ImageState::Failed => {}
            }
        }
        pending
    }

    fn cache_gpu_image(&mut self, device: &wgpu::Device, key: &ImageRef, image: GpuImage<'_>) {
        if image.size[0] == 0 || image.size[1] == 0 {
            return;
        }
        if self.image_cache.get(key).is_some_and(|cached| {
            cached._texture.is_none()
                && cached.revision == image.revision
                && cached.size == image.size
                && cached.uv_rect == image.uv_rect
        }) {
            return;
        }

        let sampler = image.sampler.unwrap_or(&self.sampler_linear);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("eui_neo_provider_image_bg"),
            layout: &self.wgpu_resources.image_texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(image.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        self.image_cache.insert(
            key.clone(),
            CachedNeoImage {
                _texture: None,
                bind_group,
                size: image.size,
                uv_rect: image.uv_rect,
                revision: image.revision,
            },
        );
        self.image_cache_revision = self.image_cache_revision.wrapping_add(1);
    }

    fn cache_pixel_image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: &ImageRef,
        pixels: ImagePixels<'_>,
    ) {
        if pixels.size[0] == 0 || pixels.size[1] == 0 {
            return;
        }
        let expected_len = pixels.size[0] as usize * pixels.size[1] as usize * 4;
        if pixels.rgba.len() < expected_len {
            return;
        }
        if self.image_cache.get(key).is_some_and(|cached| {
            cached._texture.is_some()
                && cached.revision == pixels.revision
                && cached.size == pixels.size
                && cached.uv_rect == pixels.uv_rect
        }) {
            return;
        }

        let cached = upload_image_pixels(
            device,
            queue,
            &self.sampler_linear,
            &self.wgpu_resources.image_texture_layout,
            pixels,
        );
        self.image_cache.insert(key.clone(), cached);
        self.image_cache_revision = self.image_cache_revision.wrapping_add(1);
    }
}

pub(super) fn upload_image_pixels(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    sampler: &wgpu::Sampler,
    image_texture_layout: &wgpu::BindGroupLayout,
    pixels: ImagePixels<'_>,
) -> CachedNeoImage {
    let [width, height] = pixels.size;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("eui_neo_image_texture"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        pixels.rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * width.max(1)),
            rows_per_image: Some(height.max(1)),
        },
        wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("eui_neo_image_texture_bg"),
        layout: image_texture_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    });
    CachedNeoImage {
        _texture: Some(texture),
        bind_group,
        size: [width.max(1), height.max(1)],
        uv_rect: pixels.uv_rect,
        revision: pixels.revision,
    }
}
