use crate::render::execution::{PreparedFrame, PreparedView};
use crate::render::gi::GiSamplingBinding;
use crate::render::gpu::{GpuScene, RenderTarget, Texture};
use crate::render::lighting::shadow::SceneShadowResources;
use crate::render::resources::material::MaterialRegistry;
use crate::render::resources::mesh::MeshRegistry;

pub struct DrawContext<'ctx, 'pass, 'tex> {
    pub(super) device: &'ctx wgpu::Device,
    pub(super) sampler_nearest: &'ctx wgpu::Sampler,
    pub(super) pass: &'ctx mut wgpu::RenderPass<'pass>,
    pub(super) view_bind_group: &'ctx wgpu::BindGroup,
    pub(super) view_bind_group_layout: &'ctx wgpu::BindGroupLayout,
    #[allow(dead_code)]
    pub(super) model_bind_group_layout: &'ctx wgpu::BindGroupLayout,
    pub(super) cpu_model_matrices: Option<&'ctx [[f32; 16]]>,
    pub(super) gpu_scene: Option<&'ctx GpuScene>,
    pub(super) scene_shadows: Option<SceneShadowResources>,
    pub(super) gi_sampling: Option<GiSamplingBinding>,
    pub(super) gi_shader_source: Option<&'ctx str>,
    pub(super) gi_shader_key: u64,
    pub(super) material_registry: &'ctx mut MaterialRegistry,
    pub(super) mesh_registry: &'ctx MeshRegistry,
    pub(super) fallback_texture: Option<&'tex Texture>,
    pub(super) target_format: wgpu::TextureFormat,
    pub(super) depth_format: Option<wgpu::TextureFormat>,
}

impl<'ctx, 'pass, 'tex> DrawContext<'ctx, 'pass, 'tex> {
    #[inline]
    #[allow(
        clippy::too_many_arguments,
        reason = "constructor mirrors the complete per-pass resource contract"
    )]
    pub(crate) fn new(
        device: &'ctx wgpu::Device,
        sampler_nearest: &'ctx wgpu::Sampler,
        pass: &'ctx mut wgpu::RenderPass<'pass>,
        view_bind_group: &'ctx wgpu::BindGroup,
        view_bind_group_layout: &'ctx wgpu::BindGroupLayout,
        model_bind_group_layout: &'ctx wgpu::BindGroupLayout,
        cpu_model_matrices: Option<&'ctx [[f32; 16]]>,
        gpu_scene: Option<&'ctx GpuScene>,
        scene_shadows: Option<SceneShadowResources>,
        gi_sampling: Option<GiSamplingBinding>,
        gi_shader_source: Option<&'ctx str>,
        gi_shader_key: u64,
        material_registry: &'ctx mut MaterialRegistry,
        mesh_registry: &'ctx MeshRegistry,
        fallback_texture: Option<&'tex Texture>,
        target_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
    ) -> Self {
        Self {
            device,
            sampler_nearest,
            pass,
            view_bind_group,
            view_bind_group_layout,
            model_bind_group_layout,
            cpu_model_matrices,
            gpu_scene,
            scene_shadows,
            gi_sampling,
            gi_shader_source,
            gi_shader_key,
            material_registry,
            mesh_registry,
            fallback_texture,
            target_format,
            depth_format,
        }
    }

    #[inline]
    pub fn gpu_scene(&self) -> Option<&GpuScene> {
        self.gpu_scene
    }

    #[inline]
    pub fn cpu_model_matrix(&self, slot: u32) -> Option<&[f32; 16]> {
        self.cpu_model_matrices
            .and_then(|matrices| matrices.get(slot as usize))
    }

    #[inline]
    pub(crate) fn device(&self) -> &wgpu::Device {
        self.device
    }

    #[inline]
    pub(crate) fn sampler_nearest(&self) -> &wgpu::Sampler {
        self.sampler_nearest
    }

    #[inline]
    pub(crate) fn pass(&mut self) -> &mut wgpu::RenderPass<'pass> {
        self.pass
    }

    #[inline]
    pub(crate) fn view_bind_group(&self) -> &wgpu::BindGroup {
        self.view_bind_group
    }

    #[inline]
    pub(crate) fn view_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        self.view_bind_group_layout
    }

    #[inline]
    pub(crate) fn model_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        self.model_bind_group_layout
    }

    #[inline]
    pub(crate) fn mesh_registry(&self) -> &MeshRegistry {
        self.mesh_registry
    }

    #[inline]
    pub(crate) fn fallback_texture(&self) -> Option<&Texture> {
        self.fallback_texture
    }

    #[inline]
    pub(crate) fn target_format(&self) -> wgpu::TextureFormat {
        self.target_format
    }

    #[inline]
    pub(crate) fn depth_format(&self) -> Option<wgpu::TextureFormat> {
        self.depth_format
    }
}

pub struct StandaloneDrawContext<'ctx, 'frame, 'tex> {
    gpu: &'ctx mut crate::gpu::GpuContext,
    target: &'ctx RenderTarget,
    frame: &'frame PreparedFrame<'frame>,
    view: &'frame PreparedView<'frame>,
    device: &'ctx wgpu::Device,
    sampler_linear: &'ctx wgpu::Sampler,
    sampler_nearest: &'ctx wgpu::Sampler,
    view_bind_group: &'ctx wgpu::BindGroup,
    view_bind_group_layout: &'ctx wgpu::BindGroupLayout,
    model_bind_group_layout: &'ctx wgpu::BindGroupLayout,
    material_registry: &'ctx mut MaterialRegistry,
    fallback_texture: Option<&'tex Texture>,
    target_format: wgpu::TextureFormat,
    depth_format: Option<wgpu::TextureFormat>,
}

impl<'ctx, 'frame, 'tex> StandaloneDrawContext<'ctx, 'frame, 'tex> {
    #[inline]
    #[allow(
        clippy::too_many_arguments,
        reason = "constructor mirrors the standalone draw resource contract"
    )]
    pub fn new(
        gpu: &'ctx mut crate::gpu::GpuContext,
        target: &'ctx RenderTarget,
        frame: &'frame PreparedFrame<'frame>,
        view: &'frame PreparedView<'frame>,
        device: &'ctx wgpu::Device,
        sampler_linear: &'ctx wgpu::Sampler,
        sampler_nearest: &'ctx wgpu::Sampler,
        view_bind_group: &'ctx wgpu::BindGroup,
        view_bind_group_layout: &'ctx wgpu::BindGroupLayout,
        model_bind_group_layout: &'ctx wgpu::BindGroupLayout,
        material_registry: &'ctx mut MaterialRegistry,
        fallback_texture: Option<&'tex Texture>,
        target_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
    ) -> Self {
        Self {
            gpu,
            target,
            frame,
            view,
            device,
            sampler_linear,
            sampler_nearest,
            view_bind_group,
            view_bind_group_layout,
            model_bind_group_layout,
            material_registry,
            fallback_texture,
            target_format,
            depth_format,
        }
    }

    #[inline]
    pub fn gpu(&mut self) -> &mut crate::gpu::GpuContext {
        self.gpu
    }

    pub fn gpu_and_target(&mut self) -> (&mut crate::gpu::GpuContext, &RenderTarget) {
        let gpu = self.gpu as *mut crate::gpu::GpuContext;
        let target = self.target as *const RenderTarget;
        // These references point at disjoint fields stored inside the context.
        unsafe { (&mut *gpu, &*target) }
    }

    #[inline]
    pub fn target(&self) -> &RenderTarget {
        self.target
    }

    #[inline]
    pub fn frame_payload<T: std::any::Any>(&self) -> Option<&'frame T> {
        self.frame.payload::<T>()
    }

    #[inline]
    pub fn view_payload<T: std::any::Any>(&self) -> Option<&'frame T> {
        self.view.payload::<T>()
    }

    #[inline]
    pub fn material_registry(&mut self) -> &mut MaterialRegistry {
        self.material_registry
    }

    #[inline]
    pub fn device(&self) -> &wgpu::Device {
        self.device
    }

    #[inline]
    pub fn sampler_linear(&self) -> &wgpu::Sampler {
        self.sampler_linear
    }

    #[inline]
    pub fn sampler_nearest(&self) -> &wgpu::Sampler {
        self.sampler_nearest
    }

    #[inline]
    pub fn view_bind_group(&self) -> &wgpu::BindGroup {
        self.view_bind_group
    }

    #[inline]
    pub fn view_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        self.view_bind_group_layout
    }

    #[inline]
    pub fn model_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        self.model_bind_group_layout
    }

    #[inline]
    pub fn fallback_texture(&self) -> Option<&Texture> {
        self.fallback_texture
    }

    #[inline]
    pub fn target_format(&self) -> wgpu::TextureFormat {
        self.target_format
    }

    #[inline]
    pub fn depth_format(&self) -> Option<wgpu::TextureFormat> {
        self.depth_format
    }
}
