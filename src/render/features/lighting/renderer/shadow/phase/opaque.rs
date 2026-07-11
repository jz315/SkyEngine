use super::*;

impl Default for DirectionalShadowPhase {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderPhase for DirectionalShadowPhase {
    fn name(&self) -> &'static str {
        "directional_shadow"
    }

    fn is_enabled(
        &self,
        _frame: &crate::render::execution::PreparedFrame<'_>,
        view: &crate::render::execution::PreparedView<'_>,
    ) -> bool {
        view.payload::<SceneView>()
            .is_some_and(SceneView::is_shadow)
            && view
                .payload::<ShadowViewBinding>()
                .is_some_and(ShadowViewBinding::enabled)
    }

    fn setup(&mut self, ctx: &mut PhaseSetupContext<'_, '_>) {
        let Some(scene_view) = ctx.view().payload::<SceneView>() else {
            return;
        };
        if !scene_view.is_shadow() {
            return;
        }
        let Some(shadow_view) = ctx.view().payload::<ShadowViewBinding>() else {
            return;
        };
        if !shadow_view.enabled() {
            return;
        }
        if let Some(scene_layout) = ctx.frame_payload::<ShadowSceneBindingLayout>() {
            let _ = ctx.publish_scene_shadows(SceneShadowResources::from_bind_group(
                ShadowResourceKind::DirectionalCascades,
                scene_layout.bind_group_layout(),
                shadow_view.bind_group(),
            ));
        }
        let cascade_index = scene_view.shadow_cascade();
        let slot_name = format!(
            "directional_shadow_atlas_{}",
            scene_view.shadow_binding().unwrap_or(0)
        );
        let graph_texture_name = format!("{slot_name}_cascade_{cascade_index}");
        let depth_handle = ctx
            .state()
            .texture_slot(&slot_name)
            .map(|slot| slot.handle())
            .unwrap_or_else(|| {
                let handle = ctx.graph().create_texture(|builder| {
                    builder
                        .name(graph_texture_name)
                        .import_external(import_shadow_target(shadow_view));
                });
                ctx.state()
                    .set_texture_slot(slot_name, handle, shadow_view.target().format());
                handle
            });
        let transparent_slot_name = format!(
            "directional_transparent_shadow_atlas_{}",
            scene_view.shadow_binding().unwrap_or(0)
        );
        let transparent_graph_texture_name =
            format!("{transparent_slot_name}_cascade_{cascade_index}");
        let transparent_handle = ctx
            .state()
            .texture_slot(&transparent_slot_name)
            .map(|slot| slot.handle())
            .unwrap_or_else(|| {
                let handle = ctx.graph().create_texture(|builder| {
                    builder
                        .name(transparent_graph_texture_name)
                        .import_external(import_transparent_shadow_target(shadow_view));
                });
                ctx.state().set_texture_slot(
                    transparent_slot_name,
                    handle,
                    shadow_view.transparent_target().format(),
                );
                handle
            });
        if let Some(resources) =
            SceneShadowGraphResources::new(shadow_view.enabled(), depth_handle, transparent_handle)
        {
            let key =
                SceneShadowGraphResources::blackboard_key(scene_view.shadow_binding().unwrap_or(0));
            ctx.blackboard_set(key, resources);
        }
        if !shadow_view.should_update_cascade(cascade_index) {
            return;
        }
        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.set_depth_stencil_loaded(depth_handle);
        });
        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.write_color_loaded(0, transparent_handle);
            setup.set_depth_stencil_loaded(depth_handle);
        });
    }

    fn execute(
        &mut self,
        ctx: &mut PhaseExecuteContext<'_, '_, '_>,
    ) -> Result<(), RenderGraphError> {
        let (gpu, pass, resources, execution, draw_services) = ctx.split();
        let (draw_functions, material_registry, mesh_registry, _fallback_texture) =
            draw_services.split();
        let draw_functions = &*draw_functions;
        let material_registry = &*material_registry;
        let shadow_pass_layout = execution
            .frame_payload::<ShadowPassBindingLayout>()
            .ok_or_else(|| {
                RenderGraphError::ExecutionFailed("missing shadow pass layout payload".into())
            })?;
        let shadow_view = execution
            .view_payload::<ShadowViewBinding>()
            .ok_or_else(|| {
                RenderGraphError::ExecutionFailed("missing shadow view payload".into())
            })?;
        if !shadow_view.enabled() {
            return Ok(());
        }
        let scene_view = execution
            .view_payload::<SceneView>()
            .ok_or_else(|| RenderGraphError::ExecutionFailed("missing SceneView payload".into()))?;
        if !scene_view.is_shadow() {
            return Ok(());
        }
        if !shadow_view.should_update_cascade(scene_view.shadow_cascade()) {
            return Ok(());
        }
        let opaque_phase = execution.view_payload::<OpaquePhase>().ok_or_else(|| {
            RenderGraphError::ExecutionFailed("missing opaque phase payload".into())
        })?;
        let model_matrices = execution.frame_payload::<Vec<[f32; 16]>>().ok_or_else(|| {
            RenderGraphError::ExecutionFailed("missing model matrix payload".into())
        })?;
        let depth_handle = pass
            .depth_stencil
            .as_ref()
            .ok_or_else(|| {
                RenderGraphError::ExecutionFailed(
                    "directional shadow pass missing depth target".into(),
                )
            })?
            .handle;

        let device = gpu.device().clone();
        let standard_material_layout = material_registry
            .pipeline_cache()
            .layout::<StandardMaterial>()
            .cloned();
        let cascade_index = scene_view.shadow_cascade();
        let rect = shadow_view.atlas_layout().cascade_rect(cascade_index);
        if !pass.color_outputs.is_empty() {
            let transparent_phase =
                execution
                    .view_payload::<TransparentPhase>()
                    .ok_or_else(|| {
                        RenderGraphError::ExecutionFailed(
                            "missing transparent phase payload".into(),
                        )
                    })?;
            let color_handle = match pass.color_outputs[0].target {
                ResourceRef::Texture(handle) => handle,
                _ => {
                    return Err(RenderGraphError::ExecutionFailed(
                        "directional transparent shadow pass target must be a texture".into(),
                    ));
                }
            };
            let color_load = match pass.color_outputs[0].load {
                LoadOp::Clear(color) => wgpu::LoadOp::Clear(wgpu::Color {
                    r: color[0] as f64,
                    g: color[1] as f64,
                    b: color[2] as f64,
                    a: color[3] as f64,
                }),
                LoadOp::Load => wgpu::LoadOp::Load,
                LoadOp::DontCare => wgpu::LoadOp::Load,
            };
            let depth_load = pass
                .depth_stencil
                .as_ref()
                .and_then(|depth| depth.clear_depth)
                .map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear);
            let depth_store = pass
                .depth_stencil
                .as_ref()
                .is_none_or(|depth| depth.depth_store);
            let color_attachments = [Some(wgpu::RenderPassColorAttachment {
                view: resources.view(color_handle),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: color_load,
                    store: wgpu::StoreOp::Store,
                },
            })];
            let mut frame = gpu.frame();
            let mut render_pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("directional_transparent_shadow"),
                color_attachments: &color_attachments,
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: resources.view(depth_handle),
                    depth_ops: Some(wgpu::Operations {
                        load: depth_load,
                        store: if depth_store {
                            wgpu::StoreOp::Store
                        } else {
                            wgpu::StoreOp::Discard
                        },
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            render_pass.set_viewport(
                rect.x as f32,
                rect.y as f32,
                rect.width as f32,
                rect.height as f32,
                0.0,
                1.0,
            );
            render_pass.set_scissor_rect(rect.x, rect.y, rect.width, rect.height);
            {
                let clear_pipeline = self.transparent_clear_pipeline_for(&device);
                render_pass.set_pipeline(clear_pipeline);
                render_pass.draw(0..3, 0..1);
            }
            if !has_transparent_mesh_shadow_candidates(transparent_phase.items(), draw_functions) {
                return Ok(());
            }

            let material_layout = standard_material_layout.as_ref().ok_or_else(|| {
                RenderGraphError::ExecutionFailed(
                    "transparent shadow caster requires registered StandardMaterial layout".into(),
                )
            })?;
            if !material_registry.is_registered::<StandardMaterial>() {
                return Err(RenderGraphError::ExecutionFailed(
                    "transparent shadow caster requires registered StandardMaterial".into(),
                ));
            }
            let mut bind_group_keepalive = Vec::new();
            let mut cursor = 0usize;
            while cursor < transparent_phase.items().len() {
                let base_item = &transparent_phase.items()[cursor];
                if !base_item.has_payload::<MeshDrawData>() {
                    cursor += 1;
                    continue;
                }
                let base = *base_item.data::<MeshDrawData>();
                let mesh_handle = base.mesh_handle();
                let sub_mesh_index = base.sub_mesh_index();
                let base_draw_function = base_item.draw_function_id;
                let base_material = transparent_shadow_material_handle(
                    base_item,
                    draw_functions,
                    material_registry,
                );
                let mut batch_end = cursor + 1;
                while batch_end < transparent_phase.items().len() {
                    let next_item = &transparent_phase.items()[batch_end];
                    if !next_item.has_payload::<MeshDrawData>() {
                        break;
                    }
                    let next = *next_item.data::<MeshDrawData>();
                    if next.mesh_handle() != mesh_handle
                        || next.sub_mesh_index() != sub_mesh_index
                        || next_item.draw_function_id != base_draw_function
                        || transparent_shadow_material_handle(
                            next_item,
                            draw_functions,
                            material_registry,
                        ) != base_material
                    {
                        break;
                    }
                    batch_end += 1;
                }

                let Some(material_handle) = base_material else {
                    cursor = batch_end;
                    continue;
                };
                if material_registry
                    .get_erased::<StandardMaterial>(material_handle)
                    .is_err()
                {
                    cursor = batch_end;
                    continue;
                }
                let Some(mesh) = mesh_registry.get(mesh_handle) else {
                    cursor = batch_end;
                    continue;
                };
                let pipeline = self.pipeline_for(
                    &device,
                    shadow_pass_layout.bind_group_layout(),
                    Some(material_layout),
                    mesh.vertex_layout(),
                    shadow_view.raster_bias(),
                    ShadowPipelineKind::Transparent,
                )?;
                bind_group_keepalive.push(
                    material_registry
                        .prepared(material_handle)
                        .map_err(shadow_pipeline_material_error)?
                        .bind_group()
                        .clone(),
                );
                let instances: Vec<[f32; 16]> = transparent_phase.items()[cursor..batch_end]
                    .iter()
                    .map(|item| {
                        let mesh_data = *item.data::<MeshDrawData>();
                        model_matrices
                            .get(mesh_data.model_slot() as usize)
                            .copied()
                            .unwrap_or(IDENTITY_MATRIX)
                    })
                    .collect();
                let instance_buffer =
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("directional_transparent_shadow_instances"),
                        contents: bytemuck::cast_slice(&instances),
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    });

                render_pass.set_pipeline(pipeline);
                render_pass.set_bind_group(
                    0,
                    shadow_view.shadow_pass_bind_group(scene_view.shadow_cascade()),
                    &[],
                );
                render_pass.set_bind_group(
                    1,
                    bind_group_keepalive
                        .last()
                        .expect("transparent shadow caster should cache material bind group"),
                    &[],
                );
                render_pass.set_vertex_buffer(0, mesh.vertex_buffer().slice(..));
                render_pass.set_vertex_buffer(1, instance_buffer.slice(..));

                let Some(sub_mesh) = mesh.sub_meshes().get(sub_mesh_index as usize) else {
                    cursor = batch_end;
                    continue;
                };
                if mesh.has_indices() {
                    let Some(index_buffer) = mesh.index_buffer() else {
                        cursor = batch_end;
                        continue;
                    };
                    let index_count = if sub_mesh.index_count == 0 {
                        mesh.index_count()
                    } else {
                        sub_mesh.index_count
                    };
                    let index_offset = if sub_mesh.index_count == 0 {
                        0
                    } else {
                        sub_mesh.index_offset
                    };
                    render_pass.set_index_buffer(
                        index_buffer.slice(..),
                        mesh.index_format()
                            .expect("indexed meshes provide an index format"),
                    );
                    render_pass.draw_indexed(
                        index_offset..(index_offset + index_count),
                        sub_mesh.vertex_offset,
                        0..instances.len() as u32,
                    );
                } else {
                    render_pass.draw(0..mesh.vertex_count(), 0..instances.len() as u32);
                }

                cursor = batch_end;
            }

            return Ok(());
        }
        let mut frame = gpu.frame();
        let depth_load = pass
            .depth_stencil
            .as_ref()
            .and_then(|depth| depth.clear_depth)
            .map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear);
        let depth_store = pass
            .depth_stencil
            .as_ref()
            .is_none_or(|depth| depth.depth_store);
        let mut render_pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("directional_shadow"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: resources.view(depth_handle),
                depth_ops: Some(wgpu::Operations {
                    load: depth_load,
                    store: if depth_store {
                        wgpu::StoreOp::Store
                    } else {
                        wgpu::StoreOp::Discard
                    },
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        render_pass.set_viewport(
            rect.x as f32,
            rect.y as f32,
            rect.width as f32,
            rect.height as f32,
            0.0,
            1.0,
        );
        render_pass.set_scissor_rect(rect.x, rect.y, rect.width, rect.height);
        {
            let clear_pipeline = self.clear_pipeline_for(&device);
            render_pass.set_pipeline(clear_pipeline);
            render_pass.draw(0..3, 0..1);
        }
        let mut bind_group_keepalive = Vec::new();
        let mut cursor = 0usize;
        while cursor < opaque_phase.items().len() {
            let base_item = &opaque_phase.items()[cursor];
            if !base_item.has_payload::<MeshDrawData>() {
                cursor += 1;
                continue;
            }
            let base = *base_item.data::<MeshDrawData>();
            let mesh_handle = base.mesh_handle();
            let sub_mesh_index = base.sub_mesh_index();
            let caster_kind = shadow_caster_kind(base_item, draw_functions, material_registry);
            let mut batch_end = cursor + 1;
            while batch_end < opaque_phase.items().len() {
                let next_item = &opaque_phase.items()[batch_end];
                if !next_item.has_payload::<MeshDrawData>() {
                    break;
                }
                let next = *next_item.data::<MeshDrawData>();
                if next.mesh_handle() != mesh_handle
                    || next.sub_mesh_index() != sub_mesh_index
                    || shadow_caster_kind(next_item, draw_functions, material_registry)
                        != caster_kind
                {
                    break;
                }
                batch_end += 1;
            }

            let Some(mesh) = mesh_registry.get(mesh_handle) else {
                cursor = batch_end;
                continue;
            };
            let material_layout = match caster_kind {
                ShadowCasterKind::Opaque => None,
                ShadowCasterKind::AlphaTest(_) => {
                    Some(standard_material_layout.as_ref().ok_or_else(|| {
                        RenderGraphError::ExecutionFailed(
                            "alpha-test shadow caster requires registered StandardMaterial layout"
                                .into(),
                        )
                    })?)
                }
            };
            let pipeline = self.pipeline_for(
                &device,
                shadow_pass_layout.bind_group_layout(),
                material_layout,
                mesh.vertex_layout(),
                shadow_view.raster_bias(),
                caster_kind.pipeline_kind(),
            )?;
            if let ShadowCasterKind::AlphaTest(material_handle) = caster_kind {
                let _ = material_registry
                    .get_erased::<StandardMaterial>(material_handle)
                    .map_err(|_| {
                        RenderGraphError::ExecutionFailed(
                            "alpha-test shadow caster material handle no longer resolves".into(),
                        )
                    })?;
                bind_group_keepalive.push(
                    material_registry
                        .prepared(material_handle)
                        .map_err(shadow_pipeline_material_error)?
                        .bind_group()
                        .clone(),
                );
            }
            let instances: Vec<[f32; 16]> = opaque_phase.items()[cursor..batch_end]
                .iter()
                .map(|item| {
                    let mesh_data = *item.data::<MeshDrawData>();
                    model_matrices
                        .get(mesh_data.model_slot() as usize)
                        .copied()
                        .unwrap_or(IDENTITY_MATRIX)
                })
                .collect();
            let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("directional_shadow_instances"),
                contents: bytemuck::cast_slice(&instances),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });

            render_pass.set_pipeline(pipeline);
            render_pass.set_bind_group(
                0,
                shadow_view.shadow_pass_bind_group(scene_view.shadow_cascade()),
                &[],
            );
            if matches!(caster_kind, ShadowCasterKind::AlphaTest(_)) {
                render_pass.set_bind_group(
                    1,
                    bind_group_keepalive
                        .last()
                        .expect("alpha-test shadow caster should cache material bind group"),
                    &[],
                );
            }
            render_pass.set_vertex_buffer(0, mesh.vertex_buffer().slice(..));
            render_pass.set_vertex_buffer(1, instance_buffer.slice(..));

            let Some(sub_mesh) = mesh.sub_meshes().get(sub_mesh_index as usize) else {
                cursor = batch_end;
                continue;
            };
            if mesh.has_indices() {
                let Some(index_buffer) = mesh.index_buffer() else {
                    cursor = batch_end;
                    continue;
                };
                let index_count = if sub_mesh.index_count == 0 {
                    mesh.index_count()
                } else {
                    sub_mesh.index_count
                };
                let index_offset = if sub_mesh.index_count == 0 {
                    0
                } else {
                    sub_mesh.index_offset
                };
                render_pass.set_index_buffer(
                    index_buffer.slice(..),
                    mesh.index_format()
                        .expect("indexed meshes provide an index format"),
                );
                render_pass.draw_indexed(
                    index_offset..(index_offset + index_count),
                    sub_mesh.vertex_offset,
                    0..instances.len() as u32,
                );
            } else {
                render_pass.draw(0..mesh.vertex_count(), 0..instances.len() as u32);
            }

            cursor = batch_end;
        }

        Ok(())
    }

    fn draw_calls(&self, execution: &crate::render::execution::ViewExecutionContext<'_>) -> usize {
        if !execution
            .view_payload::<SceneView>()
            .is_some_and(SceneView::is_shadow)
        {
            return 0;
        }
        let Some(shadow_view) = execution.view_payload::<ShadowViewBinding>() else {
            return 0;
        };
        if !shadow_view.enabled() {
            return 0;
        }
        let Some(scene_view) = execution.view_payload::<SceneView>() else {
            return 0;
        };
        if !shadow_view.should_update_cascade(scene_view.shadow_cascade()) {
            return 0;
        }

        let opaque_draws = execution
            .view_payload::<OpaquePhase>()
            .map_or(0, |phase| count_phase_shadow_batches(phase.items()));
        let transparent_draws = execution
            .view_payload::<TransparentPhase>()
            .map_or(0, |phase| count_phase_shadow_batches(phase.items()));
        opaque_draws + transparent_draws
    }
}
