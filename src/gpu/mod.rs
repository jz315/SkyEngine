//! SkyEngine GPU layer — thin wgpu wrapper.
//!
//! This module provides [`GpuContext`], a lightweight wrapper around
//! `wgpu::Device`, `wgpu::Queue`, and `wgpu::Surface` that manages the
//! frame lifecycle (begin/end/present) and surface configuration.
//!
//! All GPU resource creation is done directly through `wgpu` types.
//! Resource cleanup is automatic via Rust's `Drop`.
//!
//! # Layers
//!
//! ```text
//! ┌─────────────────────────────────┐
//! │  render/ (SpriteBatch, Camera)  │  ← uses wgpu types + GpuContext
//! ├─────────────────────────────────┤
//! │  gpu/   (GpuContext)            │  ← this module (frame lifecycle)
//! ├─────────────────────────────────┤
//! │  wgpu                          │  ← actual GPU backend
//! └─────────────────────────────────┘
//! ```

mod context;

pub use context::{
    ColorTargetView, DynamicUniformBuffer, FrameUploadArena, GpuComputePass, GpuContext, GpuError,
    GpuFrame, GpuInitError, GpuRenderPass, UploadSlice,
};
