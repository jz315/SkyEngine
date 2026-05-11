//! WickedEngine-inspired screen-space diffuse GI.
//!
//! The provider is split into small layers: settings, pure CPU layout, graph
//! declaration, semantic pass contracts, GPU bindings/pipelines, uniform
//! construction, execution, and provider integration.

mod bindings;
mod constants;
mod contract;
mod debug;
mod executor;
mod graph;
mod layout;
mod pipelines;
mod provider;
mod settings;
mod uniforms;

#[cfg(test)]
mod tests;

pub use executor::SsgiExecutor as SsgiPass;
pub use layout::{SsgiComputeTextureLayout, SsgiMipLevel, SsgiResources};
pub use provider::SsgiProviderFactory;
pub use settings::{global_illumination, SsgiSettings, SSGI_PROVIDER_ID};
