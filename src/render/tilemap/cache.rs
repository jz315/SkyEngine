use std::sync::{Arc, Mutex};

use rustc_hash::FxHashMap;
use wgpu::util::DeviceExt;

use crate::render::Texture;

use super::TilemapHandle;

pub(crate) type SharedTilemapFrameCache = Arc<Mutex<TilemapFrameCache>>;

const DEFAULT_TILEMAP_CACHE_MAX_BYTES: usize = 256 * 1024 * 1024;
const DEFAULT_TILEMAP_CACHE_MAX_NEW_CHUNKS_PER_FRAME: usize = 32;

/// Budget controls for the tilemap renderer cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TilemapCacheConfig {
    /// Approximate combined CPU/GPU bytes retained by prepared tile chunks.
    pub max_bytes: usize,
    /// Maximum background/prewarm cache misses to build per frame.
    pub max_new_chunks_per_frame: usize,
}

impl Default for TilemapCacheConfig {
    #[inline]
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_TILEMAP_CACHE_MAX_BYTES,
            max_new_chunks_per_frame: DEFAULT_TILEMAP_CACHE_MAX_NEW_CHUNKS_PER_FRAME,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct TilemapInstance {
    pub(crate) origin: [f32; 4],
    pub(crate) axis_x: [f32; 4],
    pub(crate) axis_y: [f32; 4],
    pub(crate) color: [f32; 4],
    pub(crate) uv_origin: [f32; 4],
    pub(crate) uv_axis_x: [f32; 4],
    pub(crate) uv_axis_y: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TilemapGpuChunkKey {
    pub(crate) map: TilemapHandle,
    pub(crate) layer: u32,
    pub(crate) chunk_x: u32,
    pub(crate) chunk_y: u32,
    pub(crate) texture_key: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TilemapChunkState {
    pub(crate) chunk_version: u64,
    pub(crate) renderer_hash: u64,
    pub(crate) animation_frame_key: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TilemapInstanceSpan {
    pub(crate) first_instance: u32,
    pub(crate) instance_count: u32,
    pub(crate) sort_order: u64,
}

pub(crate) struct TilemapPreparedInstances {
    pub(crate) instances: Vec<TilemapInstance>,
    pub(crate) spans: Vec<TilemapInstanceSpan>,
}

pub(crate) struct TilemapPreparedChunk {
    pub(crate) key: TilemapGpuChunkKey,
    pub(crate) texture: Option<Texture>,
    pub(crate) texture_key: u64,
}

pub(crate) struct TilemapGpuChunk {
    pub(crate) buffer: wgpu::Buffer,
    pub(crate) instance_count: u32,
    pub(crate) spans: Vec<TilemapInstanceSpan>,
    state: TilemapChunkState,
    instances: Vec<TilemapInstance>,
    buffer_capacity_bytes: usize,
    byte_size: usize,
    last_used_frame: u64,
}

pub(crate) struct TilemapFrameCache {
    config: TilemapCacheConfig,
    chunks: Vec<TilemapPreparedChunk>,
    gpu_chunks: FxHashMap<TilemapGpuChunkKey, TilemapGpuChunk>,
    frame_index: u64,
    new_chunks_this_frame: usize,
    retained_bytes: usize,
}

impl Default for TilemapFrameCache {
    #[inline]
    fn default() -> Self {
        Self::with_config(TilemapCacheConfig::default())
    }
}

impl TilemapFrameCache {
    #[inline]
    pub(crate) fn with_config(config: TilemapCacheConfig) -> Self {
        Self {
            config,
            chunks: Vec::new(),
            gpu_chunks: FxHashMap::default(),
            frame_index: 0,
            new_chunks_this_frame: 0,
            retained_bytes: 0,
        }
    }

    #[inline]
    pub(crate) fn begin_frame(&mut self) {
        self.frame_index = self.frame_index.wrapping_add(1).max(1);
        self.chunks.clear();
        self.new_chunks_this_frame = 0;
    }

    #[inline]
    pub(crate) fn can_prepare_background_chunk(&self) -> bool {
        self.new_chunks_this_frame < self.config.max_new_chunks_per_frame
    }

    pub(crate) fn cached_chunk_index(
        &mut self,
        key: TilemapGpuChunkKey,
        state: TilemapChunkState,
        texture: Option<Texture>,
        texture_key: u64,
    ) -> Option<u32> {
        let chunk = self.gpu_chunks.get_mut(&key)?;
        if chunk.state != state {
            return None;
        }

        chunk.last_used_frame = self.frame_index;
        Some(self.push_frame_chunk(key, texture, texture_key))
    }

    pub(crate) fn insert_or_update_chunk(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: TilemapGpuChunkKey,
        state: TilemapChunkState,
        texture: Option<Texture>,
        texture_key: u64,
        prepared: TilemapPreparedInstances,
    ) -> u32 {
        let bytes = bytemuck::cast_slice(&prepared.instances);
        let byte_len = bytes.len();
        let byte_size = retained_chunk_bytes(&prepared.instances, &prepared.spans);
        let frame_index = self.frame_index;
        self.new_chunks_this_frame = self.new_chunks_this_frame.saturating_add(1);

        if let Some(chunk) = self.gpu_chunks.get_mut(&key) {
            self.retained_bytes = self.retained_bytes.saturating_sub(chunk.byte_size);
            if byte_len <= chunk.buffer_capacity_bytes {
                if !bytes.is_empty() {
                    queue.write_buffer(&chunk.buffer, 0, bytes);
                }
            } else {
                chunk.buffer = create_instance_buffer(device, bytes);
                chunk.buffer_capacity_bytes = byte_len.max(1);
            }
            chunk.instance_count = prepared.instances.len() as u32;
            chunk.spans = prepared.spans;
            chunk.state = state;
            chunk.instances = prepared.instances;
            chunk.byte_size = byte_size;
            chunk.last_used_frame = frame_index;
            self.retained_bytes = self.retained_bytes.saturating_add(chunk.byte_size);
        } else {
            let chunk = TilemapGpuChunk {
                buffer: create_instance_buffer(device, bytes),
                instance_count: prepared.instances.len() as u32,
                spans: prepared.spans,
                state,
                instances: prepared.instances,
                buffer_capacity_bytes: byte_len.max(1),
                byte_size,
                last_used_frame: frame_index,
            };
            self.retained_bytes = self.retained_bytes.saturating_add(chunk.byte_size);
            self.gpu_chunks.insert(key, chunk);
        }

        self.evict_to_budget();
        self.push_frame_chunk(key, texture, texture_key)
    }

    #[inline]
    pub(crate) fn chunk(&self, index: u32) -> Option<(&TilemapPreparedChunk, &TilemapGpuChunk)> {
        let prepared = self.chunks.get(index as usize)?;
        let gpu = self.gpu_chunks.get(&prepared.key)?;
        Some((prepared, gpu))
    }

    fn push_frame_chunk(
        &mut self,
        key: TilemapGpuChunkKey,
        texture: Option<Texture>,
        texture_key: u64,
    ) -> u32 {
        let index = self.chunks.len() as u32;
        self.chunks.push(TilemapPreparedChunk {
            key,
            texture,
            texture_key,
        });
        index
    }

    fn evict_to_budget(&mut self) {
        let max_bytes = self.config.max_bytes.max(1);
        while self.retained_bytes > max_bytes {
            let Some(key) = self
                .gpu_chunks
                .iter()
                .filter(|(_, chunk)| chunk.last_used_frame != self.frame_index)
                .min_by_key(|(_, chunk)| chunk.last_used_frame)
                .map(|(key, _)| *key)
            else {
                break;
            };
            if let Some(chunk) = self.gpu_chunks.remove(&key) {
                self.retained_bytes = self.retained_bytes.saturating_sub(chunk.byte_size);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn retained_chunk_count(&self) -> usize {
        self.gpu_chunks.len()
    }

    #[cfg(test)]
    pub(crate) fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }
}

fn create_instance_buffer(device: &wgpu::Device, bytes: &[u8]) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("tilemap_chunk_instances"),
        contents: bytes,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
    })
}

fn retained_chunk_bytes(instances: &[TilemapInstance], spans: &[TilemapInstanceSpan]) -> usize {
    let instance_bytes = std::mem::size_of_val(instances);
    let span_bytes = std::mem::size_of_val(spans);
    // Count both CPU-retained instance data and the matching GPU buffer.
    instance_bytes.saturating_mul(2).saturating_add(span_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("test adapter should be available");

        pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("tilemap_cache_test_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .expect("test device should be available")
    }

    fn key(chunk_x: u32) -> TilemapGpuChunkKey {
        TilemapGpuChunkKey {
            map: TilemapHandle::new(0, 0),
            layer: 0,
            chunk_x,
            chunk_y: 0,
            texture_key: 7,
        }
    }

    fn state(version: u64) -> TilemapChunkState {
        TilemapChunkState {
            chunk_version: version,
            renderer_hash: 11,
            animation_frame_key: 13,
        }
    }

    fn prepared(instance_count: usize) -> TilemapPreparedInstances {
        let instance = TilemapInstance {
            origin: [0.0, 0.0, 0.0, 1.0],
            axis_x: [1.0, 0.0, 0.0, 0.0],
            axis_y: [0.0, 1.0, 0.0, 0.0],
            color: [1.0, 1.0, 1.0, 1.0],
            uv_origin: [0.0, 0.0, 0.0, 0.0],
            uv_axis_x: [1.0, 0.0, 0.0, 0.0],
            uv_axis_y: [0.0, 1.0, 0.0, 0.0],
        };
        TilemapPreparedInstances {
            instances: vec![instance; instance_count],
            spans: vec![TilemapInstanceSpan {
                first_instance: 0,
                instance_count: instance_count as u32,
                sort_order: 0,
            }],
        }
    }

    #[test]
    fn cached_chunk_hits_only_when_state_matches() {
        let (device, queue) = create_test_device();
        let mut cache = TilemapFrameCache::with_config(TilemapCacheConfig::default());
        cache.begin_frame();
        let key = key(0);
        let initial_state = state(1);
        cache.insert_or_update_chunk(&device, &queue, key, initial_state, None, 7, prepared(2));

        cache.begin_frame();
        assert!(cache
            .cached_chunk_index(key, initial_state, None, 7)
            .is_some());
        assert!(cache.cached_chunk_index(key, state(2), None, 7).is_none());
    }

    #[test]
    fn lru_eviction_preserves_current_frame_entries() {
        let (device, queue) = create_test_device();
        let mut cache = TilemapFrameCache::with_config(TilemapCacheConfig {
            max_bytes: std::mem::size_of::<TilemapInstance>() * 3,
            max_new_chunks_per_frame: 32,
        });

        cache.begin_frame();
        cache.insert_or_update_chunk(&device, &queue, key(0), state(1), None, 7, prepared(2));
        let retained_after_first = cache.retained_chunk_count();
        assert_eq!(retained_after_first, 1);

        cache.begin_frame();
        assert!(cache
            .cached_chunk_index(key(0), state(1), None, 7)
            .is_some());
        cache.insert_or_update_chunk(&device, &queue, key(1), state(1), None, 7, prepared(2));

        assert_eq!(cache.retained_chunk_count(), 2);
        assert!(cache.retained_bytes() > cache.config.max_bytes);

        cache.begin_frame();
        cache.insert_or_update_chunk(&device, &queue, key(2), state(1), None, 7, prepared(2));

        assert!(cache.retained_chunk_count() <= 2);
        assert!(cache
            .cached_chunk_index(key(0), state(1), None, 7)
            .is_none());
    }
}
