use crate::render::execution::{PreparedFrame, PreparedView};
use crate::render::features::lighting::renderer::collect_gpu_lights_into;
use crate::render::lighting::shadow::{
    append_directional_shadow_views_into, create_shadow_compare_sampler, sync_shadow_views,
    DirectionalShadowSetup, DirectionalShadowViewScratch, ShadowDebugResources,
    ShadowPassBindingLayout, ShadowSceneBindingLayout, ShadowViewBinding,
};
use crate::render::phase::{MeshDrawData, OpaquePhase, PhaseItem, TransparentPhase};
use crate::render::resources::{SceneShadowResources, ShadowResourceKind};
use crate::render::runtime::{
    FrameExtension, FrameExtensionError, FrameExtensionInitContext, FrameExtensionPrepareContext,
    FrameExtensionUploadContext, FrameExtensionViewContext,
};
use crate::render::view::{RenderStats, SceneView};
use crate::render::LightTable;

/// Lighting-owned GPU and per-frame shadow state.
#[derive(Default)]
pub(crate) struct ShadowFrameExtension {
    layout: Option<ShadowSceneBindingLayout>,
    pass_layout: Option<ShadowPassBindingLayout>,
    compare_sampler: Option<wgpu::Sampler>,
    views: Vec<ShadowViewBinding>,
    scene_resources: Vec<SceneShadowResources>,
    setups: Vec<DirectionalShadowSetup>,
    view_scratch: DirectionalShadowViewScratch,
    summary: ShadowFrameSummary,
}

impl FrameExtension for ShadowFrameExtension {
    #[cfg(test)]
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn initialize(&mut self, ctx: FrameExtensionInitContext<'_>) {
        if self.layout.is_none() {
            self.layout = Some(ShadowSceneBindingLayout::new(ctx.gpu.device()));
        }
        if self.pass_layout.is_none() {
            self.pass_layout = Some(ShadowPassBindingLayout::new(ctx.gpu.device()));
        }
        if self.compare_sampler.is_none() {
            self.compare_sampler = Some(create_shadow_compare_sampler(ctx.gpu.device()));
        }
    }

    fn collect_views(&mut self, ctx: FrameExtensionViewContext<'_>) {
        append_directional_shadow_views_into(
            ctx.world,
            ctx.views,
            &mut self.setups,
            &mut self.view_scratch,
        );
    }

    fn upload_scene(&mut self, ctx: FrameExtensionUploadContext<'_>) {
        collect_gpu_lights_into(ctx.world, ctx.transforms, &mut ctx.upload.lights);
    }

    fn prepare(
        &mut self,
        ctx: FrameExtensionPrepareContext<'_>,
    ) -> Result<(), FrameExtensionError> {
        sync_shadow_views(
            &mut self.views,
            ctx.gpu,
            &ctx.extracted.views,
            &ctx.extracted.opaque_phases,
            &ctx.extracted.transparent_phases,
            &ctx.uploads.model_matrices,
            &self.setups,
            self.layout
                .as_ref()
                .expect("shadow extension must initialize a shared shadow layout"),
            self.pass_layout
                .as_ref()
                .expect("shadow extension must initialize a shared shadow-pass layout"),
            self.compare_sampler
                .as_ref()
                .expect("shadow extension must initialize a shared shadow sampler"),
            ctx.runtime
                .gpu_scene
                .as_ref()
                .expect("core runtime must initialize a GpuScene")
                .table::<LightTable>(),
            ctx.runtime.frame_settings.debug_view,
        );
        self.summary = summarize_shadow_frame(
            &ctx.extracted.views,
            &ctx.extracted.opaque_phases,
            &ctx.extracted.transparent_phases,
            &self.views,
        );
        self.scene_resources.clear();
        let layout = self
            .layout
            .as_ref()
            .expect("shadow extension must initialize before frame preparation");
        self.scene_resources
            .extend(self.views.iter().map(|binding| {
                SceneShadowResources::from_bind_group(
                    ShadowResourceKind::DirectionalCascades,
                    layout.bind_group_layout(),
                    binding.bind_group(),
                )
            }));
        Ok(())
    }

    fn insert_frame_payloads<'a>(&'a self, frame: &mut PreparedFrame<'a>) {
        let _ = frame.insert_payload(
            self.layout
                .as_ref()
                .expect("shadow extension must initialize before frame assembly"),
        );
        let _ = frame.insert_payload(
            self.pass_layout
                .as_ref()
                .expect("shadow extension must initialize before frame assembly"),
        );
        if let Some(debug_resources) = self.summary.debug_resources.as_ref() {
            let _ = frame.insert_payload(debug_resources);
        }
    }

    fn insert_view_payloads<'a>(
        &'a self,
        _view_index: usize,
        view: &SceneView,
        prepared_view: &mut PreparedView<'a>,
    ) {
        if let Some(index) = view.shadow_binding() {
            let Some(binding) = self.views.get(index) else {
                return;
            };
            let _ = prepared_view.insert_payload(binding);
            if let Some(resources) = self.scene_resources.get(index) {
                let _ = prepared_view.insert_payload(resources);
            }
        }
    }

    fn update_render_stats(&self, stats: &mut RenderStats) {
        stats.shadow_cascade_count = self.summary.stats.cascade_count;
        stats.shadow_caster_count = self.summary.stats.caster_count;
        stats.shadow_caster_count_by_cascade = self.summary.stats.caster_count_by_cascade;
        stats.shadow_draw_calls = self.summary.draw_calls;
        stats.shadow_draw_calls_by_cascade = self.summary.draw_calls_by_cascade;
        stats.shadow_atlas_width = self.summary.stats.atlas_size[0];
        stats.shadow_atlas_height = self.summary.stats.atlas_size[1];
        stats.shadow_atlas_rect_count = self.summary.stats.rect_count;
        stats.shadow_atlas_used_pixel_ratio = self.summary.stats.used_pixel_ratio;
        stats.shadow_atlas_guard_band_texels = self.summary.stats.guard_band_texels;
    }

    fn invalidate(&mut self) {
        self.views.clear();
        self.scene_resources.clear();
        self.setups.clear();
        self.summary = ShadowFrameSummary::default();
    }
}

impl ShadowFrameExtension {
    #[cfg(test)]
    #[inline]
    pub(crate) fn views(&self) -> &[ShadowViewBinding] {
        &self.views
    }
}

fn summarize_shadow_frame(
    views: &[SceneView],
    opaque_phases: &[OpaquePhase],
    transparent_phases: &[TransparentPhase],
    shadow_views: &[ShadowViewBinding],
) -> ShadowFrameSummary {
    ShadowFrameSummary {
        debug_resources: shadow_views.iter().find_map(|shadow| {
            shadow.enabled().then(|| {
                ShadowDebugResources::from_directional_atlas(
                    shadow.target(),
                    shadow.cascade_count(),
                    shadow.atlas_layout().shadow_atlas_mul_add(),
                )
            })
        }),
        stats: collect_shadow_stats(shadow_views),
        draw_calls: count_shadow_draw_calls(views, opaque_phases, transparent_phases, shadow_views),
        draw_calls_by_cascade: count_shadow_draw_calls_by_cascade(
            views,
            opaque_phases,
            transparent_phases,
            shadow_views,
        ),
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct ShadowFrameStats {
    cascade_count: usize,
    caster_count: usize,
    caster_count_by_cascade:
        [usize; crate::render::core::resources::MAX_DIRECTIONAL_SHADOW_CASCADES],
    atlas_size: [u32; 2],
    rect_count: usize,
    used_pixel_ratio: f32,
    guard_band_texels: f32,
}

#[derive(Default)]
struct ShadowFrameSummary {
    debug_resources: Option<ShadowDebugResources>,
    stats: ShadowFrameStats,
    draw_calls: usize,
    draw_calls_by_cascade: [usize; crate::render::core::resources::MAX_DIRECTIONAL_SHADOW_CASCADES],
}

fn collect_shadow_stats(shadow_views: &[ShadowViewBinding]) -> ShadowFrameStats {
    let mut stats = ShadowFrameStats::default();
    let mut atlas_area = 0u64;
    let mut used_area = 0f64;

    for shadow_view in shadow_views.iter().filter(|shadow| shadow.enabled()) {
        let atlas = shadow_view.atlas_stats();
        stats.cascade_count += atlas.active_cascade_count as usize;
        stats.caster_count += shadow_view.caster_count();
        for (index, count) in shadow_view
            .caster_count_by_cascade()
            .into_iter()
            .enumerate()
        {
            stats.caster_count_by_cascade[index] += count;
        }
        stats.rect_count += atlas.used_rect_count as usize;
        stats.atlas_size[0] = stats.atlas_size[0].max(atlas.atlas_size[0]);
        stats.atlas_size[1] = stats.atlas_size[1].max(atlas.atlas_size[1]);
        stats.guard_band_texels = stats.guard_band_texels.max(atlas.guard_band_texels);
        let area = atlas.atlas_size[0] as u64 * atlas.atlas_size[1] as u64;
        atlas_area += area;
        used_area += atlas.used_pixel_ratio as f64 * area as f64;
    }

    if atlas_area > 0 {
        stats.used_pixel_ratio = (used_area / atlas_area as f64) as f32;
    }

    stats
}

fn count_shadow_draw_calls(
    views: &[SceneView],
    opaque_phases: &[OpaquePhase],
    transparent_phases: &[TransparentPhase],
    shadow_views: &[ShadowViewBinding],
) -> usize {
    views
        .iter()
        .enumerate()
        .filter(|(_, view)| view.is_shadow())
        .filter(|(_, view)| {
            view.shadow_binding()
                .and_then(|binding| shadow_views.get(binding))
                .is_some_and(|shadow| {
                    shadow.enabled() && shadow.should_update_cascade(view.shadow_cascade())
                })
        })
        .map(|(index, _)| {
            opaque_phases
                .get(index)
                .map_or(0, |phase| count_shadow_batches(phase.items()))
                + transparent_phases
                    .get(index)
                    .map_or(0, |phase| count_shadow_batches(phase.items()))
        })
        .sum()
}

fn count_shadow_draw_calls_by_cascade(
    views: &[SceneView],
    opaque_phases: &[OpaquePhase],
    transparent_phases: &[TransparentPhase],
    shadow_views: &[ShadowViewBinding],
) -> [usize; crate::render::core::resources::MAX_DIRECTIONAL_SHADOW_CASCADES] {
    let mut draw_calls = [0usize; crate::render::core::resources::MAX_DIRECTIONAL_SHADOW_CASCADES];
    for (view_index, view) in views
        .iter()
        .enumerate()
        .filter(|(_, view)| view.is_shadow())
    {
        let cascade = view.shadow_cascade() as usize;
        if cascade >= draw_calls.len() {
            continue;
        }
        let Some(shadow_view) = view
            .shadow_binding()
            .and_then(|binding| shadow_views.get(binding))
        else {
            continue;
        };
        if !shadow_view.enabled() || !shadow_view.should_update_cascade(view.shadow_cascade()) {
            continue;
        }
        draw_calls[cascade] += opaque_phases
            .get(view_index)
            .map_or(0, |phase| count_shadow_batches(phase.items()))
            + transparent_phases
                .get(view_index)
                .map_or(0, |phase| count_shadow_batches(phase.items()));
    }
    draw_calls
}

fn count_shadow_batches(items: &[PhaseItem]) -> usize {
    let mut draws = 0usize;
    let mut cursor = 0usize;
    while cursor < items.len() {
        if !items[cursor].has_payload::<MeshDrawData>() {
            cursor += 1;
            continue;
        }
        let base = *items[cursor].data::<MeshDrawData>();
        let base_draw_function = items[cursor].draw_function_id;
        let base_material = base.material_handle::<crate::render::StandardMaterial>();
        let mut batch_end = cursor + 1;
        while batch_end < items.len() {
            if !items[batch_end].has_payload::<MeshDrawData>() {
                break;
            }
            let next = *items[batch_end].data::<MeshDrawData>();
            let next_draw_function = items[batch_end].draw_function_id;
            if next.mesh_handle() != base.mesh_handle()
                || next.sub_mesh_index() != base.sub_mesh_index()
                || next_draw_function != base_draw_function
                || next.material_handle::<crate::render::StandardMaterial>() != base_material
            {
                break;
            }
            batch_end += 1;
        }
        draws += 1;
        cursor = batch_end;
    }
    draws
}
