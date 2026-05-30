#![allow(unused_imports)]
use super::backdrop::*;
use super::collect::*;
use super::images::*;
use super::primitives::*;
use super::text::*;
use super::*;

pub(super) struct WgpuVertexBuffer {
    buffer: wgpu::Buffer,
    capacity: u64,
    upload_cache: CacheCell<UploadCacheKey, UploadState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct UploadCacheKey {
    used: u64,
    content_hash: u64,
}

#[derive(Debug, Clone, Copy, Default)]
struct UploadState {
    used: u64,
}

impl WgpuVertexBuffer {
    pub(super) fn upload<T: bytemuck::Pod>(
        slot: &mut Option<Self>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        label: &str,
        data: &[T],
    ) {
        if data.is_empty() {
            if let Some(buffer) = slot {
                buffer.upload_cache.value_mut().used = 0;
                buffer.upload_cache.invalidate();
            }
            return;
        }

        let used = std::mem::size_of_val(data) as u64;
        let bytes = bytemuck::cast_slice(data);
        let content_hash = hash_bytes(bytes);
        let needs_buffer = slot.as_ref().is_none_or(|buffer| buffer.capacity < used);
        if needs_buffer {
            let capacity = used.next_power_of_two().max(256);
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: capacity,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            *slot = Some(Self {
                buffer,
                capacity,
                upload_cache: CacheCell::default(),
            });
        }

        let Some(buffer) = slot.as_mut() else {
            return;
        };
        buffer
            .upload_cache
            .get_or_rebuild(UploadCacheKey { used, content_hash }, |state| {
                queue.write_buffer(&buffer.buffer, 0, bytes);
                state.used = used;
            });
    }

    #[inline]
    pub(super) fn slice(&self) -> wgpu::BufferSlice<'_> {
        self.buffer.slice(0..self.upload_cache.value().used)
    }

    #[inline]
    pub(super) fn ready(&self) -> Option<&Self> {
        (self.upload_cache.value().used > 0).then_some(self)
    }
}

pub(super) fn create_linear_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("eui_neo_linear_sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    })
}

pub(super) fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut hasher = FxHasher::default();
    bytes.hash(&mut hasher);
    hasher.finish()
}
