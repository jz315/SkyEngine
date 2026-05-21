use std::path::PathBuf;
use std::sync::Arc;

use eui_neo::expert::{UiDrawCommand, UiDrawList};
use eui_neo::{FontRef, Runtime};
use eui_neo_wgpu::WgpuRenderer;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::asset::{AssetId, Assets, FontAsset, Handle};

pub(crate) struct SkyNeoFontStore {
    handles: FxHashMap<FontRef, Handle<FontAsset>>,
    revisions: FxHashMap<FontRef, u64>,
    failed: FxHashSet<FontRef>,
    pending_frame: FxHashSet<FontRef>,
}

impl Default for SkyNeoFontStore {
    fn default() -> Self {
        Self {
            handles: FxHashMap::default(),
            revisions: FxHashMap::default(),
            failed: FxHashSet::default(),
            pending_frame: FxHashSet::default(),
        }
    }
}

impl SkyNeoFontStore {
    pub(crate) fn prepare(
        &mut self,
        runtime: &mut Runtime,
        renderer: &mut WgpuRenderer,
        draw_list: &UiDrawList,
        asset_server: Option<&Assets>,
    ) -> bool {
        self.pending_frame.clear();
        let keys = font_keys(draw_list);
        if keys.is_empty() {
            return false;
        }

        let Some(asset_server) = asset_server else {
            return false;
        };

        for key in keys {
            let Some(handle) = self.resolve_handle(asset_server, &key) else {
                continue;
            };
            if let Some(asset) = asset_server.try_get(&handle) {
                self.register_ready_font(runtime, renderer, &key, asset);
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

        !self.pending_frame.is_empty()
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
        match load_font_handle(asset_server, source) {
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
    ) {
        let revision = Arc::as_ptr(&asset) as usize as u64;
        if self
            .revisions
            .get(key)
            .is_some_and(|current| *current == revision)
        {
            return;
        }
        runtime.register_font(key, asset.bytes());
        renderer.register_font(key, asset.bytes(), revision);
        self.revisions.insert(key.clone(), revision);
    }
}

fn font_keys(draw_list: &UiDrawList) -> Vec<FontRef> {
    let mut seen = FxHashSet::default();
    let mut keys = Vec::new();
    for command in draw_list.commands() {
        let UiDrawCommand::Text(draw) = command else {
            continue;
        };
        if !matches!(draw.font, FontRef::Source(_)) {
            continue;
        }
        if seen.insert(draw.font.clone()) {
            keys.push(draw.font.clone());
        }
    }
    keys
}

fn load_font_handle(asset_server: &Assets, source: &str) -> Result<Handle<FontAsset>, String> {
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
