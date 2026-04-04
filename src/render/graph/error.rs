//! Error types and profiler trait for the render graph.

use std::borrow::Cow;

use super::types::*;

// ── Error types ─────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum RenderGraphError {
    CycleDetected,
    InvalidResourceHandle {
        pass: Option<Cow<'static, str>>,
        resource: ResourceRef,
    },
    ReadBeforeWrite {
        pass: Cow<'static, str>,
        resource: ResourceRef,
    },
    InvalidBufferTextureCopyLayout {
        buffer: BufferHandle,
        texture: TextureHandle,
        details: Cow<'static, str>,
    },
    InvalidTextureCopy {
        src: TextureHandle,
        dst: TextureHandle,
        details: Cow<'static, str>,
    },
    InvalidBufferCopy {
        src: BufferHandle,
        dst: BufferHandle,
        details: Cow<'static, str>,
    },
    UnsupportedBufferTextureCopyFormat {
        texture: TextureHandle,
        format: wgpu::TextureFormat,
    },
    SourceBufferTooSmall {
        buffer: BufferHandle,
        required_bytes: u64,
        actual_bytes: u64,
    },
    InvalidTextureUpload {
        texture: TextureHandle,
        details: Cow<'static, str>,
    },
    UnsupportedTextureUploadFormat {
        texture: TextureHandle,
        format: wgpu::TextureFormat,
    },
    MissingPhysicalResource {
        resource: ResourceRef,
    },
    ExecutionFailed(String),
}

impl std::fmt::Display for RenderGraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CycleDetected => write!(f, "Render graph contains a dependency cycle"),
            Self::InvalidResourceHandle { pass, resource } => match pass {
                Some(pass) => {
                    write!(
                        f,
                        "Pass \"{pass}\" references invalid or stale handle {resource:?}"
                    )
                }
                None => write!(f, "Invalid or stale render-graph handle {resource:?}"),
            },
            Self::ReadBeforeWrite { pass, resource } => {
                write!(
                    f,
                    "Pass \"{pass}\" reads {resource:?} before it has a writer or import"
                )
            }
            Self::InvalidBufferTextureCopyLayout {
                buffer,
                texture,
                details,
            } => {
                write!(
                    f,
                    "buffer_to_texture copy from {buffer:?} to {texture:?} has \
                     invalid layout: {details}"
                )
            }
            Self::InvalidTextureCopy { src, dst, details } => {
                write!(
                    f,
                    "texture copy from {src:?} to {dst:?} is invalid: {details}"
                )
            }
            Self::InvalidBufferCopy { src, dst, details } => {
                write!(
                    f,
                    "buffer copy from {src:?} to {dst:?} is invalid: {details}"
                )
            }
            Self::UnsupportedBufferTextureCopyFormat { texture, format } => {
                write!(
                    f,
                    "buffer_to_texture copy into {texture:?} is unsupported for format {format:?}"
                )
            }
            Self::SourceBufferTooSmall {
                buffer,
                required_bytes,
                actual_bytes,
            } => {
                write!(
                    f,
                    "copy source buffer {buffer:?} is too small: needs {required_bytes} bytes, \
                     has {actual_bytes}"
                )
            }
            Self::InvalidTextureUpload { texture, details } => {
                write!(f, "texture upload into {texture:?} is invalid: {details}")
            }
            Self::UnsupportedTextureUploadFormat { texture, format } => {
                write!(
                    f,
                    "texture upload into {texture:?} is unsupported for format {format:?}"
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
