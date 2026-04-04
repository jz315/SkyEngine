//! Transient resource pools for the render graph.

use std::borrow::Cow;

use rustc_hash::FxHashMap;

use crate::gpu::GpuContext;
use crate::render::target::RenderTarget;

use super::types::TextureFormat;

// ── Texture pool ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PoolKey {
    pub format: TextureFormat,
    pub width: u32,
    pub height: u32,
}

pub(crate) struct TransientPool {
    pool: FxHashMap<PoolKey, Vec<RenderTarget>>,
}

impl TransientPool {
    pub fn new() -> Self {
        Self {
            pool: FxHashMap::default(),
        }
    }

    pub fn acquire(
        &mut self,
        ctx: &GpuContext,
        key: PoolKey,
        label: Cow<'static, str>,
    ) -> RenderTarget {
        if let Some(targets) = self.pool.get_mut(&key) {
            if let Some(target) = targets.pop() {
                return target;
            }
        }
        RenderTarget::new(ctx, key.width, key.height, key.format, label)
    }

    pub fn release(&mut self, key: PoolKey, target: RenderTarget) {
        self.pool.entry(key).or_default().push(target);
    }

    pub fn destroy_all(&mut self) {
        self.pool.clear();
    }
}

// ── Buffer pool ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct BufferPoolKey {
    pub size_bytes: u64,
    pub usage: wgpu::BufferUsages,
}

pub(crate) struct TransientBufferPool {
    pool: FxHashMap<BufferPoolKey, Vec<wgpu::Buffer>>,
}

impl TransientBufferPool {
    pub fn new() -> Self {
        Self {
            pool: FxHashMap::default(),
        }
    }

    pub fn acquire(
        &mut self,
        ctx: &GpuContext,
        key: BufferPoolKey,
        label: Cow<'static, str>,
    ) -> wgpu::Buffer {
        if let Some(buffers) = self.pool.get_mut(&key) {
            if let Some(buffer) = buffers.pop() {
                return buffer;
            }
        }
        ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some(&label),
            size: key.size_bytes,
            usage: key.usage,
            mapped_at_creation: false,
        })
    }

    pub fn release(&mut self, key: BufferPoolKey, buffer: wgpu::Buffer) {
        self.pool.entry(key).or_default().push(buffer);
    }

    pub fn destroy_all(&mut self) {
        self.pool.clear();
    }
}
