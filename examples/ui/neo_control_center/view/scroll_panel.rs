use sky_engine::ui::neo::widgets;
use sky_engine::ui::neo::{Binding, Ui};

pub fn scroll_panel<T: 'static>(
    ui: &mut Ui,
    id: &str,
    width: f32,
    height: f32,
    content_height: f32,
    scroll_offset: f32,
    scroll: Binding<T, f32>,
    content: impl FnOnce(&mut Ui, f32),
) {
    let max_scroll = (content_height - height).max(0.0);
    let offset = scroll_offset.clamp(0.0, max_scroll);
    let scrollable = max_scroll > 0.0;

    let bar_w = if scrollable { 8.0 } else { 0.0 };
    let bar_gap = if scrollable { 16.0 } else { 0.0 };
    let body_w = (width - bar_w - bar_gap).max(0.0);

    let viewport = ui
        .stack(format!("{id}.viewport"))
        .size(width, height)
        .clip();
    let viewport = if scrollable {
        let scroll = scroll.clone();
        viewport.on_scroll(move |event| {
            let next = (offset - event.y * 48.0).clamp(0.0, max_scroll);
            scroll.set(next);
        })
    } else {
        viewport
    };

    viewport.content(|ui| {
        ui.column(format!("{id}.content"))
            .y(-offset)
            .size(body_w, content_height)
            .content(|ui| {
                content(ui, body_w);
            });

        if scrollable {
            widgets::scrollbar(ui, format!("{id}.scrollbar"))
                .x((width - bar_w).max(0.0))
                .size(bar_w, height)
                .viewport(height)
                .content(content_height)
                .offset_bind(scroll)
                .build();
        }
    });
}
