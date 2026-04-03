//! SkyEngine GPU abstraction layer.
//!
//! # Architecture
//!
//! This module defines a **handle-based, descriptor-driven** GPU abstraction
//! inspired by:
//!
//! - **SakuraEngine CGPU**: `ProcTable` dispatch, descriptor structs, pass
//!   encoder separation, flat C API
//! - **sokol-gfx / bgfx**: opaque handles, procedural API, minimal surface
//! - **wgpu**: safe Rust API, automatic synchronisation
//!
//! The [`Gpu`] trait sits *above* wgpu's abstraction level — it does **not**
//! re-expose synchronisation primitives (fences, barriers, semaphores) because
//! the wgpu backend already handles those automatically. The purpose of this
//! trait is to allow **backend substitution** (e.g. a future `ash`-based
//! Vulkan backend, a mock backend for testing, etc.) without touching any
//! rendering code above.
//!
//! # Layers
//!
//! ```text
//! ┌─────────────────────────────────┐
//! │  render/ (SpriteBatch, Camera)  │  ← uses Gpu trait only
//! ├─────────────────────────────────┤
//! │  gpu/   (Gpu trait, handles)    │  ← this module
//! ├─────────────────────────────────┤
//! │  backend/wgpu  (WgpuBackend)   │  ← implements Gpu
//! └─────────────────────────────────┘
//! ```

#[cfg(feature = "gpu-wgpu")]
pub mod backend;
pub mod desc;
pub mod handle;
pub mod types;

pub use desc::*;
pub use handle::*;
pub use types::*;

/// A scoped render pass encoder.
pub trait RenderPassEncoder {
    fn set_pipeline(&mut self, pip: Pipeline);
    fn set_bind_group(&mut self, slot: u32, bg: BindGroup);
    fn set_vertex_buffer(&mut self, slot: u32, buf: Buffer);
    fn set_index_buffer(&mut self, buf: Buffer, format: IndexFormat);
    fn set_viewport(&mut self, x: f32, y: f32, w: f32, h: f32);
    fn set_scissor(&mut self, x: u32, y: u32, w: u32, h: u32);
    fn draw(&mut self, vertices: std::ops::Range<u32>, instances: std::ops::Range<u32>);
    fn draw_indexed(
        &mut self,
        indices: std::ops::Range<u32>,
        base_vertex: i32,
        instances: std::ops::Range<u32>,
    );

    /// Draw primitives using arguments read from a GPU buffer.
    ///
    /// The buffer must contain a `DrawIndirectArgs` struct at `offset`:
    /// `[vertex_count: u32, instance_count: u32, first_vertex: u32, first_instance: u32]`
    fn draw_indirect(&mut self, buffer: Buffer, offset: u64);

    /// Draw indexed primitives using arguments read from a GPU buffer.
    ///
    /// The buffer must contain a `DrawIndexedIndirectArgs` struct at `offset`:
    /// `[index_count: u32, instance_count: u32, first_index: u32, base_vertex: i32, first_instance: u32]`
    fn draw_indexed_indirect(&mut self, buffer: Buffer, offset: u64);
}

/// A scoped compute pass encoder.
pub trait ComputePassEncoder {
    fn set_pipeline(&mut self, pip: ComputePipeline);
    fn set_bind_group(&mut self, slot: u32, bg: BindGroup);
    fn dispatch(&mut self, x: u32, y: u32, z: u32);
    /// Dispatch using arguments read from a GPU buffer.
    ///
    /// The buffer must contain `[x: u32, y: u32, z: u32]` at `offset`.
    fn dispatch_indirect(&mut self, buffer: Buffer, offset: u64);
}

/// SkyEngine render hardware interface.
///
/// All GPU operations flow through this trait. Backends (wgpu, future ash,
/// etc.) provide concrete implementations. The API is **descriptor-driven**
/// (every `create_*` takes a descriptor struct) and **handle-based** (resources
/// are identified by lightweight, `Copy` handles).
///
/// # Frame lifecycle
///
/// ```text
/// gpu.begin_frame()?;
///     gpu.with_render_pass(&desc, |pass| {
///         pass.set_pipeline(pip);
///         pass.set_bind_group(0, bg);
///         pass.set_vertex_buffer(0, vb);
///         pass.draw(0..3, 0..1);
///     });
/// gpu.end_frame();
/// ```
pub trait Gpu {
    // ── Resource creation ───────────────────────────────────────────────────────

    fn create_buffer(&mut self, desc: &BufferDesc) -> Buffer;
    fn create_image(&mut self, desc: &ImageDesc) -> Image;
    /// Create a sub-view of an image (specific mip level, array layer, etc.).
    fn create_image_view(&mut self, desc: &ImageViewDesc) -> ImageView;
    fn create_sampler(&mut self, desc: &SamplerDesc) -> Sampler;
    fn create_shader(&mut self, desc: &ShaderDesc) -> Shader;
    fn create_bind_group_layout(&mut self, desc: &BindGroupLayoutDesc) -> BindGroupLayout;
    fn create_bind_group(&mut self, desc: &BindGroupDesc) -> BindGroup;
    fn create_render_pipeline(&mut self, desc: &RenderPipelineDesc) -> Pipeline;
    /// Create a compute pipeline for dispatch-based GPU work.
    fn create_compute_pipeline(&mut self, desc: &ComputePipelineDesc) -> ComputePipeline;

    // ── Resource destruction ──────────────────────────────────────────────────────

    fn destroy_buffer(&mut self, buf: Buffer);
    fn destroy_image(&mut self, img: Image);
    fn destroy_image_view(&mut self, view: ImageView);
    fn destroy_sampler(&mut self, s: Sampler);
    fn destroy_shader(&mut self, s: Shader);
    fn destroy_bind_group_layout(&mut self, l: BindGroupLayout);
    fn destroy_bind_group(&mut self, bg: BindGroup);
    fn destroy_pipeline(&mut self, p: Pipeline);
    fn destroy_compute_pipeline(&mut self, p: ComputePipeline);

    // ── Data upload ───────────────────────────────────────────────────────────

    fn write_buffer(&self, buf: Buffer, offset: u64, data: &[u8]);
    fn write_image(&self, img: Image, data: &[u8], layout: &ImageCopyLayout);
    fn copy_buffer_to_buffer(&mut self, desc: &BufferCopyDesc);
    fn copy_buffer_to_image(&mut self, desc: &BufferToImageCopyDesc);
    fn copy_image_to_image(&mut self, desc: &ImageCopyDesc);

    // ── Frame rendering ───────────────────────────────────────────────────────

    /// Begin a new frame. Returns `Err(SurfaceLost)` if the surface is
    /// unavailable (e.g. window minimised) — caller should skip the frame.
    fn begin_frame(&mut self) -> Result<(), GpuError>;

    /// Execute a scoped render pass against the surface or an off-screen image.
    fn with_render_pass<F>(&mut self, desc: &RenderPassDesc, f: F)
    where
        F: FnOnce(&mut dyn RenderPassEncoder);

    /// Execute a scoped compute pass.
    fn with_compute_pass<F>(&mut self, desc: &ComputePassDesc, f: F)
    where
        F: FnOnce(&mut dyn ComputePassEncoder);

    /// Finish the frame and present to the surface.
    fn end_frame(&mut self);

    // ── Surface / info ────────────────────────────────────────────────────────

    /// Resize the presentation surface (call on window resize).
    fn resize_surface(&mut self, width: u32, height: u32);

    /// Current surface dimensions in pixels.
    fn surface_size(&self) -> [u32; 2];

    /// Pixel format of the surface.
    fn surface_format(&self) -> TextureFormat;

    /// Human-readable name of the GPU adapter (e.g. "NVIDIA GeForce RTX 4090").
    fn adapter_name(&self) -> &str;

    /// Name of the graphics API backend (e.g. "Vulkan", "DX12", "Metal").
    fn backend_name(&self) -> &str;
}
