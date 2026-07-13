//! SkyEngine adapter for the standalone `serein-wgpu` renderer.

use crate::gpu::GpuContext;

use super::font_provider::SkySereinFontStore;
use super::image_provider::SkySereinImageStore;
use super::Runtime;
use crate::asset::Assets;
use crate::render::SharedRenderAssetCache;
use serein::{RendererResourceDirty, ResourceDirty};

pub(crate) use serein_wgpu::RenderStatus as SereinRenderStatus;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SereinRenderResourceStatus {
    pub(crate) render: SereinRenderStatus,
    pub(crate) ready_images: bool,
    pub(crate) ready_fonts: bool,
}

impl SereinRenderResourceStatus {
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

/// Thin SkyEngine surface-frame adapter around [`serein_wgpu::WgpuRenderer`].
pub(crate) struct SereinRenderer {
    inner: serein_wgpu::WgpuRenderer,
    images: SkySereinImageStore,
    fonts: SkySereinFontStore,
}

impl std::fmt::Debug for SereinRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SereinRenderer")
            .field("inner", &self.inner)
            .finish()
    }
}

impl SereinRenderer {
    pub(crate) fn new(gpu: &GpuContext) -> Self {
        Self {
            inner: serein_wgpu::WgpuRenderer::new(gpu.device(), gpu.queue(), gpu.surface_format()),
            images: SkySereinImageStore::default(),
            fonts: SkySereinFontStore::default(),
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
    ) -> SereinRenderResourceStatus {
        let frame = runtime.current_frame();
        let font_status =
            self.fonts
                .prepare(runtime, &mut self.inner, frame.draw_list(), asset_server);
        let image_status = self
            .images
            .prepare(gpu, frame.draw_list(), asset_server, render_assets);
        let mut resources = self.images.provider();
        let mut render_status = gpu.with_surface_frame_parts(|parts| {
            let mut target = serein_wgpu::Target {
                device: parts.device,
                queue: parts.queue,
                encoder: parts.encoder,
                view: parts.surface_view,
                target_texture: parts.surface_copy_supported.then_some(
                    serein_wgpu::TargetTexture {
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
        SereinRenderResourceStatus {
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
        assert!(!SereinRenderResourceStatus::default().has_pending_resources());
        assert!(SereinRenderResourceStatus {
            render: SereinRenderStatus {
                pending_images: true,
                pending_fonts: false,
                ..SereinRenderStatus::default()
            },
            ready_images: true,
            ready_fonts: true,
        }
        .has_pending_resources());
    }

    #[test]
    fn render_resource_status_separates_draw_and_layout_dirty_records() {
        let status = SereinRenderResourceStatus {
            render: SereinRenderStatus {
                pending_images: true,
                pending_fonts: true,
                ..SereinRenderStatus::default()
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
            assert_eq!(dirty.flags(), serein::DirtyFlags::DRAW);
        }
        assert_eq!(
            dirty.last().unwrap().flags(),
            serein::DirtyFlags::COMPOSE | serein::DirtyFlags::LAYOUT | serein::DirtyFlags::DRAW
        );
    }
}
