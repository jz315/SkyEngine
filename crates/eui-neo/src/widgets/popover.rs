//! Root-layer popup composition helper.

use super::super::{LayoutRect, Response, Size, Ui};
use super::layout::WidgetLayout;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PopoverPlacement {
    #[default]
    BottomStart,
    BottomEnd,
    TopStart,
    TopEnd,
    RightStart,
    RightEnd,
    LeftStart,
    LeftEnd,
    Center,
}

pub struct PopoverBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    open: bool,
    anchor: Option<String>,
    fallback_anchor: Option<LayoutRect>,
    placement: PopoverPlacement,
    layout: WidgetLayout,
    offset: [f32; 2],
    gap: f32,
    z_index: i32,
}

impl<'ui> PopoverBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            open: true,
            anchor: None,
            fallback_anchor: None,
            placement: PopoverPlacement::BottomStart,
            layout: WidgetLayout::new(240.0, 160.0),
            offset: [0.0, 0.0],
            gap: 8.0,
            z_index: 100,
        }
    }

    pub fn open(mut self, value: bool) -> Self {
        self.open = value;
        self
    }

    pub fn anchor(mut self, id: impl Into<String>) -> Self {
        self.anchor = Some(id.into());
        self
    }

    pub fn fallback_anchor(mut self, rect: LayoutRect) -> Self {
        self.fallback_anchor = Some(rect);
        self
    }

    pub fn placement(mut self, value: PopoverPlacement) -> Self {
        self.placement = value;
        self
    }

    pub fn size(mut self, width: impl Into<Size>, height: impl Into<Size>) -> Self {
        self.layout = self.layout.size(width, height);
        self
    }

    pub fn width(mut self, value: impl Into<Size>) -> Self {
        self.layout = self.layout.width(value);
        self
    }

    pub fn height(mut self, value: impl Into<Size>) -> Self {
        self.layout = self.layout.height(value);
        self
    }

    pub fn min_width(mut self, value: f32) -> Self {
        self.layout = self.layout.min_width(value);
        self
    }

    pub fn max_width(mut self, value: f32) -> Self {
        self.layout = self.layout.max_width(value);
        self
    }

    pub fn min_height(mut self, value: f32) -> Self {
        self.layout = self.layout.min_height(value);
        self
    }

    pub fn max_height(mut self, value: f32) -> Self {
        self.layout = self.layout.max_height(value);
        self
    }

    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset = [x, y];
        self
    }

    pub fn gap(mut self, value: f32) -> Self {
        self.gap = value.max(0.0);
        self
    }

    pub fn z_index(mut self, value: i32) -> Self {
        self.z_index = value;
        self
    }

    pub fn z(self, value: i32) -> Self {
        self.z_index(value)
    }

    pub fn content(self, content: impl FnOnce(&mut Ui)) -> Response {
        let id = self.id.clone();
        if !self.open {
            return self.ui.response(&id);
        }

        let anchor = self
            .anchor
            .as_deref()
            .and_then(|id| self.ui.previous_frame(id))
            .or(self.fallback_anchor)
            .unwrap_or(LayoutRect::ZERO);
        let width = self.layout.fixed_width_or(anchor.width.max(1.0));
        let height = self.layout.fixed_height_or(1.0);
        let [x, y] = popover_position(anchor, width, height, self.placement, self.gap, self.offset);

        self.ui.with_root_layer(|ui| {
            self.layout
                .apply_to_size(ui.stack(id.clone()), self.layout.width, self.layout.height)
                .position(x, y)
                .z_index(self.z_index)
                .content(content)
        })
    }
}

pub fn popover(ui: &mut Ui, id: impl Into<String>) -> PopoverBuilder<'_> {
    PopoverBuilder::new(ui, id)
}

fn popover_position(
    anchor: LayoutRect,
    width: f32,
    height: f32,
    placement: PopoverPlacement,
    gap: f32,
    offset: [f32; 2],
) -> [f32; 2] {
    let [mut x, mut y] = match placement {
        PopoverPlacement::BottomStart => [anchor.x, anchor.bottom() + gap],
        PopoverPlacement::BottomEnd => [anchor.right() - width, anchor.bottom() + gap],
        PopoverPlacement::TopStart => [anchor.x, anchor.y - height - gap],
        PopoverPlacement::TopEnd => [anchor.right() - width, anchor.y - height - gap],
        PopoverPlacement::RightStart => [anchor.right() + gap, anchor.y],
        PopoverPlacement::RightEnd => [anchor.right() + gap, anchor.bottom() - height],
        PopoverPlacement::LeftStart => [anchor.x - width - gap, anchor.y],
        PopoverPlacement::LeftEnd => [anchor.x - width - gap, anchor.bottom() - height],
        PopoverPlacement::Center => [
            anchor.x + (anchor.width - width) * 0.5,
            anchor.y + (anchor.height - height) * 0.5,
        ],
    };
    x += offset[0];
    y += offset[1];
    [x, y]
}

#[cfg(test)]
mod tests {
    use super::{popover_position, PopoverPlacement};
    use crate::LayoutRect;

    #[test]
    fn bottom_start_positions_below_anchor() {
        assert_eq!(
            popover_position(
                LayoutRect::new(10.0, 20.0, 100.0, 30.0),
                80.0,
                60.0,
                PopoverPlacement::BottomStart,
                8.0,
                [2.0, 3.0],
            ),
            [12.0, 61.0],
        );
    }
}
