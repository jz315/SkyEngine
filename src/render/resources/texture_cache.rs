use std::cell::RefCell;
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::asset::{
    Asset, AssetError, AssetEvent, AssetEventKind, AssetId, AssetState, Assets, Handle,
    TextureAsset,
};
use crate::gpu::GpuContext;
use crate::render::runtime::{elapsed_ms, timing_start};
use crate::render::Texture;

const RENDER_TEXTURE_ASSET_MISSING_LOG: &str = "render.asset.texture.missing";
const RENDER_TEXTURE_ASSET_FAILED_LOG: &str = "render.asset.texture.failed";
const TEXTURE_PREPARE_BATCH_SIZE: usize = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RenderAssetStats {
    pub resident_assets: usize,
    pub uploaded_assets: usize,
    pub uploaded_bytes: usize,
    pub queued_assets: usize,
    pub visible_queued_assets: usize,
    pub loading_assets: usize,
    pub fallback_assets: usize,
    pub missing_assets: usize,
    pub failed_assets: usize,
    pub upload_ms: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureReadiness {
    MissingCpu,
    CpuLoading,
    CpuReady,
    GpuQueued,
    GpuReady,
    Failed,
}

struct CachedGpuTexture {
    source: Arc<TextureAsset>,
    texture: Texture,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TexturePreparePriority {
    Background,
    Preload,
    Imminent,
    Visible,
}

#[derive(Clone)]
struct QueuedGpuTexture {
    source: Arc<TextureAsset>,
    priority: TexturePreparePriority,
    requested_order: u64,
    byte_size: usize,
}

#[derive(Default)]
struct TextureGpuCache {
    textures: FxHashMap<AssetId, CachedGpuTexture>,
    queue: TexturePrepareQueue,
    requested: FxHashSet<AssetId>,
}

#[derive(Default)]
struct TexturePrepareQueue {
    queued: FxHashMap<AssetId, QueuedGpuTexture>,
    next_order: u64,
}

impl TexturePrepareQueue {
    #[inline]
    fn len(&self) -> usize {
        self.queued.len()
    }

    #[inline]
    fn visible_len(&self) -> usize {
        self.queued
            .values()
            .filter(|entry| entry.priority == TexturePreparePriority::Visible)
            .count()
    }

    #[inline]
    fn contains_current(&self, id: AssetId, source: &Arc<TextureAsset>) -> bool {
        self.queued
            .get(&id)
            .is_some_and(|entry| Arc::ptr_eq(&entry.source, source))
    }

    fn push(&mut self, id: AssetId, source: Arc<TextureAsset>, priority: TexturePreparePriority) {
        let byte_size = source.pixels().len();
        if let Some(entry) = self.queued.get_mut(&id) {
            if Arc::ptr_eq(&entry.source, &source) {
                entry.priority = entry.priority.max(priority);
                entry.byte_size = byte_size;
                return;
            }
        }

        let requested_order = self.next_order;
        self.next_order = self.next_order.wrapping_add(1);
        self.queued.insert(
            id,
            QueuedGpuTexture {
                source,
                priority,
                requested_order,
                byte_size,
            },
        );
    }

    fn pop_next(&mut self) -> Option<(AssetId, QueuedGpuTexture)> {
        let id = self
            .queued
            .iter()
            .max_by(|(_, left), (_, right)| {
                left.priority
                    .cmp(&right.priority)
                    .then_with(|| right.requested_order.cmp(&left.requested_order))
                    .then_with(|| right.byte_size.cmp(&left.byte_size))
            })
            .map(|(id, _)| *id)?;
        self.queued.remove_entry(&id)
    }

    fn pop_id(&mut self, id: AssetId) -> Option<QueuedGpuTexture> {
        self.queued.remove(&id)
    }

    fn remove(&mut self, id: AssetId) {
        self.queued.remove(&id);
    }

    fn retain_current(&mut self, id: AssetId, source: Option<&Arc<TextureAsset>>) {
        if source.map_or(true, |source| {
            self.queued
                .get(&id)
                .is_some_and(|entry| !Arc::ptr_eq(&entry.source, source))
        }) {
            self.queued.remove(&id);
        }
    }

    fn clear(&mut self) {
        self.queued.clear();
    }
}

#[derive(Default)]
struct RenderAssetFrameStats {
    uploaded: FxHashSet<AssetId>,
    uploaded_bytes: usize,
    upload_ms: f64,
    loading: FxHashSet<AssetId>,
    fallback: FxHashSet<AssetId>,
    missing: FxHashSet<AssetId>,
    failed: FxHashSet<AssetId>,
}

impl RenderAssetFrameStats {
    #[inline]
    fn clear(&mut self) {
        self.uploaded.clear();
        self.uploaded_bytes = 0;
        self.upload_ms = 0.0;
        self.loading.clear();
        self.fallback.clear();
        self.missing.clear();
        self.failed.clear();
    }

    #[inline]
    fn record_upload(&mut self, id: AssetId, bytes: usize, upload_ms: f64) {
        self.uploaded.insert(id);
        self.uploaded_bytes = self.uploaded_bytes.saturating_add(bytes);
        self.upload_ms += upload_ms;
    }

    #[inline]
    fn record_loading(&mut self, id: AssetId) {
        self.loading.insert(id);
    }

    #[inline]
    fn record_fallback(&mut self, id: AssetId) {
        self.fallback.insert(id);
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
    fn snapshot(
        &self,
        resident_assets: usize,
        queued_assets: usize,
        visible_queued_assets: usize,
    ) -> RenderAssetStats {
        RenderAssetStats {
            resident_assets,
            uploaded_assets: self.uploaded.len(),
            uploaded_bytes: self.uploaded_bytes,
            queued_assets,
            visible_queued_assets,
            loading_assets: self.loading.len(),
            fallback_assets: self.fallback.len(),
            missing_assets: self.missing.len(),
            failed_assets: self.failed.len(),
            upload_ms: self.upload_ms,
        }
    }

    fn log_missing_and_failed_once(
        &self,
        logged_missing: &mut FxHashSet<AssetId>,
        logged_failed: &mut FxHashSet<AssetId>,
    ) {
        for id in &self.missing {
            if logged_missing.insert(*id) {
                log::warn!(
                    target: "sky_engine::render::asset",
                    "{}: texture asset {} ({}) is missing; fallback material will be used",
                    RENDER_TEXTURE_ASSET_MISSING_LOG,
                    id,
                    TextureAsset::TYPE,
                );
            }
        }

        for id in &self.failed {
            if logged_failed.insert(*id) {
                log::error!(
                    target: "sky_engine::render::asset",
                    "{}: texture asset {} ({}) failed to load or prepare; fallback material will be used",
                    RENDER_TEXTURE_ASSET_FAILED_LOG,
                    id,
                    TextureAsset::TYPE,
                );
            }
        }
    }
}

#[derive(Default)]
pub struct RenderAssetCache {
    textures: TextureGpuCache,
    frame: RenderAssetFrameStats,
    stats: RenderAssetStats,
    logged_missing_textures: FxHashSet<AssetId>,
    logged_failed_textures: FxHashSet<AssetId>,
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
        assets: &Assets,
        handle: &Handle<TextureAsset>,
    ) -> Option<Texture> {
        let id = handle.id();
        match self.request_texture_gpu_with_priority(
            gpu,
            assets,
            handle,
            TexturePreparePriority::Visible,
        ) {
            TextureReadiness::GpuReady => self
                .textures
                .textures
                .get(&id)
                .map(|cached| cached.texture.clone()),
            TextureReadiness::CpuReady
            | TextureReadiness::GpuQueued
            | TextureReadiness::CpuLoading => {
                self.frame.record_loading(id);
                self.frame.record_fallback(id);
                None
            }
            TextureReadiness::MissingCpu => {
                self.frame.record_missing(id);
                self.frame.record_fallback(id);
                None
            }
            TextureReadiness::Failed => {
                self.frame.record_failed(id);
                self.frame.record_fallback(id);
                None
            }
        }
    }

    pub fn texture_readiness(
        &mut self,
        assets: Option<&Assets>,
        handle: &Handle<TextureAsset>,
    ) -> TextureReadiness {
        let Some(assets) = assets else {
            self.invalidate_texture(handle.id());
            return TextureReadiness::MissingCpu;
        };

        let id = handle.id();
        let Some(source) = assets.try_get(&handle) else {
            self.invalidate_stale_cpu_asset(id, None);
            return self.cpu_readiness(assets, handle);
        };

        self.invalidate_stale_cpu_asset(id, Some(&source));
        self.textures.requested.remove(&id);
        if let Some(cached) = self.textures.textures.get(&id) {
            if Arc::ptr_eq(&cached.source, &source) {
                return TextureReadiness::GpuReady;
            }
        }
        if self.textures.queue.contains_current(id, &source) {
            return TextureReadiness::GpuQueued;
        }
        TextureReadiness::CpuReady
    }

    pub fn request_texture_gpu(
        &mut self,
        gpu: &GpuContext,
        assets: &Assets,
        handle: &Handle<TextureAsset>,
    ) -> TextureReadiness {
        self.request_texture_gpu_with_priority(gpu, assets, handle, TexturePreparePriority::Preload)
    }

    pub fn request_texture_gpu_with_priority(
        &mut self,
        _gpu: &GpuContext,
        assets: &Assets,
        handle: &Handle<TextureAsset>,
        priority: TexturePreparePriority,
    ) -> TextureReadiness {
        let id = handle.id();
        let Some(source) = assets.try_get(&handle) else {
            self.invalidate_stale_cpu_asset(id, None);
            return self.cpu_readiness(assets, handle);
        };

        self.invalidate_stale_cpu_asset(id, Some(&source));
        self.textures.requested.remove(&id);
        if let Some(cached) = self.textures.textures.get(&id) {
            if Arc::ptr_eq(&cached.source, &source) {
                return TextureReadiness::GpuReady;
            }
        }

        self.queue_texture_gpu(id, source, priority);
        TextureReadiness::GpuQueued
    }

    pub fn wait_texture_gpu(
        &mut self,
        gpu: &GpuContext,
        assets: &Assets,
        handle: &Handle<TextureAsset>,
        timeout: std::time::Duration,
    ) -> TextureReadiness {
        let deadline = std::time::Instant::now().checked_add(timeout);
        loop {
            let mut readiness = self.request_texture_gpu(gpu, assets, handle);
            if matches!(
                readiness,
                TextureReadiness::GpuQueued | TextureReadiness::CpuReady
            ) {
                self.prepare_queued_texture(gpu, handle.id());
                if self.texture_readiness(Some(assets), handle) == TextureReadiness::GpuQueued {
                    self.prepare_queued_textures(gpu);
                }
                readiness = self.texture_readiness(Some(assets), handle);
            }
            if matches!(
                readiness,
                TextureReadiness::GpuReady
                    | TextureReadiness::MissingCpu
                    | TextureReadiness::Failed
            ) || timeout.is_zero()
            {
                return readiness;
            }
            if deadline.is_some_and(|deadline| std::time::Instant::now() >= deadline) {
                return readiness;
            }
            std::thread::yield_now();
        }
    }

    pub fn prepare_queued_textures(&mut self, gpu: &GpuContext) {
        for _ in 0..TEXTURE_PREPARE_BATCH_SIZE {
            let Some((id, entry)) = self.textures.queue.pop_next() else {
                break;
            };
            self.prepare_queued_entry(gpu, id, entry);
        }
    }

    pub fn prepare_queued_texture(&mut self, gpu: &GpuContext, id: AssetId) -> TextureReadiness {
        let Some(entry) = self.textures.queue.pop_id(id) else {
            return if self.textures.textures.contains_key(&id) {
                TextureReadiness::GpuReady
            } else {
                TextureReadiness::CpuReady
            };
        };
        self.prepare_queued_entry(gpu, id, entry);
        if self.textures.textures.contains_key(&id) {
            TextureReadiness::GpuReady
        } else {
            TextureReadiness::CpuReady
        }
    }

    #[inline]
    pub fn mark_texture_missing(&mut self, handle: &Handle<TextureAsset>) {
        self.frame.record_missing(handle.id());
    }

    #[inline]
    pub fn finish_frame(&mut self) -> RenderAssetStats {
        self.frame.log_missing_and_failed_once(
            &mut self.logged_missing_textures,
            &mut self.logged_failed_textures,
        );
        self.stats = self.frame.snapshot(
            self.textures.textures.len(),
            self.textures.queue.len(),
            self.textures.queue.visible_len(),
        );
        self.stats
    }

    #[inline]
    pub fn handle_asset_event(&mut self, event: AssetEvent, assets: Option<&Assets>) {
        match event.kind {
            AssetEventKind::Failed | AssetEventKind::Unloaded | AssetEventKind::ReloadQueued => {
                self.invalidate_texture(event.id);
            }
            AssetEventKind::Installed => {
                let source = assets.and_then(|assets| assets.try_get_id::<TextureAsset>(event.id));
                self.invalidate_stale_cpu_asset(event.id, source.as_ref());
            }
        }
    }

    #[inline]
    pub fn stats(&self) -> RenderAssetStats {
        self.stats
    }

    #[inline]
    pub fn contains_texture(&self, handle: &Handle<TextureAsset>) -> bool {
        self.textures.textures.contains_key(&handle.id())
    }

    #[inline]
    pub fn invalidate_texture(&mut self, id: AssetId) {
        self.textures.textures.remove(&id);
        self.textures.queue.remove(id);
        self.textures.requested.remove(&id);
    }

    #[inline]
    pub fn clear(&mut self) {
        self.textures.textures.clear();
        self.textures.queue.clear();
        self.textures.requested.clear();
        self.frame.clear();
        self.stats = RenderAssetStats::default();
        self.logged_missing_textures.clear();
        self.logged_failed_textures.clear();
    }

    fn cpu_readiness(
        &mut self,
        assets: &Assets,
        handle: &Handle<TextureAsset>,
    ) -> TextureReadiness {
        let id = handle.id();
        match assets.state(&handle) {
            AssetState::Unloaded => {
                if self.textures.requested.insert(id) {
                    if let Err(error) = assets.load_id::<TextureAsset>(id) {
                        self.textures.requested.remove(&id);
                        if matches!(
                            error,
                            AssetError::AssetNotFound { .. } | AssetError::AssetPathNotFound { .. }
                        ) {
                            TextureReadiness::MissingCpu
                        } else {
                            TextureReadiness::Failed
                        }
                    } else {
                        TextureReadiness::CpuLoading
                    }
                } else {
                    TextureReadiness::CpuLoading
                }
            }
            AssetState::Loading
            | AssetState::Loaded
            | AssetState::WaitingDependencies
            | AssetState::Installing
            | AssetState::Uninstalling
            | AssetState::Unloading => TextureReadiness::CpuLoading,
            AssetState::Installed => TextureReadiness::CpuReady,
            AssetState::Failed => TextureReadiness::Failed,
        }
    }

    fn queue_texture_gpu(
        &mut self,
        id: AssetId,
        source: Arc<TextureAsset>,
        priority: TexturePreparePriority,
    ) {
        self.textures.textures.remove(&id);
        self.textures.queue.push(id, source, priority);
    }

    fn invalidate_stale_cpu_asset(&mut self, id: AssetId, source: Option<&Arc<TextureAsset>>) {
        if source.map_or(true, |source| {
            self.textures
                .textures
                .get(&id)
                .is_some_and(|cached| !Arc::ptr_eq(&cached.source, source))
        }) {
            self.textures.textures.remove(&id);
        }
        self.textures.queue.retain_current(id, source);
    }

    fn prepare_queued_entry(&mut self, gpu: &GpuContext, id: AssetId, entry: QueuedGpuTexture) {
        if let Some(cached) = self.textures.textures.get(&id) {
            if Arc::ptr_eq(&cached.source, &entry.source) {
                return;
            }
        }

        let upload_start = timing_start();
        let texture = prepare_texture_asset(gpu, &entry.source);
        let upload_ms = elapsed_ms(upload_start);
        self.textures.textures.insert(
            id,
            CachedGpuTexture {
                source: entry.source,
                texture,
            },
        );
        self.frame.record_upload(id, entry.byte_size, upload_ms);
    }
}

#[derive(Default)]
pub struct SharedRenderAssetCache {
    inner: RefCell<RenderAssetCache>,
}

impl SharedRenderAssetCache {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn borrow_mut(&self) -> std::cell::RefMut<'_, RenderAssetCache> {
        self.inner.borrow_mut()
    }
}

fn prepare_texture_asset(gpu: &GpuContext, source: &TextureAsset) -> Texture {
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
