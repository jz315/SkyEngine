#[cfg_attr(not(feature = "asset-watch"), allow(dead_code))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AssetWatchEvent {
    Changed(std::path::PathBuf),
    Rescan,
}

#[cfg_attr(not(feature = "asset-watch"), allow(dead_code))]
pub(crate) fn configured_watch_roots(
    config: &crate::asset::AssetConfig,
) -> Vec<std::path::PathBuf> {
    let mut roots = vec![config.asset_root.clone()];
    for root in config.package_roots() {
        if !roots.iter().any(|existing| root.starts_with(existing)) {
            roots.push(root);
        }
    }
    for file in config.package_files() {
        let root = file
            .parent()
            .map_or(file.as_path(), |parent| parent)
            .to_path_buf();
        if !roots.iter().any(|existing| root.starts_with(existing)) {
            roots.push(root);
        }
    }
    roots
}

#[cfg(feature = "asset-watch")]
mod imp {
    use std::sync::mpsc::{self, Receiver, TryRecvError};

    use notify::{RecommendedWatcher, RecursiveMode, Watcher};

    use super::{configured_watch_roots, AssetWatchEvent};
    use crate::asset::AssetConfig;

    pub(crate) struct AssetFileWatcher {
        _watcher: Option<RecommendedWatcher>,
        rx: Receiver<AssetWatchEvent>,
    }

    impl AssetFileWatcher {
        pub(crate) fn new(config: &AssetConfig) -> Self {
            let (tx, rx) = mpsc::channel();
            if !config.file_watcher {
                return Self { _watcher: None, rx };
            }

            let watch_roots = configured_watch_roots(config);
            let watcher_label = watch_roots
                .first()
                .map(|root| root.display().to_string())
                .unwrap_or_else(|| "<none>".to_string());
            let handler_tx = tx.clone();
            let mut watcher = match notify::recommended_watcher(move |event| match event {
                Ok(event) => {
                    if should_report_event(&event) {
                        if event.paths.is_empty() {
                            let _ = handler_tx.send(AssetWatchEvent::Rescan);
                        } else {
                            for path in event.paths {
                                let _ = handler_tx.send(AssetWatchEvent::Changed(path));
                            }
                        }
                    }
                }
                Err(error) => {
                    log::warn!(target: "sky_engine::asset", "asset watcher error: {error}");
                    let _ = handler_tx.send(AssetWatchEvent::Rescan);
                }
            }) {
                Ok(watcher) => watcher,
                Err(error) => {
                    log::warn!(
                        target: "sky_engine::asset",
                        "asset watcher could not start for {watcher_label}: {error}",
                    );
                    let _ = tx.send(AssetWatchEvent::Rescan);
                    return Self { _watcher: None, rx };
                }
            };

            let mut watched_any = false;
            for root in watch_roots {
                match watcher.watch(&root, RecursiveMode::Recursive) {
                    Ok(()) => watched_any = true,
                    Err(error) => {
                        log::warn!(
                            target: "sky_engine::asset",
                            "asset watcher could not watch {}: {error}",
                            root.display()
                        );
                        let _ = tx.send(AssetWatchEvent::Rescan);
                    }
                }
            }

            if !watched_any {
                return Self { _watcher: None, rx };
            }

            Self {
                _watcher: Some(watcher),
                rx,
            }
        }

        pub(crate) fn drain(&mut self) -> Vec<AssetWatchEvent> {
            let mut events = Vec::new();
            loop {
                match self.rx.try_recv() {
                    Ok(event) => events.push(event),
                    Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
                }
            }
            events
        }
    }

    fn should_report_event(event: &notify::Event) -> bool {
        use notify::event::{CreateKind, DataChange, ModifyKind, RemoveKind};
        use notify::EventKind;

        matches!(
            event.kind,
            EventKind::Create(CreateKind::Any | CreateKind::File | CreateKind::Folder)
                | EventKind::Modify(ModifyKind::Any | ModifyKind::Data(DataChange::Any))
                | EventKind::Modify(ModifyKind::Data(DataChange::Content))
                | EventKind::Modify(ModifyKind::Data(DataChange::Size))
                | EventKind::Modify(ModifyKind::Name(_))
                | EventKind::Remove(RemoveKind::Any | RemoveKind::File | RemoveKind::Folder)
                | EventKind::Any
        )
    }
}

#[cfg(not(feature = "asset-watch"))]
mod imp {
    use super::AssetWatchEvent;
    use crate::asset::AssetConfig;

    pub(crate) struct AssetFileWatcher {
        _enabled_without_feature: bool,
    }

    impl AssetFileWatcher {
        pub(crate) fn new(config: &AssetConfig) -> Self {
            if config.file_watcher {
                log::warn!(
                    target: "sky_engine::asset",
                    "asset file watcher requested but the `asset-watch` feature is disabled"
                );
            }
            Self {
                _enabled_without_feature: config.file_watcher,
            }
        }

        pub(crate) fn drain(&mut self) -> Vec<AssetWatchEvent> {
            Vec::new()
        }
    }
}

pub(crate) use imp::AssetFileWatcher;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::AssetConfig;

    #[test]
    fn configured_watch_roots_include_external_package_roots_without_nested_duplicates() {
        let asset_root = tempfile::tempdir().expect("asset root");
        let external_package = tempfile::tempdir().expect("external package root");
        let external_bundle = tempfile::tempdir().expect("external package file root");
        let config = AssetConfig::new(asset_root.path(), "native")
            .with_package_root("packages/base")
            .with_package_root(external_package.path())
            .with_package_file("packages/base.skybundle")
            .with_package_file(external_bundle.path().join("base.skybundle"));

        let roots = configured_watch_roots(&config);

        assert_eq!(roots.len(), 3);
        assert_eq!(roots[0], asset_root.path());
        assert_eq!(roots[1], external_package.path());
        assert_eq!(roots[2], external_bundle.path());
    }
}
