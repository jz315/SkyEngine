use super::constants::SSGI_PROVIDER_ID;
use super::executor::SsgiExecutor;
use super::settings::SsgiSettings;
use crate::gpu::GpuContext;
use crate::render::execution::{PostFxPassExecuteContext, PostFxPassSetupContext};
use crate::render::gi::{
    downcast_settings, GiCompositeDescriptor, GiProviderFactory, GiProviderId, GiProviderRuntime,
    GiSamplingBinding, GiSceneInput, GiSettings, GiShaderDescriptor,
};
use crate::render::graph::RenderGraphError;

pub struct SsgiProviderFactory;

impl GiProviderFactory for SsgiProviderFactory {
    fn id(&self) -> GiProviderId {
        SSGI_PROVIDER_ID
    }

    fn create(&self, _gpu: &GpuContext) -> Box<dyn GiProviderRuntime> {
        Box::new(SsgiRuntime {
            executor: SsgiExecutor::default(),
            settings: SsgiSettings::default(),
            sampling_layout: None,
            sampling_bind_group: None,
        })
    }
}

struct SsgiRuntime {
    executor: SsgiExecutor,
    settings: SsgiSettings,
    sampling_layout: Option<wgpu::BindGroupLayout>,
    sampling_bind_group: Option<wgpu::BindGroup>,
}

impl SsgiRuntime {
    fn ensure_sampling_binding(&mut self, gpu: &GpuContext) {
        if self.sampling_layout.is_none() {
            self.sampling_layout = Some(gpu.device().create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("ssgi_sampling_bgl"),
                    entries: &[],
                },
            ));
        }
        if self.sampling_bind_group.is_none() {
            let layout = self
                .sampling_layout
                .as_ref()
                .expect("SSGI sampling layout should exist");
            self.sampling_bind_group =
                Some(gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("ssgi_sampling_bg"),
                    layout,
                    entries: &[],
                }));
        }
    }
}

impl GiProviderRuntime for SsgiRuntime {
    fn prepare(&mut self, gpu: &GpuContext, _scene: &GiSceneInput<'_>, settings: &dyn GiSettings) {
        if let Some(settings) = downcast_settings::<SsgiSettings>(settings, SSGI_PROVIDER_ID) {
            self.settings = *settings;
        }
        self.ensure_sampling_binding(gpu);
    }

    fn setup_composite(&mut self, ctx: &mut PostFxPassSetupContext<'_, '_>) {
        self.executor.setup_with_settings(ctx, self.settings);
    }

    fn execute_composite(
        &mut self,
        ctx: &mut PostFxPassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        self.executor.execute_with_settings(ctx, self.settings)
    }

    fn sampling_binding(&self) -> GiSamplingBinding {
        GiSamplingBinding {
            layout: self
                .sampling_layout
                .as_ref()
                .expect("SSGI sampling layout should be initialized during prepare")
                .clone(),
            bind_group: self
                .sampling_bind_group
                .as_ref()
                .expect("SSGI sampling bind group should be initialized during prepare")
                .clone(),
        }
    }

    fn shader_descriptor(&self) -> GiShaderDescriptor {
        GiShaderDescriptor {
            key: "ssgi",
            source: crate::render::gi::NULL_GI_SHADER,
        }
    }

    fn composite_descriptor(&self) -> Option<GiCompositeDescriptor> {
        Some(GiCompositeDescriptor {
            label: "gi_composite",
            requires_hdr_input: self.executor.requires_hdr_input(),
        })
    }
}
