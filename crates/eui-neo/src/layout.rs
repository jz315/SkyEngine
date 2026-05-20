use super::text_measure::{
    measure_text_size_with_system, with_default_text_system, TextMeasure, TextMeasureRequest,
    TextSystem,
};
use super::{Align, Element, ElementKind, LayoutRect, Size};

#[derive(Debug, Clone)]
struct MeasuredNode {
    width: f32,
    height: f32,
    children: Vec<MeasuredNode>,
}

/// Measure one element in logical pixels.
pub fn measure_element(element: &Element, available_width: f32, available_height: f32) -> [f32; 2] {
    with_default_text_system(|text_system| {
        measure_element_with_text_system(element, available_width, available_height, text_system)
    })
}

/// Compute layout frames for all root elements.
pub fn layout_roots(roots: &mut [Element], width: f32, height: f32) {
    with_default_text_system(|text_system| {
        layout_roots_with_text_system(roots, width, height, text_system);
    });
}

pub(crate) fn measure_element_with_text_system(
    element: &Element,
    available_width: f32,
    available_height: f32,
    text_system: &mut dyn TextSystem,
) -> [f32; 2] {
    let measured = measure_node(element, available_width, available_height, text_system);
    [measured.width, measured.height]
}

pub(crate) fn layout_roots_with_text_system(
    roots: &mut [Element],
    width: f32,
    height: f32,
    text_system: &mut dyn TextSystem,
) {
    for root in roots {
        let measured = measure_node(root, width, height, text_system);
        let x = if root.has_x { root.x } else { 0.0 };
        let y = if root.has_y { root.y } else { 0.0 };
        layout_element(root, &measured, x, y, text_system);
    }
}

fn measure_node(
    element: &Element,
    available_width: f32,
    available_height: f32,
    text_system: &mut dyn TextSystem,
) -> MeasuredNode {
    let child_available_width = child_available_width(element, available_width);
    let child_available_height = child_available_height(element, available_height);
    let children: Vec<_> = element
        .children
        .iter()
        .map(|child| {
            measure_node(
                child,
                child_available_width,
                child_available_height,
                text_system,
            )
        })
        .collect();
    let content_width = measure_content_width(element, &children, child_available_width, text_system);
    let content_height = measure_content_height(
        element,
        &children,
        child_available_width,
        child_available_height,
        text_system,
    );
    MeasuredNode {
        width: resolve_width(
            element,
            content_width + element.padding.horizontal(),
            available_width,
        ),
        height: resolve_height(
            element,
            content_height + element.padding.vertical(),
            available_height,
        ),
        children,
    }
}

fn child_available_width(element: &Element, available_width: f32) -> f32 {
    let base = match element.width {
        Size::Fixed(value) => value,
        Size::WrapContent | Size::Fill => available_width,
    };
    positive_subtract(clamp_width(element, base), element.padding.horizontal())
}

fn child_available_height(element: &Element, available_height: f32) -> f32 {
    let base = match element.height {
        Size::Fixed(value) => value,
        Size::WrapContent | Size::Fill => available_height,
    };
    positive_subtract(clamp_height(element, base), element.padding.vertical())
}

fn measure_content_width(
    element: &Element,
    children: &[MeasuredNode],
    available_width: f32,
    text_system: &mut dyn TextSystem,
) -> f32 {
    if children.is_empty() {
        if element.kind == ElementKind::Text {
            return measure_text_leaf(element, available_width, text_system).width;
        }
        return if let Size::Fixed(value) = element.width {
            value
        } else {
            available_width
        };
    }

    if element.kind == ElementKind::Row {
        let mut total = 0.0;
        for (index, (child, measured)) in element.children.iter().zip(children).enumerate() {
            total += outer_width(child, measured);
            if index + 1 < children.len() {
                total += element.spacing;
            }
        }
        return total;
    }

    let mut max_width: f32 = 0.0;
    for (child, measured) in element.children.iter().zip(children) {
        max_width = max_width.max(outer_width(child, measured));
    }
    max_width
}

fn measure_content_height(
    element: &Element,
    children: &[MeasuredNode],
    available_width: f32,
    available_height: f32,
    text_system: &mut dyn TextSystem,
) -> f32 {
    if children.is_empty() {
        if element.kind == ElementKind::Text {
            return measure_text_leaf(element, available_width, text_system).height;
        }
        return if let Size::Fixed(value) = element.height {
            value
        } else {
            available_height
        };
    }

    if element.kind == ElementKind::Column {
        let mut total = 0.0;
        for (index, (child, measured)) in element.children.iter().zip(children).enumerate() {
            total += outer_height(child, measured);
            if index + 1 < children.len() {
                total += element.spacing;
            }
        }
        return total;
    }

    let mut max_height: f32 = 0.0;
    for (child, measured) in element.children.iter().zip(children) {
        max_height = max_height.max(outer_height(child, measured));
    }
    max_height
}

fn measure_text_leaf(
    element: &Element,
    available_width: f32,
    text_system: &mut dyn TextSystem,
) -> TextMeasure {
    let max_width = text_measure_width(element, available_width);
    measure_text_size_with_system(
        text_system,
        TextMeasureRequest {
            text: &element.text,
            font: &element.font,
            font_size: element.font_size,
            font_weight: element.font_weight,
            line_height: element.line_height,
            max_width,
            wrap: element.wrap,
        },
    )
}

fn text_measure_width(element: &Element, available_width: f32) -> f32 {
    match element.width {
        Size::Fixed(value) => value,
        Size::Fill => available_width,
        Size::WrapContent => {
            if element.text_max_width > 0.0 {
                element.text_max_width
            } else if element.max_layout_width > 0.0 {
                element.max_layout_width
            } else if element.wrap {
                available_width
            } else {
                0.0
            }
        }
    }
}

fn resolve_width(element: &Element, content: f32, available: f32) -> f32 {
    let resolved = match element.width {
        Size::Fixed(value) => value,
        Size::WrapContent => content,
        Size::Fill => {
            if available > 0.0 {
                available
            } else {
                content
            }
        }
    };
    clamp_width(element, resolved)
}

fn resolve_height(element: &Element, content: f32, available: f32) -> f32 {
    let resolved = match element.height {
        Size::Fixed(value) => value,
        Size::WrapContent => content,
        Size::Fill => {
            if available > 0.0 {
                available
            } else {
                content
            }
        }
    };
    clamp_height(element, resolved)
}

fn clamp_width(element: &Element, value: f32) -> f32 {
    let min = element.min_width.max(0.0);
    let max = element.max_layout_width.max(0.0);
    if max > 0.0 {
        value.max(min).min(max.max(min))
    } else {
        value.max(min)
    }
}

fn clamp_height(element: &Element, value: f32) -> f32 {
    let min = element.min_height.max(0.0);
    let max = element.max_height.max(0.0);
    if max > 0.0 {
        value.max(min).min(max.max(min))
    } else {
        value.max(min)
    }
}

fn layout_element(
    element: &mut Element,
    measured: &MeasuredNode,
    x: f32,
    y: f32,
    text_system: &mut dyn TextSystem,
) {
    element.frame = LayoutRect::new(x, y, measured.width, measured.height);
    if element.children.is_empty() {
        return;
    }
    let content = content_rect(element);
    let layout_children = element
        .children
        .iter()
        .map(|child| measure_node(child, content.width, content.height, text_system))
        .collect();
    let layout_measured = MeasuredNode {
        width: measured.width,
        height: measured.height,
        children: layout_children,
    };
    match element.kind {
        ElementKind::Row => layout_row(element, &layout_measured, text_system),
        ElementKind::Column => layout_column(element, &layout_measured, text_system),
        ElementKind::Stack => layout_stack(element, &layout_measured, text_system),
        ElementKind::Rect | ElementKind::Polygon | ElementKind::Text | ElementKind::Image => {}
    }
}

fn layout_row(element: &mut Element, measured: &MeasuredNode, text_system: &mut dyn TextSystem) {
    let content = content_rect(element);
    let child_widths = row_child_widths(element, measured, content.width);
    let total_width = row_total_width_from(element, measured, &child_widths);
    let mut cursor = content.x + align_offset(element.main_align, content.width, total_width);

    for ((child, child_measured), child_width) in element
        .children
        .iter_mut()
        .zip(&measured.children)
        .zip(child_widths)
    {
        let mut adjusted = child_measured.clone();
        adjusted.width = child_width;
        let child_outer_height = outer_height(child, child_measured);
        let child_x = cursor + child.margin.left;
        let child_y = content.y
            + align_offset(element.cross_align, content.height, child_outer_height)
            + child.margin.top;
        layout_element(child, &adjusted, child_x, child_y, text_system);
        cursor += adjusted.width + child.margin.left + child.margin.right + element.spacing;
    }
}

fn layout_column(element: &mut Element, measured: &MeasuredNode, text_system: &mut dyn TextSystem) {
    let content = content_rect(element);
    let child_heights = column_child_heights(element, measured, content.height);
    let total_height = column_total_height_from(element, measured, &child_heights);
    let mut cursor = content.y + align_offset(element.main_align, content.height, total_height);

    for ((child, child_measured), child_height) in element
        .children
        .iter_mut()
        .zip(&measured.children)
        .zip(child_heights)
    {
        let mut adjusted = child_measured.clone();
        adjusted.height = child_height;
        let child_outer_width = outer_width(child, child_measured);
        let child_x = content.x
            + align_offset(element.cross_align, content.width, child_outer_width)
            + child.margin.left;
        let child_y = cursor + child.margin.top;
        layout_element(child, &adjusted, child_x, child_y, text_system);
        cursor += adjusted.height + child.margin.top + child.margin.bottom + element.spacing;
    }
}

fn layout_stack(element: &mut Element, measured: &MeasuredNode, text_system: &mut dyn TextSystem) {
    let parent = content_rect(element);
    for (child, child_measured) in element.children.iter_mut().zip(&measured.children) {
        let child_outer_width = outer_width(child, child_measured);
        let child_outer_height = outer_height(child, child_measured);
        let x = if child.has_x {
            parent.x + child.x + child.margin.left
        } else {
            parent.x
                + align_offset(element.cross_align, parent.width, child_outer_width)
                + child.margin.left
        };
        let y = if child.has_y {
            parent.y + child.y + child.margin.top
        } else {
            parent.y
                + align_offset(element.main_align, parent.height, child_outer_height)
                + child.margin.top
        };
        layout_element(child, child_measured, x, y, text_system);
    }
}

fn content_rect(element: &Element) -> LayoutRect {
    let frame = element.frame;
    LayoutRect::new(
        frame.x + element.padding.left,
        frame.y + element.padding.top,
        (frame.width - element.padding.horizontal()).max(0.0),
        (frame.height - element.padding.vertical()).max(0.0),
    )
}

fn outer_width(element: &Element, measured: &MeasuredNode) -> f32 {
    measured.width + element.margin.left + element.margin.right
}

fn outer_height(element: &Element, measured: &MeasuredNode) -> f32 {
    measured.height + element.margin.top + element.margin.bottom
}

fn row_total_width_from(element: &Element, measured: &MeasuredNode, widths: &[f32]) -> f32 {
    let mut total = 0.0;
    for (index, ((child, _), width)) in element
        .children
        .iter()
        .zip(&measured.children)
        .zip(widths.iter())
        .enumerate()
    {
        total += width + child.margin.left + child.margin.right;
        if index + 1 < measured.children.len() {
            total += element.spacing;
        }
    }
    total
}

fn row_child_widths(element: &Element, measured: &MeasuredNode, available_width: f32) -> Vec<f32> {
    let mut widths: Vec<f32> = element
        .children
        .iter()
        .zip(&measured.children)
        .map(|(child, child_measured)| clamp_width(child, child_measured.width))
        .collect();
    let total_width = row_total_width_from(element, measured, &widths);
    let mut remaining = (available_width - total_width).max(0.0);
    let mut active: Vec<usize> = element
        .children
        .iter()
        .enumerate()
        .filter(|(_, child)| child.grow > 0.0)
        .map(|(index, _)| index)
        .collect();

    while remaining > 0.0 && !active.is_empty() {
        let grow_sum: f32 = active
            .iter()
            .map(|&index| element.children[index].grow.max(0.0))
            .sum();
        if grow_sum <= 0.0 {
            break;
        }

        let mut consumed = 0.0;
        let mut next_active = Vec::with_capacity(active.len());
        for &index in &active {
            let child = &element.children[index];
            let weight = child.grow.max(0.0);
            let share = remaining * weight / grow_sum;
            let before = widths[index];
            let after = clamp_width(child, before + share);
            widths[index] = after;
            consumed += after - before;

            let max = child.max_layout_width.max(0.0);
            if max <= 0.0 || after < max {
                next_active.push(index);
            }
        }

        if consumed <= 0.0 {
            break;
        }

        remaining = (remaining - consumed).max(0.0);
        active = next_active;
    }

    widths
}

fn column_total_height_from(element: &Element, measured: &MeasuredNode, heights: &[f32]) -> f32 {
    let mut total = 0.0;
    for (index, ((child, _), height)) in element
        .children
        .iter()
        .zip(&measured.children)
        .zip(heights.iter())
        .enumerate()
    {
        total += height + child.margin.top + child.margin.bottom;
        if index + 1 < measured.children.len() {
            total += element.spacing;
        }
    }
    total
}

fn column_child_heights(
    element: &Element,
    measured: &MeasuredNode,
    available_height: f32,
) -> Vec<f32> {
    let mut heights: Vec<f32> = element
        .children
        .iter()
        .zip(&measured.children)
        .map(|(child, child_measured)| clamp_height(child, child_measured.height))
        .collect();
    let total_height = column_total_height_from(element, measured, &heights);
    let mut remaining = (available_height - total_height).max(0.0);
    let mut active: Vec<usize> = element
        .children
        .iter()
        .enumerate()
        .filter(|(_, child)| child.grow > 0.0)
        .map(|(index, _)| index)
        .collect();

    while remaining > 0.0 && !active.is_empty() {
        let grow_sum: f32 = active
            .iter()
            .map(|&index| element.children[index].grow.max(0.0))
            .sum();
        if grow_sum <= 0.0 {
            break;
        }

        let mut consumed = 0.0;
        let mut next_active = Vec::with_capacity(active.len());
        for &index in &active {
            let child = &element.children[index];
            let weight = child.grow.max(0.0);
            let share = remaining * weight / grow_sum;
            let before = heights[index];
            let after = clamp_height(child, before + share);
            heights[index] = after;
            consumed += after - before;

            let max = child.max_height.max(0.0);
            if max <= 0.0 || after < max {
                next_active.push(index);
            }
        }

        if consumed <= 0.0 {
            break;
        }

        remaining = (remaining - consumed).max(0.0);
        active = next_active;
    }

    heights
}

fn align_offset(align: Align, available: f32, size: f32) -> f32 {
    let remaining = available - size;
    match align {
        Align::Start => 0.0,
        Align::Center => remaining * 0.5,
        Align::End => remaining,
    }
}

fn positive_subtract(left: f32, right: f32) -> f32 {
    (left.max(0.0) - right.max(0.0)).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::layout_roots;
    use crate::{Align, Size, Ui};

    #[test]
    fn row_lays_out_children_with_gap() {
        let mut ui = Ui::new("test");
        ui.row("row").size(300.0, 50.0).gap(10.0).content(|ui| {
            ui.rect("a").size(50.0, 20.0).build();
            ui.rect("b").size(70.0, 20.0).build();
        });
        let mut roots = ui.into_roots();
        layout_roots(&mut roots, 300.0, 50.0);

        assert_eq!(roots[0].children[0].frame.x, 0.0);
        assert_eq!(roots[0].children[1].frame.x, 60.0);
    }

    #[test]
    fn column_centers_cross_axis() {
        let mut ui = Ui::new("test");
        ui.column("column")
            .size(200.0, 100.0)
            .align_items(Align::Center)
            .content(|ui| {
                ui.rect("child").size(40.0, 20.0).build();
            });
        let mut roots = ui.into_roots();
        layout_roots(&mut roots, 200.0, 100.0);

        assert_eq!(roots[0].children[0].frame.x, 80.0);
    }

    #[test]
    fn stack_fill_child_uses_parent_space() {
        let mut ui = Ui::new("test");
        ui.stack("stack").size(320.0, 180.0).content(|ui| {
            ui.rect("fill").size(Size::fill(), Size::fill()).build();
        });
        let mut roots = ui.into_roots();
        layout_roots(&mut roots, 320.0, 180.0);

        assert_eq!(roots[0].children[0].frame.width, 320.0);
        assert_eq!(roots[0].children[0].frame.height, 180.0);
    }

    #[test]
    fn fill_child_inside_fixed_parent_uses_parent_content_box() {
        let mut ui = Ui::new("test");
        ui.stack("root").size(300.0, 200.0).content(|ui| {
            ui.rect("fill").size(Size::fill(), Size::fill()).build();
        });
        let mut roots = ui.into_roots();
        layout_roots(&mut roots, 800.0, 600.0);

        assert_eq!(roots[0].frame.width, 300.0);
        assert_eq!(roots[0].frame.height, 200.0);
        assert_eq!(roots[0].children[0].frame.width, 300.0);
        assert_eq!(roots[0].children[0].frame.height, 200.0);
    }

    #[test]
    fn wrap_leaf_uses_available_size_like_eui_node() {
        let mut ui = Ui::new("test");
        ui.rect("leaf").wrap_content().build();
        let mut roots = ui.into_roots();
        layout_roots(&mut roots, 640.0, 480.0);

        assert_eq!(roots[0].frame.width, 640.0);
        assert_eq!(roots[0].frame.height, 480.0);
    }

    #[test]
    fn centered_oversized_row_content_can_start_negative_like_eui() {
        let mut ui = Ui::new("test");
        ui.row("row")
            .size(100.0, 20.0)
            .justify_content(Align::Center)
            .content(|ui| {
                ui.rect("wide").size(200.0, 10.0).build();
            });
        let mut roots = ui.into_roots();
        layout_roots(&mut roots, 100.0, 20.0);

        assert_eq!(roots[0].children[0].frame.x, -50.0);
    }

    #[test]
    fn stack_explicit_position_adds_margin_like_eui() {
        let mut ui = Ui::new("test");
        ui.stack("stack").size(200.0, 100.0).content(|ui| {
            ui.rect("child")
                .position(20.0, 10.0)
                .margin_each(3.0, 4.0, 5.0, 6.0)
                .size(40.0, 20.0)
                .build();
        });
        let mut roots = ui.into_roots();
        layout_roots(&mut roots, 200.0, 100.0);

        assert_eq!(roots[0].children[0].frame.x, 23.0);
        assert_eq!(roots[0].children[0].frame.y, 14.0);
    }

    #[test]
    fn row_ignores_child_explicit_position_like_eui() {
        let mut ui = Ui::new("test");
        ui.row("row").size(200.0, 40.0).content(|ui| {
            ui.rect("child")
                .position(90.0, 90.0)
                .size(30.0, 10.0)
                .build();
        });
        let mut roots = ui.into_roots();
        layout_roots(&mut roots, 200.0, 40.0);

        assert_eq!(roots[0].children[0].frame.x, 0.0);
        assert_eq!(roots[0].children[0].frame.y, 0.0);
    }

    #[test]
    fn padding_lays_out_children_inside_content_box() {
        let mut ui = Ui::new("test");
        ui.column("panel")
            .size(200.0, 120.0)
            .padding_each(20.0, 12.0, 30.0, 8.0)
            .content(|ui| {
                ui.rect("child").size(Size::fill(), Size::fill()).build();
            });
        let mut roots = ui.into_roots();
        layout_roots(&mut roots, 200.0, 120.0);

        let child = roots[0].children[0].frame;
        assert_eq!(child.x, 20.0);
        assert_eq!(child.y, 12.0);
        assert_eq!(child.width, 150.0);
        assert_eq!(child.height, 100.0);
    }

    #[test]
    fn row_grow_distributes_remaining_width() {
        let mut ui = Ui::new("test");
        ui.row("toolbar").size(500.0, 40.0).gap(10.0).content(|ui| {
            ui.rect("fixed").size(100.0, 20.0).build();
            ui.rect("grow_a").size(50.0, 20.0).grow(1.0).build();
            ui.rect("grow_b").size(50.0, 20.0).grow(2.0).build();
        });
        let mut roots = ui.into_roots();
        layout_roots(&mut roots, 500.0, 40.0);

        assert_eq!(roots[0].children[0].frame.width, 100.0);
        assert!((roots[0].children[1].frame.width - 143.33334).abs() < 0.001);
        assert!((roots[0].children[2].frame.width - 236.66667).abs() < 0.001);
    }

    #[test]
    fn column_grow_distributes_remaining_height() {
        let mut ui = Ui::new("test");
        ui.column("panel")
            .size(120.0, 340.0)
            .gap(10.0)
            .content(|ui| {
                ui.rect("fixed").size(80.0, 60.0).build();
                ui.rect("grow_a").size(80.0, 40.0).grow(1.0).build();
                ui.rect("grow_b").size(80.0, 40.0).grow(2.0).build();
            });
        let mut roots = ui.into_roots();
        layout_roots(&mut roots, 120.0, 340.0);

        assert_eq!(roots[0].children[0].frame.height, 60.0);
        assert!((roots[0].children[1].frame.height - 100.0).abs() < 0.001);
        assert!((roots[0].children[2].frame.height - 160.0).abs() < 0.001);
    }

    #[test]
    fn min_and_max_width_clamp_layout_size() {
        let mut ui = Ui::new("test");
        ui.row("row").size(500.0, 40.0).content(|ui| {
            ui.rect("min").size(40.0, 20.0).min_width(80.0).build();
            ui.rect("max").size(300.0, 20.0).max_width(120.0).build();
        });
        let mut roots = ui.into_roots();
        layout_roots(&mut roots, 500.0, 40.0);

        assert_eq!(roots[0].children[0].frame.width, 80.0);
        assert_eq!(roots[0].children[1].frame.width, 120.0);
    }

    #[test]
    fn grow_reallocates_space_when_one_child_hits_max_width() {
        let mut ui = Ui::new("test");
        ui.row("toolbar").size(500.0, 40.0).content(|ui| {
            ui.rect("fixed").size(100.0, 20.0).build();
            ui.rect("limited")
                .size(50.0, 20.0)
                .grow(1.0)
                .max_width(120.0)
                .build();
            ui.rect("flex").size(50.0, 20.0).grow(1.0).build();
        });
        let mut roots = ui.into_roots();
        layout_roots(&mut roots, 500.0, 40.0);

        assert_eq!(roots[0].children[0].frame.width, 100.0);
        assert_eq!(roots[0].children[1].frame.width, 120.0);
        assert_eq!(roots[0].children[2].frame.width, 280.0);
    }

    #[test]
    fn max_width_on_text_updates_layout_and_text_wrapping_budget() {
        let mut ui = Ui::new("test");
        ui.text("headline")
            .text("Hello from neo")
            .wrap(true)
            .max_width(120.0)
            .build();
        let mut roots = ui.into_roots();
        layout_roots(&mut roots, 640.0, 480.0);

        assert!(roots[0].frame.width > 0.0);
        assert!(roots[0].frame.width <= 120.0);
        assert_eq!(roots[0].text_max_width, 120.0);
    }

    #[test]
    fn text_wrap_content_uses_natural_text_size() {
        let mut ui = Ui::new("test");
        ui.text("label")
            .text("Sky")
            .font_size(20.0)
            .wrap_content()
            .build();
        let mut roots = ui.into_roots();
        layout_roots(&mut roots, 640.0, 480.0);

        assert!(roots[0].frame.width > 0.0);
        assert!(roots[0].frame.width < 120.0);
        assert!(roots[0].frame.height > 0.0);
        assert!(roots[0].frame.height < 60.0);
    }

    #[test]
    fn wrapped_text_natural_height_respects_max_width() {
        let mut ui = Ui::new("test");
        ui.text("label")
            .text("EUI layout primitives")
            .font_size(20.0)
            .wrap(true)
            .max_width(80.0)
            .wrap_content()
            .build();
        let mut roots = ui.into_roots();
        layout_roots(&mut roots, 640.0, 480.0);

        assert!(roots[0].frame.width > 0.0);
        assert!(roots[0].frame.width <= 80.0);
        assert!(roots[0].frame.height > 30.0);
    }
}
