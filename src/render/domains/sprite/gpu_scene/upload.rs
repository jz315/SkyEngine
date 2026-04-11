use crate::gpu::GpuContext;

use super::super::prepared::PreparedRenderWorld2D;
use super::super::scene_cache::SceneCache2D;
use super::records::{
    light_record_for_slot, sprite_record_for_slot, GpuLightRecord, GpuSpriteRecord,
};
use super::GpuScene2D;

impl GpuScene2D {
    pub(crate) fn upload_scene_frame(
        &mut self,
        ctx: &GpuContext,
        scene: &mut SceneCache2D,
        prepared: &PreparedRenderWorld2D,
    ) {
        let rewrite_sprite_table =
            self.ensure_sprite_table_capacity(ctx, scene.sprite_slot_capacity());
        let rewrite_light_table =
            self.ensure_light_table_capacity(ctx, scene.light_slot_capacity());

        self.dirty_sprite_slot_uploads = self.upload_sprite_table(ctx, scene, rewrite_sprite_table);
        self.dirty_light_slot_uploads = self.upload_light_table(ctx, scene, rewrite_light_table);

        self.ensure_visible_sprite_capacity(ctx, prepared.visible_sprite_slots.len());
        self.ensure_visible_light_capacity(ctx, prepared.visible_light_slots.len());

        if !prepared.visible_sprite_slots.is_empty() {
            ctx.queue().write_buffer(
                &self.visible_sprite_index_buffer,
                0,
                bytemuck::cast_slice(&prepared.visible_sprite_slots),
            );
        }
        if !prepared.visible_light_slots.is_empty() {
            ctx.queue().write_buffer(
                &self.visible_light_index_buffer,
                0,
                bytemuck::cast_slice(&prepared.visible_light_slots),
            );
        }

        self.views.clone_from(&prepared.views);
        self.draw_spans.clone_from(&prepared.draw_spans);
        self.textures.clone_from(&prepared.textures);
        self.sprite_count = prepared.sprite_count;
        self.light_count = prepared.light_count;
        self.visible_sprite_upload_count = prepared.visible_sprite_slots.len();
        self.visible_light_upload_count = prepared.visible_light_slots.len();

        scene.clear_dirty_tracking();
    }

    fn upload_sprite_table(
        &mut self,
        ctx: &GpuContext,
        scene: &SceneCache2D,
        rewrite_full_table: bool,
    ) -> usize {
        if rewrite_full_table {
            self.sprite_upload_scratch.clear();
            self.sprite_upload_scratch.extend(
                (0..scene.sprite_slot_capacity()).map(|slot| sprite_record_for_slot(scene, slot)),
            );
            if !self.sprite_upload_scratch.is_empty() {
                ctx.queue().write_buffer(
                    &self.sprite_table_buffer,
                    0,
                    bytemuck::cast_slice(&self.sprite_upload_scratch),
                );
            }
            return self.sprite_upload_scratch.len();
        }

        if scene.dirty_sprite_slots().is_empty() {
            return 0;
        }

        self.sprite_dirty_scratch.clear();
        self.sprite_dirty_scratch
            .extend(scene.dirty_sprite_slots().iter().map(|&slot| slot as usize));
        self.sprite_dirty_scratch.sort_unstable();

        let mut uploaded = 0usize;
        let record_size = std::mem::size_of::<GpuSpriteRecord>() as u64;
        let mut run_start = 0usize;
        while run_start < self.sprite_dirty_scratch.len() {
            let first_slot = self.sprite_dirty_scratch[run_start];
            let mut run_end = run_start + 1;
            while run_end < self.sprite_dirty_scratch.len()
                && self.sprite_dirty_scratch[run_end] == self.sprite_dirty_scratch[run_end - 1] + 1
            {
                run_end += 1;
            }

            self.sprite_upload_scratch.clear();
            self.sprite_upload_scratch.extend(
                self.sprite_dirty_scratch[run_start..run_end]
                    .iter()
                    .map(|&slot| sprite_record_for_slot(scene, slot)),
            );
            ctx.queue().write_buffer(
                &self.sprite_table_buffer,
                first_slot as u64 * record_size,
                bytemuck::cast_slice(&self.sprite_upload_scratch),
            );
            uploaded += run_end - run_start;
            run_start = run_end;
        }

        uploaded
    }

    fn upload_light_table(
        &mut self,
        ctx: &GpuContext,
        scene: &SceneCache2D,
        rewrite_full_table: bool,
    ) -> usize {
        if rewrite_full_table {
            self.light_upload_scratch.clear();
            self.light_upload_scratch.extend(
                (0..scene.light_slot_capacity()).map(|slot| light_record_for_slot(scene, slot)),
            );
            if !self.light_upload_scratch.is_empty() {
                ctx.queue().write_buffer(
                    &self.light_table_buffer,
                    0,
                    bytemuck::cast_slice(&self.light_upload_scratch),
                );
            }
            return self.light_upload_scratch.len();
        }

        if scene.dirty_light_slots().is_empty() {
            return 0;
        }

        self.light_dirty_scratch.clear();
        self.light_dirty_scratch
            .extend(scene.dirty_light_slots().iter().map(|&slot| slot as usize));
        self.light_dirty_scratch.sort_unstable();

        let mut uploaded = 0usize;
        let record_size = std::mem::size_of::<GpuLightRecord>() as u64;
        let mut run_start = 0usize;
        while run_start < self.light_dirty_scratch.len() {
            let first_slot = self.light_dirty_scratch[run_start];
            let mut run_end = run_start + 1;
            while run_end < self.light_dirty_scratch.len()
                && self.light_dirty_scratch[run_end] == self.light_dirty_scratch[run_end - 1] + 1
            {
                run_end += 1;
            }

            self.light_upload_scratch.clear();
            self.light_upload_scratch.extend(
                self.light_dirty_scratch[run_start..run_end]
                    .iter()
                    .map(|&slot| light_record_for_slot(scene, slot)),
            );
            ctx.queue().write_buffer(
                &self.light_table_buffer,
                first_slot as u64 * record_size,
                bytemuck::cast_slice(&self.light_upload_scratch),
            );
            uploaded += run_end - run_start;
            run_start = run_end;
        }

        uploaded
    }
}
