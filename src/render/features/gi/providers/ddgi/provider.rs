use super::*;

pub struct DdgiProviderFactory;

impl GiProviderFactory for DdgiProviderFactory {
    fn id(&self) -> GiProviderId {
        DDGI_PROVIDER_ID
    }

    fn create(&self, gpu: &GpuContext) -> Box<dyn GiProviderRuntime> {
        Box::new(DdgiRuntime::new(gpu))
    }
}

pub(crate) struct DdgiRuntime {
    pub(crate) bind_group_layout: wgpu::BindGroupLayout,
    pub(crate) bind_group: wgpu::BindGroup,
    pub(crate) sampling_bind_group_layout: wgpu::BindGroupLayout,
    pub(crate) sampling_bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    triangle_buffer: wgpu::Buffer,
    node_buffer: wgpu::Buffer,
    light_buffer: wgpu::Buffer,
    light_meta_buffer: wgpu::Buffer,
    irradiance_a: RenderTarget,
    irradiance_b: RenderTarget,
    visibility_a: RenderTarget,
    visibility_b: RenderTarget,
    sampler: wgpu::Sampler,
    triangle_capacity: usize,
    node_capacity: usize,
    light_capacity: usize,
    current_is_a: bool,
    irradiance_atlas_width: u32,
    irradiance_atlas_height: u32,
    visibility_atlas_width: u32,
    visibility_atlas_height: u32,
    frame_index: u32,
    force_full_update: bool,
    history_valid: bool,
    history_origin: [f32; 3],
    history_spacing: f32,
    history_counts: [u32; 3],
    history_irradiance_resolution: u32,
    history_visibility_resolution: u32,
    pub(crate) last_uniform: DdgiUniform,
    pub(crate) update_pipeline: Option<ComputePipelineCache>,
}

impl DdgiRuntime {
    pub(crate) fn new(gpu: &GpuContext) -> Self {
        let bind_group_layout = create_ddgi_update_bind_group_layout(gpu.device());
        let sampling_bind_group_layout = create_ddgi_sampling_bind_group_layout(gpu.device());
        let uniform_buffer = gpu.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("ddgi_uniform"),
            size: std::mem::size_of::<DdgiUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let triangle_buffer = create_storage_buffer(
            gpu,
            "ddgi_triangles",
            1,
            std::mem::size_of::<GpuGiTriangle>(),
        );
        let node_buffer = create_storage_buffer(
            gpu,
            "ddgi_bvh_nodes",
            1,
            std::mem::size_of::<GpuGiBvhNode>(),
        );
        let light_buffer =
            create_storage_buffer(gpu, "ddgi_lights", 1, std::mem::size_of::<GpuLight>());
        let light_meta_buffer = gpu.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("ddgi_light_meta"),
            size: std::mem::size_of::<DdgiLightMeta>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let irradiance_a =
            create_ddgi_texture(gpu, 1, 1, DDGI_IRRADIANCE_FORMAT, "ddgi_irradiance_a");
        let irradiance_b =
            create_ddgi_texture(gpu, 1, 1, DDGI_IRRADIANCE_FORMAT, "ddgi_irradiance_b");
        let visibility_a =
            create_ddgi_texture(gpu, 1, 1, DDGI_VISIBILITY_FORMAT, "ddgi_visibility_a");
        let visibility_b =
            create_ddgi_texture(gpu, 1, 1, DDGI_VISIBILITY_FORMAT, "ddgi_visibility_b");
        let sampler = gpu.device().create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ddgi_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let last_uniform = disabled_uniform();
        gpu.queue()
            .write_buffer(&uniform_buffer, 0, bytemuck::bytes_of(&last_uniform));
        gpu.queue().write_buffer(
            &light_meta_buffer,
            0,
            bytemuck::bytes_of(&DdgiLightMeta::default()),
        );
        let bind_group = create_ddgi_update_bind_group(
            gpu.device(),
            &bind_group_layout,
            &uniform_buffer,
            &triangle_buffer,
            &node_buffer,
            &light_buffer,
            &light_meta_buffer,
            irradiance_b.view(),
            irradiance_a.view(),
            visibility_b.view(),
            visibility_a.view(),
            &sampler,
        );
        let sampling_bind_group = create_ddgi_sampling_bind_group(
            gpu.device(),
            &sampling_bind_group_layout,
            &uniform_buffer,
            irradiance_a.view(),
            visibility_a.view(),
            &sampler,
        );
        Self {
            bind_group_layout,
            bind_group,
            sampling_bind_group_layout,
            sampling_bind_group,
            uniform_buffer,
            triangle_buffer,
            node_buffer,
            light_buffer,
            light_meta_buffer,
            irradiance_a,
            irradiance_b,
            visibility_a,
            visibility_b,
            sampler,
            triangle_capacity: 1,
            node_capacity: 1,
            light_capacity: 1,
            current_is_a: true,
            irradiance_atlas_width: 1,
            irradiance_atlas_height: 1,
            visibility_atlas_width: 1,
            visibility_atlas_height: 1,
            frame_index: 0,
            force_full_update: true,
            history_valid: false,
            history_origin: [0.0; 3],
            history_spacing: 1.0,
            history_counts: [1; 3],
            history_irradiance_resolution: 1,
            history_visibility_resolution: 1,
            last_uniform,
            update_pipeline: None,
        }
    }

    pub(crate) fn prepare(
        &mut self,
        gpu: &GpuContext,
        ddgi: DdgiSettings,
        scene: &GiSceneInput<'_>,
    ) {
        let Some(scene_view) = scene.primary_view else {
            self.write_disabled(gpu);
            return;
        };
        let counts = ddgi.volume.counts.map(|value| value.max(1));
        let irradiance_res = ddgi.irradiance_resolution.clamp(2, 16);
        let visibility_res = ddgi.visibility_resolution.clamp(2, 16);
        let (irradiance_atlas_width, irradiance_atlas_height) =
            ddgi_atlas_size(counts, irradiance_res);
        let (visibility_atlas_width, visibility_atlas_height) =
            ddgi_atlas_size(counts, visibility_res);
        self.resize_atlas_if_needed(
            gpu,
            irradiance_atlas_width,
            irradiance_atlas_height,
            visibility_atlas_width,
            visibility_atlas_height,
        );
        let origin = ddgi_origin(ddgi, scene_view);
        let spacing = ddgi.volume.spacing.max(0.05);
        self.invalidate_history_if_volume_changed(
            origin,
            spacing,
            counts,
            irradiance_res,
            visibility_res,
        );
        self.current_is_a = !self.current_is_a;

        let triangles = collect_gi_triangles(scene.renderables);
        let bvh_nodes = build_gpu_bvh(&triangles);
        self.ensure_triangle_capacity(gpu, triangles.len().max(1));
        if !triangles.is_empty() {
            gpu.queue()
                .write_buffer(&self.triangle_buffer, 0, bytemuck::cast_slice(&triangles));
        }
        self.ensure_node_capacity(gpu, bvh_nodes.len().max(1));
        if !bvh_nodes.is_empty() {
            gpu.queue()
                .write_buffer(&self.node_buffer, 0, bytemuck::cast_slice(&bvh_nodes));
        }
        self.ensure_light_capacity(gpu, scene.lights.len().max(1));
        if !scene.lights.is_empty() {
            gpu.queue()
                .write_buffer(&self.light_buffer, 0, bytemuck::cast_slice(scene.lights));
        }
        gpu.queue().write_buffer(
            &self.light_meta_buffer,
            0,
            bytemuck::bytes_of(&DdgiLightMeta {
                count: scene.lights.len() as u32,
                _pad: [0; 3],
            }),
        );

        let debug_mode = match ddgi.debug {
            DdgiDebugMode::Off => 0,
            DdgiDebugMode::Probes => 1,
            DdgiDebugMode::Irradiance => 2,
            DdgiDebugMode::Visibility => 3,
            DdgiDebugMode::RayBudget => 4,
        };
        let probe_count = counts[0]
            .saturating_mul(counts[1])
            .saturating_mul(counts[2])
            .max(1);
        let probes_per_frame = if self.force_full_update {
            probe_count
        } else {
            ddgi.probes_per_frame.max(1)
        };
        self.last_uniform = DdgiUniform {
            origin_spacing: [origin[0], origin[1], origin[2], spacing],
            counts_enabled: [counts[0], counts[1], counts[2], 1],
            irradiance_atlas_params: [
                irradiance_atlas_width,
                irradiance_atlas_height,
                irradiance_res,
                bvh_nodes.len() as u32,
            ],
            visibility_atlas_params: [
                visibility_atlas_width,
                visibility_atlas_height,
                visibility_res,
                0,
            ],
            trace_params: [
                ddgi.max_ray_distance.max(0.1),
                ddgi.hysteresis.clamp(0.0, 0.99),
                ddgi.normal_bias.max(0.0),
                ddgi.view_bias.max(0.0),
            ],
            frame_params: [
                self.frame_index,
                ddgi.rays_per_probe.max(1),
                probes_per_frame,
                ddgi.bounces.clamp(1, 4) | (debug_mode << 16),
            ],
            ambient: scene.ambient_color.to_array(),
        };
        self.frame_index = self.frame_index.wrapping_add(1);
        self.force_full_update = false;
        gpu.queue().write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&self.last_uniform),
        );
        let (write_target, read_target) = if self.current_is_a {
            (&self.irradiance_a, &self.irradiance_b)
        } else {
            (&self.irradiance_b, &self.irradiance_a)
        };
        let (write_visibility, read_visibility) = if self.current_is_a {
            (&self.visibility_a, &self.visibility_b)
        } else {
            (&self.visibility_b, &self.visibility_a)
        };
        self.bind_group = create_ddgi_update_bind_group(
            gpu.device(),
            &self.bind_group_layout,
            &self.uniform_buffer,
            &self.triangle_buffer,
            &self.node_buffer,
            &self.light_buffer,
            &self.light_meta_buffer,
            write_target.view(),
            read_target.view(),
            write_visibility.view(),
            read_visibility.view(),
            &self.sampler,
        );
        self.refresh_sampling_bind_group(gpu);
    }

    #[inline]
    pub(crate) fn dispatch_size(&self) -> (u32, u32) {
        (
            self.irradiance_atlas_width.max(self.visibility_atlas_width),
            self.irradiance_atlas_height
                .max(self.visibility_atlas_height),
        )
    }

    fn refresh_sampling_bind_group(&mut self, gpu: &GpuContext) {
        self.sampling_bind_group = create_ddgi_sampling_bind_group(
            gpu.device(),
            &self.sampling_bind_group_layout,
            &self.uniform_buffer,
            self.current_irradiance().view(),
            self.current_visibility().view(),
            &self.sampler,
        );
    }

    fn current_irradiance(&self) -> &RenderTarget {
        if self.current_is_a {
            &self.irradiance_a
        } else {
            &self.irradiance_b
        }
    }

    fn current_visibility(&self) -> &RenderTarget {
        if self.current_is_a {
            &self.visibility_a
        } else {
            &self.visibility_b
        }
    }

    pub(crate) fn write_disabled(&mut self, gpu: &GpuContext) {
        self.last_uniform = disabled_uniform();
        self.frame_index = 0;
        self.force_full_update = true;
        self.history_valid = false;
        gpu.queue().write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&self.last_uniform),
        );
        self.bind_group = create_ddgi_update_bind_group(
            gpu.device(),
            &self.bind_group_layout,
            &self.uniform_buffer,
            &self.triangle_buffer,
            &self.node_buffer,
            &self.light_buffer,
            &self.light_meta_buffer,
            self.current_irradiance().view(),
            self.current_irradiance().view(),
            self.current_visibility().view(),
            self.current_visibility().view(),
            &self.sampler,
        );
        self.refresh_sampling_bind_group(gpu);
    }

    fn resize_atlas_if_needed(
        &mut self,
        gpu: &GpuContext,
        irradiance_width: u32,
        irradiance_height: u32,
        visibility_width: u32,
        visibility_height: u32,
    ) {
        if self.irradiance_atlas_width == irradiance_width
            && self.irradiance_atlas_height == irradiance_height
            && self.visibility_atlas_width == visibility_width
            && self.visibility_atlas_height == visibility_height
        {
            return;
        }
        self.irradiance_a = create_ddgi_texture(
            gpu,
            irradiance_width,
            irradiance_height,
            DDGI_IRRADIANCE_FORMAT,
            "ddgi_irradiance_a",
        );
        self.irradiance_b = create_ddgi_texture(
            gpu,
            irradiance_width,
            irradiance_height,
            DDGI_IRRADIANCE_FORMAT,
            "ddgi_irradiance_b",
        );
        self.visibility_a = create_ddgi_texture(
            gpu,
            visibility_width,
            visibility_height,
            DDGI_VISIBILITY_FORMAT,
            "ddgi_visibility_a",
        );
        self.visibility_b = create_ddgi_texture(
            gpu,
            visibility_width,
            visibility_height,
            DDGI_VISIBILITY_FORMAT,
            "ddgi_visibility_b",
        );
        self.irradiance_atlas_width = irradiance_width;
        self.irradiance_atlas_height = irradiance_height;
        self.visibility_atlas_width = visibility_width;
        self.visibility_atlas_height = visibility_height;
        self.current_is_a = true;
        self.frame_index = 0;
        self.force_full_update = true;
        self.history_valid = false;
        self.refresh_sampling_bind_group(gpu);
    }

    fn invalidate_history_if_volume_changed(
        &mut self,
        origin: [f32; 3],
        spacing: f32,
        counts: [u32; 3],
        irradiance_resolution: u32,
        visibility_resolution: u32,
    ) {
        let origin_changed = self
            .history_origin
            .iter()
            .zip(origin)
            .any(|(previous, current)| (*previous - current).abs() > 0.0001);
        let changed = !self.history_valid
            || origin_changed
            || (self.history_spacing - spacing).abs() > 0.0001
            || self.history_counts != counts
            || self.history_irradiance_resolution != irradiance_resolution
            || self.history_visibility_resolution != visibility_resolution;
        if !changed {
            return;
        }

        self.history_valid = true;
        self.history_origin = origin;
        self.history_spacing = spacing;
        self.history_counts = counts;
        self.history_irradiance_resolution = irradiance_resolution;
        self.history_visibility_resolution = visibility_resolution;
        self.frame_index = 0;
        self.force_full_update = true;
    }

    fn ensure_triangle_capacity(&mut self, gpu: &GpuContext, required: usize) {
        if required <= self.triangle_capacity {
            return;
        }
        self.triangle_capacity = required.next_power_of_two();
        self.triangle_buffer = create_storage_buffer(
            gpu,
            "ddgi_triangles",
            self.triangle_capacity,
            std::mem::size_of::<GpuGiTriangle>(),
        );
    }

    fn ensure_node_capacity(&mut self, gpu: &GpuContext, required: usize) {
        if required <= self.node_capacity {
            return;
        }
        self.node_capacity = required.next_power_of_two();
        self.node_buffer = create_storage_buffer(
            gpu,
            "ddgi_bvh_nodes",
            self.node_capacity,
            std::mem::size_of::<GpuGiBvhNode>(),
        );
    }

    fn ensure_light_capacity(&mut self, gpu: &GpuContext, required: usize) {
        if required <= self.light_capacity {
            return;
        }
        self.light_capacity = required.next_power_of_two();
        self.light_buffer = create_storage_buffer(
            gpu,
            "ddgi_lights",
            self.light_capacity,
            std::mem::size_of::<GpuLight>(),
        );
    }
}
