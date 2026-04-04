//! Resource and pass builders for the render graph.

use std::borrow::Cow;
use std::sync::Arc;

use super::types::*;

/// Builder for creating virtual textures.
pub struct TextureBuilder {
    pub(crate) name: Cow<'static, str>,
    pub(crate) size: TargetSize,
    pub(crate) format: TextureFormat,
    pub(crate) transient: bool,
    pub(crate) imported: Option<ImportedTexture>,
}

impl TextureBuilder {
    pub(crate) fn new() -> Self {
        Self {
            name: Cow::Borrowed("unnamed_texture"),
            size: TargetSize::Surface,
            format: TextureFormat::Rgba8Unorm,
            transient: true,
            imported: None,
        }
    }

    pub fn name(&mut self, name: impl Into<Cow<'static, str>>) -> &mut Self {
        self.name = name.into();
        self
    }

    pub fn size(&mut self, size: TargetSize) -> &mut Self {
        self.size = size;
        self
    }

    pub fn format(&mut self, format: TextureFormat) -> &mut Self {
        self.format = format;
        self
    }

    pub fn persistent(&mut self) -> &mut Self {
        self.transient = false;
        self
    }

    pub fn import(
        &mut self,
        texture: Arc<wgpu::Texture>,
        view: Arc<wgpu::TextureView>,
    ) -> &mut Self {
        let size = texture.size();
        let format = texture.format();
        self.size = TargetSize::Exact(size.width, size.height);
        self.format = format;
        self.imported = Some(ImportedTexture {
            texture,
            view,
            size: [size.width, size.height],
            format,
        });
        self.transient = false;
        self
    }

    pub fn import_external(&mut self, tex: ImportedTexture) -> &mut Self {
        self.size = TargetSize::Exact(tex.size[0], tex.size[1]);
        self.format = tex.format;
        self.imported = Some(tex);
        self.transient = false;
        self
    }
}

/// Builder for creating virtual buffers.
pub struct BufferBuilder {
    pub(crate) name: Cow<'static, str>,
    pub(crate) size_bytes: u64,
    pub(crate) usage: wgpu::BufferUsages,
    pub(crate) transient: bool,
    pub(crate) imported: Option<Arc<wgpu::Buffer>>,
}

impl BufferBuilder {
    pub(crate) fn new() -> Self {
        Self {
            name: Cow::Borrowed("unnamed_buffer"),
            size_bytes: 0,
            usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
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

    pub fn usage(&mut self, usage: wgpu::BufferUsages) -> &mut Self {
        self.usage = usage;
        self
    }

    pub fn persistent(&mut self) -> &mut Self {
        self.transient = false;
        self
    }

    pub fn import(&mut self, buffer: Arc<wgpu::Buffer>) -> &mut Self {
        self.size_bytes = buffer.size();
        self.imported = Some(buffer);
        self.transient = false;
        self
    }
}

// ── Pass setup builder ──────────────────────────────────────────────────────

/// Builder that passes use to declare their resource dependencies.
pub struct PassSetup {
    pub(crate) reads: Vec<ResourceRef>,
    pub(crate) writes: Vec<ResourceRef>,
    pub(crate) color_outputs: Vec<ColorOutput>,
    pub(crate) depth_stencil: Option<DepthStencilOutput>,
    pub(crate) flags: PassFlags,
}

impl PassSetup {
    pub(crate) fn new() -> Self {
        Self {
            reads: Vec::new(),
            writes: Vec::new(),
            color_outputs: Vec::new(),
            depth_stencil: None,
            flags: PassFlags::empty(),
        }
    }

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

    pub fn read(&mut self, handle: TextureHandle) {
        self.push_read(ResourceRef::Texture(handle));
    }

    pub fn read_buffer(&mut self, handle: BufferHandle) {
        self.push_read(ResourceRef::Buffer(handle));
    }

    pub fn write(&mut self, handle: TextureHandle) {
        self.push_write(ResourceRef::Texture(handle));
    }

    pub fn write_buffer(&mut self, handle: BufferHandle) {
        self.push_write(ResourceRef::Buffer(handle));
    }

    pub fn readwrite(&mut self, handle: TextureHandle) {
        self.push_read(ResourceRef::Texture(handle));
        self.push_write(ResourceRef::Texture(handle));
    }

    pub fn readwrite_buffer(&mut self, handle: BufferHandle) {
        self.push_read(ResourceRef::Buffer(handle));
        self.push_write(ResourceRef::Buffer(handle));
    }

    pub fn write_surface(&mut self) {
        self.push_write(ResourceRef::Surface);
    }
    // NOTE: There is no `read_surface()`. The presentation surface is write-only
    // in wgpu — it has no TextureView accessible for sampling. If a pass needs to
    // read the previous frame's output, use an explicit render-to-texture followed
    // by a copy or composite pass.

    /// Declare a color attachment that this pass will fully overwrite.
    ///
    /// Uses `LoadOp::DontCare` by default so pooled transient targets do not
    /// accidentally preserve stale contents. Use `write_color_loaded()` when
    /// blending over existing attachment contents is intentional.
    pub fn write_color(&mut self, slot: u32, handle: TextureHandle) {
        assert!(
            !self.color_outputs.iter().any(|o| o.slot == slot),
            "MRT slot {slot} already declared for this pass"
        );
        self.push_write(ResourceRef::Texture(handle));
        self.color_outputs.push(ColorOutput {
            slot,
            target: ResourceRef::Texture(handle),
            load: LoadOp::DontCare,
        });
    }

    /// Declare a color attachment whose previous contents must be preserved.
    pub fn write_color_loaded(&mut self, slot: u32, handle: TextureHandle) {
        assert!(
            !self.color_outputs.iter().any(|o| o.slot == slot),
            "MRT slot {slot} already declared for this pass"
        );
        self.push_write(ResourceRef::Texture(handle));
        self.color_outputs.push(ColorOutput {
            slot,
            target: ResourceRef::Texture(handle),
            load: LoadOp::Load,
        });
    }

    pub fn write_color_cleared(&mut self, slot: u32, handle: TextureHandle, color: [f32; 4]) {
        assert!(
            !self.color_outputs.iter().any(|o| o.slot == slot),
            "MRT slot {slot} already declared for this pass"
        );
        self.push_write(ResourceRef::Texture(handle));
        self.color_outputs.push(ColorOutput {
            slot,
            target: ResourceRef::Texture(handle),
            load: LoadOp::Clear(color),
        });
    }

    pub fn write_surface_color(&mut self, slot: u32, load: LoadOp) {
        assert!(
            !self.color_outputs.iter().any(|o| o.slot == slot),
            "MRT slot {slot} already declared for this pass"
        );
        self.push_write(ResourceRef::Surface);
        self.color_outputs.push(ColorOutput {
            slot,
            target: ResourceRef::Surface,
            load,
        });
    }

    /// Declare a depth attachment for a pass that starts from a fresh depth
    /// buffer. The depth plane is cleared to `1.0` by default.
    pub fn set_depth_stencil(&mut self, handle: TextureHandle) {
        self.set_depth_stencil_cleared(handle, 1.0);
    }

    /// Declare a depth attachment whose previous contents must be preserved.
    pub fn set_depth_stencil_loaded(&mut self, handle: TextureHandle) {
        self.push_write(ResourceRef::Texture(handle));
        self.depth_stencil = Some(DepthStencilOutput {
            handle,
            clear_depth: None,
            clear_stencil: None,
            depth_store: true,
            stencil_store: false,
        });
    }

    pub fn set_depth_stencil_cleared(&mut self, handle: TextureHandle, depth: f32) {
        self.push_write(ResourceRef::Texture(handle));
        self.depth_stencil = Some(DepthStencilOutput {
            handle,
            clear_depth: Some(depth),
            clear_stencil: None,
            depth_store: true,
            stencil_store: false,
        });
    }

    pub fn with_flags(&mut self, flags: PassFlags) {
        self.flags = flags;
    }
}

/// Builder for copy passes — declares explicit copy operations.
pub struct CopyPassSetup {
    pub(crate) ops: Vec<CopyOp>,
    pub(crate) reads: Vec<ResourceRef>,
    pub(crate) writes: Vec<ResourceRef>,
    pub(crate) flags: PassFlags,
}

impl CopyPassSetup {
    pub(crate) fn new() -> Self {
        Self {
            ops: Vec::new(),
            reads: Vec::new(),
            writes: Vec::new(),
            flags: PassFlags::empty(),
        }
    }

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

    pub fn texture_to_texture(&mut self, src: TextureHandle, dst: TextureHandle) {
        self.push_read(ResourceRef::Texture(src));
        self.push_write(ResourceRef::Texture(dst));
        self.ops.push(CopyOp::TextureToTexture { src, dst });
    }

    pub fn buffer_to_buffer(&mut self, src: BufferHandle, dst: BufferHandle) {
        self.push_read(ResourceRef::Buffer(src));
        self.push_write(ResourceRef::Buffer(dst));
        self.ops.push(CopyOp::BufferToBuffer { src, dst });
    }

    pub fn buffer_to_texture(&mut self, src: BufferHandle, dst: TextureHandle) {
        self.buffer_to_texture_with_layout(src, dst, None, None);
    }

    pub fn buffer_to_texture_with_layout(
        &mut self,
        src: BufferHandle,
        dst: TextureHandle,
        bytes_per_row: Option<u32>,
        rows_per_image: Option<u32>,
    ) {
        self.push_read(ResourceRef::Buffer(src));
        self.push_write(ResourceRef::Texture(dst));
        self.ops.push(CopyOp::BufferToTexture {
            src,
            dst,
            bytes_per_row,
            rows_per_image,
        });
    }

    pub fn upload_to_texture(
        &mut self,
        data: Vec<u8>,
        dst: TextureHandle,
        width: u32,
        height: u32,
        bytes_per_pixel: u32,
    ) {
        self.push_write(ResourceRef::Texture(dst));
        self.ops.push(CopyOp::UploadToTexture {
            data,
            dst,
            width,
            height,
            bytes_per_pixel,
        });
    }

    pub fn with_flags(&mut self, flags: PassFlags) {
        self.flags = flags;
    }
}
