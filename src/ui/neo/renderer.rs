//! SkyEngine adapter for the standalone `eui-neo-wgpu` renderer.

use crate::gpu::GpuContext;

use super::font_provider::SkyNeoFontStore;
use super::image_provider::SkyNeoImageStore;
use super::Runtime;
use crate::asset::Assets;
use crate::render::SharedRenderAssetCache;

pub(crate) use eui_neo_wgpu::RenderStatus as NeoRenderStatus;

/// Thin SkyEngine surface-frame adapter around [`eui_neo_wgpu::WgpuRenderer`].
pub(crate) struct NeoRenderer {
    inner: eui_neo_wgpu::WgpuRenderer,
    images: SkyNeoImageStore,
    fonts: SkyNeoFontStore,
}

impl std::fmt::Debug for NeoRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NeoRenderer")
            .field("inner", &self.inner)
            .finish()
    }
}

impl NeoRenderer {
    pub(crate) fn new(gpu: &GpuContext) -> Self {
        Self {
            inner: eui_neo_wgpu::WgpuRenderer::new(gpu.device(), gpu.queue(), gpu.surface_format()),
            images: SkyNeoImageStore::default(),
            fonts: SkyNeoFontStore::default(),
        }
    }

    pub(crate) fn matches_surface(&self, format: wgpu::TextureFormat) -> bool {
        self.inner.matches_format(format)
    }

    pub(crate) fn render(
        &mut self,
        gpu: &mut GpuContext,
        runtime: &mut Runtime,
        asset_server: Option<&Assets>,
        render_assets: Option<&SharedRenderAssetCache>,
    ) -> NeoRenderStatus {
        let frame = runtime.current_frame();
        let pending_fonts =
            self.fonts
                .prepare(runtime, &mut self.inner, frame.draw_list(), asset_server);
        let pending_images =
            self.images
                .prepare(gpu, frame.draw_list(), asset_server, render_assets);
        let mut resources = self.images.provider();
        let mut status = gpu.with_surface_frame_parts(|parts| {
            let mut target = eui_neo_wgpu::Target {
                device: parts.device,
                queue: parts.queue,
                encoder: parts.encoder,
                view: parts.surface_view,
                target_texture: parts.surface_copy_supported.then_some(
                    eui_neo_wgpu::TargetTexture {
                        texture: parts.surface_texture,
                    },
                ),
                format: parts.surface_format,
                physical_size: parts.surface_size,
            };
            self.inner.render(&mut target, &frame, &mut resources)
        });
        status.pending_images |= pending_images;
        status.pending_fonts |= pending_fonts;
        status
    }
}
