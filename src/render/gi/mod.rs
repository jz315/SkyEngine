//! Provider-driven global illumination integration.

pub mod providers;

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use crate::gpu::GpuContext;
use crate::math::Mat4;
use crate::render::component::GlobalIllumination;
use crate::render::execution::{
    ComputePassExecuteContext, PostFxPassExecuteContext, PostFxPassSetupContext,
};
use crate::render::graph::RenderGraphError;
use crate::render::resources::mesh::RayTriangle;
use crate::render::view::SceneView;
use crate::render::Color;
use crate::render::GpuLight;

pub type GiProviderId = &'static str;

pub trait GiSettings: Any + Send + Sync {
    fn as_any(&self) -> &dyn Any;
}

impl<T> GiSettings for T
where
    T: Any + Send + Sync,
{
    #[inline]
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone)]
pub struct GiProviderConfig {
    pub id: GiProviderId,
    pub settings: Arc<dyn GiSettings>,
}

impl GiProviderConfig {
    #[inline]
    pub fn new<T>(id: GiProviderId, settings: T) -> Self
    where
        T: GiSettings + 'static,
    {
        Self {
            id,
            settings: Arc::new(settings),
        }
    }
}

impl fmt::Debug for GiProviderConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GiProviderConfig")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

pub trait GiProviderFactory: Send + Sync {
    fn id(&self) -> GiProviderId;
    fn create(&self, gpu: &GpuContext) -> Box<dyn GiProviderRuntime>;
}

pub trait GiProviderRuntime: Send {
    fn prepare(&mut self, gpu: &GpuContext, scene: &GiSceneInput<'_>, settings: &dyn GiSettings);
    fn update(
        &mut self,
        _ctx: &mut ComputePassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        Ok(())
    }
    fn setup_composite(&mut self, _ctx: &mut PostFxPassSetupContext<'_, '_>) {}
    fn execute_composite(
        &mut self,
        _ctx: &mut PostFxPassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        Ok(())
    }
    fn update_descriptor(&self) -> Option<GiUpdateDescriptor> {
        None
    }
    fn sampling_binding(&self) -> GiSamplingBinding;
    fn shader_descriptor(&self) -> GiShaderDescriptor;
    fn composite_descriptor(&self) -> Option<GiCompositeDescriptor> {
        None
    }
}

#[derive(Clone)]
pub struct GiUpdateDescriptor {
    pub label: &'static str,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    pub shader: &'static str,
    pub entry_point: &'static str,
    pub dispatch_size: [u32; 3],
    pub workgroup_size: [u32; 3],
    pub flags: crate::render::graph::PassFlags,
}

#[derive(Clone, Copy)]
pub struct GiCompositeDescriptor {
    pub label: &'static str,
    pub requires_hdr_input: bool,
}

#[derive(Clone)]
pub struct GiSamplingBinding {
    pub layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GiShaderDescriptor {
    pub key: &'static str,
    pub source: &'static str,
}

#[derive(Clone, Debug)]
pub struct GiRenderable<'a> {
    pub model: Mat4,
    pub layer_mask: u32,
    pub opaque: bool,
    pub ray_triangles: &'a [RayTriangle],
    pub triangle_range: std::ops::Range<usize>,
    pub material: GiMaterial,
}

#[derive(Clone, Copy, Debug)]
pub struct GiMaterial {
    pub albedo: Color,
    pub emissive: Color,
    pub metallic: f32,
}

pub struct GiSceneInput<'a> {
    pub primary_view_index: Option<usize>,
    pub primary_view: Option<&'a SceneView>,
    pub views: &'a [SceneView],
    pub lights: &'a [GpuLight],
    pub ambient_color: Color,
    pub frame_index: u64,
    pub renderables: &'a [GiRenderable<'a>],
}

#[derive(Default)]
pub struct GiProviderRegistry {
    factories: HashMap<GiProviderId, Arc<dyn GiProviderFactory>>,
}

impl GiProviderRegistry {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_default_providers() -> Self {
        let mut registry = Self::new();
        providers::register_default_providers(&mut registry);
        registry
    }

    pub fn register<F>(&mut self, factory: F)
    where
        F: GiProviderFactory + 'static,
    {
        self.factories.insert(factory.id(), Arc::new(factory));
    }

    pub fn get(&self, id: GiProviderId) -> Option<Arc<dyn GiProviderFactory>> {
        self.factories.get(id).cloned()
    }
}

pub struct GiRuntime {
    registry: GiProviderRegistry,
    active_id: Option<GiProviderId>,
    active: RefCell<Box<dyn GiProviderRuntime>>,
    frame_index: u64,
}

impl GiRuntime {
    pub fn new(gpu: &GpuContext) -> Self {
        let null = NullGiProvider::new(gpu);
        Self {
            registry: GiProviderRegistry::with_default_providers(),
            active_id: None,
            active: RefCell::new(Box::new(null)),
            frame_index: 0,
        }
    }

    pub fn register_provider<F>(&mut self, factory: F)
    where
        F: GiProviderFactory + 'static,
    {
        self.registry.register(factory);
    }

    pub fn prepare(
        &mut self,
        gpu: &GpuContext,
        global_illumination: &GlobalIllumination,
        mut scene: GiSceneInput<'_>,
    ) {
        scene.frame_index = self.frame_index;
        self.frame_index = self.frame_index.wrapping_add(1);

        match global_illumination {
            GlobalIllumination::Off => {
                if self.active_id.is_some() {
                    self.active = RefCell::new(Box::new(NullGiProvider::new(gpu)));
                    self.active_id = None;
                }
                self.active.get_mut().prepare(gpu, &scene, &NullGiSettings);
            }
            GlobalIllumination::Provider(config) => {
                if self.active_id != Some(config.id) {
                    let Some(factory) = self.registry.get(config.id) else {
                        self.active_id = None;
                        self.active = RefCell::new(Box::new(NullGiProvider::new(gpu)));
                        self.active.get_mut().prepare(gpu, &scene, &NullGiSettings);
                        return;
                    };
                    self.active = RefCell::new(factory.create(gpu));
                    self.active_id = Some(config.id);
                }
                self.active
                    .get_mut()
                    .prepare(gpu, &scene, config.settings.as_ref());
            }
        }
    }

    #[inline]
    pub fn update_descriptor(&self) -> Option<GiUpdateDescriptor> {
        self.active.borrow().update_descriptor()
    }

    #[inline]
    pub fn sampling_binding(&self) -> GiSamplingBinding {
        self.active.borrow().sampling_binding()
    }

    #[inline]
    pub fn shader_descriptor(&self) -> GiShaderDescriptor {
        self.active.borrow().shader_descriptor()
    }

    #[inline]
    pub fn composite_descriptor(&self) -> Option<GiCompositeDescriptor> {
        self.active.borrow().composite_descriptor()
    }

    #[inline]
    pub(crate) fn update(
        &self,
        ctx: &mut ComputePassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        self.active.borrow_mut().update(ctx)
    }

    #[inline]
    pub(crate) fn setup_composite(&self, ctx: &mut PostFxPassSetupContext<'_, '_>) {
        self.active.borrow_mut().setup_composite(ctx);
    }

    #[inline]
    pub(crate) fn execute_composite(
        &self,
        ctx: &mut PostFxPassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        self.active.borrow_mut().execute_composite(ctx)
    }

    #[inline]
    pub fn active_provider_id(&self) -> Option<GiProviderId> {
        self.active_id
    }
}

struct NullGiSettings;

pub struct NullGiProvider {
    layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
}

impl NullGiProvider {
    fn new(gpu: &GpuContext) -> Self {
        let layout = gpu
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("gi_null_sampling_bgl"),
                entries: &[],
            });
        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gi_null_sampling_bg"),
            layout: &layout,
            entries: &[],
        });
        Self { layout, bind_group }
    }
}

impl GiProviderRuntime for NullGiProvider {
    fn prepare(
        &mut self,
        _gpu: &GpuContext,
        _scene: &GiSceneInput<'_>,
        _settings: &dyn GiSettings,
    ) {
    }

    fn sampling_binding(&self) -> GiSamplingBinding {
        GiSamplingBinding {
            layout: self.layout.clone(),
            bind_group: self.bind_group.clone(),
        }
    }

    fn shader_descriptor(&self) -> GiShaderDescriptor {
        GiShaderDescriptor {
            key: "null",
            source: NULL_GI_SHADER,
        }
    }
}

pub(crate) fn downcast_settings<T: GiSettings + 'static>(
    settings: &dyn GiSettings,
    provider_id: GiProviderId,
) -> Option<&T> {
    let value = settings.as_any().downcast_ref::<T>();
    debug_assert!(
        value.is_some(),
        "GI provider `{provider_id}` received settings with an unexpected type"
    );
    value
}

pub const NULL_GI_SHADER: &str = r#"
fn gi_debug_mode() -> u32 {
    return 0u;
}

fn gi_debug_color(world_position: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    _ = world_position;
    _ = normal;
    return vec3<f32>(0.0);
}

fn gi_sample_indirect_diffuse(
    world_position: vec3<f32>,
    normal: vec3<f32>,
    base_color: vec3<f32>,
    metallic: f32,
    roughness: f32,
) -> vec3<f32> {
    _ = world_position;
    _ = normal;
    return base_color * (1.0 - metallic) * mix(0.0015, 0.0065, roughness);
}
"#;

pub(crate) fn first_lit_view(views: &[SceneView]) -> Option<(usize, &SceneView)> {
    views.iter().enumerate().find(|(_, view)| !view.is_shadow())
}
