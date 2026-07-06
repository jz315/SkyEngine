use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::Arc;
use std::sync::Mutex;
#[cfg(test)]
use std::time::Duration;
use std::time::SystemTime;

use super::font::FontAsset;
use super::registry::AssetRegistry;
use super::store::AssetStore;
use super::texture::TextureAsset;
use super::types::{
    normalize_source_key, Asset, AssetConfig, AssetError, AssetId, AssetManifestEntry,
    AssetProviderStats,
};
use super::watcher::AssetWatchEvent;

const ASSET_BUNDLE_MAGIC: &[u8] = b"SKYASSETBUNDLE1\n";
const ASSET_BUNDLE_VERSION: u32 = 1;
const MAX_BUNDLE_INDEX_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RawSourceRequest {
    pub(crate) key: String,
    pub(crate) path: PathBuf,
}

impl RawSourceRequest {
    pub(crate) fn new(config: &AssetConfig, path: &Path) -> Self {
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else if config.asset_root.is_absolute() {
            config.asset_root.join(path)
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(&config.asset_root)
                .join(path)
        };
        let key = config.source_key(&path);
        Self { key, path }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AssetSourceLocation {
    Raw(PathBuf),
    Cooked(PathBuf),
    Package(PathBuf),
    Bundle {
        bundle_path: PathBuf,
        cooked_path: String,
        offset: u64,
        len: u64,
    },
    #[cfg(test)]
    Memory {
        label: String,
        bytes: Arc<[u8]>,
        read_error: Option<AssetError>,
        read_delay: Option<Duration>,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct ResolvedAssetSource {
    entry: AssetManifestEntry,
    location: AssetSourceLocation,
}

impl ResolvedAssetSource {
    pub(crate) fn new(entry: AssetManifestEntry, location: AssetSourceLocation) -> Self {
        Self { entry, location }
    }

    pub(crate) fn entry(&self) -> &AssetManifestEntry {
        &self.entry
    }

    pub(crate) fn location(&self) -> &AssetSourceLocation {
        &self.location
    }

    pub(crate) fn read_bytes(&self, id: AssetId) -> Result<Vec<u8>, AssetError> {
        match &self.location {
            #[cfg(test)]
            AssetSourceLocation::Memory {
                bytes,
                read_error,
                read_delay,
                ..
            } => {
                if let Some(delay) = read_delay {
                    std::thread::sleep(*delay);
                }
                if let Some(error) = read_error {
                    return Err(error.clone());
                }
                Ok(bytes.to_vec())
            }
            AssetSourceLocation::Bundle {
                bundle_path,
                offset,
                len,
                ..
            } => read_bundle_bytes(bundle_path, *offset, *len, id),
            AssetSourceLocation::Raw(path)
            | AssetSourceLocation::Cooked(path)
            | AssetSourceLocation::Package(path) => {
                std::fs::read(path).map_err(|error| match &self.location {
                    AssetSourceLocation::Raw(path) => AssetError::Io {
                        path: path.clone(),
                        message: error.to_string(),
                    },
                    AssetSourceLocation::Cooked(path)
                        if error.kind() == std::io::ErrorKind::NotFound =>
                    {
                        AssetError::MissingCookedArtifact {
                            id,
                            path: path.clone(),
                        }
                    }
                    AssetSourceLocation::Cooked(path) => AssetError::Io {
                        path: path.clone(),
                        message: error.to_string(),
                    },
                    AssetSourceLocation::Package(path)
                        if error.kind() == std::io::ErrorKind::NotFound =>
                    {
                        AssetError::MissingCookedArtifact {
                            id,
                            path: path.clone(),
                        }
                    }
                    AssetSourceLocation::Package(path) => AssetError::Io {
                        path: path.clone(),
                        message: error.to_string(),
                    },
                    AssetSourceLocation::Bundle { .. } => unreachable!("bundle handled above"),
                    #[cfg(test)]
                    AssetSourceLocation::Memory { .. } => unreachable!("memory handled above"),
                })
            }
        }
    }
}

pub(crate) fn raw_texture_manifest_entry(
    id: AssetId,
    source_path: String,
    import_settings: serde_json::Value,
) -> AssetManifestEntry {
    AssetManifestEntry {
        asset_id: id,
        asset_type: TextureAsset::TYPE.to_string(),
        importer: "texture.raw".to_string(),
        cooker: "texture.raw_rgba8".to_string(),
        version: 1,
        source_path,
        cooked_path: String::new(),
        dependencies: Vec::new(),
        import_settings,
    }
}

pub(crate) fn raw_font_manifest_entry(id: AssetId, source_path: String) -> AssetManifestEntry {
    AssetManifestEntry {
        asset_id: id,
        asset_type: FontAsset::TYPE.to_string(),
        importer: "font.raw".to_string(),
        cooker: "font.raw_bytes".to_string(),
        version: 1,
        source_path,
        cooked_path: String::new(),
        dependencies: Vec::new(),
        import_settings: serde_json::Value::Null,
    }
}

pub(crate) fn record_manifest_entry(
    config: &AssetConfig,
    manifest: &impl AssetRegistry,
    store: &AssetStore,
    id: AssetId,
) -> Result<AssetManifestEntry, AssetError> {
    if let Some(entry) = manifest.entry(id) {
        return Ok(entry.clone());
    }

    let raw_record = store.raw_source_record(id)?;

    if raw_record.asset_type == FontAsset::TYPE {
        Ok(raw_font_manifest_entry(
            id,
            config.source_key(&raw_record.path),
        ))
    } else {
        Ok(raw_texture_manifest_entry(
            id,
            config.source_key(&raw_record.path),
            serde_json::json!({ "srgb": true }),
        ))
    }
}

pub(crate) fn resolve_record_source(
    config: &AssetConfig,
    provider: &dyn AssetProvider,
    manifest: &impl AssetRegistry,
    store: &AssetStore,
    id: AssetId,
) -> Result<ResolvedAssetSource, AssetError> {
    let entry = record_manifest_entry(config, manifest, store, id)?;
    let raw_source_path = store.raw_source_path(id);
    provider.resolve(id, entry, raw_source_path)
}

#[cfg(test)]
#[derive(Clone, Debug, Default)]
pub(crate) struct MemoryAssetProvider {
    sources: HashMap<AssetId, MemoryAssetProviderSource>,
}

#[cfg(test)]
impl MemoryAssetProvider {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with_asset(mut self, id: AssetId, bytes: impl Into<Vec<u8>>) -> Self {
        self.sources.insert(
            id,
            MemoryAssetProviderSource::Bytes {
                bytes: Arc::from(bytes.into()),
                read_delay: None,
            },
        );
        self
    }

    pub(crate) fn with_delayed_asset(
        mut self,
        id: AssetId,
        bytes: impl Into<Vec<u8>>,
        read_delay: Duration,
    ) -> Self {
        self.sources.insert(
            id,
            MemoryAssetProviderSource::Bytes {
                bytes: Arc::from(bytes.into()),
                read_delay: Some(read_delay),
            },
        );
        self
    }

    pub(crate) fn with_read_error(mut self, id: AssetId, error: AssetError) -> Self {
        self.sources.insert(
            id,
            MemoryAssetProviderSource::ReadError {
                error,
                read_delay: None,
            },
        );
        self
    }

    pub(crate) fn with_resolve_error(mut self, id: AssetId, error: AssetError) -> Self {
        self.sources
            .insert(id, MemoryAssetProviderSource::ResolveError(error));
        self
    }
}

#[cfg(test)]
#[derive(Clone, Debug)]
enum MemoryAssetProviderSource {
    Bytes {
        bytes: Arc<[u8]>,
        read_delay: Option<Duration>,
    },
    ReadError {
        error: AssetError,
        read_delay: Option<Duration>,
    },
    ResolveError(AssetError),
}

#[cfg(test)]
impl AssetProvider for MemoryAssetProvider {
    fn resolve(
        &self,
        id: AssetId,
        entry: AssetManifestEntry,
        _raw_source_path: Option<PathBuf>,
    ) -> Result<ResolvedAssetSource, AssetError> {
        let source = self
            .sources
            .get(&id)
            .cloned()
            .ok_or(AssetError::AssetNotFound { id })?;
        let (bytes, read_error, read_delay) = match source {
            MemoryAssetProviderSource::Bytes { bytes, read_delay } => (bytes, None, read_delay),
            MemoryAssetProviderSource::ReadError { error, read_delay } => {
                (Arc::from(Vec::new()), Some(error), read_delay)
            }
            MemoryAssetProviderSource::ResolveError(error) => return Err(error),
        };
        Ok(ResolvedAssetSource::new(
            entry,
            AssetSourceLocation::Memory {
                label: format!("memory://{id}"),
                bytes,
                read_error,
                read_delay,
            },
        ))
    }
}

pub(crate) trait AssetProvider: Send + Sync {
    fn resolve(
        &self,
        id: AssetId,
        entry: AssetManifestEntry,
        raw_source_path: Option<PathBuf>,
    ) -> Result<ResolvedAssetSource, AssetError>;

    fn stats(&self) -> AssetProviderStats {
        AssetProviderStats::default()
    }

    fn invalidate_changed_paths(&self, _paths: &[PathBuf]) {}

    fn invalidate_all(&self) {}
}

pub(crate) fn invalidate_from_watch_events(
    provider: &dyn AssetProvider,
    watcher_events: &[AssetWatchEvent],
) {
    let mut changed_paths = Vec::new();
    let mut full_invalidation = false;
    for event in watcher_events {
        match event {
            AssetWatchEvent::Changed(path) => changed_paths.push(path.clone()),
            AssetWatchEvent::Rescan => full_invalidation = true,
        }
    }

    if full_invalidation {
        provider.invalidate_all();
    } else {
        provider.invalidate_changed_paths(&changed_paths);
    }
}

#[derive(Debug)]
pub(crate) struct LocalAssetProvider {
    cooked_root: PathBuf,
    package_roots: Vec<PathBuf>,
    package_files: Vec<PathBuf>,
    bundle_indexes: Mutex<HashMap<PathBuf, CachedBundleIndex>>,
    counters: Mutex<ProviderResolveCounters>,
}

impl LocalAssetProvider {
    pub(crate) fn new(config: &AssetConfig) -> Self {
        Self {
            cooked_root: config.cooked_root(),
            package_roots: config.package_roots(),
            package_files: config.package_files(),
            bundle_indexes: Mutex::new(HashMap::new()),
            counters: Mutex::new(ProviderResolveCounters::default()),
        }
    }

    fn resolve_cooked_location(
        &self,
        cooked_path: &str,
    ) -> Result<AssetSourceLocation, AssetError> {
        let local = self.cooked_root.join(cooked_path);
        if local.exists() {
            return Ok(AssetSourceLocation::Cooked(local));
        }

        if let Some(path) = self
            .package_roots
            .iter()
            .map(|root| root.join(cooked_path))
            .find(|path| path.exists())
        {
            return Ok(AssetSourceLocation::Package(path));
        }

        if let Some(location) = self.resolve_bundle_location(cooked_path)? {
            return Ok(location);
        }

        Ok(AssetSourceLocation::Cooked(local))
    }

    fn resolve_bundle_location(
        &self,
        cooked_path: &str,
    ) -> Result<Option<AssetSourceLocation>, AssetError> {
        let key = normalize_source_key(cooked_path);
        for bundle_path in &self.package_files {
            if !bundle_path.exists() {
                continue;
            }
            if let Some(entry) = self.read_bundle_index_entry(bundle_path, &key)? {
                return Ok(Some(AssetSourceLocation::Bundle {
                    bundle_path: bundle_path.clone(),
                    cooked_path: cooked_path.to_string(),
                    offset: entry.offset,
                    len: entry.len,
                }));
            }
        }
        Ok(None)
    }

    fn read_bundle_index_entry(
        &self,
        bundle_path: &Path,
        cooked_key: &str,
    ) -> Result<Option<BundleReadEntry>, AssetError> {
        let fingerprint = BundleFingerprint::read(bundle_path)?;
        let mut indexes = self
            .bundle_indexes
            .lock()
            .expect("asset bundle index cache poisoned");

        if let Some(cached) = indexes
            .get(bundle_path)
            .filter(|cached| cached.fingerprint == fingerprint)
        {
            return Ok(cached.entries.get(cooked_key).copied());
        }

        let entries = read_bundle_index(bundle_path)?;
        let result = entries.get(cooked_key).copied();
        indexes.insert(
            bundle_path.to_path_buf(),
            CachedBundleIndex {
                fingerprint,
                entries,
            },
        );
        Ok(result)
    }

    pub(crate) fn cached_bundle_count(&self) -> usize {
        self.bundle_indexes
            .lock()
            .expect("asset bundle index cache poisoned")
            .len()
    }

    fn cached_bundle_entry_count(&self) -> usize {
        self.bundle_indexes
            .lock()
            .expect("asset bundle index cache poisoned")
            .values()
            .map(|index| index.entries.len())
            .sum()
    }

    fn record_resolved_location(&self, location: &AssetSourceLocation) {
        let mut counters = self
            .counters
            .lock()
            .expect("asset provider counters poisoned");
        match location {
            AssetSourceLocation::Raw(_) => counters.raw += 1,
            AssetSourceLocation::Cooked(_) => counters.cooked += 1,
            AssetSourceLocation::Package(_) => counters.package += 1,
            AssetSourceLocation::Bundle { .. } => counters.bundle += 1,
            #[cfg(test)]
            AssetSourceLocation::Memory { .. } => {}
        }
    }

    fn record_resolve_error(&self) {
        self.counters
            .lock()
            .expect("asset provider counters poisoned")
            .errors += 1;
    }

    fn record_cache_invalidation(&self, full: bool) {
        let mut counters = self
            .counters
            .lock()
            .expect("asset provider counters poisoned");
        counters.invalidations += 1;
        if full {
            counters.full_invalidations += 1;
        }
    }

    fn resolve_counters(&self) -> ProviderResolveCounters {
        *self
            .counters
            .lock()
            .expect("asset provider counters poisoned")
    }
}

impl AssetProvider for LocalAssetProvider {
    fn resolve(
        &self,
        _id: AssetId,
        entry: AssetManifestEntry,
        raw_source_path: Option<PathBuf>,
    ) -> Result<ResolvedAssetSource, AssetError> {
        let location = match raw_source_path {
            Some(path) => AssetSourceLocation::Raw(path),
            None => match self.resolve_cooked_location(&entry.cooked_path) {
                Ok(location) => location,
                Err(error) => {
                    self.record_resolve_error();
                    return Err(error);
                }
            },
        };
        self.record_resolved_location(&location);
        Ok(ResolvedAssetSource::new(entry, location))
    }

    fn stats(&self) -> AssetProviderStats {
        let counters = self.resolve_counters();
        AssetProviderStats {
            package_roots: self.package_roots.len(),
            package_files: self.package_files.len(),
            cached_bundle_indexes: self.cached_bundle_count(),
            cached_bundle_index_entries: self.cached_bundle_entry_count(),
            resolved_raw_sources: counters.raw,
            resolved_cooked_sources: counters.cooked,
            resolved_package_sources: counters.package,
            resolved_bundle_sources: counters.bundle,
            resolve_errors: counters.errors,
            cache_invalidations: counters.invalidations,
            full_cache_invalidations: counters.full_invalidations,
        }
    }

    fn invalidate_changed_paths(&self, paths: &[PathBuf]) {
        if paths.is_empty() {
            return;
        }

        self.record_cache_invalidation(false);

        let mut indexes = self
            .bundle_indexes
            .lock()
            .expect("asset bundle index cache poisoned");
        indexes.retain(|bundle_path, _| {
            !paths
                .iter()
                .any(|changed| provider_paths_match(changed, bundle_path))
        });
    }

    fn invalidate_all(&self) {
        self.record_cache_invalidation(true);
        self.bundle_indexes
            .lock()
            .expect("asset bundle index cache poisoned")
            .clear();
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ProviderResolveCounters {
    raw: usize,
    cooked: usize,
    package: usize,
    bundle: usize,
    errors: usize,
    invalidations: usize,
    full_invalidations: usize,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct BundleIndexFile {
    version: u32,
    entries: Vec<BundleIndexEntryFile>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct BundleIndexEntryFile {
    path: String,
    offset: u64,
    len: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BundleReadEntry {
    offset: u64,
    len: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BundleFingerprint {
    len: u64,
    modified: Option<SystemTime>,
}

impl BundleFingerprint {
    fn read(path: &Path) -> Result<Self, AssetError> {
        let metadata = std::fs::metadata(path).map_err(|error| AssetError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
        Ok(Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
        })
    }
}

#[derive(Clone, Debug)]
struct CachedBundleIndex {
    fingerprint: BundleFingerprint,
    entries: HashMap<String, BundleReadEntry>,
}

fn read_bundle_index(bundle_path: &Path) -> Result<HashMap<String, BundleReadEntry>, AssetError> {
    let mut file = File::open(bundle_path).map_err(|error| AssetError::Io {
        path: bundle_path.to_path_buf(),
        message: error.to_string(),
    })?;

    let mut magic = vec![0u8; ASSET_BUNDLE_MAGIC.len()];
    file.read_exact(&mut magic)
        .map_err(|error| AssetError::InvalidCookedAsset {
            id: None,
            message: format!(
                "asset bundle {} is missing a valid header: {error}",
                bundle_path.display()
            ),
        })?;
    if magic != ASSET_BUNDLE_MAGIC {
        return Err(AssetError::InvalidCookedAsset {
            id: None,
            message: format!(
                "asset bundle {} has an unsupported magic header",
                bundle_path.display()
            ),
        });
    }

    let mut index_len_bytes = [0u8; 8];
    file.read_exact(&mut index_len_bytes)
        .map_err(|error| AssetError::InvalidCookedAsset {
            id: None,
            message: format!(
                "asset bundle {} is missing its index length: {error}",
                bundle_path.display()
            ),
        })?;
    let index_len = u64::from_le_bytes(index_len_bytes);
    if index_len > MAX_BUNDLE_INDEX_BYTES {
        return Err(AssetError::InvalidCookedAsset {
            id: None,
            message: format!(
                "asset bundle {} index is too large: {index_len} bytes",
                bundle_path.display()
            ),
        });
    }

    let mut index_bytes = vec![0u8; index_len as usize];
    file.read_exact(&mut index_bytes)
        .map_err(|error| AssetError::InvalidCookedAsset {
            id: None,
            message: format!(
                "asset bundle {} index is truncated: {error}",
                bundle_path.display()
            ),
        })?;
    let index: BundleIndexFile =
        serde_json::from_slice(&index_bytes).map_err(|error| AssetError::Json {
            path: bundle_path.to_path_buf(),
            message: error.to_string(),
        })?;
    if index.version != ASSET_BUNDLE_VERSION {
        return Err(AssetError::InvalidCookedAsset {
            id: None,
            message: format!(
                "asset bundle {} version mismatch: expected {}, got {}",
                bundle_path.display(),
                ASSET_BUNDLE_VERSION,
                index.version
            ),
        });
    }

    let payload_start = ASSET_BUNDLE_MAGIC.len() as u64 + 8 + index_len;
    Ok(index
        .entries
        .into_iter()
        .map(|entry| {
            (
                normalize_source_key(&entry.path),
                BundleReadEntry {
                    offset: payload_start.saturating_add(entry.offset),
                    len: entry.len,
                },
            )
        })
        .collect())
}

fn provider_paths_match(left: &Path, right: &Path) -> bool {
    left == right
        || std::fs::canonicalize(left)
            .ok()
            .zip(std::fs::canonicalize(right).ok())
            .is_some_and(|(left, right)| left == right)
}

fn read_bundle_bytes(
    bundle_path: &Path,
    offset: u64,
    len: u64,
    id: AssetId,
) -> Result<Vec<u8>, AssetError> {
    let len = usize::try_from(len).map_err(|_| AssetError::InvalidCookedAsset {
        id: Some(id),
        message: format!(
            "asset bundle {} entry is too large to read on this platform",
            bundle_path.display()
        ),
    })?;
    let mut bytes = vec![0u8; len];
    let mut file = File::open(bundle_path).map_err(|error| AssetError::Io {
        path: bundle_path.to_path_buf(),
        message: error.to_string(),
    })?;
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| AssetError::Io {
            path: bundle_path.to_path_buf(),
            message: error.to_string(),
        })?;
    file.read_exact(&mut bytes)
        .map_err(|error| AssetError::Io {
            path: bundle_path.to_path_buf(),
            message: error.to_string(),
        })?;
    Ok(bytes)
}

#[cfg(test)]
pub(crate) fn write_test_bundle(path: &Path, entries: &[(&str, &[u8])]) -> Result<(), AssetError> {
    let mut payload = Vec::new();
    let mut index_entries = Vec::new();
    for (entry_path, bytes) in entries {
        let offset = payload.len() as u64;
        payload.extend_from_slice(bytes);
        index_entries.push(BundleIndexEntryFile {
            path: (*entry_path).to_string(),
            offset,
            len: bytes.len() as u64,
        });
    }

    let index = BundleIndexFile {
        version: ASSET_BUNDLE_VERSION,
        entries: index_entries,
    };
    let index_bytes = serde_json::to_vec(&index).map_err(|error| AssetError::Json {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    let mut bundle = Vec::new();
    bundle.extend_from_slice(ASSET_BUNDLE_MAGIC);
    bundle.extend_from_slice(&(index_bytes.len() as u64).to_le_bytes());
    bundle.extend_from_slice(&index_bytes);
    bundle.extend_from_slice(&payload);
    std::fs::write(path, bundle).map_err(|error| AssetError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}
