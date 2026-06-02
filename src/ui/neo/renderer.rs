//! SkyEngine adapter for the standalone `eui-neo-wgpu` renderer.

use crate::gpu::GpuContext;

use super::font_provider::SkyNeoFontStore;
use super::image_provider::SkyNeoImageStore;
use super::Runtime;
use crate::asset::Assets;
use crate::render::SharedRenderAssetCache;
use eui_neo::{RendererResourceDirty, ResourceDirty};

pub(crate) use eui_neo_wgpu::RenderStatus as NeoRenderStatus;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct NeoRenderResourceStatus {
    pub(crate) render: NeoRenderStatus,
    pub(crate) ready_images: bool,
    pub(crate) ready_fonts: bool,
}

impl NeoRenderResourceStatus {
    pub(crate) fn has_pending_resources(self) -> bool {
        self.render.pending_images || self.render.pending_fonts
    }

    pub(crate) fn resource_dirty(self) -> Vec<ResourceDirty> {
        let mut dirty = Vec::with_capacity(4);
        if self.render.pending_images {
            dirty.push(ResourceDirty::draw(RendererResourceDirty::PendingImages));
        }
        if self.render.pending_fonts {
            dirty.push(ResourceDirty::draw(RendererResourceDirty::PendingFonts));
        }
        if self.ready_images {
            dirty.push(ResourceDirty::draw(RendererResourceDirty::ReadyImages));
        }
        if self.ready_fonts {
            dirty.push(ResourceDirty::layout(RendererResourceDirty::ReadyFonts));
        }
        dirty
    }
}

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
    ) -> NeoRenderResourceStatus {
        let frame = runtime.current_frame();
        let font_status =
            self.fonts
                .prepare(runtime, &mut self.inner, frame.draw_list(), asset_server);
        let image_status = self
            .images
            .prepare(gpu, frame.draw_list(), asset_server, render_assets);
        let mut resources = self.images.provider();
        let mut render_status = gpu.with_surface_frame_parts(|parts| {
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
        render_status.pending_images |= image_status.pending;
        render_status.pending_fonts |= font_status.pending;
        NeoRenderResourceStatus {
            render: render_status,
            ready_images: image_status.ready_changed,
            ready_fonts: font_status.ready_changed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_resource_status_reports_pending_resources() {
        assert!(!NeoRenderResourceStatus::default().has_pending_resources());
        assert!(NeoRenderResourceStatus {
            render: NeoRenderStatus {
                pending_images: true,
                pending_fonts: false,
            },
            ready_images: true,
            ready_fonts: true,
        }
        .has_pending_resources());
    }

    #[test]
    fn render_resource_status_separates_draw_and_layout_dirty_records() {
        let status = NeoRenderResourceStatus {
            render: NeoRenderStatus {
                pending_images: true,
                pending_fonts: true,
            },
            ready_images: true,
            ready_fonts: true,
        };

        let dirty = status.resource_dirty();
        let sources: Vec<_> = dirty
            .iter()
            .map(|dirty| dirty.source_id().renderer_kind())
            .collect();
        assert_eq!(
            sources,
            vec![
                Some(RendererResourceDirty::PendingImages),
                Some(RendererResourceDirty::PendingFonts),
                Some(RendererResourceDirty::ReadyImages),
                Some(RendererResourceDirty::ReadyFonts),
            ]
        );
        let labels: Vec<_> = dirty.iter().map(|dirty| dirty.source()).collect();
        assert_eq!(
            labels,
            vec![
                RendererResourceDirty::PendingImages.label(),
                RendererResourceDirty::PendingFonts.label(),
                RendererResourceDirty::ReadyImages.label(),
                RendererResourceDirty::ReadyFonts.label(),
            ]
        );
        for dirty in dirty.iter().take(3) {
            assert_eq!(dirty.flags(), eui_neo::DirtyFlags::DRAW);
        }
        assert_eq!(
            dirty.last().unwrap().flags(),
            eui_neo::DirtyFlags::COMPOSE | eui_neo::DirtyFlags::LAYOUT | eui_neo::DirtyFlags::DRAW
        );
    }
}
