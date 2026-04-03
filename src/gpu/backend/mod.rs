//! Backend selection and factory.

#[cfg(feature = "gpu-wgpu")]
mod wgpu_backend;
#[cfg(feature = "gpu-wgpu")]
pub use wgpu_backend::WgpuBackend;
