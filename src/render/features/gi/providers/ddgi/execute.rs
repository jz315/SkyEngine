use super::*;

impl GiProviderRuntime for DdgiRuntime {
    fn prepare(&mut self, gpu: &GpuContext, scene: &GiSceneInput<'_>, settings: &dyn GiSettings) {
        let Some(settings) = downcast_settings::<DdgiSettings>(settings, DDGI_PROVIDER_ID) else {
            self.write_disabled(gpu);
            return;
        };
        DdgiRuntime::prepare(self, gpu, *settings, scene);
    }

    fn update_descriptor(&self) -> Option<GiUpdateDescriptor> {
        let (width, height) = self.dispatch_size();
        if self.last_uniform.counts_enabled[3] == 0 || width == 0 || height == 0 {
            return None;
        }
        Some(GiUpdateDescriptor {
            label: "gi_update",
            bind_group_layout: self.bind_group_layout.clone(),
            bind_group: self.bind_group.clone(),
            shader: DDGI_SHADER,
            entry_point: "cs_main",
            dispatch_size: [width, height, 1],
            workgroup_size: [DDGI_WORKGROUP_SIZE, DDGI_WORKGROUP_SIZE, 1],
            flags: PassFlags::PREFER_ASYNC_COMPUTE | PassFlags::COMPUTE_INTENSIVE,
        })
    }

    fn update(
        &mut self,
        ctx: &mut ComputePassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        let Some((shader, entry_point, label, bind_group_layout, dispatch_size, workgroup_size)) =
            self.update_descriptor().map(|descriptor| {
                (
                    descriptor.shader,
                    descriptor.entry_point,
                    descriptor.label,
                    descriptor.bind_group_layout,
                    descriptor.dispatch_size,
                    descriptor.workgroup_size,
                )
            })
        else {
            return Ok(());
        };
        let (gpu, _pass, _resources, _execution) = ctx.split();
        let pipeline = self
            .update_pipeline
            .get_or_insert_with(|| {
                ComputePipelineCache::new(gpu, shader, entry_point, &[&bind_group_layout], label)
            })
            .pipeline(gpu);
        let mut frame = gpu.frame();
        let mut pass = frame.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(label),
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(
            dispatch_size[0].div_ceil(workgroup_size[0].max(1)),
            dispatch_size[1].div_ceil(workgroup_size[1].max(1)),
            dispatch_size[2].div_ceil(workgroup_size[2].max(1)),
        );
        Ok(())
    }

    fn sampling_binding(&self) -> GiSamplingBinding {
        GiSamplingBinding {
            layout: self.sampling_bind_group_layout.clone(),
            bind_group: self.sampling_bind_group.clone(),
        }
    }

    fn shader_descriptor(&self) -> GiShaderDescriptor {
        GiShaderDescriptor {
            key: "ddgi",
            source: DDGI_SAMPLING_SHADER,
        }
    }
}
