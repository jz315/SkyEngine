use std::cell::RefCell;
use std::{fmt, sync::Arc};

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
    pub resident_bytes: usize,
    pub uploaded_assets: usize,
    pub uploaded_bytes: usize,
    pub evicted_assets: usize,
    pub evicted_bytes: usize,
    pub cached_failed_assets: usize,
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
    byte_size: usize,
    last_used_order: u64,
}

struct FailedGpuTexturePrepare {
    source: Arc<TextureAsset>,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct TextureEvictionStats {
    ids: Vec<AssetId>,
    bytes: usize,
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
    prepare_failures: FxHashMap<AssetId, FailedGpuTexturePrepare>,
    queue: TexturePrepareQueue,
    requested: FxHashSet<AssetId>,
    pinned: FxHashSet<AssetId>,
    resident_bytes: usize,
    next_access_order: u64,
}

impl TextureGpuCache {
    fn next_access_order(&mut self) -> u64 {
        let order = self.next_access_order;
        self.next_access_order = self.next_access_order.wrapping_add(1);
        order
    }

    fn touch(&mut self, id: AssetId) {
        let order = self.next_access_order();
        if let Some(cached) = self.textures.get_mut(&id) {
            cached.last_used_order = order;
        }
    }

    fn remove_resident(&mut self, id: AssetId) -> Option<CachedGpuTexture> {
        let removed = self.textures.remove(&id)?;
        self.resident_bytes = self.resident_bytes.saturating_sub(removed.byte_size);
        Some(removed)
    }

    fn insert_resident(
        &mut self,
        id: AssetId,
        source: Arc<TextureAsset>,
        texture: Texture,
    ) -> usize {
        if let Some(removed) = self.remove_resident(id) {
            drop(removed);
        }
        self.prepare_failures.remove(&id);
        let byte_size = texture
            .resident_bytes()
            .unwrap_or_else(|| source.pixels().len());
        let last_used_order = self.next_access_order();
        self.resident_bytes = self.resident_bytes.saturating_add(byte_size);
        self.textures.insert(
            id,
            CachedGpuTexture {
                source,
                texture,
                byte_size,
                last_used_order,
            },
        );
        byte_size
    }

    fn insert_prepare_failure(&mut self, id: AssetId, source: Arc<TextureAsset>) {
        self.remove_resident(id);
        self.queue.remove(id);
        self.requested.remove(&id);
        self.prepare_failures
            .insert(id, FailedGpuTexturePrepare { source });
    }

    fn prepare_failed_current(&self, id: AssetId, source: &Arc<TextureAsset>) -> bool {
        self.prepare_failures
            .get(&id)
            .is_some_and(|failure| Arc::ptr_eq(&failure.source, source))
    }

    fn evict_to_budget(
        &mut self,
        budget: usize,
        protected: Option<AssetId>,
    ) -> TextureEvictionStats {
        let mut stats = TextureEvictionStats::default();
        while self.resident_bytes > budget {
            let Some(id) = self
                .textures
                .iter()
                .filter(|(id, _)| protected != Some(**id) && !self.pinned.contains(*id))
                .min_by_key(|(_, cached)| cached.last_used_order)
                .map(|(id, _)| *id)
            else {
                break;
            };
            if let Some(removed) = self.remove_resident(id) {
                stats.ids.push(id);
                stats.bytes = stats.bytes.saturating_add(removed.byte_size);
            }
            self.queue.remove(id);
            self.requested.remove(&id);
        }
        stats
    }

    fn clear(&mut self) {
        self.textures.clear();
        self.prepare_failures.clear();
        self.queue.clear();
        self.requested.clear();
        self.pinned.clear();
        self.resident_bytes = 0;
        self.next_access_order = 0;
    }
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
        if source.is_none_or(|source| {
            self.queued
                .get(&id)
                .is_some_and(|entry| !Arc::ptr_eq(&entry.source, source))
        }) {
            self.queued.remove(&id);
        }
    }

    fn clear(&mut self) {
        self.queued.clear();
        self.next_order = 0;
    }
}

#[derive(Default)]
struct RenderAssetFrameStats {
    uploaded: FxHashSet<AssetId>,
    uploaded_bytes: usize,
    evicted: FxHashSet<AssetId>,
    evicted_bytes: usize,
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
        self.evicted.clear();
        self.evicted_bytes = 0;
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
    fn record_evictions(&mut self, evictions: TextureEvictionStats) {
        self.evicted_bytes = self.evicted_bytes.saturating_add(evictions.bytes);
        self.evicted.extend(evictions.ids);
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
        resident_bytes: usize,
        cached_failed_assets: usize,
        queued_assets: usize,
        visible_queued_assets: usize,
    ) -> RenderAssetStats {
        RenderAssetStats {
            resident_assets,
            resident_bytes,
            uploaded_assets: self.uploaded.len(),
            uploaded_bytes: self.uploaded_bytes,
            evicted_assets: self.evicted.len(),
            evicted_bytes: self.evicted_bytes,
            cached_failed_assets,
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
    texture_memory_budget_bytes: Option<usize>,
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

    #[inline]
    pub fn set_texture_memory_budget(&mut self, budget_bytes: Option<usize>) {
        self.texture_memory_budget_bytes = budget_bytes;
        if let Some(budget_bytes) = budget_bytes {
            let evictions = self.textures.evict_to_budget(budget_bytes, None);
            self.frame.record_evictions(evictions);
        }
    }

    #[inline]
    pub fn pin_texture(&mut self, handle: &Handle<TextureAsset>) {
        self.textures.pinned.insert(handle.id());
    }

    #[inline]
    pub fn unpin_texture(&mut self, handle: &Handle<TextureAsset>) {
        self.textures.pinned.remove(&handle.id());
        if let Some(budget_bytes) = self.texture_memory_budget_bytes {
            let evictions = self.textures.evict_to_budget(budget_bytes, None);
            self.frame.record_evictions(evictions);
        }
    }

    #[inline]
    pub fn is_texture_pinned(&self, handle: &Handle<TextureAsset>) -> bool {
        self.textures.pinned.contains(&handle.id())
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
        let Some(source) = assets.try_get(handle) else {
            self.invalidate_stale_cpu_asset(id, None);
            return self.cpu_readiness(assets, handle);
        };

        self.invalidate_stale_cpu_asset(id, Some(&source));
        self.textures.requested.remove(&id);
        if let Some(cached) = self.textures.textures.get(&id) {
            if Arc::ptr_eq(&cached.source, &source) {
                self.textures.touch(id);
                return TextureReadiness::GpuReady;
            }
        }
        if self.textures.queue.contains_current(id, &source) {
            return TextureReadiness::GpuQueued;
        }
        if self.textures.prepare_failed_current(id, &source) {
            return TextureReadiness::Failed;
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
        let Some(source) = assets.try_get(handle) else {
            self.invalidate_stale_cpu_asset(id, None);
            return self.cpu_readiness(assets, handle);
        };

        self.invalidate_stale_cpu_asset(id, Some(&source));
        self.textures.requested.remove(&id);
        if let Some(cached) = self.textures.textures.get(&id) {
            if Arc::ptr_eq(&cached.source, &source) {
                self.textures.touch(id);
                return TextureReadiness::GpuReady;
            }
        }
        if self.textures.prepare_failed_current(id, &source) {
            return TextureReadiness::Failed;
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
        #[cfg(feature = "profile")]
        let _scope = sky_profile::profile_scope!("render_asset", "prepare_queued_textures");

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
            } else if self.textures.prepare_failures.contains_key(&id) {
                TextureReadiness::Failed
            } else {
                TextureReadiness::CpuReady
            };
        };
        self.prepare_queued_entry(gpu, id, entry);
        if self.textures.textures.contains_key(&id) {
            TextureReadiness::GpuReady
        } else if self.textures.prepare_failures.contains_key(&id) {
            TextureReadiness::Failed
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
            self.textures.resident_bytes,
            self.textures.prepare_failures.len(),
            self.textures.queue.len(),
            self.textures.queue.visible_len(),
        );
        self.stats
    }

    #[inline]
    pub fn handle_asset_event(&mut self, event: AssetEvent, assets: Option<&Assets>) {
        if !event.asset_type.is_empty() && event.asset_type != TextureAsset::TYPE {
            return;
        }

        match event.kind {
            AssetEventKind::Loaded => {}
            AssetEventKind::Failed if event.state == AssetState::Installed => {}
            AssetEventKind::Failed | AssetEventKind::Unloaded | AssetEventKind::ReloadQueued => {
                self.invalidate_texture(event.id);
            }
            AssetEventKind::Installed | AssetEventKind::Reloaded => {
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
        self.textures.remove_resident(id);
        self.textures.queue.remove(id);
        self.textures.requested.remove(&id);
        self.textures.prepare_failures.remove(&id);
        self.textures.pinned.remove(&id);
    }

    #[inline]
    pub fn clear(&mut self) {
        self.textures.clear();
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
        match assets.state(handle) {
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
        self.textures.remove_resident(id);
        self.textures.prepare_failures.remove(&id);
        self.textures.queue.push(id, source, priority);
    }

    fn invalidate_stale_cpu_asset(&mut self, id: AssetId, source: Option<&Arc<TextureAsset>>) {
        if source.is_none_or(|source| {
            self.textures
                .textures
                .get(&id)
                .is_some_and(|cached| !Arc::ptr_eq(&cached.source, source))
        }) {
            self.textures.remove_resident(id);
        }
        if source.is_none_or(|source| {
            self.textures
                .prepare_failures
                .get(&id)
                .is_some_and(|failure| !Arc::ptr_eq(&failure.source, source))
        }) {
            self.textures.prepare_failures.remove(&id);
        }
        self.textures.queue.retain_current(id, source);
    }

    fn prepare_queued_entry(&mut self, gpu: &GpuContext, id: AssetId, entry: QueuedGpuTexture) {
        #[cfg(feature = "profile")]
        let _scope = sky_profile::profile_scope!("render_asset", format!("prepare_texture:{id}"));

        if let Some(cached) = self.textures.textures.get(&id) {
            if Arc::ptr_eq(&cached.source, &entry.source) {
                self.textures.touch(id);
                return;
            }
        }

        let upload_start = timing_start();
        let texture = prepare_texture_asset(gpu, &entry.source);
        let upload_ms = elapsed_ms(upload_start);
        match texture {
            Ok(texture) => {
                let uploaded_bytes = self.textures.insert_resident(id, entry.source, texture);
                if let Some(budget_bytes) = self.texture_memory_budget_bytes {
                    let evictions = self.textures.evict_to_budget(budget_bytes, Some(id));
                    self.frame.record_evictions(evictions);
                }
                self.frame.record_upload(id, uploaded_bytes, upload_ms);
            }
            Err(error) => {
                log::error!(
                    target: "sky_engine::render::asset",
                    "{}: texture asset {} ({}) failed GPU prepare: {}",
                    RENDER_TEXTURE_ASSET_FAILED_LOG,
                    id,
                    TextureAsset::TYPE,
                    error,
                );
                self.textures.insert_prepare_failure(id, entry.source);
                self.frame.record_failed(id);
            }
        }
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum TexturePrepareError {
    EmptyDimensions { width: u32, height: u32 },
    PixelDataSizeOverflow { width: u32, height: u32 },
    PixelDataLengthMismatch { expected: usize, actual: usize },
}

impl fmt::Display for TexturePrepareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyDimensions { width, height } => {
                write!(f, "invalid empty dimensions {width}x{height}")
            }
            Self::PixelDataSizeOverflow { width, height } => {
                write!(
                    f,
                    "texture dimensions {width}x{height} overflow RGBA8 byte size"
                )
            }
            Self::PixelDataLengthMismatch { expected, actual } => {
                write!(f, "expected {expected} RGBA8 bytes, got {actual}")
            }
        }
    }
}

fn prepare_texture_asset(
    gpu: &GpuContext,
    source: &TextureAsset,
) -> Result<Texture, TexturePrepareError> {
    let width = source.width();
    let height = source.height();
    if width == 0 || height == 0 {
        return Err(TexturePrepareError::EmptyDimensions { width, height });
    }
    let expected_len = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(TexturePrepareError::PixelDataSizeOverflow { width, height })?;
    if source.pixels().len() != expected_len {
        return Err(TexturePrepareError::PixelDataLengthMismatch {
            expected: expected_len,
            actual: source.pixels().len(),
        });
    }

    let format = match source.color_space() {
        crate::asset::TextureColorSpace::Linear => wgpu::TextureFormat::Rgba8Unorm,
        crate::asset::TextureColorSpace::Srgb => wgpu::TextureFormat::Rgba8UnormSrgb,
    };

    Ok(Texture::from_rgba8_with_format(
        gpu,
        width,
        height,
        source.pixels(),
        format,
        "asset_texture",
    ))
}
