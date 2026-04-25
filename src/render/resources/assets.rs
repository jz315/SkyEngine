use std::marker::PhantomData;
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::asset::{
    Asset, AssetError, AssetEvent, AssetEventKind, AssetId, AssetServer, AssetState, Handle,
    TextureAsset,
};
use crate::diagnostics::{DiagnosticEvent, DiagnosticSubsystem, Diagnostics};
use crate::gpu::GpuContext;
use crate::render::Texture;

const RENDER_TEXTURE_ASSET_MISSING_DIAGNOSTIC: &str = "render.asset.texture.missing";
const RENDER_TEXTURE_ASSET_FAILED_DIAGNOSTIC: &str = "render.asset.texture.failed";

pub trait RenderAsset {
    type Source: Asset;
    type Gpu: Clone;

    fn prepare(gpu: &GpuContext, source: &Self::Source) -> Self::Gpu;
}

pub struct RenderAssets<R: RenderAsset> {
    cached: FxHashMap<AssetId, CachedRenderAsset<R>>,
    requested: FxHashSet<AssetId>,
    marker: PhantomData<fn() -> R>,
}

struct CachedRenderAsset<R: RenderAsset> {
    source: Arc<R::Source>,
    gpu: R::Gpu,
}

impl<R: RenderAsset> RenderAssets<R> {
    #[inline]
    pub fn new() -> Self {
        Self {
            cached: FxHashMap::default(),
            requested: FxHashSet::default(),
            marker: PhantomData,
        }
    }

    pub fn resolve(
        &mut self,
        gpu: &GpuContext,
        assets: &AssetServer,
        handle: Handle<R::Source>,
    ) -> RenderAssetResolve<R::Gpu> {
        let id = handle.id();
        let Some(source) = assets.try_get(&handle) else {
            return self.resolve_pending(assets, handle);
        };

        self.requested.remove(&id);
        if let Some(cached) = self.cached.get(&id) {
            if Arc::ptr_eq(&cached.source, &source) {
                return RenderAssetResolve::Ready(cached.gpu.clone());
            }
        }

        let prepared = R::prepare(gpu, &source);
        self.cached.insert(
            id,
            CachedRenderAsset {
                source,
                gpu: prepared.clone(),
            },
        );
        RenderAssetResolve::Uploaded(prepared)
    }

    fn resolve_pending(
        &mut self,
        assets: &AssetServer,
        handle: Handle<R::Source>,
    ) -> RenderAssetResolve<R::Gpu> {
        let id = handle.id();
        match assets.state(&handle) {
            AssetState::Unloaded => {
                self.cached.remove(&id);
                if self.requested.insert(id) {
                    match assets.load::<R::Source>(id) {
                        Ok(_) => RenderAssetResolve::Loading,
                        Err(error) => {
                            self.requested.remove(&id);
                            self.cached.remove(&id);
                            if matches!(
                                error,
                                AssetError::AssetNotFound { .. }
                                    | AssetError::AssetPathNotFound { .. }
                            ) {
                                RenderAssetResolve::Missing
                            } else {
                                RenderAssetResolve::Failed
                            }
                        }
                    }
                } else {
                    RenderAssetResolve::Loading
                }
            }
            AssetState::Loading
            | AssetState::Loaded
            | AssetState::WaitingDependencies
            | AssetState::Installing => self
                .cached
                .get(&id)
                .map_or(RenderAssetResolve::Loading, |cached| {
                    RenderAssetResolve::Stale(cached.gpu.clone())
                }),
            AssetState::Uninstalling | AssetState::Unloading => {
                self.requested.remove(&id);
                self.cached.remove(&id);
                RenderAssetResolve::Loading
            }
            AssetState::Installed | AssetState::Failed => {
                self.requested.remove(&id);
                self.cached.remove(&id);
                RenderAssetResolve::Failed
            }
        }
    }

    #[inline]
    pub fn contains(&self, handle: Handle<R::Source>) -> bool {
        self.cached.contains_key(&handle.id())
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.cached.len()
    }

    #[inline]
    pub fn invalidate(&mut self, id: AssetId) {
        self.cached.remove(&id);
        self.requested.remove(&id);
    }

    #[inline]
    pub fn clear(&mut self) {
        self.cached.clear();
        self.requested.clear();
    }
}

impl<R: RenderAsset> Default for RenderAssets<R> {
    fn default() -> Self {
        Self::new()
    }
}

pub enum RenderAssetResolve<T> {
    Ready(T),
    Uploaded(T),
    Stale(T),
    Loading,
    Missing,
    Failed,
}

pub struct GpuTextureAsset;

impl RenderAsset for GpuTextureAsset {
    type Source = TextureAsset;
    type Gpu = Texture;

    fn prepare(gpu: &GpuContext, source: &TextureAsset) -> Texture {
        let format = match source.color_space() {
            crate::asset::TextureColorSpace::Linear => wgpu::TextureFormat::Rgba8Unorm,
            crate::asset::TextureColorSpace::Srgb => wgpu::TextureFormat::Rgba8UnormSrgb,
        };

        Texture::from_rgba8_with_format(
            gpu,
            source.width(),
            source.height(),
            source.pixels(),
            format,
            "asset_texture",
        )
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderAssetStats {
    pub resident_assets: usize,
    pub uploaded_assets: usize,
    pub loading_assets: usize,
    pub missing_assets: usize,
    pub failed_assets: usize,
}

#[derive(Default)]
struct RenderAssetFrameDiagnostics {
    uploaded: FxHashSet<AssetId>,
    loading: FxHashSet<AssetId>,
    missing: FxHashSet<AssetId>,
    failed: FxHashSet<AssetId>,
}

impl RenderAssetFrameDiagnostics {
    #[inline]
    fn clear(&mut self) {
        self.uploaded.clear();
        self.loading.clear();
        self.missing.clear();
        self.failed.clear();
    }

    #[inline]
    fn record_upload(&mut self, id: AssetId) {
        self.uploaded.insert(id);
    }

    #[inline]
    fn record_loading(&mut self, id: AssetId) {
        self.loading.insert(id);
    }

    #[inline]
    fn record_missing(&mut self, id: AssetId) {
        self.missing.insert(id);
    }

    #[inline]
    fn record_failed(&mut self, id: AssetId) {
        self.failed.insert(id);
    }

    #[inline]
    fn snapshot(&self, resident_assets: usize) -> RenderAssetStats {
        RenderAssetStats {
            resident_assets,
            uploaded_assets: self.uploaded.len(),
            loading_assets: self.loading.len(),
            missing_assets: self.missing.len(),
            failed_assets: self.failed.len(),
        }
    }

    fn report(&self, diagnostics: &Diagnostics) {
        for id in &self.missing {
            diagnostics.report_once(
                DiagnosticEvent::warning(
                    RENDER_TEXTURE_ASSET_MISSING_DIAGNOSTIC,
                    DiagnosticSubsystem::render(),
                    format!(
                        "A sprite referenced texture asset {id}, but it could not be resolved for \
                         rendering. The sprite fallback material will be used."
                    ),
                )
                .with_title("Texture asset is missing")
                .with_help(
                    "Check that the texture is registered in the asset manifest or inserted as a \
                     runtime asset before rendering.",
                )
                .with_field("asset_id", id.to_string())
                .with_field("asset_type", TextureAsset::TYPE)
                .with_once_key(format!("{RENDER_TEXTURE_ASSET_MISSING_DIAGNOSTIC}:{id}")),
            );
        }

        for id in &self.failed {
            diagnostics.report_once(
                DiagnosticEvent::error(
                    RENDER_TEXTURE_ASSET_FAILED_DIAGNOSTIC,
                    DiagnosticSubsystem::render(),
                    format!(
                        "Texture asset {id} failed to load or prepare for rendering. The sprite \
                         fallback material will be used."
                    ),
                )
                .with_title("Texture asset failed")
                .with_help(
                    "Check the asset server error for this asset and verify the source texture can \
                     be decoded.",
                )
                .with_field("asset_id", id.to_string())
                .with_field("asset_type", TextureAsset::TYPE)
                .with_once_key(format!("{RENDER_TEXTURE_ASSET_FAILED_DIAGNOSTIC}:{id}")),
            );
        }
    }
}

#[derive(Default)]
pub struct RenderAssetCache {
    textures: RenderAssets<GpuTextureAsset>,
    frame: RenderAssetFrameDiagnostics,
    stats: RenderAssetStats,
}

impl RenderAssetCache {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn begin_frame(&mut self) {
        self.frame.clear();
        self.stats = RenderAssetStats::default();
    }

    pub fn texture(
        &mut self,
        gpu: &GpuContext,
        assets: &AssetServer,
        handle: Handle<TextureAsset>,
    ) -> Option<Texture> {
        let id = handle.id();
        match self.textures.resolve(gpu, assets, handle) {
            RenderAssetResolve::Ready(texture) => Some(texture),
            RenderAssetResolve::Uploaded(texture) => {
                self.frame.record_upload(id);
                Some(texture)
            }
            RenderAssetResolve::Stale(texture) => {
                self.frame.record_loading(id);
                Some(texture)
            }
            RenderAssetResolve::Loading => {
                self.frame.record_loading(id);
                None
            }
            RenderAssetResolve::Missing => {
                self.frame.record_missing(id);
                None
            }
            RenderAssetResolve::Failed => {
                self.frame.record_failed(id);
                None
            }
        }
    }

    #[inline]
    pub fn mark_texture_missing(&mut self, handle: Handle<TextureAsset>) {
        self.frame.record_missing(handle.id());
    }

    #[inline]
    pub fn finish_frame(&mut self, diagnostics: Option<&Diagnostics>) -> RenderAssetStats {
        if let Some(diagnostics) = diagnostics {
            self.frame.report(diagnostics);
        }
        self.stats = self.frame.snapshot(self.textures.len());
        self.stats
    }

    #[inline]
    pub fn handle_asset_event(&mut self, event: AssetEvent) {
        match event.kind {
            AssetEventKind::Failed | AssetEventKind::Unloaded => {
                self.invalidate_texture(event.id);
            }
            AssetEventKind::ReloadQueued | AssetEventKind::Installed => {}
        }
    }

    #[inline]
    pub fn stats(&self) -> RenderAssetStats {
        self.stats
    }

    #[inline]
    pub fn contains_texture(&self, handle: Handle<TextureAsset>) -> bool {
        self.textures.contains(handle)
    }

    #[inline]
    pub fn invalidate_texture(&mut self, id: AssetId) {
        self.textures.invalidate(id);
    }

    #[inline]
    pub fn clear(&mut self) {
        self.textures.clear();
        self.frame.clear();
        self.stats = RenderAssetStats::default();
    }
}
