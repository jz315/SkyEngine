use super::*;

impl DirectionalShadowPhase {
    #[inline]
    pub fn new() -> Self {
        Self {
            pipelines: FxHashMap::default(),
            clear_pipeline: None,
            transparent_clear_pipeline: None,
        }
    }

    pub(super) fn clear_pipeline_for(&mut self, device: &wgpu::Device) -> &wgpu::RenderPipeline {
        self.clear_pipeline.get_or_insert_with(|| {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("directional_shadow_atlas_rect_clear_shader"),
                source: wgpu::ShaderSource::Wgsl(SHADOW_ATLAS_CLEAR_SHADER.into()),
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("directional_shadow_atlas_rect_clear_pipeline_layout"),
                bind_group_layouts: &[],
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("directional_shadow_atlas_rect_clear_pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: None,
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEFAULT_DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        })
    }

    pub(super) fn transparent_clear_pipeline_for(
        &mut self,
        device: &wgpu::Device,
    ) -> &wgpu::RenderPipeline {
        self.transparent_clear_pipeline.get_or_insert_with(|| {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("directional_transparent_shadow_atlas_rect_clear_shader"),
                source: wgpu::ShaderSource::Wgsl(TRANSPARENT_SHADOW_ATLAS_CLEAR_SHADER.into()),
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("directional_transparent_shadow_atlas_rect_clear_pipeline_layout"),
                bind_group_layouts: &[],
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("directional_transparent_shadow_atlas_rect_clear_pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: TRANSPARENT_SHADOW_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEFAULT_DEPTH_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        })
    }

    pub(super) fn pipeline_for(
        &mut self,
        device: &wgpu::Device,
        shadow_layout: &wgpu::BindGroupLayout,
        material_layout: Option<&wgpu::BindGroupLayout>,
        mesh_layout: &VertexLayout,
        raster_bias: ShadowRasterBias,
        kind: ShadowPipelineKind,
    ) -> Result<&wgpu::RenderPipeline, RenderGraphError> {
        if matches!(
            kind,
            ShadowPipelineKind::AlphaTest | ShadowPipelineKind::Transparent
        ) && material_layout.is_none()
        {
            return Err(RenderGraphError::ExecutionFailed(
                "material-aware shadow pipeline requires a material bind-group layout".into(),
            ));
        }
        let mut hasher = rustc_hash::FxHasher::default();
        kind.hash(&mut hasher);
        mesh_layout.hash(&mut hasher);
        raster_bias.constant.hash(&mut hasher);
        raster_bias.slope_scale.to_bits().hash(&mut hasher);
        raster_bias.clamp.to_bits().hash(&mut hasher);
        if let Some(material_layout) = material_layout {
            (std::ptr::from_ref(material_layout) as usize).hash(&mut hasher);
        }
        let key = hasher.finish();
        if let std::collections::hash_map::Entry::Vacant(e) = self.pipelines.entry(key) {
            let vertex_attributes = shadow_vertex_attributes(mesh_layout, kind)
                .map_err(shadow_pipeline_material_error)?;
            let (shader_label, shader_source) = match kind {
                ShadowPipelineKind::Opaque => (
                    "directional_shadow_shader",
                    include_str!("../../../../../shaders/lighting/shadow_depth.wgsl"),
                ),
                ShadowPipelineKind::AlphaTest => (
                    "directional_shadow_alpha_test_shader",
                    include_str!("../../../../../shaders/lighting/shadow_depth_alpha_test.wgsl"),
                ),
                ShadowPipelineKind::Transparent => (
                    "directional_transparent_shadow_shader",
                    include_str!("../../../../../shaders/lighting/shadow_transparent.wgsl"),
                ),
            };
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(shader_label),
                source: wgpu::ShaderSource::Wgsl(shader_source.into()),
            });
            let material_layouts = material_layout.into_iter();
            let bind_group_layouts: Vec<Option<&wgpu::BindGroupLayout>> =
                std::iter::once(shadow_layout)
                    .chain(material_layouts)
                    .map(Some)
                    .collect();
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("directional_shadow_pipeline_layout"),
                bind_group_layouts: &bind_group_layouts,
                immediate_size: 0,
            });
            let instance_attributes = [
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 8,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 9,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 10,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 48,
                    shader_location: 11,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ];
            let vertex_buffers = [
                wgpu::VertexBufferLayout {
                    array_stride: mesh_layout.stride() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &vertex_attributes,
                },
                wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<[f32; 16]>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &instance_attributes,
                },
            ];
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("directional_shadow_pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &vertex_buffers,
                    compilation_options: Default::default(),
                },
                fragment: match kind {
                    ShadowPipelineKind::Opaque => Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_main"),
                        targets: &[],
                        compilation_options: Default::default(),
                    }),
                    ShadowPipelineKind::AlphaTest => Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_main"),
                        targets: &[],
                        compilation_options: Default::default(),
                    }),
                    ShadowPipelineKind::Transparent => Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_main"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: TRANSPARENT_SHADOW_FORMAT,
                            blend: Some(TRANSPARENT_SHADOW_BLEND_STATE),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: Default::default(),
                    }),
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: if kind == ShadowPipelineKind::Transparent {
                        None
                    } else {
                        Some(wgpu::Face::Back)
                    },
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEFAULT_DEPTH_FORMAT,
                    depth_write_enabled: Some(kind != ShadowPipelineKind::Transparent),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState {
                        constant: raster_bias.constant,
                        slope_scale: raster_bias.slope_scale,
                        clamp: raster_bias.clamp,
                    },
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });
            e.insert(pipeline);
        }
        Ok(self
            .pipelines
            .get(&key)
            .expect("directional shadow pipeline inserted for mesh layout"))
    }
}
