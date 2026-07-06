use std::path::{Path, PathBuf};
use std::time::Duration;

pub const ASSET_SYSTEM_VERSION: u32 = 1;
pub const DEFAULT_ASSET_IO_WORKER_THREADS: usize = 2;
pub const DEFAULT_ASSET_IO_QUEUE_CAPACITY: usize = 256;
pub const DEFAULT_ASSET_IO_PRIORITY: i32 = 0;
pub const DEFAULT_ASSET_IO_SHUTDOWN_TIMEOUT: Option<Duration> = None;
pub const DEFAULT_ASSET_INSTALL_TIME_BUDGET: Duration = Duration::from_millis(4);

#[derive(Clone, Debug)]
pub struct AssetConfig {
    pub asset_root: PathBuf,
    pub target: String,
    pub profile: String,
    pub background_loading: bool,
    pub install_budget_per_update: Option<usize>,
    pub install_time_budget: Option<Duration>,
    pub auto_reload: bool,
    pub auto_reload_interval: Duration,
    pub auto_reload_debounce: Duration,
    pub file_watcher: bool,
    pub package_roots: Vec<PathBuf>,
    pub package_files: Vec<PathBuf>,
    pub io_worker_threads: usize,
    pub io_queue_capacity: usize,
    pub io_default_priority: i32,
    pub io_shutdown_timeout: Option<Duration>,
}

impl AssetConfig {
    #[must_use]
    pub fn new(asset_root: impl Into<PathBuf>, target: impl Into<String>) -> Self {
        Self {
            asset_root: asset_root.into(),
            target: target.into(),
            profile: "default".to_string(),
            background_loading: false,
            install_budget_per_update: None,
            install_time_budget: None,
            auto_reload: false,
            auto_reload_interval: Duration::from_millis(250),
            auto_reload_debounce: Duration::ZERO,
            file_watcher: false,
            package_roots: Vec::new(),
            package_files: Vec::new(),
            io_worker_threads: DEFAULT_ASSET_IO_WORKER_THREADS,
            io_queue_capacity: DEFAULT_ASSET_IO_QUEUE_CAPACITY,
            io_default_priority: DEFAULT_ASSET_IO_PRIORITY,
            io_shutdown_timeout: DEFAULT_ASSET_IO_SHUTDOWN_TIMEOUT,
        }
    }

    #[must_use]
    pub fn with_background_loading(mut self, enabled: bool) -> Self {
        self.background_loading = enabled;
        self
    }

    #[must_use]
    pub fn with_profile(mut self, profile: impl Into<String>) -> Self {
        self.profile = profile.into();
        self
    }

    #[must_use]
    pub fn with_install_budget_per_update(mut self, budget: usize) -> Self {
        self.install_budget_per_update = Some(budget);
        self
    }

    #[must_use]
    pub fn with_install_time_budget(mut self, budget: Duration) -> Self {
        self.install_time_budget = Some(budget);
        self
    }

    #[must_use]
    pub fn without_install_time_budget(mut self) -> Self {
        self.install_time_budget = None;
        self
    }

    #[must_use]
    pub fn with_auto_reload(mut self, enabled: bool) -> Self {
        self.auto_reload = enabled;
        self
    }

    #[must_use]
    pub fn with_auto_reload_interval(mut self, interval: Duration) -> Self {
        self.auto_reload_interval = interval;
        self
    }

    #[must_use]
    pub fn with_auto_reload_debounce(mut self, debounce: Duration) -> Self {
        self.auto_reload_debounce = debounce;
        self
    }

    #[must_use]
    pub fn with_file_watcher(mut self, enabled: bool) -> Self {
        self.file_watcher = enabled;
        self
    }

    #[must_use]
    pub fn with_package_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.package_roots.push(root.into());
        self
    }

    #[must_use]
    pub fn with_package_roots<I, P>(mut self, roots: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        self.package_roots.extend(roots.into_iter().map(Into::into));
        self
    }

    #[must_use]
    pub fn without_package_roots(mut self) -> Self {
        self.package_roots.clear();
        self
    }

    #[must_use]
    pub fn with_package_file(mut self, file: impl Into<PathBuf>) -> Self {
        self.package_files.push(file.into());
        self
    }

    #[must_use]
    pub fn with_package_files<I, P>(mut self, files: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        self.package_files.extend(files.into_iter().map(Into::into));
        self
    }

    #[must_use]
    pub fn without_package_files(mut self) -> Self {
        self.package_files.clear();
        self
    }

    #[must_use]
    pub fn with_io_worker_threads(mut self, worker_threads: usize) -> Self {
        self.io_worker_threads = worker_threads.max(1);
        self
    }

    #[must_use]
    pub fn with_io_queue_capacity(mut self, queue_capacity: usize) -> Self {
        self.io_queue_capacity = queue_capacity.max(1);
        self
    }

    #[must_use]
    pub fn with_io_default_priority(mut self, priority: i32) -> Self {
        self.io_default_priority = priority;
        self
    }

    #[must_use]
    pub fn with_io_shutdown_timeout(mut self, timeout: Duration) -> Self {
        self.io_shutdown_timeout = Some(timeout);
        self
    }

    #[must_use]
    pub fn without_io_shutdown_timeout(mut self) -> Self {
        self.io_shutdown_timeout = None;
        self
    }

    #[must_use]
    pub fn default_target() -> String {
        if cfg!(target_arch = "wasm32") {
            "web".to_string()
        } else {
            "native".to_string()
        }
    }

    #[must_use]
    pub fn cooked_root(&self) -> PathBuf {
        self.asset_root
            .join(".sky")
            .join("cooked")
            .join(&self.target)
    }

    #[must_use]
    pub fn package_roots(&self) -> Vec<PathBuf> {
        self.package_roots
            .iter()
            .map(|root| {
                if root.is_absolute() {
                    root.clone()
                } else {
                    self.asset_root.join(root)
                }
            })
            .collect()
    }

    #[must_use]
    pub fn package_files(&self) -> Vec<PathBuf> {
        self.package_files
            .iter()
            .map(|file| {
                if file.is_absolute() {
                    file.clone()
                } else {
                    self.asset_root.join(file)
                }
            })
            .collect()
    }

    #[must_use]
    pub fn manifest_path(&self) -> PathBuf {
        self.cooked_root().join("manifest.json")
    }

    #[must_use]
    pub fn source_key(&self, path: &Path) -> String {
        let relative = if path.is_absolute() {
            path.strip_prefix(&self.asset_root).unwrap_or(path)
        } else {
            path
        };
        normalize_source_key(&relative.to_string_lossy())
    }
}

impl Default for AssetConfig {
    fn default() -> Self {
        Self::new("assets", Self::default_target())
    }
}

#[must_use]
pub fn normalize_source_key(value: &str) -> String {
    let normalized = value.replace('\\', "/");
    if cfg!(windows) {
        normalized.to_ascii_lowercase()
    } else {
        normalized
    }
}
