//! Declarative render-graph orchestration.
//!
//! # Architecture
//!
//! The render graph is a **declarative**, **virtual resource** system inspired
//! by SakuraEngine's RenderGraph.  Passes declare their resource reads/writes
//! through builder closures, and the graph automatically:
//!
//! 1. Resolves dependencies via topological sort
//! 2. Culls unused passes (dead-code elimination)
//! 3. Tracks virtual resource lifetimes
//! 4. Allocates/recycles physical `RenderTarget` objects from an internal pool
//! 5. Executes passes in dependency order
//!
//! # Pass types
//!
//! - **Render pass**: rasterisation draw calls (`add_render_pass`)
//! - **Compute pass**: compute shader dispatches (`add_compute_pass`)
//! - **Copy pass**: resource-to-resource copies (`add_copy_pass`)
//!
//! # Example
//!
//! ```rust,ignore
//! let mut graph = RenderGraph::new();
//!
//! let hdr = graph.create_texture(|b| {
//!     b.name("hdr_color")
//!      .size(TargetSize::Surface)
//!      .format(TextureFormat::Rgba16Float);
//! });
//!
//! graph.add_render_pass("scene", |setup| {
//!     setup.write_color(0, hdr);
//! });
//!
//! graph.add_render_pass("post_fx", |setup| {
//!     setup.read(hdr);
//!     setup.write_surface();
//! });
//!
//! let plan = graph.compile()?;
//!
//! // Walk the compiled plan and execute passes yourself:
//! for pass_info in plan.order() {
//!     // use pass_info.reads / writes / name / pass_type
//! }
//! ```

use std::borrow::Cow;
use std::collections::VecDeque;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::gpu::{self, Gpu, TextureFormat};
use crate::render::blackboard::Blackboard;
use crate::render::target::RenderTarget;

// ── Virtual resource handles ────────────────────────────────────────────────

/// Opaque handle to a virtual texture in the render graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureHandle(usize);

/// Opaque handle to a virtual buffer in the render graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BufferHandle(usize);

/// Opaque handle to a graph pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PassHandle(usize);

/// A resource that a pass can read from or write to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceRef {
    Surface,
    Texture(TextureHandle),
    Buffer(BufferHandle),
}

/// Render-target sizing policy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TargetSize {
    /// Match the presentation surface dimensions.
    Surface,
    /// Scale factor relative to the surface (e.g. 0.5 = half res).
    Scale(f32),
    /// Fixed pixel dimensions.
    Exact(u32, u32),
}

/// Type of a render graph pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassType {
    Render,
    Compute,
    Copy,
}

// ── Pass flags (scheduling hints) ──────────────────────────────────────────

bitflags::bitflags! {
    /// Performance and scheduling hints for a pass.
    ///
    /// Modelled after SakuraEngine's `EPassFlags`.  These are *advisory* —
    /// the graph executor is free to ignore them, but a future multi-queue
    /// or async-compute scheduler can use them.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct PassFlags: u32 {
        /// Pass should run on an async-compute queue when available.
        const PREFER_ASYNC_COMPUTE    = 0x02;
        /// Long-running compute (hint for queue scheduling).
        const COMPUTE_INTENSIVE       = 0x10;
        /// Vertex/geometry-bound (hint for GPU workload balance).
        const VERTEX_BOUND_INTENSIVE  = 0x20;
        /// Pixel/fragment-bound (hint).
        const PIXEL_BOUND_INTENSIVE   = 0x40;
        /// Memory-bandwidth-bound (hint).
        const BANDWIDTH_INTENSIVE     = 0x80;
    }
}

// ── Copy operation descriptors ─────────────────────────────────────────────

/// A single copy operation inside a `CopyPass`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyOp {
    /// Copy the contents of one virtual texture to another.
    TextureToTexture {
        src: TextureHandle,
        dst: TextureHandle,
    },
    /// Copy the contents of one virtual buffer to another.
    BufferToBuffer {
        src: BufferHandle,
        dst: BufferHandle,
    },
    /// Upload from a buffer to a texture (GPU-side copy).
    BufferToTexture {
        src: BufferHandle,
        dst: TextureHandle,
        bytes_per_row: Option<u32>,
        rows_per_image: Option<u32>,
    },
    /// Upload CPU-owned pixel data directly to a virtual texture.
    ///
    /// The owned data is consumed during execution — it does not persist
    /// across frames.  This path uses `Gpu::write_image` (queue write)
    /// instead of `copy_buffer_to_image`, so it does **not** require
    /// 256-byte row pitch alignment.
    UploadToTexture {
        data: Vec<u8>,
        dst: TextureHandle,
        width: u32,
        height: u32,
        bytes_per_pixel: u32,
    },
}

// ── Load operation ─────────────────────────────────────────────────────────

/// How existing contents of an attachment are loaded at pass start.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LoadOp {
    /// Clear to a given colour.
    Clear([f32; 4]),
    /// Preserve previous contents.
    Load,
    /// Contents are undefined (fastest, if you know you'll overwrite all).
    DontCare,
}

// ── MRT colour output slot ─────────────────────────────────────────────────

/// Describes one colour attachment slot in a render pass (MRT).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorOutput {
    /// MRT slot index (0 for the single-output case).
    pub slot: u32,
    /// Which virtual texture (or `None` for the Surface).
    pub target: ResourceRef,
    /// Load operation.
    pub load: LoadOp,
}

// ── Depth/stencil pass declaration ─────────────────────────────────────────

/// Depth/stencil attachment declared at the graph level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DepthStencilOutput {
    pub handle: TextureHandle,
    pub clear_depth: Option<f32>,
    pub clear_stencil: Option<u32>,
    pub depth_store: bool,
    pub stencil_store: bool,
}

// ── Resource descriptors ────────────────────────────────────────────────────

/// Description of a virtual texture resource.
struct TextureDesc {
    name: Cow<'static, str>,
    size: TargetSize,
    format: TextureFormat,
    transient: bool,
    /// If `Some`, this texture is imported from an external GPU resource
    /// and should not be allocated/recycled by the transient pool.
    imported: Option<ImportedTexture>,
}

/// Metadata for a texture imported into the render graph from outside.
///
/// Imported textures bypass pool allocation entirely — the caller is
/// responsible for the underlying GPU resources' lifetime.
#[derive(Debug, Clone, Copy)]
pub struct ImportedTexture {
    /// The GPU image handle.
    pub image: gpu::Image,
    /// Optional sampler.  Required if the texture will be sampled in a
    /// pass that needs a combined image+sampler bind.
    pub sampler: Option<gpu::Sampler>,
    /// Width and height in pixels.
    pub size: [u32; 2],
    /// The pixel format.
    pub format: TextureFormat,
}

/// A reference to a physical texture during graph execution.
///
/// This unifies pool-allocated `RenderTarget`s and externally imported
/// textures behind a single accessor.  For pool-allocated textures,
/// `render_target` provides the full `RenderTarget` reference; for
/// imported textures, only `image`, `sampler`, `size`, and `format`
/// are guaranteed.
#[derive(Debug, Clone, Copy)]
pub struct PhysicalTextureRef<'a> {
    /// The underlying GPU image.
    pub image: gpu::Image,
    /// The sampler, if available.
    pub sampler: Option<gpu::Sampler>,
    /// Width and height.
    pub size: [u32; 2],
    /// Pixel format.
    pub format: TextureFormat,
    /// The full render target (only available for pool-allocated textures).
    pub render_target: Option<&'a RenderTarget>,
}

/// Description of a virtual buffer resource.
#[allow(dead_code)]
struct BufferDesc {
    name: Cow<'static, str>,
    size_bytes: u64,
    usage: gpu::BufferUsage,
    transient: bool,
    /// If `Some`, this buffer is imported from an external GPU resource.
    imported: Option<gpu::Buffer>,
}

/// Builder for creating virtual textures.
pub struct TextureBuilder {
    name: Cow<'static, str>,
    size: TargetSize,
    format: TextureFormat,
    transient: bool,
    imported: Option<ImportedTexture>,
}

impl TextureBuilder {
    fn new() -> Self {
        Self {
            name: Cow::Borrowed("unnamed_texture"),
            size: TargetSize::Surface,
            format: TextureFormat::Rgba8Unorm,
            transient: true,
            imported: None,
        }
    }

    /// Set the debug name.
    pub fn name(&mut self, name: impl Into<Cow<'static, str>>) -> &mut Self {
        self.name = name.into();
        self
    }

    /// Set the sizing policy.
    pub fn size(&mut self, size: TargetSize) -> &mut Self {
        self.size = size;
        self
    }

    /// Set the texture format.
    pub fn format(&mut self, format: TextureFormat) -> &mut Self {
        self.format = format;
        self
    }

    /// Mark as persistent (not eligible for pool recycle).
    pub fn persistent(&mut self) -> &mut Self {
        self.transient = false;
        self
    }

    /// Import an externally created GPU image into the render graph.
    ///
    /// Imported textures bypass pool allocation/recycling.  The caller
    /// is responsible for the image's lifetime.
    ///
    /// This is a convenience wrapper for [`import_external`] that only
    /// stores the image handle.  Prefer `import_external` when you also
    /// need a sampler available during execution.
    pub fn import(&mut self, image: gpu::Image) -> &mut Self {
        self.imported = Some(ImportedTexture {
            image,
            sampler: None,
            size: [0; 2], // unknown at declaration time
            format: self.format,
        });
        self.transient = false;
        self
    }

    /// Import a fully described external texture.
    ///
    /// Unlike [`import`], this carries the sampler, dimensions, and format
    /// alongside the image handle, so [`PhysicalResources::texture_ref`]
    /// can return a complete [`PhysicalTextureRef`].
    pub fn import_external(&mut self, tex: ImportedTexture) -> &mut Self {
        self.format = tex.format;
        self.imported = Some(tex);
        self.transient = false;
        self
    }
}

/// Builder for creating virtual buffers.
pub struct BufferBuilder {
    name: Cow<'static, str>,
    size_bytes: u64,
    usage: gpu::BufferUsage,
    transient: bool,
    imported: Option<gpu::Buffer>,
}

impl BufferBuilder {
    fn new() -> Self {
        Self {
            name: Cow::Borrowed("unnamed_buffer"),
            size_bytes: 0,
            usage: gpu::BufferUsage::COPY_SRC | gpu::BufferUsage::COPY_DST,
            transient: true,
            imported: None,
        }
    }

    pub fn name(&mut self, name: impl Into<Cow<'static, str>>) -> &mut Self {
        self.name = name.into();
        self
    }

    pub fn size(&mut self, size_bytes: u64) -> &mut Self {
        self.size_bytes = size_bytes;
        self
    }

    /// Set explicit GPU usage flags for the buffer.
    pub fn usage(&mut self, usage: gpu::BufferUsage) -> &mut Self {
        self.usage = usage;
        self
    }

    pub fn persistent(&mut self) -> &mut Self {
        self.transient = false;
        self
    }

    /// Import an externally created GPU buffer into the render graph.
    ///
    /// Imported buffers bypass pool allocation.  The caller is responsible
    /// for the buffer's lifetime.
    pub fn import(&mut self, buffer: gpu::Buffer) -> &mut Self {
        self.imported = Some(buffer);
        self.transient = false;
        self
    }
}

// ── Pass setup builder ──────────────────────────────────────────────────────

/// Builder that passes use to declare their resource dependencies.
pub struct PassSetup {
    reads: Vec<ResourceRef>,
    writes: Vec<ResourceRef>,
    /// MRT colour outputs (render passes only).
    color_outputs: Vec<ColorOutput>,
    /// Depth/stencil attachment (render passes only).
    depth_stencil: Option<DepthStencilOutput>,
    /// Scheduling / performance hints.
    flags: PassFlags,
}

impl PassSetup {
    fn new() -> Self {
        Self {
            reads: Vec::new(),
            writes: Vec::new(),
            color_outputs: Vec::new(),
            depth_stencil: None,
            flags: PassFlags::empty(),
        }
    }

    // ── Dedup helpers ───────────────────────────────────────────────────

    fn push_read(&mut self, resource: ResourceRef) {
        if !self.reads.contains(&resource) {
            self.reads.push(resource);
        }
    }

    fn push_write(&mut self, resource: ResourceRef) {
        if !self.writes.contains(&resource) {
            self.writes.push(resource);
        }
    }

    // ── Generic read / write ────────────────────────────────────────────

    /// Declare that this pass reads from a texture.
    pub fn read(&mut self, handle: TextureHandle) {
        self.push_read(ResourceRef::Texture(handle));
    }

    /// Declare that this pass reads from a buffer.
    pub fn read_buffer(&mut self, handle: BufferHandle) {
        self.push_read(ResourceRef::Buffer(handle));
    }

    /// Declare that this pass writes to a texture.
    pub fn write(&mut self, handle: TextureHandle) {
        self.push_write(ResourceRef::Texture(handle));
    }

    /// Declare that this pass writes to a buffer.
    pub fn write_buffer(&mut self, handle: BufferHandle) {
        self.push_write(ResourceRef::Buffer(handle));
    }

    /// Declare that this pass reads and writes a texture.
    pub fn readwrite(&mut self, handle: TextureHandle) {
        self.push_read(ResourceRef::Texture(handle));
        self.push_write(ResourceRef::Texture(handle));
    }

    /// Declare that this pass reads and writes a buffer.
    pub fn readwrite_buffer(&mut self, handle: BufferHandle) {
        self.push_read(ResourceRef::Buffer(handle));
        self.push_write(ResourceRef::Buffer(handle));
    }

    /// Declare that this pass writes to the presentation surface.
    pub fn write_surface(&mut self) {
        self.push_write(ResourceRef::Surface);
    }

    /// Declare that this pass reads the presentation surface (rare).
    pub fn read_surface(&mut self) {
        self.push_read(ResourceRef::Surface);
    }

    // ── MRT colour output (Sakura-style) ────────────────────────────────

    /// Declare a colour output at MRT `slot`, loading previous contents.
    pub fn write_color(&mut self, slot: u32, handle: TextureHandle) {
        self.push_write(ResourceRef::Texture(handle));
        self.color_outputs.push(ColorOutput {
            slot,
            target: ResourceRef::Texture(handle),
            load: LoadOp::Load,
        });
    }

    /// Declare a colour output at MRT `slot`, clearing to `color`.
    pub fn write_color_cleared(&mut self, slot: u32, handle: TextureHandle, color: [f32; 4]) {
        self.push_write(ResourceRef::Texture(handle));
        self.color_outputs.push(ColorOutput {
            slot,
            target: ResourceRef::Texture(handle),
            load: LoadOp::Clear(color),
        });
    }

    /// Declare a colour output at MRT `slot` targeting the surface.
    pub fn write_surface_color(&mut self, slot: u32, load: LoadOp) {
        self.push_write(ResourceRef::Surface);
        self.color_outputs.push(ColorOutput {
            slot,
            target: ResourceRef::Surface,
            load,
        });
    }

    // ── Depth / stencil ─────────────────────────────────────────────────

    /// Declare a depth/stencil attachment that preserves previous contents.
    pub fn set_depth_stencil(&mut self, handle: TextureHandle) {
        self.push_write(ResourceRef::Texture(handle));
        self.depth_stencil = Some(DepthStencilOutput {
            handle,
            clear_depth: None,
            clear_stencil: None,
            depth_store: true,
            stencil_store: false,
        });
    }

    /// Declare a depth/stencil attachment that clears depth to `depth`.
    pub fn set_depth_stencil_cleared(&mut self, handle: TextureHandle, depth: f32) {
        self.writes.push(ResourceRef::Texture(handle));
        self.depth_stencil = Some(DepthStencilOutput {
            handle,
            clear_depth: Some(depth),
            clear_stencil: None,
            depth_store: true,
            stencil_store: false,
        });
    }

    // ── Flags ───────────────────────────────────────────────────────────

    /// Set scheduling / performance hint flags.
    pub fn with_flags(&mut self, flags: PassFlags) {
        self.flags = flags;
    }
}

/// Builder for copy passes — declares explicit copy operations.
pub struct CopyPassSetup {
    ops: Vec<CopyOp>,
    reads: Vec<ResourceRef>,
    writes: Vec<ResourceRef>,
    flags: PassFlags,
}

impl CopyPassSetup {
    fn new() -> Self {
        Self {
            ops: Vec::new(),
            reads: Vec::new(),
            writes: Vec::new(),
            flags: PassFlags::empty(),
        }
    }

    /// Copy one virtual texture to another.
    pub fn texture_to_texture(&mut self, src: TextureHandle, dst: TextureHandle) {
        self.reads.push(ResourceRef::Texture(src));
        self.writes.push(ResourceRef::Texture(dst));
        self.ops.push(CopyOp::TextureToTexture { src, dst });
    }

    /// Copy one virtual buffer to another.
    pub fn buffer_to_buffer(&mut self, src: BufferHandle, dst: BufferHandle) {
        self.reads.push(ResourceRef::Buffer(src));
        self.writes.push(ResourceRef::Buffer(dst));
        self.ops.push(CopyOp::BufferToBuffer { src, dst });
    }

    /// Upload from a buffer to a texture.
    pub fn buffer_to_texture(&mut self, src: BufferHandle, dst: TextureHandle) {
        self.buffer_to_texture_with_layout(src, dst, None, None);
    }

    /// Upload from a buffer to a texture with an explicit row layout.
    pub fn buffer_to_texture_with_layout(
        &mut self,
        src: BufferHandle,
        dst: TextureHandle,
        bytes_per_row: Option<u32>,
        rows_per_image: Option<u32>,
    ) {
        self.reads.push(ResourceRef::Buffer(src));
        self.writes.push(ResourceRef::Texture(dst));
        self.ops.push(CopyOp::BufferToTexture {
            src,
            dst,
            bytes_per_row,
            rows_per_image,
        });
    }

    /// Upload CPU-owned pixel data directly to a virtual texture.
    ///
    /// This bypasses the GPU buffer → texture copy path entirely and uses
    /// `Gpu::write_image` (queue write), which has no row pitch alignment
    /// constraints.  The data is consumed on execution.
    pub fn upload_to_texture(
        &mut self,
        data: Vec<u8>,
        dst: TextureHandle,
        width: u32,
        height: u32,
        bytes_per_pixel: u32,
    ) {
        self.writes.push(ResourceRef::Texture(dst));
        self.ops.push(CopyOp::UploadToTexture {
            data,
            dst,
            width,
            height,
            bytes_per_pixel,
        });
    }

    /// Set scheduling hints.
    pub fn with_flags(&mut self, flags: PassFlags) {
        self.flags = flags;
    }
}

// ── Physical resource accessor ──────────────────────────────────────────────

/// Read-only accessor for physical resources during graph execution.
///
/// Passed to the user's closure inside [`RenderGraph::execute`].  Provides
/// safe, handle-based lookup of the physical `RenderTarget` that backs each
/// virtual texture.
pub struct PhysicalResources<'a> {
    textures: &'a [Option<RenderTarget>],
    buffers: &'a [Option<gpu::Buffer>],
    texture_descs: &'a [TextureDesc],
    buffer_descs: &'a [BufferDesc],
}

impl<'a> PhysicalResources<'a> {
    /// Get the physical `RenderTarget` for a virtual texture handle.
    ///
    /// # Panics
    ///
    /// Panics if the handle is invalid or the resource hasn't been allocated
    /// (e.g. an imported texture that has no physical target in the pool).
    #[inline]
    pub fn get(&self, handle: TextureHandle) -> &RenderTarget {
        self.textures[handle.0]
            .as_ref()
            .expect("virtual texture not allocated")
    }

    /// Try to get the physical `RenderTarget` (returns `None` for imported
    /// textures that bypass pool allocation).
    #[inline]
    pub fn try_get(&self, handle: TextureHandle) -> Option<&RenderTarget> {
        self.textures.get(handle.0).and_then(|opt| opt.as_ref())
    }

    /// Get a [`PhysicalTextureRef`] that works for both pool-allocated and
    /// imported textures.
    ///
    /// This is the preferred accessor when you need to support imported
    /// textures that don't go through the transient pool.
    #[inline]
    pub fn texture_ref(&self, handle: TextureHandle) -> PhysicalTextureRef<'_> {
        // Pool-allocated path
        if let Some(rt) = self.textures.get(handle.0).and_then(|o| o.as_ref()) {
            return PhysicalTextureRef {
                image: rt.image(),
                sampler: Some(rt.sampler()),
                size: [rt.width(), rt.height()],
                format: rt.format(),
                render_target: Some(rt),
            };
        }
        // Imported path
        let desc = &self.texture_descs[handle.0];
        let imp = desc.imported.as_ref().expect(
            "texture_ref: virtual texture has no pool allocation and is not imported",
        );
        PhysicalTextureRef {
            image: imp.image,
            sampler: imp.sampler,
            size: imp.size,
            format: imp.format,
            render_target: None,
        }
    }

    /// Resolve a virtual texture to its physical image.
    #[inline]
    pub fn image(&self, handle: TextureHandle) -> gpu::Image {
        self.try_image(handle)
            .expect("virtual texture image not allocated")
    }

    /// Try to resolve a virtual texture to its physical image.
    #[inline]
    pub fn try_image(&self, handle: TextureHandle) -> Option<gpu::Image> {
        self.textures
            .get(handle.0)
            .and_then(|opt| opt.as_ref().map(RenderTarget::image))
            .or_else(|| {
                self.texture_descs
                    .get(handle.0)
                    .and_then(|desc| desc.imported.as_ref().map(|imp| imp.image))
            })
    }

    /// Resolve a virtual buffer to its physical GPU buffer.
    #[inline]
    pub fn buffer(&self, handle: BufferHandle) -> gpu::Buffer {
        self.try_buffer(handle)
            .expect("virtual buffer not allocated")
    }

    /// Try to resolve a virtual buffer to its physical GPU buffer.
    #[inline]
    pub fn try_buffer(&self, handle: BufferHandle) -> Option<gpu::Buffer> {
        self.buffers.get(handle.0).and_then(|opt| *opt).or_else(|| {
            self.buffer_descs
                .get(handle.0)
                .and_then(|desc| desc.imported)
        })
    }
}

// ── Internal pass storage ───────────────────────────────────────────────────

struct PassEntry {
    name: Cow<'static, str>,
    pass_type: PassType,
    reads: Vec<ResourceRef>,
    writes: Vec<ResourceRef>,
    /// MRT colour outputs.
    color_outputs: Vec<ColorOutput>,
    /// Depth/stencil attachment.
    depth_stencil: Option<DepthStencilOutput>,
    /// Copy operations (only for Copy passes).
    copy_ops: Vec<CopyOp>,
    /// Scheduling hints.
    flags: PassFlags,
    /// Dependency level (longest-path distance from a root node).
    dep_level: u32,
    /// Whether this pass is alive (not culled).
    alive: bool,
}

// ── Compiled pass info ──────────────────────────────────────────────────────

/// Information about a compiled pass, returned to the caller for execution.
#[derive(Debug, Clone)]
pub struct CompiledPass {
    /// Stable typed token for the pass.
    pub handle: PassHandle,
    /// Index of the pass in the original insertion order.
    pub index: usize,
    /// Debug name.
    pub name: Cow<'static, str>,
    /// Type of pass (Render, Compute, Copy).
    pub pass_type: PassType,
    /// Resources this pass reads.
    pub reads: Vec<ResourceRef>,
    /// Resources this pass writes.
    pub writes: Vec<ResourceRef>,
    /// MRT colour outputs.
    pub color_outputs: Vec<ColorOutput>,
    /// Depth/stencil attachment.
    pub depth_stencil: Option<DepthStencilOutput>,
    /// Copy operations (Copy passes only).
    pub copy_ops: Vec<CopyOp>,
    /// Scheduling hints.
    pub flags: PassFlags,
    /// Dependency level.
    pub dep_level: u32,
}

// ── Resource lifetime tracking ──────────────────────────────────────────────

struct ResourceLifetime {
    /// Index of the first pass that uses this resource (for future aliasing).
    #[allow(dead_code)]
    first_use: usize,
    /// Index of the last pass that uses this resource.
    last_use: usize,
}

// ── Transient RT pool ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PoolKey {
    format: TextureFormat,
    width: u32,
    height: u32,
}

/// Pool of reusable `RenderTarget`s for transient resources.
///
/// Targets with matching `(format, width, height)` are recycled instead of
/// re-created.  This implements basic memory aliasing for transient graph
/// resources.
struct TransientPool {
    pool: FxHashMap<PoolKey, Vec<RenderTarget>>,
}

impl TransientPool {
    fn new() -> Self {
        Self {
            pool: FxHashMap::default(),
        }
    }

    fn acquire(
        &mut self,
        gpu: &mut impl Gpu,
        key: PoolKey,
        label: Cow<'static, str>,
    ) -> RenderTarget {
        if let Some(targets) = self.pool.get_mut(&key) {
            if let Some(target) = targets.pop() {
                return target;
            }
        }
        RenderTarget::new(gpu, key.width, key.height, key.format, label)
    }

    fn release(&mut self, key: PoolKey, target: RenderTarget) {
        self.pool.entry(key).or_default().push(target);
    }

    fn destroy_all(&mut self, gpu: &mut impl Gpu) {
        for targets in self.pool.values_mut() {
            for target in targets.drain(..) {
                target.destroy(gpu);
            }
        }
        self.pool.clear();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct BufferPoolKey {
    size_bytes: u64,
    usage: gpu::BufferUsage,
}

struct TransientBufferPool {
    pool: FxHashMap<BufferPoolKey, Vec<gpu::Buffer>>,
}

impl TransientBufferPool {
    fn new() -> Self {
        Self {
            pool: FxHashMap::default(),
        }
    }

    fn acquire(
        &mut self,
        gpu: &mut impl Gpu,
        key: BufferPoolKey,
        label: Cow<'static, str>,
    ) -> gpu::Buffer {
        if let Some(buffers) = self.pool.get_mut(&key) {
            if let Some(buffer) = buffers.pop() {
                return buffer;
            }
        }

        gpu.create_buffer(&gpu::BufferDesc {
            label,
            size: key.size_bytes,
            usage: key.usage,
        })
    }

    fn release(&mut self, key: BufferPoolKey, buffer: gpu::Buffer) {
        self.pool.entry(key).or_default().push(buffer);
    }

    fn destroy_all(&mut self, gpu: &mut impl Gpu) {
        for buffers in self.pool.values_mut() {
            for buffer in buffers.drain(..) {
                gpu.destroy_buffer(buffer);
            }
        }
        self.pool.clear();
    }
}

// ── Error types ─────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum RenderGraphError {
    // ── Compile-time errors ─────────────────────────────────────────
    CycleDetected,
    ReadBeforeWrite {
        pass: Cow<'static, str>,
        resource: ResourceRef,
    },

    // ── Runtime / execution errors ─────────────────────────────────
    /// `copy_buffer_to_image` would require row pitch that is not
    /// 256-byte aligned, and no CPU fallback data is available.
    InvalidBufferTextureCopyLayout {
        buffer: BufferHandle,
        texture: TextureHandle,
        bytes_per_row: u32,
    },
    /// A physical resource expected during execution was not allocated.
    MissingPhysicalResource {
        resource: ResourceRef,
    },
    /// Catch-all for backend-level execution failures.
    ExecutionFailed(String),
}

impl std::fmt::Display for RenderGraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CycleDetected => write!(f, "Render graph contains a dependency cycle"),
            Self::ReadBeforeWrite { pass, resource } => {
                write!(
                    f,
                    "Pass \"{pass}\" reads {resource:?} before it has a writer or import"
                )
            }
            Self::InvalidBufferTextureCopyLayout {
                buffer,
                texture,
                bytes_per_row,
            } => {
                write!(
                    f,
                    "buffer_to_texture copy from {buffer:?} to {texture:?} requires \
                     bytes_per_row={bytes_per_row} which is not 256-byte aligned, \
                     and no CPU fallback data is available"
                )
            }
            Self::MissingPhysicalResource { resource } => {
                write!(
                    f,
                    "Physical resource {resource:?} not allocated during execution"
                )
            }
            Self::ExecutionFailed(msg) => write!(f, "Render graph execution failed: {msg}"),
        }
    }
}

impl std::error::Error for RenderGraphError {}

// ── Profiler ────────────────────────────────────────────────────────────────

/// Optional profiler callbacks for render graph execution.
pub trait RenderGraphProfiler {
    fn on_pass_begin(&mut self, name: &str, pass_type: PassType);
    fn on_pass_end(&mut self, name: &str, elapsed: std::time::Duration);
    fn on_compile(&mut self, pass_count: usize, culled: usize, dep_levels: u32);
}

/// Debug profiler that prints to stderr.
pub struct DebugProfiler;

impl RenderGraphProfiler for DebugProfiler {
    fn on_pass_begin(&mut self, name: &str, pass_type: PassType) {
        eprint!("[RenderGraph] {pass_type:?} pass \"{name}\"...");
    }
    fn on_pass_end(&mut self, name: &str, elapsed: std::time::Duration) {
        let _ = name;
        eprintln!(" {:.2}ms", elapsed.as_secs_f64() * 1000.0);
    }
    fn on_compile(&mut self, pass_count: usize, culled: usize, dep_levels: u32) {
        eprintln!(
            "[RenderGraph] compiled: {pass_count} passes, \
             {culled} culled, {dep_levels} dependency levels"
        );
    }
}

// ── The Render Graph ────────────────────────────────────────────────────────

/// Declarative render graph with virtual resources and automatic scheduling.
///
/// The graph is split into two phases:
///
/// 1. **Declaration** — register virtual resources and passes with their
///    read/write dependencies using builder closures.
/// 2. **Execution** — call [`compile`] then [`execute`] to allocate physical
///    resources and run passes in dependency order.
///
/// Passes don't contain closures; instead, `compile()` returns a
/// `Vec<CompiledPass>` and the caller drives execution.  For the common
/// "self-contained graph" case, use [`execute`] which does everything.
pub struct RenderGraph {
    // Virtual resource descriptors
    textures: Vec<TextureDesc>,
    buffers: Vec<BufferDesc>,

    // Name → handle lookup (Sakura-style)
    texture_names: FxHashMap<String, TextureHandle>,
    buffer_names: FxHashMap<String, BufferHandle>,

    // Passes (declaration only, no closures)
    passes: Vec<PassEntry>,

    // Compilation results (cached to avoid per-frame recomputation)
    order: Vec<usize>,
    cached_compiled: Vec<CompiledPass>,
    compiled: bool,
    max_dep_level: u32,
    culled_count: usize,

    // Resource lifetime tracking
    lifetimes: FxHashMap<ResourceRef, ResourceLifetime>,

    // Physical resource management
    physical_textures: Vec<Option<RenderTarget>>,
    physical_buffers: Vec<Option<gpu::Buffer>>,
    transient_pool: TransientPool,
    transient_buffer_pool: TransientBufferPool,

    // Cross-pass data sharing
    blackboard: Blackboard,
}

impl Default for RenderGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderGraph {
    fn resource_has_external_source(&self, resource: ResourceRef) -> bool {
        match resource {
            ResourceRef::Surface => true,
            ResourceRef::Texture(handle) => self.textures[handle.0].imported.is_some(),
            ResourceRef::Buffer(handle) => self.buffers[handle.0].imported.is_some(),
        }
    }

    pub fn new() -> Self {
        Self {
            textures: Vec::new(),
            buffers: Vec::new(),
            texture_names: FxHashMap::default(),
            buffer_names: FxHashMap::default(),
            passes: Vec::new(),
            order: Vec::new(),
            cached_compiled: Vec::new(),
            compiled: false,
            max_dep_level: 0,
            culled_count: 0,
            lifetimes: FxHashMap::default(),
            physical_textures: Vec::new(),
            physical_buffers: Vec::new(),
            transient_pool: TransientPool::new(),
            transient_buffer_pool: TransientBufferPool::new(),
            blackboard: Blackboard::new(),
        }
    }

    // ── Resource creation ───────────────────────────────────────────────

    /// Create a virtual texture resource using a builder closure.
    pub fn create_texture(&mut self, build: impl FnOnce(&mut TextureBuilder)) -> TextureHandle {
        let mut builder = TextureBuilder::new();
        build(&mut builder);
        let handle = TextureHandle(self.textures.len());
        // Register name → handle mapping.
        self.texture_names.insert(builder.name.to_string(), handle);
        self.textures.push(TextureDesc {
            name: builder.name,
            size: builder.size,
            format: builder.format,
            transient: builder.transient,
            imported: builder.imported,
        });
        self.compiled = false;
        handle
    }

    /// Create a virtual buffer resource using a builder closure.
    pub fn create_buffer(&mut self, build: impl FnOnce(&mut BufferBuilder)) -> BufferHandle {
        let mut builder = BufferBuilder::new();
        build(&mut builder);
        let handle = BufferHandle(self.buffers.len());
        self.buffer_names.insert(builder.name.to_string(), handle);
        self.buffers.push(BufferDesc {
            name: builder.name,
            size_bytes: builder.size_bytes,
            usage: builder.usage,
            transient: builder.transient,
            imported: builder.imported,
        });
        self.compiled = false;
        handle
    }

    /// Look up a texture handle by its name.
    #[must_use]
    pub fn get_texture(&self, name: &str) -> Option<TextureHandle> {
        self.texture_names.get(name).copied()
    }

    /// Look up a buffer handle by its name.
    #[must_use]
    pub fn get_buffer(&self, name: &str) -> Option<BufferHandle> {
        self.buffer_names.get(name).copied()
    }

    /// Access the blackboard for storing shared data.
    pub fn blackboard(&mut self) -> &mut Blackboard {
        &mut self.blackboard
    }

    /// Read-only access to the blackboard.
    pub fn blackboard_ref(&self) -> &Blackboard {
        &self.blackboard
    }

    // ── Pass registration ───────────────────────────────────────────────

    /// Add a render (rasterisation) pass to the graph.
    pub fn add_render_pass(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        setup_fn: impl FnOnce(&mut PassSetup),
    ) -> PassHandle {
        self.add_pass_inner(name.into(), PassType::Render, setup_fn)
    }

    /// Add a compute pass to the graph.
    pub fn add_compute_pass(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        setup_fn: impl FnOnce(&mut PassSetup),
    ) -> PassHandle {
        self.add_pass_inner(name.into(), PassType::Compute, setup_fn)
    }

    /// Add a copy pass with explicit copy operations.
    ///
    /// Unlike render/compute passes, copy passes use [`CopyPassSetup`]
    /// which declares typed operations (tex→tex, buf→buf, buf→tex).
    pub fn add_copy_pass(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        setup_fn: impl FnOnce(&mut CopyPassSetup),
    ) -> PassHandle {
        let mut setup = CopyPassSetup::new();
        setup_fn(&mut setup);
        let handle = PassHandle(self.passes.len());
        self.passes.push(PassEntry {
            name: name.into(),
            pass_type: PassType::Copy,
            reads: setup.reads,
            writes: setup.writes,
            color_outputs: Vec::new(),
            depth_stencil: None,
            copy_ops: setup.ops,
            flags: setup.flags,
            dep_level: 0,
            alive: true,
        });
        self.compiled = false;
        handle
    }

    fn add_pass_inner(
        &mut self,
        name: Cow<'static, str>,
        pass_type: PassType,
        setup_fn: impl FnOnce(&mut PassSetup),
    ) -> PassHandle {
        let mut setup = PassSetup::new();
        setup_fn(&mut setup);
        let handle = PassHandle(self.passes.len());
        self.passes.push(PassEntry {
            name,
            pass_type,
            reads: setup.reads,
            writes: setup.writes,
            color_outputs: setup.color_outputs,
            depth_stencil: setup.depth_stencil,
            copy_ops: Vec::new(),
            flags: setup.flags,
            dep_level: 0,
            alive: true,
        });
        self.compiled = false;
        handle
    }

    // ── Compilation pipeline ────────────────────────────────────────────

    /// Compile the graph: dependency analysis, topological sort, dead-pass
    /// culling, and resource lifetime analysis.
    ///
    /// Returns a list of [`CompiledPass`] in execution order.  Passes that
    /// were culled (no path to a surface write) are excluded.
    ///
    /// This method is **idempotent**: repeated calls return the cached result
    /// without recomputation.  The cache is invalidated when passes or
    /// resources are added.
    #[must_use]
    pub fn compile(&mut self) -> Result<Vec<CompiledPass>, RenderGraphError> {
        // Fast path: return cached result if nothing changed.
        if self.compiled {
            return Ok(self.cached_compiled.clone());
        }

        let n = self.passes.len();
        if n == 0 {
            self.order.clear();
            self.cached_compiled.clear();
            self.compiled = true;
            return Ok(Vec::new());
        }

        // ── Phase 1: PassDependencyAnalysis ─────────────────────────────
        let mut edges = vec![Vec::<usize>::new(); n];
        let mut indegree = vec![0usize; n];
        let mut reverse_edges = vec![Vec::<usize>::new(); n];
        let mut edge_set: FxHashSet<(usize, usize)> = FxHashSet::default();
        let mut last_writer_for: FxHashMap<ResourceRef, usize> = FxHashMap::default();
        let mut readers_since_write: FxHashMap<ResourceRef, Vec<usize>> = FxHashMap::default();

        let mut add_edge = |from: usize, to: usize| {
            if from == to || !edge_set.insert((from, to)) {
                return;
            }
            edges[from].push(to);
            reverse_edges[to].push(from);
            indegree[to] += 1;
        };

        for (idx, pass) in self.passes.iter().enumerate() {
            for &resource in &pass.reads {
                if let Some(&writer) = last_writer_for.get(&resource) {
                    add_edge(writer, idx);
                } else if !self.resource_has_external_source(resource)
                    && !pass.writes.contains(&resource)
                {
                    return Err(RenderGraphError::ReadBeforeWrite {
                        pass: pass.name.clone(),
                        resource,
                    });
                }

                let readers = readers_since_write.entry(resource).or_default();
                if readers.last().copied() != Some(idx) {
                    readers.push(idx);
                }
            }

            for &resource in &pass.writes {
                if let Some(&writer) = last_writer_for.get(&resource) {
                    add_edge(writer, idx);
                }

                if let Some(readers) = readers_since_write.get_mut(&resource) {
                    for &reader in readers.iter() {
                        add_edge(reader, idx);
                    }
                    readers.clear();
                }

                last_writer_for.insert(resource, idx);
            }
        }

        // Kahn's algorithm with index-sorted ready queue.
        // The sorted insert is O(N) per element, which is fine for typical
        // render graph sizes (<50 passes).
        let mut ready = VecDeque::new();
        for (idx, &deg) in indegree.iter().enumerate() {
            if deg == 0 {
                ready.push_back(idx);
            }
        }

        let mut order = Vec::with_capacity(n);
        let mut dep_levels = vec![0u32; n];

        while let Some(node) = ready.pop_front() {
            order.push(node);
            for &next in &edges[node] {
                dep_levels[next] = dep_levels[next].max(dep_levels[node] + 1);
                indegree[next] -= 1;
                if indegree[next] == 0 {
                    let insert_pos = ready.iter().position(|&pending| pending > next);
                    if let Some(pos) = insert_pos {
                        ready.insert(pos, next);
                    } else {
                        ready.push_back(next);
                    }
                }
            }
        }

        if order.len() != n {
            return Err(RenderGraphError::CycleDetected);
        }

        let mut max_dep_level = 0u32;
        for (idx, level) in dep_levels.iter().enumerate() {
            self.passes[idx].dep_level = *level;
            max_dep_level = max_dep_level.max(*level);
        }

        // ── Phase 2: CullPhase ──────────────────────────────────────────
        for pass in &mut self.passes {
            pass.alive = false;
        }

        let mut alive_set = vec![false; n];
        for (idx, pass) in self.passes.iter().enumerate() {
            if pass.writes.contains(&ResourceRef::Surface) {
                alive_set[idx] = true;
            }
        }

        let mut changed = true;
        while changed {
            changed = false;
            for idx in (0..n).rev() {
                if !alive_set[idx] {
                    continue;
                }
                for &dependency in &reverse_edges[idx] {
                    if !alive_set[dependency] {
                        alive_set[dependency] = true;
                        changed = true;
                    }
                }
            }
        }

        let mut culled = 0usize;
        for (idx, &alive) in alive_set.iter().enumerate() {
            self.passes[idx].alive = alive;
            if !alive {
                culled += 1;
            }
        }

        let order: Vec<usize> = order.into_iter().filter(|&i| alive_set[i]).collect();

        // ── Phase 3: ResourceLifetimeAnalysis ───────────────────────────
        let mut lifetimes: FxHashMap<ResourceRef, ResourceLifetime> = FxHashMap::default();
        for (exec_order, &pass_idx) in order.iter().enumerate() {
            let pass = &self.passes[pass_idx];
            for resource in pass.reads.iter().chain(pass.writes.iter()) {
                lifetimes
                    .entry(*resource)
                    .and_modify(|lt| {
                        lt.last_use = exec_order;
                    })
                    .or_insert(ResourceLifetime {
                        first_use: exec_order,
                        last_use: exec_order,
                    });
            }
        }

        // Build the CompiledPass list
        let compiled_passes: Vec<CompiledPass> = order
            .iter()
            .map(|&idx| {
                let pass = &self.passes[idx];
                CompiledPass {
                    handle: PassHandle(idx),
                    index: idx,
                    name: pass.name.clone(),
                    pass_type: pass.pass_type,
                    reads: pass.reads.clone(),
                    writes: pass.writes.clone(),
                    color_outputs: pass.color_outputs.clone(),
                    depth_stencil: pass.depth_stencil,
                    copy_ops: pass.copy_ops.clone(),
                    flags: pass.flags,
                    dep_level: pass.dep_level,
                }
            })
            .collect();

        self.order = order;
        self.max_dep_level = max_dep_level;
        self.culled_count = culled;
        self.lifetimes = lifetimes;
        self.cached_compiled = compiled_passes.clone();
        self.compiled = true;
        Ok(compiled_passes)
    }

    // ── Physical resource management ────────────────────────────────────

    fn buffer_usage_for(&self, handle: BufferHandle) -> gpu::BufferUsage {
        let mut usage = self.buffers[handle.0].usage;
        let resource = ResourceRef::Buffer(handle);

        for pass in &self.passes {
            if pass.reads.contains(&resource) || pass.writes.contains(&resource) {
                usage = usage | gpu::BufferUsage::STORAGE;
            }

            for op in &pass.copy_ops {
                match op {
                    CopyOp::BufferToBuffer { src, dst } => {
                        if *src == handle {
                            usage = usage | gpu::BufferUsage::COPY_SRC;
                        }
                        if *dst == handle {
                            usage = usage | gpu::BufferUsage::COPY_DST;
                        }
                    }
                    CopyOp::BufferToTexture { src, .. } => {
                        if *src == handle {
                            usage = usage | gpu::BufferUsage::COPY_SRC;
                        }
                    }
                    CopyOp::TextureToTexture { .. } | CopyOp::UploadToTexture { .. } => {}
                }
            }
        }

        usage
    }

    fn resolve_texture_image(&self, handle: TextureHandle) -> gpu::Image {
        self.physical_textures
            .get(handle.0)
            .and_then(|opt| opt.as_ref().map(RenderTarget::image))
            .or_else(|| self.textures.get(handle.0).and_then(|desc| desc.imported.as_ref().map(|imp| imp.image)))
            .expect("virtual texture image not allocated")
    }

    fn resolve_texture_extent(&self, handle: TextureHandle, surface_size: [u32; 2]) -> [u32; 2] {
        resolve_target_size(surface_size, self.textures[handle.0].size)
    }

    fn resolve_buffer(&self, handle: BufferHandle) -> gpu::Buffer {
        self.physical_buffers
            .get(handle.0)
            .and_then(|opt| *opt)
            .or_else(|| self.buffers.get(handle.0).and_then(|desc| desc.imported))
            .expect("virtual buffer not allocated")
    }

    /// Allocate/resize physical GPU resources for all virtual textures and buffers.
    ///
    /// Call this after `compile()` and before running passes.
    pub fn allocate_physical_resources(&mut self, gpu: &mut impl Gpu) {
        let surface_size = gpu.surface_size();
        self.physical_textures
            .resize_with(self.textures.len(), || None);
        self.physical_buffers
            .resize_with(self.buffers.len(), || None);

        for (tex_idx, desc) in self.textures.iter().enumerate() {
            // Imported textures are managed externally — skip allocation.
            if desc.imported.is_some() {
                continue;
            }

            let [w, h] = resolve_target_size(surface_size, desc.size);
            let key = PoolKey {
                format: desc.format,
                width: w,
                height: h,
            };

            if desc.transient {
                // Idempotent: only acquire from pool if not already allocated
                // this frame.  Repeated calls within the same frame must not
                // replace the handle (which would discard data written by the
                // caller between allocate and execute).
                if self.physical_textures[tex_idx].is_none() {
                    let target = self.transient_pool.acquire(gpu, key, desc.name.clone());
                    self.physical_textures[tex_idx] = Some(target);
                }
            } else {
                match self.physical_textures[tex_idx].as_mut() {
                    Some(existing) => existing.resize(gpu, w, h),
                    None => {
                        self.physical_textures[tex_idx] =
                            Some(RenderTarget::new(gpu, w, h, desc.format, desc.name.clone()));
                    }
                }
            }
        }

        for (buf_idx, desc) in self.buffers.iter().enumerate() {
            if desc.imported.is_some() {
                continue;
            }

            let handle = BufferHandle(buf_idx);
            let key = BufferPoolKey {
                size_bytes: desc.size_bytes,
                usage: self.buffer_usage_for(handle),
            };

            if desc.transient {
                // Idempotent: same rationale as transient textures above.
                if self.physical_buffers[buf_idx].is_none() {
                    let buffer = self
                        .transient_buffer_pool
                        .acquire(gpu, key, desc.name.clone());
                    self.physical_buffers[buf_idx] = Some(buffer);
                }
            } else if self.physical_buffers[buf_idx].is_none() {
                self.physical_buffers[buf_idx] = Some(gpu.create_buffer(&gpu::BufferDesc {
                    label: desc.name.clone(),
                    size: desc.size_bytes,
                    usage: key.usage,
                }));
            }
        }
    }

    /// Return transient resources to the pool after frame execution.
    pub fn release_transient_resources(&mut self, gpu: &impl Gpu) {
        let surface_size = gpu.surface_size();
        for (tex_idx, desc) in self.textures.iter().enumerate() {
            if desc.transient {
                if let Some(target) = self.physical_textures[tex_idx].take() {
                    let [w, h] = resolve_target_size(surface_size, desc.size);
                    let key = PoolKey {
                        format: desc.format,
                        width: w,
                        height: h,
                    };
                    self.transient_pool.release(key, target);
                }
            }
        }

        for (buf_idx, desc) in self.buffers.iter().enumerate() {
            if desc.transient {
                if let Some(buffer) = self.physical_buffers[buf_idx].take() {
                    let key = BufferPoolKey {
                        size_bytes: desc.size_bytes,
                        usage: self.buffer_usage_for(BufferHandle(buf_idx)),
                    };
                    self.transient_buffer_pool.release(key, buffer);
                }
            }
        }
    }

    /// Get the physical `RenderTarget` for a virtual texture handle.
    ///
    /// # Panics
    ///
    /// Panics if the handle is invalid or resources haven't been allocated.
    pub fn physical_texture(&self, handle: TextureHandle) -> &RenderTarget {
        self.physical_textures[handle.0]
            .as_ref()
            .expect("virtual texture not allocated — call allocate_physical_resources first")
    }

    /// Get the physical GPU buffer for a virtual buffer handle.
    ///
    /// # Panics
    ///
    /// Panics if the handle is invalid or resources haven't been allocated.
    pub fn physical_buffer(&self, handle: BufferHandle) -> gpu::Buffer {
        self.resolve_buffer(handle)
    }

    // ── Query ───────────────────────────────────────────────────────────

    /// Number of passes (including culled).
    pub fn pass_count(&self) -> usize {
        self.passes.len()
    }

    /// Number of alive passes after compilation.
    pub fn alive_pass_count(&self) -> usize {
        self.order.len()
    }

    /// Number of culled passes.
    pub fn culled_count(&self) -> usize {
        self.culled_count
    }

    /// Maximum dependency level (graph depth).
    pub fn max_dep_level(&self) -> u32 {
        self.max_dep_level
    }

    /// Destroy all currently owned physical resources.
    ///
    /// This releases:
    /// - persistent physical render targets
    /// - persistent physical buffers
    /// - transient render targets retained in the pool
    /// - transient buffers retained in the pool
    ///
    /// Imported textures are not owned by the graph and are therefore left
    /// untouched.
    pub fn destroy_physical_resources(&mut self, gpu: &mut impl Gpu) {
        for target in &mut self.physical_textures {
            if let Some(target) = target.take() {
                target.destroy(gpu);
            }
        }
        for buffer in &mut self.physical_buffers {
            if let Some(buffer) = buffer.take() {
                gpu.destroy_buffer(buffer);
            }
        }
        self.transient_pool.destroy_all(gpu);
        self.transient_buffer_pool.destroy_all(gpu);
    }

    /// Clear all passes and resources, keeping no declaration state.
    ///
    /// This resets graph metadata and clears the blackboard. If the graph owns
    /// persistent or pooled GPU resources, call [`destroy_physical_resources`]
    /// first so the backend can reclaim them.
    ///
    /// # Panics (debug builds)
    ///
    /// Debug-asserts that all physical resources have been destroyed. Call
    /// [`destroy_physical_resources`] before `reset()` to avoid leaks.
    pub fn reset(&mut self) {
        debug_assert!(
            self.physical_textures.iter().all(|t| t.is_none()),
            "RenderGraph::reset() called with live physical textures — \
             call destroy_physical_resources() first"
        );
        debug_assert!(
            self.physical_buffers.iter().all(|b| b.is_none()),
            "RenderGraph::reset() called with live physical buffers — \
             call destroy_physical_resources() first"
        );
        self.textures.clear();
        self.buffers.clear();
        self.texture_names.clear();
        self.buffer_names.clear();
        self.passes.clear();
        self.order.clear();
        self.cached_compiled.clear();
        self.lifetimes.clear();
        self.physical_textures.clear();
        self.physical_buffers.clear();
        self.transient_pool = TransientPool::new();
        self.transient_buffer_pool = TransientBufferPool::new();
        self.blackboard.clear();
        self.compiled = false;
    }

    // ── Execution ───────────────────────────────────────────────────────

    fn execute_copy_pass<G: Gpu>(
        &self,
        gpu: &mut G,
        pass: &CompiledPass,
    ) -> Result<(), RenderGraphError> {
        let surface_size = gpu.surface_size();

        for op in &pass.copy_ops {
            match op {
                CopyOp::TextureToTexture { src, dst } => {
                    let src_extent = self.resolve_texture_extent(*src, surface_size);
                    let dst_extent = self.resolve_texture_extent(*dst, surface_size);
                    gpu.copy_image_to_image(&gpu::ImageCopyDesc {
                        src: self.resolve_texture_image(*src),
                        dst: self.resolve_texture_image(*dst),
                        width: src_extent[0].min(dst_extent[0]),
                        height: src_extent[1].min(dst_extent[1]),
                    });
                }
                CopyOp::BufferToBuffer { src, dst } => {
                    let src_desc = &self.buffers[src.0];
                    let dst_desc = &self.buffers[dst.0];
                    gpu.copy_buffer_to_buffer(&gpu::BufferCopyDesc {
                        src: self.resolve_buffer(*src),
                        src_offset: 0,
                        dst: self.resolve_buffer(*dst),
                        dst_offset: 0,
                        size: src_desc.size_bytes.min(dst_desc.size_bytes),
                    });
                }
                CopyOp::BufferToTexture {
                    src,
                    dst,
                    bytes_per_row,
                    rows_per_image,
                } => {
                    let [width, height] = self.resolve_texture_extent(*dst, surface_size);
                    let bytes_per_pixel =
                        texture_format_bytes_per_pixel(self.textures[dst.0].format)
                            .expect("buffer_to_texture requires a copyable color format");
                    let row_bytes =
                        bytes_per_row.unwrap_or(width * bytes_per_pixel);

                    // wgpu requires bytes_per_row to be a multiple of 256 for
                    // GPU-side buffer→texture copies.  If alignment is wrong,
                    // fall back to `write_image` (queue write, no alignment
                    // constraint) using the buffer's CPU shadow data when
                    // available, otherwise report an error.
                    if row_bytes % 256 != 0 {
                        // Fallback: use write_image which has no alignment
                        // requirement.  This requires CPU data, which we don't
                        // have for pure GPU buffers — report the error.
                        return Err(RenderGraphError::InvalidBufferTextureCopyLayout {
                            buffer: *src,
                            texture: *dst,
                            bytes_per_row: row_bytes,
                        });
                    }

                    gpu.copy_buffer_to_image(&gpu::BufferToImageCopyDesc {
                        src: self.resolve_buffer(*src),
                        src_offset: 0,
                        bytes_per_row: row_bytes,
                        rows_per_image: rows_per_image.unwrap_or(height),
                        dst: self.resolve_texture_image(*dst),
                        width,
                        height,
                    });
                }
                CopyOp::UploadToTexture {
                    data,
                    dst,
                    width,
                    height,
                    bytes_per_pixel,
                } => {
                    // CPU→GPU queue write — no row pitch alignment required.
                    let dst_image = self.resolve_texture_image(*dst);
                    gpu.write_image(
                        dst_image,
                        data,
                        &gpu::ImageCopyLayout {
                            offset: 0,
                            bytes_per_row: width * bytes_per_pixel,
                            rows_per_image: *height,
                        },
                    );
                }
            }
        }

        Ok(())
    }

    /// Compile, allocate resources, and execute all alive passes in order.
    ///
    /// Returns `Err` if compilation or execution encounters a recoverable
    /// error.  For the panicking version, use [`execute`].
    ///
    /// # Lifecycle
    ///
    /// 1. Compile (dependency analysis, DCE, topological sort) — cached
    /// 2. Allocate physical resources for virtual textures and buffers
    /// 3. Iterate passes in dependency order, executing copy passes internally
    ///    and calling `run_pass` for render/compute passes
    /// 4. Release transient resources back to the pool
    pub fn try_execute<G, F>(
        &mut self,
        gpu: &mut G,
        mut run_pass: F,
    ) -> Result<(), RenderGraphError>
    where
        G: Gpu,
        F: FnMut(&CompiledPass, &mut G, &PhysicalResources<'_>),
    {
        // Phase 1: compile (idempotent — returns cached on repeated calls)
        if !self.compiled {
            self.compile()?;
        }

        // Phase 2: allocate physical resources
        self.allocate_physical_resources(gpu);

        // Phase 3: execute passes in order
        // Take compiled passes out of self to avoid holding
        // &[CompiledPass] + &[Option<RenderTarget>] while &mut self exists.
        let compiled = std::mem::take(&mut self.cached_compiled);
        let result = {
            let resources = PhysicalResources {
                textures: &self.physical_textures,
                buffers: &self.physical_buffers,
                texture_descs: &self.textures,
                buffer_descs: &self.buffers,
            };
            let mut err = None;
            for pass in &compiled {
                if pass.pass_type == PassType::Copy {
                    if let Err(e) = self.execute_copy_pass(gpu, pass) {
                        err = Some(e);
                        break;
                    }
                } else {
                    run_pass(pass, gpu, &resources);
                }
            }
            err
        };
        self.cached_compiled = compiled;

        // Phase 4: release transient resources
        self.release_transient_resources(gpu);

        match result {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Compile, allocate resources, and execute all alive passes in order.
    ///
    /// This is the **primary entry point** for driving the render graph each
    /// frame.  It is a convenience wrapper around [`try_execute`] that panics
    /// on error.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let scene_pass = graph.add_render_pass("scene", |setup| {
    ///     setup.write(scene_handle);
    /// });
    /// let post_pass = graph.add_render_pass("post", |setup| {
    ///     setup.read(scene_handle);
    ///     setup.write_surface();
    /// });
    ///
    /// graph.execute(gpu, |pass, gpu, resources| {
    ///     if pass.handle == scene_pass {
    ///         let target = resources.get(scene_handle);
    ///         batch.draw_to_target(gpu, &camera, target, Some(bg));
    ///     } else if pass.handle == post_pass {
    ///         let input = resources.get(scene_handle);
    ///         tonemap.apply_to_surface(gpu, input);
    ///     }
    /// });
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if compilation or copy pass execution fails.
    pub fn execute<G, F>(&mut self, gpu: &mut G, run_pass: F)
    where
        G: Gpu,
        F: FnMut(&CompiledPass, &mut G, &PhysicalResources<'_>),
    {
        self.try_execute(gpu, run_pass)
            .expect("RenderGraph::execute failed");
    }

    /// Like [`try_execute`], but with a [`RenderGraphProfiler`] for timing.
    pub fn try_execute_profiled<G, P, F>(
        &mut self,
        gpu: &mut G,
        profiler: &mut P,
        mut run_pass: F,
    ) -> Result<(), RenderGraphError>
    where
        G: Gpu,
        P: RenderGraphProfiler,
        F: FnMut(&CompiledPass, &mut G, &PhysicalResources<'_>),
    {
        if !self.compiled {
            self.compile()?;
        }

        profiler.on_compile(self.passes.len(), self.culled_count, self.max_dep_level);

        self.allocate_physical_resources(gpu);

        let compiled = std::mem::take(&mut self.cached_compiled);
        let result = {
            let resources = PhysicalResources {
                textures: &self.physical_textures,
                buffers: &self.physical_buffers,
                texture_descs: &self.textures,
                buffer_descs: &self.buffers,
            };
            let mut err = None;
            for pass in &compiled {
                profiler.on_pass_begin(&pass.name, pass.pass_type);
                let start = std::time::Instant::now();
                if pass.pass_type == PassType::Copy {
                    if let Err(e) = self.execute_copy_pass(gpu, pass) {
                        profiler.on_pass_end(&pass.name, start.elapsed());
                        err = Some(e);
                        break;
                    }
                } else {
                    run_pass(pass, gpu, &resources);
                }
                profiler.on_pass_end(&pass.name, start.elapsed());
            }
            err
        };
        self.cached_compiled = compiled;

        self.release_transient_resources(gpu);

        match result {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Like [`execute`], but with a [`RenderGraphProfiler`] for timing.
    ///
    /// # Panics
    ///
    /// Panics if compilation or copy pass execution fails.
    pub fn execute_profiled<G, P, F>(&mut self, gpu: &mut G, profiler: &mut P, run_pass: F)
    where
        G: Gpu,
        P: RenderGraphProfiler,
        F: FnMut(&CompiledPass, &mut G, &PhysicalResources<'_>),
    {
        self.try_execute_profiled(gpu, profiler, run_pass)
            .expect("RenderGraph::execute_profiled failed");
    }

    // ── Visualization ───────────────────────────────────────────────────

    /// Export the render graph as a GraphViz DOT string.
    ///
    /// Pass nodes are colored by type: Render (steelblue), Compute (seagreen),
    /// Copy (goldenrod). Culled passes are shown in gray with dashed borders.
    /// Resource nodes are shown as boxes.
    ///
    /// ```text
    /// let dot = graph.export_dot();
    /// std::fs::write("render_graph.dot", &dot).unwrap();
    /// // Then: dot -Tpng render_graph.dot -o render_graph.png
    /// ```
    pub fn export_dot(&self) -> String {
        use std::fmt::Write;

        let mut dot = String::with_capacity(2048);
        writeln!(dot, "digraph RenderGraph {{").unwrap();
        writeln!(dot, "    rankdir=LR;").unwrap();
        writeln!(
            dot,
            "    graph [fontname=\"Helvetica\", bgcolor=\"#1a1a2e\"];"
        )
        .unwrap();
        writeln!(
            dot,
            "    node [fontname=\"Helvetica\", fontcolor=\"white\"];"
        )
        .unwrap();
        writeln!(dot, "    edge [color=\"#aaaacc\"];").unwrap();
        writeln!(dot).unwrap();

        // Resource nodes
        writeln!(dot, "    // Resources").unwrap();
        for (i, tex) in self.textures.iter().enumerate() {
            writeln!(
                dot,
                "    res_tex_{i} [label=\"{name}\\n{size:?} {format:?}\", \
                 shape=box, style=filled, fillcolor=\"#2d2d44\", color=\"#6c6c8a\"];",
                name = tex.name,
                size = tex.size,
                format = tex.format,
            )
            .unwrap();
        }
        for (i, buf) in self.buffers.iter().enumerate() {
            writeln!(
                dot,
                "    res_buf_{i} [label=\"{name}\\n{size} bytes\", \
                 shape=box, style=filled, fillcolor=\"#243447\", color=\"#4f81bd\"];",
                name = buf.name,
                size = buf.size_bytes,
            )
            .unwrap();
        }
        writeln!(
            dot,
            "    res_surface [label=\"Surface\", shape=box, style=filled, \
             fillcolor=\"#44223d\", color=\"#8a4477\"];"
        )
        .unwrap();
        writeln!(dot).unwrap();

        // Pass nodes
        writeln!(dot, "    // Passes").unwrap();
        for (i, pass) in self.passes.iter().enumerate() {
            let (fill, border) = if !pass.alive {
                ("\"#3a3a3a\"", "\"#666666\"")
            } else {
                match pass.pass_type {
                    PassType::Render => ("\"#1b4f72\"", "\"#5dade2\""),
                    PassType::Compute => ("\"#1e6a4b\"", "\"#52be80\""),
                    PassType::Copy => ("\"#7d6608\"", "\"#f4d03f\""),
                }
            };
            let style = if pass.alive {
                "filled"
            } else {
                "filled,dashed"
            };
            writeln!(
                dot,
                "    pass_{i} [label=\"{name}\\n({ty:?})\", \
                 shape=ellipse, style=\"{style}\", fillcolor={fill}, color={border}];",
                name = pass.name,
                ty = pass.pass_type,
            )
            .unwrap();
        }
        writeln!(dot).unwrap();

        // Edges: reads (resource → pass) and writes (pass → resource)
        writeln!(dot, "    // Edges").unwrap();
        for (i, pass) in self.passes.iter().enumerate() {
            for r in &pass.reads {
                let res_id = match r {
                    ResourceRef::Texture(h) => format!("res_tex_{}", h.0),
                    ResourceRef::Buffer(h) => format!("res_buf_{}", h.0),
                    ResourceRef::Surface => "res_surface".to_string(),
                };
                writeln!(
                    dot,
                    "    {res_id} -> pass_{i} [style=solid, color=\"#7fb3d8\"];",
                )
                .unwrap();
            }
            for w in &pass.writes {
                let res_id = match w {
                    ResourceRef::Texture(h) => format!("res_tex_{}", h.0),
                    ResourceRef::Buffer(h) => format!("res_buf_{}", h.0),
                    ResourceRef::Surface => "res_surface".to_string(),
                };
                writeln!(
                    dot,
                    "    pass_{i} -> {res_id} [style=bold, color=\"#e8a87c\"];",
                )
                .unwrap();
            }
        }

        writeln!(dot, "}}").unwrap();
        dot
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn resolve_target_size(surface: [u32; 2], size: TargetSize) -> [u32; 2] {
    match size {
        TargetSize::Surface => [surface[0].max(1), surface[1].max(1)],
        TargetSize::Scale(scale) => [
            (surface[0] as f32 * scale).round().max(1.0) as u32,
            (surface[1] as f32 * scale).round().max(1.0) as u32,
        ],
        TargetSize::Exact(width, height) => [width.max(1), height.max(1)],
    }
}

fn texture_format_bytes_per_pixel(format: TextureFormat) -> Option<u32> {
    match format {
        TextureFormat::Rgba8Unorm
        | TextureFormat::Rgba8UnormSrgb
        | TextureFormat::Bgra8Unorm
        | TextureFormat::Bgra8UnormSrgb
        | TextureFormat::R32Float => Some(4),
        TextureFormat::Rg32Float | TextureFormat::Rgba16Float => Some(8),
        TextureFormat::Rgba32Float => Some(16),
        TextureFormat::Depth32Float | TextureFormat::Depth24PlusStencil8 => None,
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_texture_returns_sequential_handles() {
        let mut graph = RenderGraph::new();
        let a = graph.create_texture(|b| {
            b.name("a").format(TextureFormat::Rgba16Float);
        });
        let b = graph.create_texture(|b| {
            b.name("b").format(TextureFormat::Rgba8Unorm);
        });
        assert_eq!(a, TextureHandle(0));
        assert_eq!(b, TextureHandle(1));
    }

    #[test]
    fn empty_graph_compiles() {
        let mut graph = RenderGraph::new();
        let passes = graph.compile().unwrap();
        assert!(passes.is_empty());
    }

    #[test]
    fn linear_chain_orders_correctly() {
        let mut graph = RenderGraph::new();

        let hdr = graph.create_texture(|b| {
            b.name("hdr").format(TextureFormat::Rgba16Float);
        });

        let scene = graph.add_render_pass("scene", |setup| {
            setup.write(hdr);
        });

        let post = graph.add_render_pass("post", |setup| {
            setup.read(hdr);
            setup.write_surface();
        });

        let passes = graph.compile().unwrap();
        assert_eq!(passes.len(), 2);
        assert_eq!(passes[0].handle, scene);
        assert_eq!(passes[1].handle, post);
        assert_eq!(passes[0].name, "scene");
        assert_eq!(passes[1].name, "post");
    }

    #[test]
    fn read_before_write_requires_imported_or_surface_input() {
        let mut graph = RenderGraph::new();
        let a = graph.create_texture(|b| {
            b.name("a");
        });
        let b_tex = graph.create_texture(|b| {
            b.name("b");
        });

        graph.add_render_pass("pass_a", |setup| {
            setup.write(a);
            setup.read(b_tex);
        });

        graph.add_render_pass("pass_b", |setup| {
            setup.write(b_tex);
            setup.read(a);
        });

        assert!(matches!(
            graph.compile(),
            Err(RenderGraphError::ReadBeforeWrite { .. })
        ));
    }

    #[test]
    fn dead_pass_is_culled() {
        let mut graph = RenderGraph::new();

        let hdr = graph.create_texture(|b| {
            b.name("hdr");
        });
        let unused = graph.create_texture(|b| {
            b.name("unused");
        });

        graph.add_render_pass("scene", |setup| {
            setup.write(hdr);
        });

        graph.add_render_pass("present", |setup| {
            setup.read(hdr);
            setup.write_surface();
        });

        // This pass writes to an unused texture — should be culled
        graph.add_render_pass("dead_pass", |setup| {
            setup.write(unused);
        });

        let passes = graph.compile().unwrap();

        assert_eq!(passes.len(), 2);
        assert_eq!(graph.culled_count(), 1);
        assert!(!graph.passes[2].alive);
    }

    #[test]
    fn dependency_levels_computed() {
        let mut graph = RenderGraph::new();

        let a = graph.create_texture(|b| {
            b.name("a");
        });
        let b_tex = graph.create_texture(|b| {
            b.name("b");
        });

        graph.add_render_pass("root", |s| s.write(a));
        graph.add_render_pass("mid", |s| {
            s.read(a);
            s.write(b_tex);
        });
        graph.add_render_pass("leaf", |s| {
            s.read(b_tex);
            s.write_surface();
        });

        let passes = graph.compile().unwrap();

        assert_eq!(passes[0].dep_level, 0);
        assert_eq!(passes[1].dep_level, 1);
        assert_eq!(passes[2].dep_level, 2);
        assert_eq!(graph.max_dep_level(), 2);
    }

    #[test]
    fn resource_lifetimes_tracked() {
        let mut graph = RenderGraph::new();

        let hdr = graph.create_texture(|b| {
            b.name("hdr");
        });

        graph.add_render_pass("write_hdr", |s| s.write(hdr));
        graph.add_render_pass("read_hdr", |s| {
            s.read(hdr);
            s.write_surface();
        });

        graph.compile().unwrap();

        let lt = graph.lifetimes.get(&ResourceRef::Texture(hdr)).unwrap();
        assert_eq!(lt.first_use, 0);
        assert_eq!(lt.last_use, 1);
    }

    #[test]
    fn blackboard_integration() {
        let mut graph = RenderGraph::new();
        graph.blackboard().set("test_value", 42u32);
        assert_eq!(graph.blackboard().get::<u32>("test_value"), Some(&42));
    }

    #[test]
    fn reset_clears_blackboard() {
        let mut graph = RenderGraph::new();
        graph.blackboard().set("test_value", 42u32);
        graph.reset();
        assert!(graph.blackboard_ref().get::<u32>("test_value").is_none());
    }

    #[test]
    fn compute_pass_type() {
        let mut graph = RenderGraph::new();
        let buf = graph.create_texture(|b| {
            b.name("particles");
        });

        graph.add_compute_pass("update", |s| {
            s.readwrite(buf);
        });
        graph.add_render_pass("draw", |s| {
            s.read(buf);
            s.write_surface();
        });

        let passes = graph.compile().unwrap();
        assert_eq!(passes[0].pass_type, PassType::Compute);
        assert_eq!(passes[1].pass_type, PassType::Render);
    }

    #[test]
    fn diamond_dependency() {
        // Test graph: A writes to 2 textures, B and C read one each,
        // D reads both B and C outputs
        let mut graph = RenderGraph::new();

        let t1 = graph.create_texture(|b| {
            b.name("t1");
        });
        let t2 = graph.create_texture(|b| {
            b.name("t2");
        });
        let t3 = graph.create_texture(|b| {
            b.name("t3");
        });
        let t4 = graph.create_texture(|b| {
            b.name("t4");
        });

        graph.add_render_pass("A", |s| {
            s.write(t1);
            s.write(t2);
        });
        graph.add_render_pass("B", |s| {
            s.read(t1);
            s.write(t3);
        });
        graph.add_render_pass("C", |s| {
            s.read(t2);
            s.write(t4);
        });
        graph.add_render_pass("D", |s| {
            s.read(t3);
            s.read(t4);
            s.write_surface();
        });

        let passes = graph.compile().unwrap();
        assert_eq!(passes.len(), 4);

        // A must come before B and C, and B and C must come before D
        let pos = |name: &str| passes.iter().position(|p| p.name == name).unwrap();
        assert!(pos("A") < pos("B"));
        assert!(pos("A") < pos("C"));
        assert!(pos("B") < pos("D"));
        assert!(pos("C") < pos("D"));
    }

    #[test]
    fn resolve_target_sizes() {
        let surface = [1920, 1080];
        assert_eq!(resolve_target_size(surface, TargetSize::Surface), surface);
        assert_eq!(
            resolve_target_size(surface, TargetSize::Scale(0.5)),
            [960, 540]
        );
        assert_eq!(
            resolve_target_size(surface, TargetSize::Exact(256, 256)),
            [256, 256]
        );
    }

    // ── New feature tests ───────────────────────────────────────────────

    #[test]
    fn copy_pass_establishes_dependency() {
        let mut graph = RenderGraph::new();

        let src = graph.create_texture(|b| {
            b.name("src");
        });
        let dst = graph.create_texture(|b| {
            b.name("dst");
        });

        graph.add_render_pass("produce", |s| {
            s.write(src);
        });

        graph.add_copy_pass("copy", |s| {
            s.texture_to_texture(src, dst);
        });

        graph.add_render_pass("consume", |s| {
            s.read(dst);
            s.write_surface();
        });

        let passes = graph.compile().unwrap();
        assert_eq!(passes.len(), 3);

        let pos = |name: &str| passes.iter().position(|p| p.name == name).unwrap();
        assert!(pos("produce") < pos("copy"));
        assert!(pos("copy") < pos("consume"));

        // Verify the CompiledPass carries the CopyOp
        assert_eq!(passes[pos("copy")].copy_ops.len(), 1);
        assert!(matches!(
            passes[pos("copy")].copy_ops[0],
            CopyOp::TextureToTexture { .. }
        ));
    }

    #[test]
    fn copy_pass_multi_ops() {
        let mut graph = RenderGraph::new();

        let t1 = graph.create_texture(|b| {
            b.name("t1");
        });
        let t2 = graph.create_texture(|b| {
            b.name("t2");
        });

        graph.add_render_pass("gen", |s| {
            s.write(t1);
        });

        graph.add_copy_pass("multi_copy", |s| {
            s.texture_to_texture(t1, t2);
        });

        graph.add_render_pass("use_it", |s| {
            s.read(t2);
            s.write_surface();
        });

        let passes = graph.compile().unwrap();
        assert_eq!(passes.len(), 3);
        assert_eq!(passes[1].pass_type, PassType::Copy);
        assert_eq!(passes[1].copy_ops.len(), 1);
    }

    #[test]
    fn name_lookup_returns_correct_handles() {
        let mut graph = RenderGraph::new();
        let hdr = graph.create_texture(|b| {
            b.name("hdr").format(TextureFormat::Rgba16Float);
        });
        let _shadow = graph.create_texture(|b| {
            b.name("shadow_map");
        });
        let buf = graph.create_buffer(|b| {
            b.name("staging").size(1024);
        });

        assert_eq!(graph.get_texture("hdr"), Some(hdr));
        assert_eq!(graph.get_texture("shadow_map"), Some(TextureHandle(1)));
        assert_eq!(graph.get_texture("nonexistent"), None);
        assert_eq!(graph.get_buffer("staging"), Some(buf));
        assert_eq!(graph.get_buffer("missing"), None);
    }

    #[test]
    fn buffer_builder_preserves_usage_flags() {
        let mut graph = RenderGraph::new();
        let buffer = graph.create_buffer(|b| {
            b.name("indirect")
                .size(256)
                .usage(gpu::BufferUsage::INDIRECT | gpu::BufferUsage::STORAGE);
        });

        let desc = &graph.buffers[buffer.0];
        assert_eq!(
            desc.usage,
            gpu::BufferUsage::INDIRECT | gpu::BufferUsage::STORAGE
        );
    }

    #[test]
    fn export_dot_includes_buffer_nodes() {
        let mut graph = RenderGraph::new();
        graph.create_buffer(|b| {
            b.name("staging").size(1024);
        });

        let dot = graph.export_dot();
        assert!(dot.contains("res_buf_0"));
        assert!(dot.contains("staging\\n1024 bytes"));
    }

    #[test]
    fn mrt_color_outputs_preserved() {
        let mut graph = RenderGraph::new();

        let albedo = graph.create_texture(|b| {
            b.name("albedo");
        });
        let normal = graph.create_texture(|b| {
            b.name("normal");
        });

        graph.add_render_pass("gbuffer", |s| {
            s.write_color_cleared(0, albedo, [0.0, 0.0, 0.0, 1.0]);
            s.write_color_cleared(1, normal, [0.5, 0.5, 1.0, 1.0]);
        });

        graph.add_render_pass("lighting", |s| {
            s.read(albedo);
            s.read(normal);
            s.write_surface();
        });

        let passes = graph.compile().unwrap();
        let gbuffer = &passes[0];
        assert_eq!(gbuffer.color_outputs.len(), 2);
        assert_eq!(gbuffer.color_outputs[0].slot, 0);
        assert_eq!(gbuffer.color_outputs[1].slot, 1);
        assert!(matches!(gbuffer.color_outputs[0].load, LoadOp::Clear(_)));
    }

    #[test]
    fn depth_stencil_creates_dependency() {
        let mut graph = RenderGraph::new();

        let depth = graph.create_texture(|b| {
            b.name("depth").format(TextureFormat::Depth24PlusStencil8);
        });
        let color = graph.create_texture(|b| {
            b.name("color");
        });

        graph.add_render_pass("geometry", |s| {
            s.write_color_cleared(0, color, [0.0; 4]);
            s.set_depth_stencil_cleared(depth, 1.0);
        });

        graph.add_render_pass("post", |s| {
            s.read(color);
            s.read(depth);
            s.write_surface();
        });

        let passes = graph.compile().unwrap();
        assert_eq!(passes.len(), 2);
        assert!(passes[0].depth_stencil.is_some());
        let ds = passes[0].depth_stencil.unwrap();
        assert_eq!(ds.handle, depth);
        assert_eq!(ds.clear_depth, Some(1.0));
        assert!(ds.depth_store);
    }

    #[test]
    fn pass_flags_in_compiled_pass() {
        let mut graph = RenderGraph::new();
        let buf = graph.create_texture(|b| {
            b.name("particles");
        });

        graph.add_compute_pass("sim", |s| {
            s.readwrite(buf);
            s.with_flags(PassFlags::PREFER_ASYNC_COMPUTE | PassFlags::COMPUTE_INTENSIVE);
        });

        graph.add_render_pass("draw", |s| {
            s.read(buf);
            s.write_surface();
        });

        let passes = graph.compile().unwrap();
        assert!(passes[0].flags.contains(PassFlags::PREFER_ASYNC_COMPUTE));
        assert!(passes[0].flags.contains(PassFlags::COMPUTE_INTENSIVE));
        assert!(passes[1].flags.is_empty());
    }

    #[test]
    fn earlier_reader_depends_on_earlier_writer_not_later_overwrite() {
        let mut graph = RenderGraph::new();
        let x = graph.create_texture(|b| {
            b.name("x");
        });
        let y = graph.create_texture(|b| {
            b.name("y");
        });

        graph.add_render_pass("produce_initial", |s| {
            s.write(x);
        });
        graph.add_render_pass("consume_initial", |s| {
            s.read(x);
            s.write(y);
        });
        graph.add_render_pass("overwrite_x", |s| {
            s.write(x);
        });
        graph.add_render_pass("present_xy", |s| {
            s.read(y);
            s.read(x);
            s.write_surface();
        });

        let passes = graph.compile().unwrap();
        let pos = |name: &str| passes.iter().position(|p| p.name == name).unwrap();

        assert!(pos("produce_initial") < pos("consume_initial"));
        assert!(pos("consume_initial") < pos("overwrite_x"));
        assert!(pos("overwrite_x") < pos("present_xy"));
    }

    #[test]
    fn cull_uses_dependency_edges_instead_of_last_writer_lookup() {
        let mut graph = RenderGraph::new();
        let x = graph.create_texture(|b| {
            b.name("x");
        });
        let y = graph.create_texture(|b| {
            b.name("y");
        });

        graph.add_render_pass("produce_initial", |s| {
            s.write(x);
        });
        graph.add_render_pass("consume_initial", |s| {
            s.read(x);
            s.write(y);
        });
        graph.add_render_pass("overwrite_x", |s| {
            s.write(x);
        });
        graph.add_render_pass("present_y", |s| {
            s.read(y);
            s.write_surface();
        });

        let passes = graph.compile().unwrap();
        assert_eq!(passes.len(), 3);
        assert!(passes.iter().any(|p| p.name == "produce_initial"));
        assert!(passes.iter().any(|p| p.name == "consume_initial"));
        assert!(passes.iter().any(|p| p.name == "present_y"));
        assert!(passes.iter().all(|p| p.name != "overwrite_x"));
    }

    #[test]
    fn imported_texture_skips_pool() {
        // We can't test actual GPU allocation without a backend, but we can
        // verify the desc is correctly marked.
        let mut graph = RenderGraph::new();
        let _ext = graph.create_texture(|b| {
            b.name("external").import(gpu::Image::from_raw(42));
        });
        // Verify the desc recorded imported properly.
        assert!(graph.textures[0].imported.is_some());
        assert!(!graph.textures[0].transient);
    }

    #[test]
    fn imported_texture_can_be_read_without_writer() {
        let mut graph = RenderGraph::new();
        let ext = graph.create_texture(|b| {
            b.name("external").import(gpu::Image::from_raw(7));
        });

        graph.add_render_pass("sample_external", |s| {
            s.read(ext);
            s.write_surface();
        });

        let passes = graph.compile().unwrap();
        assert_eq!(passes.len(), 1);
        assert_eq!(passes[0].name, "sample_external");
    }

    #[test]
    fn name_lookup_after_reset() {
        let mut graph = RenderGraph::new();
        let _h = graph.create_texture(|b| {
            b.name("tmp");
        });
        assert!(graph.get_texture("tmp").is_some());
        graph.reset();
        assert!(graph.get_texture("tmp").is_none());
    }
}
