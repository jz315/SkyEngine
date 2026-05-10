use crate::render::gpu::RenderTarget;
use crate::render::graph::{ImportedTexture, TextureHandle};

use super::{ShadowSceneBindingLayout, ShadowViewBinding};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShadowResourceKind {
    SingleDirectionalMap,
    DirectionalCascades,
}

#[derive(Clone, Debug)]
pub struct SceneShadowResources {
    kind: ShadowResourceKind,
    bind_group: Option<wgpu::BindGroup>,
    bind_group_layout: Option<wgpu::BindGroupLayout>,
    enabled: bool,
}

#[derive(Clone, Debug)]
pub struct ShadowDebugResources {
    directional_shadow_atlas: ImportedTexture,
    directional_cascade_count: u32,
    directional_shadow_mul_add: [f32; 4],
}

impl SceneShadowResources {
    #[inline]
    pub fn from_bind_group(
        kind: ShadowResourceKind,
        bind_group_layout: &wgpu::BindGroupLayout,
        bind_group: &wgpu::BindGroup,
    ) -> Self {
        Self {
            kind,
            bind_group: Some(bind_group.clone()),
            bind_group_layout: Some(bind_group_layout.clone()),
            enabled: true,
        }
    }

    #[inline]
    pub(crate) fn from_directional_shadow(
        layout: &ShadowSceneBindingLayout,
        shadow: &ShadowViewBinding,
    ) -> Self {
        Self {
            kind: ShadowResourceKind::DirectionalCascades,
            bind_group: Some(shadow.bind_group().clone()),
            bind_group_layout: Some(layout.bind_group_layout().clone()),
            enabled: shadow.enabled(),
        }
    }

    #[inline]
    pub const fn disabled(kind: ShadowResourceKind) -> Self {
        Self {
            kind,
            bind_group: None,
            bind_group_layout: None,
            enabled: false,
        }
    }

    #[inline]
    pub const fn kind(&self) -> ShadowResourceKind {
        self.kind
    }

    #[inline]
    pub fn bind_group(&self) -> Option<&wgpu::BindGroup> {
        self.bind_group.as_ref()
    }

    #[inline]
    pub fn bind_group_layout(&self) -> Option<&wgpu::BindGroupLayout> {
        self.bind_group_layout.as_ref()
    }

    #[inline]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SceneShadowGraphResources {
    directional_shadow_atlas: TextureHandle,
    directional_transparent_shadow_atlas: TextureHandle,
}

impl SceneShadowGraphResources {
    #[inline]
    pub(crate) fn blackboard_key(binding_index: usize) -> String {
        format!("directional_shadow_graph_resources_{binding_index}")
    }

    #[inline]
    pub(crate) fn from_directional_shadow(
        shadow: &ShadowViewBinding,
        directional_shadow_atlas: TextureHandle,
        directional_transparent_shadow_atlas: TextureHandle,
    ) -> Option<Self> {
        if !shadow.enabled() {
            return None;
        }
        Some(Self {
            directional_shadow_atlas,
            directional_transparent_shadow_atlas,
        })
    }

    #[inline]
    pub(crate) fn directional_shadow_atlas(&self) -> TextureHandle {
        self.directional_shadow_atlas
    }

    #[inline]
    pub(crate) fn directional_transparent_shadow_atlas(&self) -> TextureHandle {
        self.directional_transparent_shadow_atlas
    }
}

impl ShadowDebugResources {
    #[inline]
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn from_directional_map(target: &RenderTarget) -> Self {
        Self::from_directional_atlas(target, 1, [1.0, 1.0, 0.0, 0.0])
    }

    #[inline]
    pub(crate) fn from_directional_shadow(shadow: &ShadowViewBinding) -> Option<Self> {
        if !shadow.enabled() {
            return None;
        }
        Some(Self::from_directional_atlas(
            shadow.target(),
            shadow.cascade_count(),
            shadow.atlas_layout().shadow_atlas_mul_add(),
        ))
    }

    #[inline]
    pub(crate) fn directional_shadow_atlas(&self) -> ImportedTexture {
        self.directional_shadow_atlas.clone()
    }

    #[inline]
    pub(crate) const fn directional_cascade_count(&self) -> u32 {
        self.directional_cascade_count
    }

    #[inline]
    pub(crate) const fn directional_shadow_mul_add(&self) -> [f32; 4] {
        self.directional_shadow_mul_add
    }

    fn from_directional_atlas(
        target: &RenderTarget,
        cascade_count: u32,
        directional_shadow_mul_add: [f32; 4],
    ) -> Self {
        Self {
            directional_shadow_atlas: import_render_target(target),
            directional_cascade_count: cascade_count.max(1),
            directional_shadow_mul_add,
        }
    }
}

fn import_render_target(target: &RenderTarget) -> ImportedTexture {
    ImportedTexture {
        texture: std::sync::Arc::new(target.texture().clone()),
        view: std::sync::Arc::new(target.view().clone()),
        size: [target.width(), target.height()],
        format: target.format(),
        usage: target.usage(),
        sample_count: target.sample_count(),
        mip_level_count: target.mip_level_count(),
        array_layer_count: target.array_layer_count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::execution::TextureFormat;
    use crate::render::gpu::RenderTargetDescriptor;

    fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("No suitable GPU adapter found for shadow-resource tests");

        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("shadow_resource_test_device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            ..Default::default()
        }))
        .expect("Failed to create test GPU device")
    }

    #[test]
    fn directional_shadow_debug_resource_keeps_wicked_atlas_slice_metadata() {
        let (device, queue) = create_test_device();
        let gpu =
            crate::gpu::GpuContext::new_headless(device, queue, TextureFormat::Bgra8Unorm, [8, 8]);
        let target = RenderTarget::from_descriptor(
            &gpu,
            RenderTargetDescriptor::new_depth(32, 8).label("directional_shadow_atlas"),
        );
        let resources =
            ShadowDebugResources::from_directional_atlas(&target, 4, [0.25, 1.0, 0.0, 0.0]);

        let atlas = resources.directional_shadow_atlas();

        assert_eq!(resources.directional_cascade_count(), 4);
        assert_eq!(
            resources.directional_shadow_mul_add(),
            [0.25, 1.0, 0.0, 0.0]
        );
        assert_eq!(atlas.array_layer_count, 1);
        assert_eq!(atlas.size, [32, 8]);
        assert_eq!(atlas.format, target.format());
    }
}
