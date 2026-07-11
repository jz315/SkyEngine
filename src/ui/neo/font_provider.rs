use std::path::PathBuf;
use std::sync::Arc;

use eui_neo::expert::{UiDrawCommand, UiDrawList};
use eui_neo::{FontRef, Runtime};
use eui_neo_wgpu::WgpuRenderer;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::asset::{AssetId, Assets, FontAsset, Handle};

#[derive(Default)]
pub(crate) struct SkyNeoFontStore {
    handles: FxHashMap<FontRef, Handle<FontAsset>>,
    revisions: FxHashMap<FontRef, u64>,
    failed: FxHashSet<FontRef>,
    pending_frame: FxHashSet<FontRef>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SkyNeoFontPrepareStatus {
    pub(crate) pending: bool,
    pub(crate) ready_changed: bool,
}

impl SkyNeoFontStore {
    pub(crate) fn prepare(
        &mut self,
        runtime: &mut Runtime,
        renderer: &mut WgpuRenderer,
        draw_list: &UiDrawList,
        asset_server: Option<&Assets>,
    ) -> SkyNeoFontPrepareStatus {
        self.pending_frame.clear();
        let mut status = SkyNeoFontPrepareStatus::default();
        let keys = font_keys(draw_list);
        if keys.is_empty() {
            return status;
        }

        let Some(asset_server) = asset_server else {
            return status;
        };

        for key in keys {
            let Some(handle) = self.resolve_handle(asset_server, &key) else {
                continue;
            };
            if let Some(asset) = asset_server.try_get(&handle) {
                status.ready_changed |= self.register_ready_font(runtime, renderer, &key, asset);
                self.failed.remove(&key);
                continue;
            }

            match asset_server.state(&handle) {
                crate::asset::AssetState::Failed => {
                    self.failed.insert(key);
                }
                crate::asset::AssetState::Unloaded
                | crate::asset::AssetState::Loading
                | crate::asset::AssetState::Loaded
                | crate::asset::AssetState::WaitingDependencies
                | crate::asset::AssetState::Installing
                | crate::asset::AssetState::Uninstalling
                | crate::asset::AssetState::Unloading => {
                    self.pending_frame.insert(key);
                }
                crate::asset::AssetState::Installed => {
                    self.pending_frame.insert(key);
                }
            }
        }

        status.pending = !self.pending_frame.is_empty();
        status
    }

    fn resolve_handle(
        &mut self,
        asset_server: &Assets,
        key: &FontRef,
    ) -> Option<Handle<FontAsset>> {
        let source = key.as_source()?;
        if let Some(handle) = self.handles.get(key).cloned() {
            return Some(handle);
        }
        match load_font_handle(asset_server, key) {
            Ok(handle) => {
                self.handles.insert(key.clone(), handle.clone());
                Some(handle)
            }
            Err(error) => {
                eprintln!("[SkyEngine] neo font asset lookup failed for {source}: {error}");
                self.failed.insert(key.clone());
                None
            }
        }
    }

    fn register_ready_font(
        &mut self,
        runtime: &mut Runtime,
        renderer: &mut WgpuRenderer,
        key: &FontRef,
        asset: Arc<FontAsset>,
    ) -> bool {
        let revision = Arc::as_ptr(&asset) as usize as u64;
        if !self.record_ready_revision(key, revision) {
            return false;
        }
        runtime.register_font(key, asset.bytes());
        renderer.register_font(key, asset.bytes(), revision);
        true
    }

    fn record_ready_revision(&mut self, key: &FontRef, revision: u64) -> bool {
        if self
            .revisions
            .get(key)
            .is_some_and(|current| *current == revision)
        {
            return false;
        }
        self.revisions.insert(key.clone(), revision);
        true
    }
}

fn font_keys(draw_list: &UiDrawList) -> Vec<FontRef> {
    let mut seen = FxHashSet::default();
    let mut keys = Vec::new();
    for command in draw_list.commands() {
        let UiDrawCommand::Text(draw) = command else {
            continue;
        };
        if !draw.font.is_resource_ref() {
            continue;
        }
        if seen.insert(draw.font.clone()) {
            keys.push(draw.font.clone());
        }
    }
    keys
}

fn load_font_handle(asset_server: &Assets, key: &FontRef) -> Result<Handle<FontAsset>, String> {
    let source = key.as_source().unwrap_or_default();
    if matches!(key, FontRef::Asset(_)) {
        if let Ok(id) = AssetId::parse_str(source) {
            return asset_server
                .load_id::<FontAsset>(id)
                .map_err(|error| error.to_string());
        }
        return asset_server
            .load_font(PathBuf::from(source))
            .map_err(|error| error.to_string());
    }
    if let Some(value) = source.strip_prefix("asset://") {
        if let Ok(id) = AssetId::parse_str(value) {
            return asset_server
                .load_id::<FontAsset>(id)
                .map_err(|error| error.to_string());
        }
        return asset_server
            .load_font(PathBuf::from(value))
            .map_err(|error| error.to_string());
    }
    asset_server
        .load_font(PathBuf::from(source))
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_font_revision_reports_only_new_asset_versions() {
        let mut store = SkyNeoFontStore::default();
        let key = FontRef::asset("font.ttf");

        assert!(store.record_ready_revision(&key, 11));
        assert!(!store.record_ready_revision(&key, 11));
        assert!(store.record_ready_revision(&key, 12));
    }
}
