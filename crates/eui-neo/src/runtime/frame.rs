use super::invalidation::NormalizedDirtyInput;
use super::*;

impl Runtime {
    pub fn current_frame(&self) -> Frame {
        Frame {
            screen: self.tree.screen,
            draw_list: self.draw_list(),
            needs_render: self.render.needs_render,
            needs_compose: self.render.needs_compose,
            full_redraw: self.render.full_redraw,
            focused_ime_rect: self.focused_ime_rect(),
        }
    }

    pub fn frame<R>(
        &mut self,
        input: FrameInput,
        compose: impl FnOnce(&mut Ui, Screen) -> R,
    ) -> FrameResult<R> {
        self.run_frame(input, None::<fn() -> Vec<DirtyInput>>, compose)
    }

    pub fn frame_incremental<R>(
        &mut self,
        input: FrameInput,
        dirty: impl FnOnce() -> Vec<DirtyInput>,
        compose: impl FnOnce(&mut Ui, Screen) -> R,
    ) -> FrameResult<R> {
        self.run_frame(input, Some(dirty), compose)
    }

    fn run_frame<R>(
        &mut self,
        input: FrameInput,
        dirty_after_input: Option<impl FnOnce() -> Vec<DirtyInput>>,
        compose: impl FnOnce(&mut Ui, Screen) -> R,
    ) -> FrameResult<R> {
        let mut frame_pass = FramePass::from_input(input);
        self.run_input_pass(&mut frame_pass);
        frame_pass.collect_dirty(dirty_after_input);

        let mut value = None;
        let should_full_compose = frame_pass.should_full_compose(self);
        if should_full_compose {
            frame_pass.record_dirty_invalidations(self);
            self.run_compose_pass(&frame_pass, None, |ui, screen| {
                value = Some(compose(ui, screen));
            });
        } else {
            let dirty = frame_pass.dirty.take();
            self.run_compose_pass(&frame_pass, dirty, |ui, screen| {
                value = Some(compose(ui, screen));
            });
        }
        self.run_animation_pass(&frame_pass);
        FrameResult {
            value: value.expect("frame compose closure did not run"),
            frame: self.run_output_pass(),
        }
    }

    fn run_input_pass(&mut self, pass: &mut FramePass) {
        let keyboard = std::mem::take(&mut pass.keyboard);
        if pass.pointer_events.is_empty() {
            self.update_events_and_timers(pass.pointer, pass.scroll, keyboard, pass.delta_seconds);
        } else {
            self.update_events_and_timers_from_pointer_events(
                &pass.pointer_events,
                pass.scroll,
                keyboard,
                pass.delta_seconds,
            );
        }
    }

    fn run_compose_pass(
        &mut self,
        pass: &FramePass,
        dirty: Option<NormalizedDirtyInput>,
        compose: impl FnOnce(&mut Ui, Screen),
    ) {
        self.compose_tree_with_normalized_dirty(
            pass.screen.width,
            pass.screen.height,
            dirty,
            compose,
        );
    }

    fn run_animation_pass(&mut self, pass: &FramePass) {
        self.tick_animations(pass.delta_seconds);
    }

    fn run_output_pass(&self) -> Frame {
        self.current_frame()
    }
}

struct FramePass {
    screen: Screen,
    delta_seconds: f32,
    pointer: PointerEvent,
    pointer_events: Vec<PointerEvent>,
    scroll: ScrollEvent,
    keyboard: KeyboardEvent,
    dirty: Option<NormalizedDirtyInput>,
    force_full_compose: bool,
}

impl FramePass {
    fn from_input(input: FrameInput) -> Self {
        let FrameInput {
            screen,
            delta_seconds,
            pointer,
            pointer_events,
            scroll,
            keyboard,
            dirty,
            force_full_compose,
        } = input;
        let dirty = NormalizedDirtyInput::from_optional_dirty_inputs(dirty);
        Self {
            screen,
            delta_seconds: delta_seconds.max(0.0),
            pointer,
            pointer_events,
            scroll,
            keyboard,
            dirty,
            force_full_compose,
        }
    }

    fn collect_dirty(&mut self, dirty_after_input: Option<impl FnOnce() -> Vec<DirtyInput>>) {
        self.dirty = dirty_after_input
            .map(|collect| NormalizedDirtyInput::from_dirty_inputs(collect()))
            .or(self.dirty.take());
    }

    fn should_full_compose(&self, runtime: &Runtime) -> bool {
        let dirty_pass_flags = self
            .dirty
            .as_ref()
            .map_or_else(PassFlags::default, |dirty| dirty.pass_flags);
        self.force_full_compose
            || (runtime.needs_compose() && !dirty_pass_flags.request_compose_ui)
            || self.dirty.is_none()
    }

    fn record_dirty_invalidations(&mut self, runtime: &mut Runtime) {
        let Some(dirty) = self.dirty.take() else {
            return;
        };
        for invalidation in dirty.invalidations {
            runtime.record_invalidation(invalidation);
        }
    }
}
