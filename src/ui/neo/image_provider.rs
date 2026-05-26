#[cfg(feature = "ui-neo-net")]
use std::collections::hash_map::DefaultHasher;
#[cfg(feature = "ui-neo-net")]
use std::hash::{Hash, Hasher};
#[cfg(feature = "ui-neo-net")]
use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use eui_neo::expert::{UiDrawCommand, UiDrawList};
use eui_neo::{ImageRef, ImageRefKind};
use eui_neo_wgpu::{GpuImage, ImageState, Resources};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::asset::{AssetId, Assets, Handle, TextureAsset, TextureColorSpace};
use crate::gpu::GpuContext;
use crate::render::{SharedRenderAssetCache, Texture, TextureReadiness};

const IMAGE_RETRY_DELAY: Duration = Duration::from_secs(5);

pub(crate) struct SkyNeoImageStore {
    handles: FxHashMap<ImageRef, Handle<TextureAsset>>,
    pending_remote: FxHashMap<ImageRef, Receiver<Result<TextureAsset, String>>>,
    failed: FxHashMap<ImageRef, Instant>,
    ready: FxHashMap<ImageRef, SkyReadyImage>,
    pending_frame: FxHashSet<ImageRef>,
}

impl Default for SkyNeoImageStore {
    fn default() -> Self {
        Self {
            handles: FxHashMap::default(),
            pending_remote: FxHashMap::default(),
            failed: FxHashMap::default(),
            ready: FxHashMap::default(),
            pending_frame: FxHashSet::default(),
        }
    }
}

impl SkyNeoImageStore {
    pub(crate) fn prepare(
        &mut self,
        gpu: &GpuContext,
        draw_list: &UiDrawList,
        asset_server: Option<&Assets>,
        render_assets: Option<&SharedRenderAssetCache>,
    ) -> bool {
        self.ready.clear();
        self.pending_frame.clear();

        let keys = image_keys(draw_list);
        if keys.is_empty() {
            return false;
        }

        let Some(asset_server) = asset_server else {
            return false;
        };
        let Some(render_assets) = render_assets else {
            return false;
        };
        self.poll_remote_images(asset_server);

        let mut cache = render_assets.borrow_mut();
        let mut pending_handles = Vec::new();

        for key in &keys {
            let Some(handle) = self.resolve_handle(asset_server, key) else {
                continue;
            };
            match cache.texture(gpu, asset_server, &handle) {
                Some(texture) => {
                    self.store_ready_texture(asset_server, key, &handle, texture);
                }
                None => {
                    let readiness = cache.texture_readiness(Some(asset_server), &handle);
                    if matches!(
                        readiness,
                        TextureReadiness::CpuLoading
                            | TextureReadiness::CpuReady
                            | TextureReadiness::GpuQueued
                    ) {
                        self.pending_frame.insert(key.clone());
                        pending_handles.push((key.clone(), handle));
                    }
                }
            }
        }

        cache.prepare_queued_textures(gpu);
        for (key, handle) in pending_handles {
            if let Some(texture) = cache.texture(gpu, asset_server, &handle) {
                self.pending_frame.remove(&key);
                self.store_ready_texture(asset_server, &key, &handle, texture);
            }
        }

        !self.pending_frame.is_empty() || !self.pending_remote.is_empty()
    }

    pub(crate) fn provider(&self) -> SkyNeoResources<'_> {
        SkyNeoResources { store: self }
    }

    fn poll_remote_images(&mut self, asset_server: &Assets) {
        let mut completed = Vec::new();
        for (key, receiver) in &self.pending_remote {
            match receiver.try_recv() {
                Ok(Ok(texture)) => {
                    let handle = asset_server.insert_runtime(texture);
                    self.handles.insert(key.clone(), handle);
                    self.failed.remove(key);
                    completed.push(key.clone());
                }
                Ok(Err(error)) => {
                    eprintln!(
                        "[SkyEngine] neo image load failed for {}: {error}",
                        key.source()
                    );
                    self.failed.insert(key.clone(), Instant::now());
                    completed.push(key.clone());
                }
                Err(TryRecvError::Disconnected) => {
                    eprintln!(
                        "[SkyEngine] neo image load worker disconnected for {}",
                        key.source()
                    );
                    self.failed.insert(key.clone(), Instant::now());
                    completed.push(key.clone());
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        for key in completed {
            self.pending_remote.remove(&key);
        }
    }

    fn resolve_handle(
        &mut self,
        asset_server: &Assets,
        key: &ImageRef,
    ) -> Option<Handle<TextureAsset>> {
        if let Some(handle) = self.handles.get(key).cloned() {
            return Some(handle);
        }
        if self.recently_failed(key) {
            return None;
        }
        if matches!(key.kind(), ImageRefKind::Url | ImageRefKind::BingDaily)
            || is_remote_image_source(key.source())
            || key.source().starts_with("bing://daily")
        {
            self.start_remote_load(key);
            self.pending_frame.insert(key.clone());
            return None;
        }

        match load_texture_handle(asset_server, key) {
            Ok(handle) => {
                self.handles.insert(key.clone(), handle.clone());
                Some(handle)
            }
            Err(error) => {
                eprintln!(
                    "[SkyEngine] neo image asset lookup failed for {}: {error}",
                    key.source()
                );
                let handle = asset_server.insert_runtime(missing_texture_asset());
                self.handles.insert(key.clone(), handle.clone());
                Some(handle)
            }
        }
    }

    fn recently_failed(&mut self, key: &ImageRef) -> bool {
        if let Some(failed_at) = self.failed.get(key) {
            if failed_at.elapsed() < IMAGE_RETRY_DELAY {
                return true;
            }
        }
        self.failed.remove(key);
        false
    }

    fn start_remote_load(&mut self, key: &ImageRef) {
        if self.pending_remote.contains_key(key) {
            return;
        }
        let key_for_thread = key.clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let result = load_remote_texture_asset(&key_for_thread);
            let _ = sender.send(result);
        });
        self.pending_remote.insert(key.clone(), receiver);
    }

    fn store_ready_texture(
        &mut self,
        asset_server: &Assets,
        key: &ImageRef,
        handle: &Handle<TextureAsset>,
        texture: Texture,
    ) {
        let Some(asset) = asset_server.try_get(handle) else {
            self.pending_frame.insert(key.clone());
            return;
        };
        let size = asset.visible_size();
        if size[0] == 0 || size[1] == 0 {
            return;
        }
        self.ready.insert(
            key.clone(),
            SkyReadyImage {
                texture,
                size,
                uv_rect: visible_uv_rect(&asset, key.flip_vertically()),
            },
        );
    }
}

pub(crate) struct SkyNeoResources<'a> {
    store: &'a SkyNeoImageStore,
}

impl Resources for SkyNeoResources<'_> {
    fn image<'a>(&'a mut self, key: &ImageRef) -> ImageState<'a> {
        if let Some(image) = self.store.ready.get(key) {
            return ImageState::Gpu(GpuImage {
                revision: image.revision(),
                size: image.size,
                uv_rect: image.uv_rect,
                view: image.texture.view(),
                sampler: None,
            });
        }
        if self.store.pending_frame.contains(key) || self.store.pending_remote.contains_key(key) {
            return ImageState::Pending;
        }
        if self.store.failed.contains_key(key) {
            return ImageState::Failed;
        }
        ImageState::Missing
    }
}

struct SkyReadyImage {
    texture: Texture,
    size: [u32; 2],
    uv_rect: [f32; 4],
}

impl SkyReadyImage {
    fn revision(&self) -> u64 {
        self.texture.texture() as *const wgpu::Texture as usize as u64
    }
}

fn image_keys(draw_list: &UiDrawList) -> Vec<ImageRef> {
    let mut seen = FxHashSet::default();
    let mut keys = Vec::new();
    for command in draw_list.commands() {
        let image = match command {
            UiDrawCommand::Image(draw) => &draw.image,
            UiDrawCommand::NineSlice(draw) => &draw.image,
            _ => continue,
        };
        if image.is_empty() {
            continue;
        }
        let key = image.clone();
        if seen.insert(key.clone()) {
            keys.push(key);
        }
    }
    keys
}

fn load_texture_handle(
    asset_server: &Assets,
    key: &ImageRef,
) -> Result<Handle<TextureAsset>, String> {
    match key.kind() {
        ImageRefKind::Asset => {
            if let Ok(id) = AssetId::parse_str(key.source()) {
                return asset_server
                    .load_id::<TextureAsset>(id)
                    .map_err(|error| error.to_string());
            }
            return asset_server
                .load_texture(PathBuf::from(key.source()))
                .map_err(|error| error.to_string());
        }
        _ => {}
    }
    if let Some(value) = key.source().strip_prefix("asset://") {
        if let Ok(id) = AssetId::parse_str(value) {
            return asset_server
                .load_id::<TextureAsset>(id)
                .map_err(|error| error.to_string());
        }
        return asset_server
            .load_texture(PathBuf::from(value))
            .map_err(|error| error.to_string());
    }
    asset_server
        .load_texture(PathBuf::from(key.source()))
        .map_err(|error| error.to_string())
}

fn missing_texture_asset() -> TextureAsset {
    const W: u32 = 32;
    const H: u32 = 32;
    let mut rgba = Vec::with_capacity((W * H * 4) as usize);
    for y in 0..H {
        for x in 0..W {
            let on = ((x / 8) + (y / 8)) % 2 == 0;
            let [r, g, b] = if on { [255, 0, 255] } else { [20, 20, 20] };
            rgba.extend_from_slice(&[r, g, b, 255]);
        }
    }
    TextureAsset::new(W, H, TextureColorSpace::Srgb, rgba)
}

fn visible_uv_rect(asset: &TextureAsset, flip_vertically: bool) -> [f32; 4] {
    let uv = asset.visible_uv_rect();
    if flip_vertically {
        [uv[0], uv[3], uv[2], uv[1]]
    } else {
        uv
    }
}

fn load_remote_texture_asset(key: &ImageRef) -> Result<TextureAsset, String> {
    #[cfg(not(feature = "ui-neo-net"))]
    {
        let _ = key;
        return Err("remote neo image loading requires the `ui-neo-net` feature".to_string());
    }

    #[cfg(feature = "ui-neo-net")]
    {
        let bytes = if key.source().starts_with("bing://daily") {
            load_bing_daily_bytes(key.source())?
        } else {
            load_url_bytes(key.source())?
        };
        let mut image = image::load_from_memory(&bytes)
            .map_err(|error| format!("image decode failed: {error}"))?
            .to_rgba8();
        if key.flip_vertically() {
            image::imageops::flip_vertical_in_place(&mut image);
        }
        let (width, height) = image.dimensions();
        Ok(TextureAsset::new(
            width,
            height,
            TextureColorSpace::Srgb,
            image.into_raw(),
        ))
    }
}

#[cfg(feature = "ui-neo-net")]
fn load_bing_daily_bytes(source: &str) -> Result<Vec<u8>, String> {
    let query = source.strip_prefix("bing://daily").unwrap_or_default();
    let idx = query_param(query, "idx").unwrap_or_else(|| "0".to_string());
    let mkt = query_param(query, "mkt").unwrap_or_else(|| "zh-CN".to_string());
    let metadata_url =
        format!("https://www.bing.com/HPImageArchive.aspx?format=js&n=1&idx={idx}&mkt={mkt}");
    let metadata = String::from_utf8(load_url_bytes(&metadata_url)?)
        .map_err(|error| format!("Bing metadata was not UTF-8: {error}"))?;
    let json: serde_json::Value = serde_json::from_str(&metadata)
        .map_err(|error| format!("Bing metadata JSON parse failed: {error}"))?;
    let image_url = json
        .get("images")
        .and_then(|images| images.get(0))
        .and_then(|image| image.get("url"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Bing metadata did not contain images[0].url".to_string())?;
    let image_url = if is_remote_image_source(image_url) {
        image_url.to_string()
    } else {
        format!("https://www.bing.com{image_url}")
    };
    load_url_bytes_cached(&image_url)
}

#[cfg(feature = "ui-neo-net")]
fn query_param(query: &str, key: &str) -> Option<String> {
    let query = query.strip_prefix('?').unwrap_or(query);
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == key).then(|| value.to_string())
    })
}

#[cfg(feature = "ui-neo-net")]
fn load_url_bytes(url: &str) -> Result<Vec<u8>, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(12))
        .build();
    let response = agent
        .get(url)
        .call()
        .map_err(|error| format!("GET {url} failed: {error}"))?;
    let mut reader = response.into_reader();
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|error| format!("read response from {url} failed: {error}"))?;
    Ok(bytes)
}

#[cfg(feature = "ui-neo-net")]
fn load_url_bytes_cached(url: &str) -> Result<Vec<u8>, String> {
    let Some(path) = remote_image_cache_path(url) else {
        return load_url_bytes(url);
    };
    if let Ok(bytes) = std::fs::read(&path) {
        if !bytes.is_empty() && image::load_from_memory(&bytes).is_ok() {
            return Ok(bytes);
        }
        let _ = std::fs::remove_file(&path);
    }
    let bytes = load_url_bytes(url)?;
    image::load_from_memory(&bytes)
        .map_err(|error| format!("downloaded image from {url} did not decode: {error}"))?;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, &bytes);
    Ok(bytes)
}

#[cfg(feature = "ui-neo-net")]
fn remote_image_cache_path(url: &str) -> Option<PathBuf> {
    if !is_remote_image_source(url) {
        return None;
    }
    let extension = remote_image_extension(url);
    Some(image_cache_path_for_key(url, extension))
}

#[cfg(feature = "ui-neo-net")]
fn image_cache_path_for_key(key: &str, extension: &str) -> PathBuf {
    std::env::temp_dir()
        .join("sky_neo_image_cache")
        .join(format!("{:016x}{extension}", stable_hash(key)))
}

#[cfg(feature = "ui-neo-net")]
fn remote_image_extension(url: &str) -> &'static str {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());
    match extension.as_deref() {
        Some("png") => ".png",
        Some("jpg") | Some("jpeg") => ".jpg",
        Some("webp") => ".webp",
        Some("bmp") => ".bmp",
        _ => ".cache",
    }
}

fn is_remote_image_source(source: &str) -> bool {
    source.starts_with("http://") || source.starts_with("https://")
}

#[cfg(feature = "ui-neo-net")]
fn stable_hash(value: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}
