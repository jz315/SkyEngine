use sky_engine::ui::serein::{Signal, Size, Ui};

pub fn scroll_panel<T: 'static>(
    ui: &mut Ui,
    id: &str,
    width: f32,
    height: f32,
    content_height: f32,
    scroll_offset: f32,
    scroll: Signal<T, f32>,
    content: impl FnOnce(&mut Ui, f32),
) {
    let body_w = (width - 24.0).max(0.0);
    ui.scroll_y(id)
        .size(width, height)
        .content_height(content_height)
        .offset(scroll_offset)
        .scrollbar_width(8.0)
        .scrollbar_gap(16.0)
        .on_change(move |next| scroll.set(next))
        .content(|ui| {
            ui.stack(format!("{id}.content.slot"))
                .size(Size::fill(), content_height)
                .content(|ui| content(ui, body_w));
        });
}
