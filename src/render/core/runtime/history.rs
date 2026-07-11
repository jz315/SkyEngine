use std::borrow::Cow;
use std::cell::RefCell;
use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::gpu::GpuContext;
use crate::render::execution::TextureFormat;
use crate::render::graph::{ImportedTexture, RenderGraph, TextureHandle};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HistoryTextureSize {
    Full,
    Scale(f32),
    Exact(u32, u32),
}

impl HistoryTextureSize {
    #[inline]
    fn resolve(self, base_size: [u32; 2]) -> [u32; 2] {
        match self {
            Self::Full => [base_size[0].max(1), base_size[1].max(1)],
            Self::Scale(scale) => [
                (base_size[0] as f32 * scale).round().max(1.0) as u32,
                (base_size[1] as f32 * scale).round().max(1.0) as u32,
            ],
            Self::Exact(width, height) => [width.max(1), height.max(1)],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryTexture {
    read: Option<TextureHandle>,
    write: TextureHandle,
    reset: bool,
    size: [u32; 2],
    format: TextureFormat,
}

impl HistoryTexture {
    #[inline]
    pub const fn read(self) -> Option<TextureHandle> {
        self.read
    }

    #[inline]
    pub const fn write(self) -> TextureHandle {
        self.write
    }

    #[inline]
    pub const fn reset(self) -> bool {
        self.reset
    }

    #[inline]
    pub const fn size(self) -> [u32; 2] {
        self.size
    }

    #[inline]
    pub const fn format(self) -> TextureFormat {
        self.format
    }
}

#[derive(Default)]
pub struct HistoryTextureStore {
    pool: RefCell<HistoryTexturePool>,
}

impl HistoryTextureStore {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub(crate) fn begin_frame(&self, gpu: &GpuContext) {
        self.pool.borrow_mut().begin_frame(gpu);
    }

    #[inline]
    pub(crate) fn invalidate(&self) {
        self.pool.borrow_mut().entries.clear();
    }

    #[inline]
    pub(crate) fn request<'a>(
        &'a self,
        graph: &'a mut RenderGraph,
        view_id: u64,
        view_size: [u32; 2],
        name: impl Into<Cow<'static, str>>,
    ) -> HistoryTextureRequest<'a> {
        HistoryTextureRequest::new(self, graph, view_id, view_size, name)
    }
}

pub struct HistoryTextureRequest<'a> {
    store: &'a HistoryTextureStore,
    graph: &'a mut RenderGraph,
    view_id: u64,
    view_size: [u32; 2],
    name: Cow<'static, str>,
    size: HistoryTextureSize,
    format: TextureFormat,
    usage: wgpu::TextureUsages,
    mip_level_count: u32,
    array_layer_count: u32,
    ping_pong: bool,
}

impl<'a> HistoryTextureRequest<'a> {
    fn new(
        store: &'a HistoryTextureStore,
        graph: &'a mut RenderGraph,
        view_id: u64,
        view_size: [u32; 2],
        name: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self {
            store,
            graph,
            view_id,
            view_size,
            name: name.into(),
            size: HistoryTextureSize::Full,
            format: TextureFormat::Rgba16Float,
            usage: default_history_texture_usage(),
            mip_level_count: 1,
            array_layer_count: 1,
            ping_pong: false,
        }
    }

    #[inline]
    pub fn size(mut self, size: HistoryTextureSize) -> Self {
        self.size = size;
        self
    }

    #[inline]
    pub fn full_res(mut self) -> Self {
        self.size = HistoryTextureSize::Full;
        self
    }

    #[inline]
    pub fn half_res(mut self) -> Self {
        self.size = HistoryTextureSize::Scale(0.5);
        self
    }

    #[inline]
    pub fn exact_size(mut self, width: u32, height: u32) -> Self {
        self.size = HistoryTextureSize::Exact(width, height);
        self
    }

    #[inline]
    pub fn format(mut self, format: TextureFormat) -> Self {
        self.format = format;
        self
    }

    #[inline]
    pub fn usage(mut self, usage: wgpu::TextureUsages) -> Self {
        self.usage = usage;
        self
    }

    #[inline]
    pub fn add_usage(mut self, usage: wgpu::TextureUsages) -> Self {
        self.usage |= usage;
        self
    }

    #[inline]
    pub fn sampled(self) -> Self {
        self.add_usage(wgpu::TextureUsages::TEXTURE_BINDING)
    }

    #[inline]
    pub fn storage_binding(self) -> Self {
        self.add_usage(wgpu::TextureUsages::STORAGE_BINDING)
    }

    #[inline]
    pub fn render_attachment(self) -> Self {
        self.add_usage(wgpu::TextureUsages::RENDER_ATTACHMENT)
    }

    #[inline]
    pub fn copy_src(self) -> Self {
        self.add_usage(wgpu::TextureUsages::COPY_SRC)
    }

    #[inline]
    pub fn copy_dst(self) -> Self {
        self.add_usage(wgpu::TextureUsages::COPY_DST)
    }

    #[inline]
    pub fn mip_level_count(mut self, mip_level_count: u32) -> Self {
        self.mip_level_count = mip_level_count.max(1);
        self
    }

    #[inline]
    pub fn array_layer_count(mut self, array_layer_count: u32) -> Self {
        self.array_layer_count = array_layer_count.max(1);
        self
    }

    #[inline]
    pub fn ping_pong(mut self) -> Self {
        self.ping_pong = true;
        self
    }

    pub fn get(self) -> HistoryTexture {
        let desc = HistoryTextureDesc {
            size: self.size.resolve(self.view_size),
            format: self.format,
            usage: self.usage,
            mip_level_count: self.mip_level_count,
            array_layer_count: self.array_layer_count,
            ping_pong: self.ping_pong,
        };
        self.store
            .pool
            .borrow_mut()
            .request(self.graph, self.view_id, self.name, desc)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct HistoryTextureKey {
    view_id: u64,
    name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HistoryTextureDesc {
    size: [u32; 2],
    format: TextureFormat,
    usage: wgpu::TextureUsages,
    mip_level_count: u32,
    array_layer_count: u32,
    ping_pong: bool,
}

#[derive(Debug)]
struct HistoryTextureResource {
    texture: Arc<wgpu::Texture>,
    view: Arc<wgpu::TextureView>,
}

#[derive(Debug)]
struct HistoryTextureEntry {
    desc: HistoryTextureDesc,
    textures: Vec<HistoryTextureResource>,
    reset_frame: u64,
    last_used_frame: u64,
}

#[derive(Default)]
pub(crate) struct HistoryTexturePool {
    device: Option<wgpu::Device>,
    frame_index: u64,
    entries: FxHashMap<HistoryTextureKey, HistoryTextureEntry>,
}

impl HistoryTexturePool {
    fn begin_frame(&mut self, gpu: &GpuContext) {
        let previous_frame = self.frame_index;
        if previous_frame != 0 {
            self.entries
                .retain(|_, entry| entry.last_used_frame == previous_frame);
        }
        self.device = Some(gpu.device().clone());
        self.frame_index = self.frame_index.wrapping_add(1).max(1);
    }

    fn request(
        &mut self,
        graph: &mut RenderGraph,
        view_id: u64,
        name: Cow<'static, str>,
        desc: HistoryTextureDesc,
    ) -> HistoryTexture {
        let key = HistoryTextureKey {
            view_id,
            name: name.to_string(),
        };
        let frame_index = self.frame_index;
        let device = self
            .device
            .as_ref()
            .expect("HistoryTextureStore::begin_frame must run before history requests")
            .clone();
        let entry = self
            .entries
            .entry(key)
            .or_insert_with(|| HistoryTextureEntry {
                desc,
                textures: create_history_textures(&device, view_id, name.as_ref(), desc),
                reset_frame: frame_index,
                last_used_frame: frame_index,
            });
        if entry.desc != desc {
            entry.desc = desc;
            entry.textures = create_history_textures(&device, view_id, name.as_ref(), desc);
            entry.reset_frame = frame_index;
        }
        entry.last_used_frame = frame_index;

        let slot_count = entry.textures.len();
        let write_index = if desc.ping_pong {
            (frame_index as usize) % slot_count
        } else {
            0
        };
        let read_index = if desc.ping_pong {
            (write_index + 1) % slot_count
        } else {
            write_index
        };
        let reset = entry.reset_frame == frame_index;
        let write = import_history_texture(
            graph,
            view_id,
            name.as_ref(),
            "write",
            &entry.textures[write_index],
            desc,
        );
        let read = if desc.ping_pong && !reset {
            Some(import_history_texture(
                graph,
                view_id,
                name.as_ref(),
                "read",
                &entry.textures[read_index],
                desc,
            ))
        } else {
            None
        };

        HistoryTexture {
            read,
            write,
            reset,
            size: desc.size,
            format: desc.format,
        }
    }
}

fn create_history_textures(
    device: &wgpu::Device,
    view_id: u64,
    name: &str,
    desc: HistoryTextureDesc,
) -> Vec<HistoryTextureResource> {
    let count = if desc.ping_pong { 2 } else { 1 };
    (0..count)
        .map(|index| {
            let label = format!("history_{view_id}_{name}_{index}");
            let texture = Arc::new(device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&label),
                size: wgpu::Extent3d {
                    width: desc.size[0].max(1),
                    height: desc.size[1].max(1),
                    depth_or_array_layers: desc.array_layer_count.max(1),
                },
                mip_level_count: desc.mip_level_count.max(1),
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: desc.format,
                usage: desc.usage,
                view_formats: &[],
            }));
            let view = Arc::new(texture.create_view(&wgpu::TextureViewDescriptor::default()));
            HistoryTextureResource { texture, view }
        })
        .collect()
}

fn import_history_texture(
    graph: &mut RenderGraph,
    view_id: u64,
    name: &str,
    role: &str,
    resource: &HistoryTextureResource,
    desc: HistoryTextureDesc,
) -> TextureHandle {
    graph.create_texture(|builder| {
        builder.name(format!("history_{view_id}_{name}_{role}"));
        builder.import_external(ImportedTexture {
            texture: Arc::clone(&resource.texture),
            view: Arc::clone(&resource.view),
            size: desc.size,
            format: desc.format,
            usage: desc.usage,
            sample_count: 1,
            mip_level_count: desc.mip_level_count,
            array_layer_count: desc.array_layer_count,
        });
    })
}

#[inline]
fn default_history_texture_usage() -> wgpu::TextureUsages {
    wgpu::TextureUsages::RENDER_ATTACHMENT
        | wgpu::TextureUsages::TEXTURE_BINDING
        | wgpu::TextureUsages::COPY_SRC
        | wgpu::TextureUsages::COPY_DST
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("No suitable GPU adapter found for history tests");

        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("history_test_device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            ..Default::default()
        }))
        .expect("Failed to create test GPU device")
    }

    fn test_context(size: [u32; 2]) -> GpuContext {
        let (device, queue) = create_test_device();
        GpuContext::new_headless(device, queue, TextureFormat::Bgra8Unorm, size)
    }

    fn entry_texture_ptr(
        store: &HistoryTextureStore,
        view_id: u64,
        name: &str,
        index: usize,
    ) -> *const wgpu::Texture {
        let pool = store.pool.borrow();
        let entry = pool
            .entries
            .get(&HistoryTextureKey {
                view_id,
                name: name.to_string(),
            })
            .expect("history entry should exist");
        Arc::as_ptr(&entry.textures[index].texture)
    }

    #[test]
    fn history_texture_persists_across_frames() {
        let ctx = test_context([32, 32]);
        let store = HistoryTextureStore::new();
        let mut graph = RenderGraph::new();
        store.begin_frame(&ctx);
        let first = store
            .request(&mut graph, 0, [32, 32], "taa_color")
            .format(TextureFormat::Rgba16Float)
            .ping_pong()
            .get();
        let first_ptr = entry_texture_ptr(&store, 0, "taa_color", 0);

        let mut graph = RenderGraph::new();
        store.begin_frame(&ctx);
        let second = store
            .request(&mut graph, 0, [32, 32], "taa_color")
            .format(TextureFormat::Rgba16Float)
            .ping_pong()
            .get();
        let second_ptr = entry_texture_ptr(&store, 0, "taa_color", 0);

        assert!(first.reset());
        assert!(!second.reset());
        assert!(second.read().is_some());
        assert_ne!(first.write(), second.write());
        assert_eq!(first_ptr, second_ptr);
    }

    #[test]
    fn history_texture_resizes_on_view_resize() {
        let ctx = test_context([64, 64]);
        let store = HistoryTextureStore::new();
        let mut graph = RenderGraph::new();
        store.begin_frame(&ctx);
        let first = store
            .request(&mut graph, 7, [16, 16], "ssgi")
            .format(TextureFormat::Rgba16Float)
            .get();
        let first_ptr = entry_texture_ptr(&store, 7, "ssgi", 0);

        let mut graph = RenderGraph::new();
        store.begin_frame(&ctx);
        let resized = store
            .request(&mut graph, 7, [32, 16], "ssgi")
            .format(TextureFormat::Rgba16Float)
            .get();
        let resized_ptr = entry_texture_ptr(&store, 7, "ssgi", 0);

        assert_eq!(first.size(), [16, 16]);
        assert_eq!(resized.size(), [32, 16]);
        assert!(resized.reset());
        assert_ne!(first_ptr, resized_ptr);
    }

    #[test]
    fn history_texture_is_isolated_per_view() {
        let ctx = test_context([64, 64]);
        let store = HistoryTextureStore::new();
        let mut graph = RenderGraph::new();
        store.begin_frame(&ctx);
        let view_a = store
            .request(&mut graph, 1, [32, 32], "taa_color")
            .format(TextureFormat::Rgba16Float)
            .get();
        let view_b = store
            .request(&mut graph, 2, [32, 32], "taa_color")
            .format(TextureFormat::Rgba16Float)
            .get();

        let ptr_a = entry_texture_ptr(&store, 1, "taa_color", 0);
        let ptr_b = entry_texture_ptr(&store, 2, "taa_color", 0);

        assert_ne!(view_a.write(), view_b.write());
        assert_ne!(ptr_a, ptr_b);
        assert_eq!(store.pool.borrow().entries.len(), 2);
    }

    #[test]
    fn history_texture_pool_releases_views_missing_for_a_full_frame() {
        let ctx = test_context([32, 32]);
        let store = HistoryTextureStore::new();
        let mut graph = RenderGraph::new();

        store.begin_frame(&ctx);
        let first = store
            .request(&mut graph, 9, [32, 32], "taa_color")
            .ping_pong()
            .get();
        assert!(first.reset());
        assert_eq!(store.pool.borrow().entries.len(), 1);

        // The first frame without a request establishes that the view is gone.
        store.begin_frame(&ctx);
        assert_eq!(store.pool.borrow().entries.len(), 1);

        // On the following frame the unused GPU history is released.
        store.begin_frame(&ctx);
        assert!(store.pool.borrow().entries.is_empty());

        let mut graph = RenderGraph::new();
        let returned = store
            .request(&mut graph, 9, [32, 32], "taa_color")
            .ping_pong()
            .get();
        assert!(returned.reset());
    }

    #[test]
    fn history_texture_store_invalidation_releases_all_gpu_history() {
        let ctx = test_context([16, 16]);
        let store = HistoryTextureStore::new();
        let mut graph = RenderGraph::new();
        store.begin_frame(&ctx);
        let _ = store.request(&mut graph, 3, [16, 16], "taa").get();
        assert_eq!(store.pool.borrow().entries.len(), 1);

        store.invalidate();

        assert!(store.pool.borrow().entries.is_empty());
    }
}
