use super::{Align, Element, ElementKind, LayoutRect, Size};

#[derive(Debug, Clone)]
struct MeasuredNode {
    width: f32,
    height: f32,
    children: Vec<MeasuredNode>,
}

/// Measure one element in logical pixels.
pub fn measure_element(element: &Element, available_width: f32, available_height: f32) -> [f32; 2] {
    let measured = measure_node(element, available_width, available_height);
    [measured.width, measured.height]
}

/// Compute layout frames for all root elements.
pub fn layout_roots(roots: &mut [Element], width: f32, height: f32) {
    for root in roots {
        let measured = measure_node(root, width, height);
        let x = if root.has_x { root.x } else { 0.0 };
        let y = if root.has_y { root.y } else { 0.0 };
        layout_element(root, &measured, x, y);
    }
}

fn measure_node(element: &Element, available_width: f32, available_height: f32) -> MeasuredNode {
    let children: Vec<_> = element
        .children
        .iter()
        .map(|child| measure_node(child, available_width, available_height))
        .collect();
    let content_width = measure_content_width(element, &children, available_width);
    let content_height = measure_content_height(element, &children, available_height);
    MeasuredNode {
        width: resolve_size(element.width, content_width, available_width),
        height: resolve_size(element.height, content_height, available_height),
        children,
    }
}

fn measure_content_width(
    element: &Element,
    children: &[MeasuredNode],
    available_width: f32,
) -> f32 {
    if children.is_empty() {
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
    available_height: f32,
) -> f32 {
    if children.is_empty() {
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

fn resolve_size(size: Size, content: f32, available: f32) -> f32 {
    match size {
        Size::Fixed(value) => value,
        Size::WrapContent => content,
        Size::Fill => {
            if available > 0.0 {
                available
            } else {
                content
            }
        }
    }
}

fn layout_element(element: &mut Element, measured: &MeasuredNode, x: f32, y: f32) {
    element.frame = LayoutRect::new(x, y, measured.width, measured.height);
    if element.children.is_empty() {
        return;
    }
    match element.kind {
        ElementKind::Row => layout_row(element, measured),
        ElementKind::Column => layout_column(element, measured),
        ElementKind::Stack => layout_stack(element, measured),
        ElementKind::Rect | ElementKind::Polygon | ElementKind::Text | ElementKind::Image => {}
    }
}

fn layout_row(element: &mut Element, measured: &MeasuredNode) {
    let parent = element.frame;
    let total_width = row_total_width(element, measured);
    let mut cursor = parent.x + align_offset(element.main_align, parent.width, total_width);

    for (child, child_measured) in element.children.iter_mut().zip(&measured.children) {
        let child_outer_height = outer_height(child, child_measured);
        let child_x = cursor + child.margin.left;
        let child_y = parent.y
            + align_offset(element.cross_align, parent.height, child_outer_height)
            + child.margin.top;
        layout_element(child, child_measured, child_x, child_y);
        cursor += outer_width(child, child_measured) + element.spacing;
    }
}

fn layout_column(element: &mut Element, measured: &MeasuredNode) {
    let parent = element.frame;
    let total_height = column_total_height(element, measured);
    let mut cursor = parent.y + align_offset(element.main_align, parent.height, total_height);

    for (child, child_measured) in element.children.iter_mut().zip(&measured.children) {
        let child_outer_width = outer_width(child, child_measured);
        let child_x = parent.x
            + align_offset(element.cross_align, parent.width, child_outer_width)
            + child.margin.left;
        let child_y = cursor + child.margin.top;
        layout_element(child, child_measured, child_x, child_y);
        cursor += outer_height(child, child_measured) + element.spacing;
    }
}

fn layout_stack(element: &mut Element, measured: &MeasuredNode) {
    let parent = element.frame;
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
        layout_element(child, child_measured, x, y);
    }
}

fn outer_width(element: &Element, measured: &MeasuredNode) -> f32 {
    measured.width + element.margin.left + element.margin.right
}

fn outer_height(element: &Element, measured: &MeasuredNode) -> f32 {
    measured.height + element.margin.top + element.margin.bottom
}

fn row_total_width(element: &Element, measured: &MeasuredNode) -> f32 {
    let mut total = 0.0;
    for (index, (child, child_measured)) in
        element.children.iter().zip(&measured.children).enumerate()
    {
        total += outer_width(child, child_measured);
        if index + 1 < measured.children.len() {
            total += element.spacing;
        }
    }
    total
}

fn column_total_height(element: &Element, measured: &MeasuredNode) -> f32 {
    let mut total = 0.0;
    for (index, (child, child_measured)) in
        element.children.iter().zip(&measured.children).enumerate()
    {
        total += outer_height(child, child_measured);
        if index + 1 < measured.children.len() {
            total += element.spacing;
        }
    }
    total
}

fn align_offset(align: Align, available: f32, size: f32) -> f32 {
    let remaining = available - size;
    match align {
        Align::Start => 0.0,
        Align::Center => remaining * 0.5,
        Align::End => remaining,
    }
}

#[cfg(test)]
mod tests {
    use super::layout_roots;
    use crate::ui::neo::{Align, Size, Ui};

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
    fn fill_child_inside_fixed_parent_uses_original_available_size_like_eui() {
        let mut ui = Ui::new("test");
        ui.stack("root").size(300.0, 200.0).content(|ui| {
            ui.rect("fill").size(Size::fill(), Size::fill()).build();
        });
        let mut roots = ui.into_roots();
        layout_roots(&mut roots, 800.0, 600.0);

        assert_eq!(roots[0].frame.width, 300.0);
        assert_eq!(roots[0].frame.height, 200.0);
        assert_eq!(roots[0].children[0].frame.width, 800.0);
        assert_eq!(roots[0].children[0].frame.height, 600.0);
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
}
